//! Upstream: `src/obfuscators/freejsobfuscator.ts`

use std::collections::BTreeMap;

use regex::Regex;

use crate::deobfuscator::ObfuscatedIdentifier;

pub fn verify(identifiers: &[ObfuscatedIdentifier], prefix: &BTreeMap<String, usize>) -> bool {
    // Upstream: Object.keys(prefix).pop()! — the caller guarantees a single
    // entry (uPrefixNames.size === 1); an empty map cannot verify.
    let Some(p_value) = prefix.keys().next_back() else {
        return false;
    };
    let Ok(regex) = Regex::new(&format!(
        "^{}[a-zA-Z]{{1,2}}[0-9]{{0,2}}$",
        regex::escape(p_value)
    )) else {
        return false;
    };

    identifiers
        .iter()
        .all(|identifier| regex.is_match(&identifier.name))
}
