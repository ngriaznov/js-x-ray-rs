//! Upstream: `src/Deobfuscator.ts`
//!
//! PORT-TODO(stub): faithful port pending (jsfuck/jjencode/morse/
//! freejsobfuscator/obfuscator.io detection + NodeCounter aggregation).

use serde_json::Value;

use crate::estree::Node;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObfuscatedIdentifier {
    pub name: String,
    /// ESTree node type context (e.g. "Property", "VariableDeclarator").
    pub r#type: String,
}

#[derive(Debug, Default)]
pub struct Deobfuscator {
    pub morse_literals: indexmap::IndexSet<String>,
    pub literal_scores: Vec<u32>,
    pub encoded_array_value: u32,
    pub identifiers: Vec<ObfuscatedIdentifier>,
    pub deep_binary_expression: u32,
    pub double_unary_array: u32,
    pub has_dictionary_string: bool,
    pub has_prefixed_identifiers: bool,
}

impl Deobfuscator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Upstream `analyzeString`.
    pub fn analyze_string(&mut self, str_: &str) {
        // PORT-TODO(stub)
        let _ = str_;
    }

    /// Upstream `walk`.
    pub fn walk(&mut self, node: &Node) {
        // PORT-TODO(stub)
        let _ = node;
    }

    /// Upstream `aggregateCounters`.
    pub fn aggregate_counters(&self) -> Value {
        // PORT-TODO(stub)
        Value::Null
    }

    /// Upstream `assertObfuscation` — returns the obfuscator name if the
    /// source is considered obfuscated.
    pub fn assert_obfuscation(&self) -> Option<String> {
        // PORT-TODO(stub)
        None
    }
}
