//! Upstream: `src/NodeCounter.ts`
//!
//! Upstream `NodeCounter` accepts `filter` and `match` callbacks. The
//! `filter` callback is kept as a plain function pointer (all upstream
//! filters are stateless). The `match` callback — which upstream uses to
//! mutate `Deobfuscator` state — is replaced by [`NodeCounter::walk`]
//! returning `true` exactly when the upstream `match` callback would have
//! been invoked, so the caller dispatches the side effect itself. This
//! avoids self-referential closures while reproducing the exact counting
//! behavior.

use indexmap::IndexMap;

use crate::estree::{Node, is_node, js_string, node_type};

/// Upstream `NodeCounterFilterCallback`.
pub type NodeCounterFilterCallback = fn(&Node) -> bool;

/// Upstream `noop` (default filter — always `true`).
fn noop(_node: &Node) -> bool {
    true
}

/// Upstream `NodeCounterOptions` (minus `match`, see module docs).
#[derive(Default)]
pub struct NodeCounterOptions {
    pub name: Option<&'static str>,
    pub filter: Option<NodeCounterFilterCallback>,
}

pub struct NodeCounter {
    pub r#type: String,
    pub name: String,
    pub lookup: Option<String>,
    count: u32,
    properties: IndexMap<String, u32>,
    filter_fn: NodeCounterFilterCallback,
}

impl NodeCounter {
    /// # Examples (upstream)
    /// ```text
    /// new NodeCounter("FunctionDeclaration");
    /// new NodeCounter("VariableDeclaration[kind]");
    /// ```
    ///
    /// # Panics
    /// Panics when the type argument syntax is invalid (upstream throws).
    pub fn new(type_expr: &str) -> Self {
        Self::with_options(type_expr, NodeCounterOptions::default())
    }

    pub fn with_options(type_expr: &str, options: NodeCounterOptions) -> Self {
        // Upstream: /([A-Za-z]+)(\[[a-zA-Z]+\])?/g
        let alpha_end = type_expr
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(type_expr.len());
        let (r#type, rest) = type_expr.split_at(alpha_end);
        assert!(!r#type.is_empty(), "invalid type argument syntax");
        let lookup = rest
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .filter(|lookup| !lookup.is_empty() && lookup.chars().all(|c| c.is_ascii_alphabetic()))
            .map(str::to_owned);

        Self {
            r#type: r#type.to_owned(),
            name: options.name.unwrap_or(r#type).to_owned(),
            lookup,
            count: 0,
            properties: IndexMap::new(),
            filter_fn: options.filter.unwrap_or(noop),
        }
    }

    pub fn count(&self) -> u32 {
        self.count
    }

    /// Lookup properties as a map (JS object key coercion applied — e.g.
    /// booleans become `"true"` / `"false"`).
    pub fn properties(&self) -> &IndexMap<String, u32> {
        &self.properties
    }

    /// Upstream `walk`. Returns `true` when the upstream `match` callback
    /// would have been invoked for this node.
    pub fn walk(&mut self, node: &Node) -> bool {
        if !is_node(node) || node_type(node) != Some(self.r#type.as_str()) {
            return false;
        }
        if !(self.filter_fn)(node) {
            return false;
        }

        self.count += 1;
        match &self.lookup {
            None => true,
            Some(lookup) => match node.get(lookup) {
                Some(key_value) => {
                    let key = js_string(key_value);
                    *self.properties.entry(key).or_insert(0) += 1;
                    true
                }
                None => false,
            },
        }
    }
}
