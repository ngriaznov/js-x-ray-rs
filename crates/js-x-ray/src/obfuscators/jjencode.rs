//! Upstream: `src/obfuscators/jjencode.ts`

use crate::deobfuscator::{ObfuscatedCounters, ObfuscatedIdentifier};

// CONSTANTS
// Upstream `kJJRegularSymbols` = new Set(["$", "_"]).
fn is_jj_regular_symbol(char: char) -> bool {
    matches!(char, '$' | '_')
}

pub fn verify(identifiers: &[ObfuscatedIdentifier], counters: &ObfuscatedCounters) -> bool {
    if counters.variable_declarator > 0 || counters.function_declaration > 0 {
        return false;
    }
    if counters.assignment_expression > counters.property {
        return false;
    }

    // NOTE: upstream's `notNullOrUndefined(name)` guard is always true here
    // (Rust names are never null); an empty name matches like upstream.
    let match_count = identifiers
        .iter()
        .filter(|identifier| identifier.name.chars().all(is_jj_regular_symbol))
        .count();
    // JS semantics: 0 / 0 => NaN, and NaN > 80 is false.
    let pourcent = (match_count as f64 / identifiers.len() as f64) * 100.0;

    pourcent > 80.0
}
