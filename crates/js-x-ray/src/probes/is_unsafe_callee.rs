//! Upstream: `src/probes/isUnsafeCallee.ts`
//!
//! `eval("this")` / `Function("return this")()`.

use serde_json::Value;

use crate::estree::{
    GetCallExpressionIdentifierOptions, Node, SourceLocation, get_call_expression_identifier,
    is_call_expression,
};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::warnings::{GenerateWarningOptions, generate_warning};

fn is_eval_callee(node: &Node) -> bool {
    let identifier =
        get_call_expression_identifier(node, &GetCallExpressionIdentifierOptions::default());

    identifier.as_deref() == Some("eval")
}

fn is_function_callee(node: &Node, identifier: Option<&str>) -> bool {
    identifier == Some("Function") && node.get("callee").is_some_and(is_call_expression)
}

/// Upstream `isUnsafeCallee`. Exposed for `ast_analyser`/other probes that
/// may want the same classification.
#[must_use]
pub fn is_unsafe_callee(node: &Node, ctx: &ProbeCtx<'_>) -> Option<&'static str> {
    if !is_call_expression(node) {
        return None;
    }

    if is_eval_callee(node) {
        return Some("eval");
    }

    if is_function_callee(node, ctx.traced_identifier) {
        return Some("Function");
    }

    None
}

#[derive(Debug, Default)]
pub struct IsUnsafeCallee;

impl Probe for IsUnsafeCallee {
    fn name(&self) -> &'static str {
        "isUnsafeCallee"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["CallExpression"])
    }

    fn validate_node(&mut self, node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        is_unsafe_callee(node, ctx).map(|callee_name| Value::String(callee_name.to_owned()))
    }

    fn main(&mut self, node: &Node, data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let Some(callee_name) = data.as_str() else {
            return ProbeReturn::Skip;
        };

        if callee_name == "Function"
            && node
                .pointer("/callee/arguments/0/value")
                .and_then(Value::as_str)
                == Some("return this")
        {
            return ProbeReturn::Skip;
        }

        let warning = generate_warning(
            "unsafe-stmt",
            GenerateWarningOptions {
                value: Some(callee_name.to_owned()),
                location: SourceLocation::from_node(node),
                ..Default::default()
            },
        );
        ctx.source_file.warnings.push(warning);

        ProbeReturn::Skip
    }
}
