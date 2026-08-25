//! Upstream: `src/probes/sql-injection.ts`

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use crate::estree::{
    Node, SourceLocation, identifier_name, is_call_expression, node_type, to_literal,
};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::warnings::{GenerateWarningOptions, generate_warning};

static SQL_INJECTION_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(select\s+.*\s+from|insert\s+into|delete\s+from|update\s+.*\s+set)")
        .expect("valid regex")
});

#[derive(Debug, Default)]
pub struct SqlInjection;

impl Probe for SqlInjection {
    fn name(&self) -> &'static str {
        "sql-injection"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["CallExpression"])
    }

    fn validate_node(&mut self, node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        if !is_call_expression(node) {
            return None;
        }

        let arguments = node.get("arguments")?.as_array()?;
        for arg_node in arguments {
            match node_type(arg_node) {
                Some("Identifier") => {
                    let Some(name) = identifier_name(arg_node) else {
                        continue;
                    };
                    let Some(literal_identifier) =
                        ctx.source_file.tracer.literal_identifiers.get(name)
                    else {
                        continue;
                    };
                    if literal_identifier.r#type != "TemplateLiteral"
                        || !SQL_INJECTION_REGEX.is_match(&literal_identifier.value)
                    {
                        continue;
                    }

                    return Some(Value::String(literal_identifier.value.clone()));
                }
                Some("TemplateLiteral") => {
                    let expressions_len = arg_node
                        .get("expressions")
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len);
                    if expressions_len == 0 {
                        continue;
                    }

                    let literal = to_literal(arg_node);
                    if !SQL_INJECTION_REGEX.is_match(&literal) {
                        continue;
                    }

                    return Some(Value::String(literal));
                }
                _ => {}
            }
        }

        None
    }

    fn main(&mut self, node: &Node, data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        ctx.source_file.warnings.push(generate_warning(
            "sql-injection",
            GenerateWarningOptions {
                value: data.as_str().map(str::to_owned),
                location: SourceLocation::from_node(node),
                ..Default::default()
            },
        ));

        ProbeReturn::Matched
    }
}
