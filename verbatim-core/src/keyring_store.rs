use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

#[cfg(not(test))]
const SERVICE: &str = "verbatim";
/// Tests use a distinct service name so they never touch the user's real
/// production keyring entries (they would otherwise share the "verbatim"
/// namespace and `test_save_to_and_load_from_roundtrip` deletes its keys
/// on cleanup).
#[cfg(test)]
const SERVICE: &str = "verbatim-test";

/// Session-level flag: once we successfully access the keyring, skip further probes.
static KEYRING_CONFIRMED: AtomicBool = AtomicBool::new(false);

/// In-memory cache of values last written to the keyring, used to skip redundant writes.
static WRITE_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn write_cache() -> &'static Mutex<HashMap<String, String>> {
    WRITE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Store a secret in the OS keyring.
/// Returns Ok(true) if stored successfully, Ok(false) if keyring unavailable.
pub fn store_secret(key_name: &str, value: &str) -> Result<bool, keyring::Error> {
    if value.is_empty() {
        // Don't store empty values; delete any existing entry
        delete_secret(key_name);
        return Ok(true);
    }
    let entry = keyring::Entry::new(SERVICE, key_name)?;
    match entry.set_password(value) {
        Ok(()) => {
            KEYRING_CONFIRMED.store(true, Ordering::Relaxed);
            tracing::debug!(key = key_name, "stored secret in keyring");
            Ok(true)
        }
        Err(e) => {
            KEYRING_CONFIRMED.store(false, Ordering::Relaxed);
            tracing::warn!(key = key_name, error = %e, "failed to store secret in keyring");
            Err(e)
        }
    }
}

/// Store a secret only if it differs from what we last wrote (or read) from the keyring.
/// Skips the keyring call entirely when the cached value matches.
pub fn store_secret_if_changed(key_name: &str, value: &str) -> Result<bool, keyring::Error> {
    if value.is_empty() {
        delete_secret(key_name);
        if let Ok(mut cache) = write_cache().lock() {
            cache.remove(key_name);
        }
        return Ok(true);
    }
    // Check write cache — skip if unchanged
    if let Ok(cache) = write_cache().lock() {
        if cache.get(key_name).map(|v| v.as_str()) == Some(value) {
            tracing::trace!(key = key_name, "keyring write skipped (cached)");
            return Ok(true);
        }
    }
    let result = store_secret(key_name, value);
    if result.is_ok() {
        if let Ok(mut cache) = write_cache().lock() {
            cache.insert(key_name.to_string(), value.to_string());
        }
    }
    result
}

/// Retrieve a secret from the OS keyring.
/// Returns None if the key doesn't exist or keyring is unavailable.
pub fn get_secret(key_name: &str) -> Option<String> {
    let entry = keyring::Entry::new(SERVICE, key_name).ok()?;
    match entry.get_password() {
        Ok(password) => {
            KEYRING_CONFIRMED.store(true, Ordering::Relaxed);
            tracing::debug!(key = key_name, "loaded secret from keyring");
            Some(password)
        }
        Err(keyring::Error::NoEntry) => {
            // NoEntry means the keyring is available, just no value stored
            KEYRING_CONFIRMED.store(true, Ordering::Relaxed);
            None
        }
        Err(e) => {
            KEYRING_CONFIRMED.store(false, Ordering::Relaxed);
            tracing::warn!(key = key_name, error = %e, "failed to read secret from keyring");
            None
        }
    }
}

/// Delete a secret from the OS keyring. Silently ignores errors.
pub fn delete_secret(key_name: &str) {
    if let Ok(entry) = keyring::Entry::new(SERVICE, key_name) {
        match entry.delete_credential() {
            Ok(()) => tracing::debug!(key = key_name, "deleted secret from keyring"),
            Err(keyring::Error::NoEntry) => {}
            Err(e) => tracing::warn!(key = key_name, error = %e, "failed to delete secret from keyring"),
        }
    }
    if let Ok(mut cache) = write_cache().lock() {
        cache.remove(key_name);
    }
}

/// Check if the keyring backend is available.
/// Uses a session-level cache: once confirmed, returns true without touching the keyring.
/// On a cache miss, does a single get_password probe (1 prompt max, down from 3).
pub fn is_available() -> bool {
    if KEYRING_CONFIRMED.load(Ordering::Relaxed) {
        return true;
    }
    // Single-operation probe: try to read a key that may or may not exist.
    // This triggers at most 1 unlock prompt (vs. set+get+delete = 3 in the old impl).
    let Ok(entry) = keyring::Entry::new(SERVICE, "_probe") else {
        return false;
    };
    match entry.get_password() {
        Ok(_) | Err(keyring::Error::NoEntry) => {
            KEYRING_CONFIRMED.store(true, Ordering::Relaxed);
            true
        }
        Err(_) => false,
    }
}

/// Seed the write cache after a successful keyring read.
/// Prevents redundant writes on the next save when keys haven't changed.
pub fn seed_write_cache(key_name: &str, value: &str) {
    if let Ok(mut cache) = write_cache().lock() {
        cache.insert(key_name.to_string(), value.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_available_returns_bool() {
        // Should not panic regardless of backend availability
        let _ = is_available();
    }

    #[test]
    fn test_get_secret_nonexistent_returns_none() {
        let result = get_secret("definitely_not_stored_xyz_test_key_12345");
        assert!(result.is_none());
    }

    #[test]
    fn test_store_empty_value_deletes() {
        // Storing empty value should call delete and return Ok(true)
        let result = store_secret("test_empty_val_key", "");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), true);
        // Verify the key doesn't exist
        assert!(get_secret("test_empty_val_key").is_none());
    }

    #[test]
    fn test_seed_write_cache_populates() {
        seed_write_cache("test_seed_key", "test_val");
        let cache = write_cache().lock().unwrap();
        assert_eq!(cache.get("test_seed_key").map(|s| s.as_str()), Some("test_val"));
    }

    #[test]
    fn test_store_secret_if_changed_skips_cached() {
        // Seed the cache with a known value
        seed_write_cache("test_skip_key", "same_value");
        // store_secret_if_changed should return Ok(true) without hitting keyring
        let result = store_secret_if_changed("test_skip_key", "same_value");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), true);
    }

    #[test]
    fn test_store_secret_if_changed_empty_clears_cache() {
        seed_write_cache("test_clear_key", "some_value");
        let result = store_secret_if_changed("test_clear_key", "");
        assert!(result.is_ok());
        let cache = write_cache().lock().unwrap();
        assert!(cache.get("test_clear_key").is_none());
    }
}
