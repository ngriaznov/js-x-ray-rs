//! Upstream: `src/probes/crypto/isWeakBcrypt.ts`

use serde_json::Value;

use crate::estree::{Node, SourceLocation, identifier_name, is_identifier, is_numeric_literal, is_string_literal};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::source_file::SourceFile;
use crate::variable_tracer::TraceOptions;
use crate::warnings::{GenerateWarningOptions, generate_warning};

const K_MIN_ROUNDS: f64 = 10.0;
const K_MODULE_NAME: &str = "bcryptjs";

/// Maps a traced bcrypt function name to the argument index holding the
/// rounds/cost value.
const K_TRACED_FUNCTIONS_WITH_ARG_INDEX: [(&str, usize); 4] =
    [("hash", 1), ("hashSync", 1), ("genSalt", 0), ("genSaltSync", 0)];

fn arg_index_for(function_name: &str) -> Option<usize> {
    K_TRACED_FUNCTIONS_WITH_ARG_INDEX
        .iter()
        .find(|(name, _)| *name == function_name)
        .map(|(_, index)| *index)
}

/// `Number(string)` for the digit-string cases this probe cares about:
/// trimmed-empty coerces to `0`, otherwise a failed parse is `NaN`.
fn js_number(value: &str) -> f64 {
    let trimmed = value.trim();
    if trimmed.is_empty() { 0.0 } else { trimmed.parse().unwrap_or(f64::NAN) }
}

#[derive(Debug, Default)]
pub struct IsWeakBcrypt;

impl Probe for IsWeakBcrypt {
    fn name(&self) -> &'static str {
        "isWeakBcrypt"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["CallExpression"])
    }

    fn initialize(&mut self, source_file: &mut SourceFile) {
        for (function_name, _) in K_TRACED_FUNCTIONS_WITH_ARG_INDEX {
            source_file.tracer.trace(
                &format!("{K_MODULE_NAME}.{function_name}"),
                TraceOptions {
                    follow_consecutive_assignment: true,
                    module_name: Some(K_MODULE_NAME.to_owned()),
                    ..Default::default()
                },
            );
        }
    }

    fn validate_node(&mut self, _node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        if !ctx.source_file.tracer.imported_modules.contains(K_MODULE_NAME) {
            return None;
        }

        let identifier_or_member_expr = ctx.traced_data?.identifier_or_member_expr.as_str();
        let (_, function_name) = identifier_or_member_expr.split_once('.')?;
        arg_index_for(function_name)?;

        Some(Value::String(function_name.to_owned()))
    }

    fn main(&mut self, node: &Node, data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let Some(arg_index) = data.as_str().and_then(arg_index_for) else {
            return ProbeReturn::Matched;
        };
        let Some(arg) = node.get("arguments").and_then(Value::as_array).and_then(|args| args.get(arg_index)) else {
            return ProbeReturn::Matched;
        };

        let low_work_factor = if is_numeric_literal(arg) {
            arg.get("value").and_then(Value::as_f64).is_some_and(|value| value < K_MIN_ROUNDS)
        } else if is_identifier(arg) {
            let name = identifier_name(arg).unwrap_or("");
            ctx.source_file
                .tracer
                .literal_identifiers
                .get(name)
                .map(|literal| js_number(&literal.value))
                .is_some_and(|value| !value.is_nan() && value < K_MIN_ROUNDS)
        } else {
            false
        };

        if low_work_factor {
            ctx.source_file.warnings.push(generate_warning(
                "crypto.weak-bcrypt",
                GenerateWarningOptions {
                    value: Some("low-work-factor".to_owned()),
                    location: SourceLocation::from_node(node),
                    ..Default::default()
                },
            ));
        } else if is_string_literal(arg) {
            ctx.source_file.warnings.push(generate_warning(
                "crypto.weak-bcrypt",
                GenerateWarningOptions {
                    value: Some("hardcoded-salt".to_owned()),
                    location: SourceLocation::from_node(node),
                    ..Default::default()
                },
            ));
        }

        ProbeReturn::Matched
    }
}
