//! Upstream: `src/Deobfuscator.ts`

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use crate::estree::{Node, get_variable_declaration_identifiers, is_identifier, node_type};
use crate::node_counter::{NodeCounter, NodeCounterOptions};
use crate::obfuscators::{freejsobfuscator, jjencode, jsfuck, obfuscator_io};
use crate::utils::{CommonHexadecimalPrefixResult, common_hexadecimal_prefix, string_suspicion_score};

// CONSTANTS
const K_DICTIONARY_STR_PARTS: [&str; 3] = [
    "abcdefghijklmnopqrstuvwxyz",
    "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
    "0123456789",
];
const K_MINIMUM_IDS_COUNT: usize = 5;

/// Upstream `ObfuscatedEngine` (returned as a plain `String` by
/// [`Deobfuscator::assert_obfuscation`]): `jsfuck`, `jjencode`, `morse`,
/// `freejsobfuscator`, `obfuscator.io`, `unknown`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObfuscatedIdentifier {
    pub name: String,
    /// ESTree node type context (e.g. "Property", "VariableDeclarator").
    pub r#type: String,
}

/// Upstream `ObfuscatedCounters`. Upstream keeps most fields optional but in
/// practice every counter is always aggregated, so plain values are used;
/// `?? 0` fallbacks in the obfuscator verifiers become direct reads.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObfuscatedCounters {
    pub identifiers: usize,
    /// `VariableDeclaration[kind]` lookup properties (e.g. `const`/`let`/`var`).
    pub variable_declaration: indexmap::IndexMap<String, u32>,
    pub variable_declarator: u32,
    pub assignment_expression: u32,
    pub function_declaration: u32,
    /// `MemberExpression[computed]` lookup properties (keys `"true"`/`"false"`).
    pub member_expression: indexmap::IndexMap<String, u32>,
    pub property: u32,
    pub double_unary_expression: u32,
}

impl ObfuscatedCounters {
    /// The aggregate as an upstream-shaped JSON object (useful for tests).
    pub fn to_value(&self) -> Value {
        let map_value = |map: &indexmap::IndexMap<String, u32>| {
            Value::Object(
                map.iter()
                    .map(|(key, count)| (key.clone(), Value::from(*count)))
                    .collect(),
            )
        };

        // Same key order as the upstream reduce over `#counters`.
        let mut object = serde_json::Map::new();
        object.insert("Identifiers".to_owned(), Value::from(self.identifiers));
        object.insert(
            "VariableDeclaration".to_owned(),
            map_value(&self.variable_declaration),
        );
        object.insert(
            "AssignmentExpression".to_owned(),
            Value::from(self.assignment_expression),
        );
        object.insert(
            "FunctionDeclaration".to_owned(),
            Value::from(self.function_declaration),
        );
        object.insert(
            "MemberExpression".to_owned(),
            map_value(&self.member_expression),
        );
        object.insert("Property".to_owned(), Value::from(self.property));
        object.insert(
            "DoubleUnaryExpression".to_owned(),
            Value::from(self.double_unary_expression),
        );
        object.insert(
            "VariableDeclarator".to_owned(),
            Value::from(self.variable_declarator),
        );

        Value::Object(object)
    }
}

// Indices into `Deobfuscator::counters`, mirroring the upstream `#counters`
// array order.
const K_VARIABLE_DECLARATION: usize = 0;
const K_ASSIGNMENT_EXPRESSION: usize = 1;
const K_FUNCTION_DECLARATION: usize = 2;
const K_MEMBER_EXPRESSION: usize = 3;
const K_PROPERTY: usize = 4;
const K_DOUBLE_UNARY_EXPRESSION: usize = 5;
const K_VARIABLE_DECLARATOR: usize = 6;

#[derive(Debug)]
pub struct Deobfuscator {
    pub deep_binary_expression: u32,
    pub encoded_array_value: u32,
    pub has_dictionary_string: bool,
    pub has_prefixed_identifiers: bool,

    pub morse_literals: indexmap::IndexSet<String>,
    pub literal_scores: Vec<u32>,

    pub identifiers: Vec<ObfuscatedIdentifier>,

    counters: [NodeCounter; 7],
}

impl Default for Deobfuscator {
    fn default() -> Self {
        Self::new()
    }
}

