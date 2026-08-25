//! Upstream: `src/obfuscators/obfuscator-io.ts`

use crate::deobfuscator::{Deobfuscator, ObfuscatedCounters};

pub fn verify(deobfuscator: &Deobfuscator, counters: &ObfuscatedCounters) -> bool {
    if counters.member_expression.get("false").copied().unwrap_or(0) > 0
        // `!counters.DoubleUnaryExpression` (JS falsiness: 0)
        || counters.double_unary_expression == 0
    {
        return false;
    }

    let has_some_patterns = counters.double_unary_expression > 0
        || deobfuscator.deep_binary_expression > 0
        || deobfuscator.encoded_array_value > 0
        || deobfuscator.has_dictionary_string;

    // TODO(upstream): hasPrefixedIdentifiers only work for hexadecimal id names generator
    deobfuscator.has_prefixed_identifiers && has_some_patterns
}
