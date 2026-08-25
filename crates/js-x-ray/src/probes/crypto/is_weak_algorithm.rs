//! Upstream: `src/probes/crypto/isWeakAlgorithm.ts`

use serde_json::Value;

use crate::estree::{Node, SourceLocation, is_string_literal};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::source_file::SourceFile;
use crate::variable_tracer::TraceOptions;
use crate::warnings::{GenerateWarningOptions, generate_warning};

const K_WEAK_ALGORITHMS: [&str; 5] = ["md5", "sha1", "ripemd160", "md4", "md2"];
const K_TRACED_FUNCTIONS: [&str; 2] = ["crypto.createHash", "crypto.createHmac"];

#[derive(Debug, Default)]
pub struct IsWeakAlgorithm;

impl Probe for IsWeakAlgorithm {
    fn name(&self) -> &'static str {
        "isWeakCrypto"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["CallExpression"])
    }

    fn initialize(&mut self, source_file: &mut SourceFile) {
        for identifier_or_member_expr in K_TRACED_FUNCTIONS {
            source_file.tracer.trace(
                identifier_or_member_expr,
                TraceOptions {
                    follow_consecutive_assignment: true,
                    module_name: Some("crypto".to_owned()),
                    ..Default::default()
                },
            );
        }
    }

    fn validate_node(&mut self, _node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        if !ctx.source_file.tracer.imported_modules.contains("crypto") {
            return None;
        }

        K_TRACED_FUNCTIONS
            .contains(&ctx.traced_data?.identifier_or_member_expr.as_str())
            .then_some(Value::Null)
    }

    fn main(&mut self, node: &Node, _data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        if let Some(arg) = node
            .get("arguments")
            .and_then(Value::as_array)
            .and_then(|args| args.first())
            && is_string_literal(arg)
            && let Some(value) = arg.get("value").and_then(Value::as_str)
            && K_WEAK_ALGORITHMS.contains(&value)
        {
            ctx.source_file.warnings.push(generate_warning(
                "crypto.weak-algorithm",
                GenerateWarningOptions {
                    value: Some(value.to_owned()),
                    location: SourceLocation::from_node(node),
                    ..Default::default()
                },
            ));
        }

        ProbeReturn::Matched
    }
}
