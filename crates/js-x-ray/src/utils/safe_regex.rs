//! Port of the `safe-regex` npm package (used by the isLiteralRegex and
//! isRegexObject probes to flag exponential-backtracking regexes).
//!
//! PORT-TODO(stub): faithful port pending (star-height analysis over the
//! regex AST via `regex-syntax`).

/// Returns `true` when the pattern is considered safe.
pub fn is_safe_regex(pattern: &str) -> bool {
    // PORT-TODO(stub)
    let _ = pattern;
    true
}
