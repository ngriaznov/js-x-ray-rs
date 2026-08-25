//! Upstream: `src/Inlined.ts`, `src/InlinedCallExpression.ts`,
//! `src/InlinedNew.ts`, and `src/probes/isRequire/InlinedRequire.ts`.
//!
//! PORT-TODO(stub): faithful port pending. `split` inspects a node and, when
//! it matches an inlined pattern, returns a virtual variable declaration plus
//! an optional rebuilt expression.

use crate::estree::Node;

#[derive(Debug, Clone)]
pub struct SplitResult {
    pub virtual_declaration: Node,
    pub rebuild_expression: Option<Node>,
}

pub struct InlinedCallExpression;

impl InlinedCallExpression {
    pub fn split(node: &Node) -> Option<SplitResult> {
        // PORT-TODO(stub)
        let _ = node;
        None
    }
}

pub struct InlinedNew;

impl InlinedNew {
    pub fn split(node: &Node) -> Option<SplitResult> {
        // PORT-TODO(stub)
        let _ = node;
        None
    }
}

pub struct InlinedRequire;

impl InlinedRequire {
    pub fn split(node: &Node) -> Option<SplitResult> {
        // PORT-TODO(stub)
        let _ = node;
        None
    }
}
