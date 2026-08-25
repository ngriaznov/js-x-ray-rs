//! Upstream: `src/obfuscators/jsfuck.ts`

use crate::deobfuscator::ObfuscatedCounters;

// CONSTANTS
const K_JSFUCK_MINIMUM_DOUBLE_UNARY_EXPR: u32 = 5;

pub fn verify(counters: &ObfuscatedCounters) -> bool {
    let has_zero_assign = counters.assignment_expression == 0
        && counters.function_declaration == 0
        && counters.property == 0
        && counters.variable_declarator == 0;

    has_zero_assign && counters.double_unary_expression >= K_JSFUCK_MINIMUM_DOUBLE_UNARY_EXPR
}
