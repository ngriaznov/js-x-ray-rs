//! Upstream: `src/probes/crypto/isWeakScrypt.ts`

use serde_json::Value;

use crate::estree::{
    Node, SourceLocation, identifier_name, is_identifier, is_numeric_literal, is_string_literal,
    is_type,
};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::source_file::SourceFile;
use crate::variable_tracer::TraceOptions;
use crate::warnings::{GenerateWarningOptions, generate_warning};

/// OWASP recommended minimum scrypt parameter combinations: `(minCost,
/// minParallelization)`, sorted by cost descending. All recommendations
/// assume `blockSize >= 8`.
/// <https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html#scrypt>
const K_OWASP_MIN_PARAMS: [(f64, f64); 5] = [
    (131072.0, 1.0),
    (65536.0, 2.0),
    (32768.0, 3.0),
    (16384.0, 5.0),
    (8192.0, 10.0),
];

const K_MIN_BLOCK_SIZE: f64 = 8.0;

// Node.js crypto.scrypt defaults.
const K_DEFAULT_COST: f64 = 16384.0;
const K_DEFAULT_BLOCK_SIZE: f64 = 8.0;
const K_DEFAULT_PARALLELIZATION: f64 = 1.0;

const K_TRACED_FUNCTIONS: [&str; 1] = ["crypto.scrypt"];

fn extract_numeric_param(properties: &[&Value], names: &[&str]) -> Option<f64> {
    properties.iter().find_map(|prop| {
        let key = prop.get("key")?;
        if !is_identifier(key) || !names.contains(&identifier_name(key)?) {
            return None;
        }
        let value = prop.get("value")?;
        is_numeric_literal(value)
            .then(|| value.get("value")?.as_f64())
            .flatten()
    })
}

fn is_weak_scrypt_params(cost: f64, block_size: f64, parallelization: f64) -> bool {
    if block_size < K_MIN_BLOCK_SIZE {
        return true;
    }

    for (min_cost, min_parallelization) in K_OWASP_MIN_PARAMS {
        if cost >= min_cost {
            return parallelization < min_parallelization;
        }
    }

    // cost is below the lowest OWASP recommendation (2^13 = 8192).
    true
}

#[derive(Debug, Default)]
pub struct IsWeakScrypt;

impl Probe for IsWeakScrypt {
    fn name(&self) -> &'static str {
        "isWeakScrypt"
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
        let arguments = node.get("arguments").and_then(Value::as_array);
        let salt = arguments.and_then(|args| args.get(1));
        let options = arguments.and_then(|args| args.get(3));

        if let Some(options) = options
            && is_type(options, "ObjectExpression")
        {
            let properties: Vec<&Value> = options
                .get("properties")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|prop| is_type(prop, "Property"))
                .collect();

            let cost_value = extract_numeric_param(&properties, &["cost", "N"]);
            let block_size_value = extract_numeric_param(&properties, &["blockSize", "r"]);
            let parallelization_value =
                extract_numeric_param(&properties, &["parallelization", "p"]);

            if (cost_value.is_some()
                || block_size_value.is_some()
                || parallelization_value.is_some())
                && is_weak_scrypt_params(
                    cost_value.unwrap_or(K_DEFAULT_COST),
                    block_size_value.unwrap_or(K_DEFAULT_BLOCK_SIZE),
                    parallelization_value.unwrap_or(K_DEFAULT_PARALLELIZATION),
                )
            {
                ctx.source_file.warnings.push(generate_warning(
                    "crypto.weak-scrypt",
                    GenerateWarningOptions {
                        value: Some("low-cost".to_owned()),
                        location: SourceLocation::from_node(node),
                        ..Default::default()
                    },
                ));
            }
        }

        if let Some(salt) = salt
            && is_string_literal(salt)
        {
            let value = salt.get("value").and_then(Value::as_str).unwrap_or("");
            let kind = if value.encode_utf16().count() < 16 {
                "short-salt"
            } else {
                "hardcoded-salt"
            };

            ctx.source_file.warnings.push(generate_warning(
                "crypto.weak-scrypt",
                GenerateWarningOptions {
                    value: Some(kind.to_owned()),
                    location: SourceLocation::from_node(node),
                    ..Default::default()
                },
            ));
        }

        ProbeReturn::Matched
    }
}
