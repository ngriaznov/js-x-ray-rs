//! Upstream: `src/probes/isRegexObject.ts`

use serde_json::Value;

use crate::estree::{Node, SourceLocation, node_type};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::utils::safe_regex::is_safe_regex;
use crate::warnings::{GenerateWarningOptions, generate_warning};

fn is_regex_constructor(node: &Node) -> bool {
    node_type(node) == Some("NewExpression")
        && node
            .get("callee")
            .is_some_and(|callee| node_type(callee) == Some("Identifier"))
        && node.pointer("/callee/name").and_then(Value::as_str) == Some("RegExp")
}

#[derive(Debug, Default)]
pub struct IsRegexObject;

impl Probe for IsRegexObject {
    fn name(&self) -> &'static str {
        "is-regex-object"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["NewExpression"])
    }

    fn validate_node(&mut self, node: &Node, _ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        let has_arguments = node
            .get("arguments")
            .and_then(Value::as_array)
            .is_some_and(|args| !args.is_empty());

        (is_regex_constructor(node) && has_arguments).then_some(Value::Null)
    }

    fn main(&mut self, node: &Node, _data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let Some(arg) = node.pointer("/arguments/0") else {
            return ProbeReturn::Matched;
        };

        let pattern = if node_type(arg) == Some("Literal") && arg.get("regex").is_some() {
            arg.pointer("/regex/pattern")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned()
        } else {
            arg.get("value").and_then(Value::as_str).unwrap_or("").to_owned()
        };

        if !is_safe_regex(&pattern) {
            ctx.source_file.warnings.push(generate_warning(
                "unsafe-regex",
                GenerateWarningOptions {
                    value: Some(pattern),
                    location: SourceLocation::from_node(node),
                    ..Default::default()
                },
            ));
        }

        ProbeReturn::Matched
    }
}
