//! Upstream: `src/probes/isPrototypePollution.ts`

use serde_json::Value;

use crate::estree::{
    Node, SourceLocation, get_member_expression_identifier, is_member_expression, is_type,
    noop_lookup,
};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::warnings::{GenerateWarningOptions, generate_warning};

#[derive(Debug, Default)]
pub struct IsPrototypePollution;

impl Probe for IsPrototypePollution {
    fn name(&self) -> &'static str {
        "isPrototypePollution"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["Literal", "MemberExpression"])
    }

    fn validate_node(&mut self, node: &Node, _ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        if is_type(node, "Literal")
            && node.get("value").and_then(Value::as_str) == Some("__proto__")
        {
            return Some(Value::String("literal".to_owned()));
        }

        if is_member_expression(node) {
            let parts = get_member_expression_identifier(node, &noop_lookup);
            if parts.last().map(String::as_str) == Some("__proto__") {
                return Some(Value::String(parts.join(".")));
            }
        }

        None
    }

    fn main(&mut self, node: &Node, data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let data = data.as_str().unwrap_or_default();
        let value = if data == "literal" {
            "__proto__".to_owned()
        } else {
            data.to_owned()
        };

        ctx.source_file.warnings.push(generate_warning(
            "prototype-pollution",
            GenerateWarningOptions {
                value: Some(value),
                location: SourceLocation::from_node(node),
                ..Default::default()
            },
        ));

        if data == "literal" {
            ProbeReturn::Matched
        } else {
            ProbeReturn::Skip
        }
    }
}
