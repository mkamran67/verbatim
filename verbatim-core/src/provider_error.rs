//! Classification of provider HTTP/API errors so the rotation engine and the
//! notification UI can react appropriately. Inputs are typically a
//! `reqwest::Error` or a `(status, body)` pair captured from a failed API
//! call.

use reqwest::StatusCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderFailure {
    /// Account is out of credit / quota / billing limit reached.
    Exhausted,
    /// 401/403 — invalid or revoked credentials.
    AuthError,
    /// 429 without a quota signal — temporary back-off.
    RateLimited,
    /// 5xx or network blip — likely transient.
    Transient,
    /// Anything else — treated as non-rotating by default.
    Other,
}

/// Classify a `(status, body)` pair into a `ProviderFailure`.
///
/// The body is searched (case-insensitive) for known exhaustion markers used
/// by OpenAI, Deepgram, and Smallest.
pub fn classify(status: Option<u16>, body: &str) -> ProviderFailure {
    let body_lc = body.to_ascii_lowercase();
    let exhausted_marker = body_lc.contains("insufficient_quota")
        || body_lc.contains("insufficient quota")
        || body_lc.contains("billing_hard_limit")
        || body_lc.contains("payment_required")
        || body_lc.contains("out of credits")
        || body_lc.contains("quota exceeded")
        || body_lc.contains("balance is too low");

    match status.and_then(|s| StatusCode::from_u16(s).ok()) {
        Some(StatusCode::PAYMENT_REQUIRED) => ProviderFailure::Exhausted,
        Some(s) if s == StatusCode::UNAUTHORIZED || s == StatusCode::FORBIDDEN => {
            ProviderFailure::AuthError
        }
        Some(StatusCode::TOO_MANY_REQUESTS) => {
            if exhausted_marker {
                ProviderFailure::Exhausted
            } else {
                ProviderFailure::RateLimited
            }
        }
        Some(s) if s.is_server_error() => ProviderFailure::Transient,
        _ if exhausted_marker => ProviderFailure::Exhausted,
        _ => ProviderFailure::Other,
    }
}

/// Convenience wrapper for a `reqwest::Error`. Network errors and timeouts
/// map to `Transient`; everything else falls back to status-based classification
/// (with an empty body).
pub fn classify_reqwest(err: &reqwest::Error) -> ProviderFailure {
    if err.is_timeout() || err.is_connect() || err.is_request() {
        return ProviderFailure::Transient;
    }
    classify(err.status().map(|s| s.as_u16()), "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_402_is_exhausted() {
        assert_eq!(classify(Some(402), ""), ProviderFailure::Exhausted);
    }

    #[test]
    fn classify_401_is_auth() {
        assert_eq!(classify(Some(401), ""), ProviderFailure::AuthError);
        assert_eq!(classify(Some(403), ""), ProviderFailure::AuthError);
    }

    #[test]
    fn classify_429_plain_is_ratelimited() {
        assert_eq!(classify(Some(429), ""), ProviderFailure::RateLimited);
    }

    #[test]
    fn classify_429_with_quota_marker_is_exhausted() {
        let body = r#"{"error":{"code":"insufficient_quota","message":"You exceeded your current quota"}}"#;
        assert_eq!(classify(Some(429), body), ProviderFailure::Exhausted);
    }

    #[test]
    fn classify_500_is_transient() {
        assert_eq!(classify(Some(500), ""), ProviderFailure::Transient);
        assert_eq!(classify(Some(503), ""), ProviderFailure::Transient);
    }

    #[test]
    fn classify_body_only_quota_signal() {
        // No status available (e.g. parsing error) but body has the marker.
        assert_eq!(
            classify(None, "Error: out of credits"),
            ProviderFailure::Exhausted,
        );
    }

    #[test]
    fn classify_unknown_is_other() {
        assert_eq!(classify(Some(418), ""), ProviderFailure::Other);
        assert_eq!(classify(None, ""), ProviderFailure::Other);
    }
}