impl Deobfuscator {
    pub fn new() -> Self {
        Self {
            deep_binary_expression: 0,
            encoded_array_value: 0,
            has_dictionary_string: false,
            has_prefixed_identifiers: false,
            morse_literals: indexmap::IndexSet::new(),
            literal_scores: Vec::new(),
            identifiers: Vec::new(),
            counters: [
                NodeCounter::new("VariableDeclaration[kind]"),
                NodeCounter::new("AssignmentExpression"),
                NodeCounter::new("FunctionDeclaration"),
                NodeCounter::new("MemberExpression[computed]"),
                NodeCounter::with_options(
                    "Property",
                    NodeCounterOptions {
                        filter: Some(|node| {
                            is_identifier(node.get("key").unwrap_or(&Value::Null))
                        }),
                        ..Default::default()
                    },
                ),
                NodeCounter::with_options(
                    "UnaryExpression",
                    NodeCounterOptions {
                        name: Some("DoubleUnaryExpression"),
                        filter: Some(|node| {
                            let argument = node.get("argument").unwrap_or(&Value::Null);
                            node_type(argument) == Some("UnaryExpression")
                                && node_type(argument.get("argument").unwrap_or(&Value::Null))
                                    == Some("ArrayExpression")
                        }),
                    },
                ),
                NodeCounter::new("VariableDeclarator"),
            ],
        }
    }

