//! Upstream: `src/utils/hex.ts` (exported as `Hex`).

use std::sync::LazyLock;

use serde_json::Value;

use crate::estree::{to_raw, to_value};

use super::string_char_diversity;

/// `["require", "length"]` hex-encoded.
static UNSAFE_HEX_VALUES: LazyLock<Vec<String>> =
    LazyLock::new(|| ["require", "length"].iter().map(|v| encode_hex(v)).collect());

pub const SAFE_HEX_VALUES: &[&str] = &[
    "0123456789",
    "123456789",
    "abcdef",
    "abc123456789",
    "0123456789abcdef",
    "abcdef0123456789abcdef",
];

pub fn encode_hex(value: &str) -> String {
    value.bytes().map(|b| format!("{b:02x}")).collect()
}

/// `Buffer.from(value, "hex").toString()` — decodes leading hex pairs,
/// interpreting the bytes as UTF-8 (lossy).
pub fn decode_hex_lossy(value: &str) -> String {
    let bytes: Vec<u8> = value
        .as_bytes()
        .chunks_exact(2)
        .map_while(|pair| {
            let s = std::str::from_utf8(pair).ok()?;
            u8::from_str_radix(s, 16).ok()
        })
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Upstream `Hex.isHex` for a Literal node or plain string value.
pub fn is_hex(any_value: &Value) -> bool {
    is_hex_str(&to_value(any_value))
}

pub fn is_hex_str(value: &str) -> bool {
    value.len() >= 4 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Upstream `Hex.isSafe`.
pub fn is_safe(any_value: &Value) -> bool {
    let Some(raw_value) = to_raw(any_value) else {
        return false;
    };
    if UNSAFE_HEX_VALUES.iter().any(|v| *v == raw_value) {
        return false;
    }

    let char_count = string_char_diversity(&raw_value, &[]);
    if is_single_class(&raw_value) || raw_value.len() <= 5 || char_count <= 2 {
        return true;
    }

    let lowered = raw_value.to_lowercase();
    SAFE_HEX_VALUES.iter().any(|value| lowered.starts_with(value))
}

/// `/^([0-9]+|[a-z]+|[A-Z]+)$/`
fn is_single_class(value: &str) -> bool {
    !value.is_empty()
        && (value.bytes().all(|b| b.is_ascii_digit())
            || value.bytes().all(|b| b.is_ascii_lowercase())
            || value.bytes().all(|b| b.is_ascii_uppercase()))
}
