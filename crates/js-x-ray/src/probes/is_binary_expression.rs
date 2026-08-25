//! Upstream: `src/probes/isBinaryExpression.ts`
//!
//! Search for suspicious BinaryExpression (obfuscator.io style), e.g.
//! `0x1*-0x12df+-0x1fb9*-0x1+0x2*-0x66d`.

use serde_json::Value;

use crate::estree::{Node, is_type, node_type};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};

/// Upstream `walkBinaryExpression`: returns `(deepness, hasUnaryExpression)`.
fn walk_binary_expression(expr: &Node, level: u32) -> (u32, bool) {
    let left = expr.get("left");
    let right = expr.get("right");
    let lt = left.and_then(node_type);
    let rt = right.and_then(node_type);

    let mut has_unary_expression = lt == Some("UnaryExpression") || rt == Some("UnaryExpression");
    let mut current_level = if lt == Some("BinaryExpression") || rt == Some("BinaryExpression") {
        level + 1
    } else {
        level
    };

    for curr_expr in [left, right].into_iter().flatten() {
        if is_type(curr_expr, "BinaryExpression") {
            let (deep_level, deep_has_unary_expression) =
                walk_binary_expression(curr_expr, current_level);
            if deep_level > current_level {
                current_level = deep_level;
            }
            if !has_unary_expression && deep_has_unary_expression {
                has_unary_expression = true;
            }
        }
    }

    (current_level, has_unary_expression)
}

#[derive(Debug, Default)]
pub struct IsBinaryExpression;

impl Probe for IsBinaryExpression {
    fn name(&self) -> &'static str {
        "isBinaryExpression"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["BinaryExpression"])
    }

    fn validate_node(&mut self, _node: &Node, _ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        Some(Value::Null)
    }

    fn main(&mut self, node: &Node, _data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let (binary_expr_deepness, has_unary_expression) = walk_binary_expression(node, 1);
        if binary_expr_deepness >= 3 && has_unary_expression {
            ctx.source_file.deobfuscator.deep_binary_expression += 1;
        }

        ProbeReturn::Matched
    }
}
