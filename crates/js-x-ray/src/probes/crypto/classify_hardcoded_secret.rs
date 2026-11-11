//! Upstream: `src/probes/crypto/classifyHardcodedSecret.ts`

/// `short-<name>` when the secret is under `min_length` UTF-8 bytes
/// (`Buffer.byteLength`), `hardcoded-<name>` otherwise.
pub fn classify_hardcoded_secret(value: &str, name: &str, min_length: usize) -> String {
    if value.len() < min_length {
        format!("short-{name}")
    } else {
        format!("hardcoded-{name}")
    }
}
