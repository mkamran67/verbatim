# Fix OpenAI Costs API Parsing

## Problem

The app crashed when fetching usage costs from the OpenAI organization costs API with:

```
Failed to parse OpenAI costs response: invalid type: string "0.0001690500000000000000000000000", expected f64
```

The `amount.value` field in the API response is a JSON **string** (e.g. `"0.00016905"`), but the `CostsAmount` struct expected a native `f64`.

## Solution

Added a custom serde deserializer (`deserialize_string_or_f64`) on `CostsAmount.value` that accepts both numeric strings and raw `f64` values. This handles the current API behavior (string) while staying forward-compatible if OpenAI ever switches to a numeric type.

**File changed:** `verbatim-core/src/stt/openai.rs`
