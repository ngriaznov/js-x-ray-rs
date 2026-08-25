//! Upstream: `src/probes/isLiteralRegex.ts`

use serde_json::Value;

use crate::estree::{Node, SourceLocation};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::utils::safe_regex::is_safe_regex;
use crate::warnings::{GenerateWarningOptions, generate_warning};

#[derive(Debug, Default)]
pub struct IsLiteralRegex;

impl Probe for IsLiteralRegex {
    fn name(&self) -> &'static str {
        "is-literal-regex"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["Literal"])
    }

    fn validate_node(&mut self, node: &Node, _ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        (node.get("type").and_then(Value::as_str) == Some("Literal") && node.get("regex").is_some())
            .then_some(Value::Null)
    }

    fn main(&mut self, node: &Node, _data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let pattern = node
            .pointer("/regex/pattern")
            .and_then(Value::as_str)
            .unwrap_or("");

        if !is_safe_regex(pattern) {
            ctx.source_file.warnings.push(generate_warning(
                "unsafe-regex",
                GenerateWarningOptions {
                    value: Some(pattern.to_owned()),
                    location: SourceLocation::from_node(node),
                    ..Default::default()
                },
            ));
        }

        ProbeReturn::Matched
    }
}
