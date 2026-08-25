//! Upstream: `src/probes/isArrayExpression.ts`
//!
//! Search for ArrayExpression AST Node (JS Arrays), e.g. `["foo", "bar", 1]`.

use serde_json::Value;

use crate::estree::{Node, is_literal};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};

#[derive(Debug, Default)]
pub struct IsArrayExpression;

impl Probe for IsArrayExpression {
    fn name(&self) -> &'static str {
        "isArrayExpression"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["ArrayExpression"])
    }

    fn validate_node(&mut self, _node: &Node, _ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        Some(Value::Null)
    }

    fn main(&mut self, node: &Node, _data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let elements = node.get("elements").and_then(Value::as_array);
        for literal_node in elements.into_iter().flatten().filter(|el| is_literal(el)) {
            ctx.source_file.analyze_literal(literal_node, true);
        }

        ProbeReturn::Matched
    }
}
