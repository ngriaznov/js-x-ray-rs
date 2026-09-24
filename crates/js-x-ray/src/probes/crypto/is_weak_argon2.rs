//! Upstream: `src/probes/crypto/isWeakArgon2.ts`

use serde_json::Value;

use crate::estree::{Node, SourceLocation, find_property_match, is_node, is_type};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::source_file::SourceFile;
use crate::variable_tracer::TraceOptions;
use crate::warnings::{GenerateWarningOptions, generate_warning};

use super::{resolve_numeric_value, resolve_string_value};

/// OWASP recommended Argon2 parameter combinations.
/// Each entry is `(minMemory (KiB), passes)` — every row provides an equal
/// level of defense, trading CPU against RAM. Sorted by ascending passes.
///
/// <https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html#argon2id>
const K_OWASP_ROWS: [(f64, f64); 5] = [
    (47104.0, 1.0),
    (19456.0, 2.0),
    (12288.0, 3.0),
    (9216.0, 4.0),
    (7168.0, 5.0),
];

/// The two lowest-pass rows are flagged "(Do not use with Argon2i)" by OWASP.
/// Argon2i is data-independent and therefore weaker against time-memory
/// trade-off attacks, which additional passes mitigate.
///
/// <https://www.rfc-editor.org/rfc/rfc9106.html#section-7.3>
const K_ARGON2I_MIN_PASSES: f64 = 3.0;

// https://www.rfc-editor.org/rfc/rfc9106.html#section-3.1
const K_MIN_NONCE_LENGTH: usize = 16;

const K_TRACED_FUNCTIONS: [&str; 2] = ["crypto.argon2", "crypto.argon2Sync"];

/// Identify which parameter drags the call below the OWASP recommendations,
/// or `None` when the combination is acceptable.
fn find_weak_param(algorithm: Option<&str>, memory: f64, passes: f64) -> Option<&'static str> {
    let rows = K_OWASP_ROWS.iter().filter(|&&(_, min_passes)| {
        algorithm != Some("argon2i") || min_passes >= K_ARGON2I_MIN_PASSES
    });

    // Rows are sorted by ascending passes, and the memory requirement drops as
    // passes grow. The last usable row is therefore the cheapest one available.
    let row = rows.rev().find(|&&(_, min_passes)| passes >= min_passes);
    match row {
        None => Some("passes"),
        Some(&(min_memory, _)) => (memory < min_memory).then_some("memory"),
    }
}

#[derive(Debug, Default)]
pub struct IsWeakArgon2;

impl Probe for IsWeakArgon2 {
    fn name(&self) -> &'static str {
        "isWeakArgon2"
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
        let literal_identifiers = &ctx.source_file.tracer.literal_identifiers;
        let arguments = node.get("arguments").and_then(Value::as_array);
        let mut reasons: Vec<String> = Vec::new();

        let algorithm =
            resolve_string_value(arguments.and_then(|args| args.first()), literal_identifiers);

        if let Some(algorithm @ "argon2d") = algorithm {
            reasons.push(format!("weak-algorithm: {algorithm}"));
        }

        let options = arguments.and_then(|args| args.get(1));

        if let Some(options) = options
            && is_type(options, "ObjectExpression")
        {
            let properties: &[Value] = options
                .get("properties")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]);

            let memory = resolve_numeric_value(
                find_property_match(properties, &["memory"], is_node),
                literal_identifiers,
            );
            let passes = resolve_numeric_value(
                find_property_match(properties, &["passes"], is_node),
                literal_identifiers,
            );
            let nonce = resolve_string_value(
                find_property_match(properties, &["nonce"], is_node),
                literal_identifiers,
            );

            if let (Some(memory), Some(passes)) = (memory, passes)
                && let Some(weak_param) = find_weak_param(algorithm, memory, passes)
            {
                reasons.push(format!("low-params: {weak_param}"));
            }

            if let Some(nonce) = nonce {
                reasons.push(
                    if nonce.len() < K_MIN_NONCE_LENGTH {
                        "short-nonce"
                    } else {
                        "hardcoded-nonce"
                    }
                    .to_owned(),
                );
            }
        }

        if !reasons.is_empty() {
            ctx.source_file.warnings.push(generate_warning(
                "crypto.weak-argon2",
                GenerateWarningOptions {
                    value: Some(reasons.join(", ")),
                    location: SourceLocation::from_node(node),
                    ..Default::default()
                },
            ));
        }

        ProbeReturn::Matched
    }
}