    /// Upstream `#isMorse`.
    fn is_morse(str_: &str) -> bool {
        // Upstream: /^[.-]{1,5}(?:[\s\t]+[.-]{1,5})*(?:[\s\t]+[.-]{1,5}(?:[\s\t]+[.-]{1,5})*)*$/
        // `[\s\t]` is spelled out with the exact JavaScript `\s` character
        // class so the semantics match the JS regex engine.
        static MORSE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
            const WS: &str = r"[\t\n\x0B\x0C\r \x{00A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]";
            Regex::new(&format!(
                r"^[.\-]{{1,5}}(?:{WS}+[.\-]{{1,5}})*(?:{WS}+[.\-]{{1,5}}(?:{WS}+[.\-]{{1,5}})*)*$"
            ))
            .expect("valid regex")
        });

        MORSE_REGEX.is_match(str_)
    }

    /// Upstream `analyzeString`.
    pub fn analyze_string(&mut self, str_: &str) {
        let score = string_suspicion_score(str_);
        if score != 0 {
            self.literal_scores.push(score);
        }

        if !self.has_dictionary_string {
            let is_dictionary_str = K_DICTIONARY_STR_PARTS
                .iter()
                .all(|word| str_.contains(word));
            if is_dictionary_str {
                self.has_dictionary_string = true;
            }
        }

        // Searching for morse string like "--.- --.--"
        if Self::is_morse(str_) {
            self.morse_literals.insert(str_.to_owned());
        }
    }

    /// Upstream `#extractCounterIdentifiers` (free function so it can run
    /// while `self.counters` is mutably borrowed).
    fn extract_counter_identifiers(
        identifiers: &mut Vec<ObfuscatedIdentifier>,
        counter_type: &str,
        node: &Node,
    ) {
        // Upstream guards on `node === null`.
        if node.is_null() {
            return;
        }

        match counter_type {
            "VariableDeclarator" | "AssignmentExpression" => {
                for (name, _) in get_variable_declaration_identifiers(node, None) {
                    identifiers.push(ObfuscatedIdentifier {
                        name,
                        r#type: counter_type.to_owned(),
                    });
                }
            }
            "Property" | "FunctionDeclaration" => {
                if is_identifier(node)
                    && let Some(name) = node.get("name").and_then(Value::as_str)
                {
                    identifiers.push(ObfuscatedIdentifier {
                        name: name.to_owned(),
                        r#type: counter_type.to_owned(),
                    });
                }
            }
            _ => {}
        }
    }

    /// Upstream `kIdentifierNodeExtractor` (`extractNode("Identifier")`).
    fn extract_identifier_nodes<'a>(
        identifiers: &mut Vec<ObfuscatedIdentifier>,
        r#type: &str,
        nodes: impl IntoIterator<Item = &'a Node>,
    ) {
        for node in nodes {
            if is_identifier(node)
                && let Some(name) = node.get("name").and_then(Value::as_str)
            {
                identifiers.push(ObfuscatedIdentifier {
                    name: name.to_owned(),
                    r#type: r#type.to_owned(),
                });
            }
        }
    }

    /// Upstream `walk`.
    pub fn walk(&mut self, node: &Node) {
        match node_type(node) {
            Some("ClassDeclaration") => {
                let candidates = [node.get("id"), node.get("superClass")]
                    .into_iter()
                    .flatten()
                    .filter(|candidate| !candidate.is_null());
                Self::extract_identifier_nodes(
                    &mut self.identifiers,
                    "ClassDeclaration",
                    candidates,
                );
            }
            Some("FunctionDeclaration" | "FunctionExpression") => {
                let params = node
                    .get("params")
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                Self::extract_identifier_nodes(&mut self.identifiers, "FunctionParams", params);
            }
            Some("MethodDefinition") => {
                Self::extract_identifier_nodes(
                    &mut self.identifiers,
                    "MethodDefinition",
                    node.get("key"),
                );
            }
            _ => {}
        }

        for counter in &mut self.counters {
            let matched = counter.walk(node);
            if !matched {
                continue;
            }

            // Upstream attaches `match` callbacks to these counters only;
            // each callback forwards a specific child node.
            let target = match counter.r#type.as_str() {
                "AssignmentExpression" => node.get("left"),
                "FunctionDeclaration" => node.get("id"),
                "Property" => node.get("key"),
                "VariableDeclarator" => node.get("id"),
                _ => None,
            };
            if let Some(target) = target {
                Self::extract_counter_identifiers(
                    &mut self.identifiers,
                    &counter.r#type,
                    target,
                );
            }
        }
    }

    /// Upstream `aggregateCounters`.
    pub fn aggregate_counters(&self) -> ObfuscatedCounters {
        ObfuscatedCounters {
            identifiers: self.identifiers.len(),
            variable_declaration: self.counters[K_VARIABLE_DECLARATION].properties().clone(),
            assignment_expression: self.counters[K_ASSIGNMENT_EXPRESSION].count(),
            function_declaration: self.counters[K_FUNCTION_DECLARATION].count(),
            member_expression: self.counters[K_MEMBER_EXPRESSION].properties().clone(),
            property: self.counters[K_PROPERTY].count(),
            double_unary_expression: self.counters[K_DOUBLE_UNARY_EXPRESSION].count(),
            variable_declarator: self.counters[K_VARIABLE_DECLARATOR].count(),
        }
    }

    /// Upstream `#calcAvgPrefixedIdentifiers`.
    fn calc_avg_prefixed_identifiers(
        counters: &ObfuscatedCounters,
        prefix: &BTreeMap<String, usize>,
    ) -> f64 {
        let mut values_arr: Vec<usize> = prefix.values().copied().collect();
        values_arr.sort_unstable();
        if values_arr.is_empty() {
            return 0.0;
        }

        let nb_of_prefixed_ids = if values_arr.len() == 1 {
            values_arr.pop().expect("non-empty")
        } else {
            values_arr.pop().expect("non-empty") + values_arr.pop().expect("non-empty")
        };
        // JS subtraction/division semantics: may be negative, zero (Infinity)
        // or 0/0 (NaN) — f64 reproduces all of them.
        let max_ids = counters.identifiers as f64 - counters.property as f64;

        (nb_of_prefixed_ids as f64 / max_ids) * 100.0
    }

    /// Upstream `assertObfuscation` — returns the obfuscator name if the
    /// source is considered obfuscated.
    pub fn assert_obfuscation(&mut self) -> Option<String> {
        let counters = self.aggregate_counters();

        if jsfuck::verify(&counters) {
            return Some("jsfuck".to_owned());
        }
        if jjencode::verify(&self.identifiers, &counters) {
            return Some("jjencode".to_owned());
        }
        if self.morse_literals.len() >= 36 {
            return Some("morse".to_owned());
        }

        let names: Vec<String> = self
            .identifiers
            .iter()
            .map(|identifier| identifier.name.clone())
            .collect();
        let CommonHexadecimalPrefixResult { prefix, .. } = common_hexadecimal_prefix(&names);
        let u_prefix_names_size = prefix.len();

        if self.identifiers.len() > K_MINIMUM_IDS_COUNT && u_prefix_names_size > 0 {
            self.has_prefixed_identifiers =
                Self::calc_avg_prefixed_identifiers(&counters, &prefix) > 80.0;
        }

        if u_prefix_names_size == 1 && freejsobfuscator::verify(&self.identifiers, &prefix) {
            return Some("freejsobfuscator".to_owned());
        }
        if obfuscator_io::verify(self, &counters) {
            return Some("obfuscator.io".to_owned());
        }
        // if ((identifierLength > (kMinimumIdsCount * 3) && this.hasPrefixedIdentifiers)
        //     && (oneTimeOccurence <= 3 || this.encodedArrayValue > 0)) {
        //     return "unknown";
        // }

        None
    }
}
