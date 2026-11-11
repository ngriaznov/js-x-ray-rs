//! Upstream: `src/probes/crypto/isWeakPbkdf2.ts`

use serde_json::Value;

use crate::estree::{Node, SourceLocation, is_string_literal};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::probes::tracing::{
    TraceAllOptions, has_imported_modules, match_traced_call, trace_all_from_module,
};
use crate::source_file::SourceFile;
use crate::warnings::{GenerateWarningOptions, generate_warning};

use super::{classify_hardcoded_secret, resolve_numeric_value, resolve_string_value};

/// OWASP recommended minimum PBKDF2 iteration counts.
///
/// <https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html#pbkdf2>
const K_MIN_ITERATIONS_SHA1: f64 = 1_300_000.0;
const K_MIN_ITERATIONS_SHA256: f64 = 600_000.0;
const K_MIN_ITERATIONS_SHA512: f64 = 210_000.0;

const K_MIN_SALT_LENGTH: usize = 16;

const K_TRACED_FUNCTIONS: [&str; 2] = ["crypto.pbkdf2", "crypto.pbkdf2Sync"];

/// Return the OWASP minimum iteration count for the given digest.
/// Digest names are matched case-insensitively (Node/OpenSSL accept
/// "SHA256", "Sha256", ... at runtime).
/// When the digest cannot be resolved statically, fall back to the lowest
/// recommendation so only unambiguously weak counts are reported.
fn min_iterations_for(digest: Option<&str>) -> f64 {
    match digest.map(str::to_lowercase).as_deref() {
        Some("sha1") => K_MIN_ITERATIONS_SHA1,
        Some("sha256") => K_MIN_ITERATIONS_SHA256,
        _ => K_MIN_ITERATIONS_SHA512,
    }
}

#[derive(Debug, Default)]
pub struct IsWeakPbkdf2;

impl Probe for IsWeakPbkdf2 {
    fn name(&self) -> &'static str {
        "isWeakPbkdf2"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["CallExpression"])
    }

    fn initialize(&mut self, source_file: &mut SourceFile) {
        trace_all_from_module(
            &mut source_file.tracer,
            K_TRACED_FUNCTIONS,
            TraceAllOptions::default(),
        );
    }

    fn validate_node(&mut self, _node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        if !has_imported_modules(ctx, &["crypto"]) {
            return None;
        }

        match_traced_call(ctx, &K_TRACED_FUNCTIONS)
    }

    fn main(&mut self, node: &Node, _data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let literal_identifiers = &ctx.source_file.tracer.literal_identifiers;
        let arguments = node.get("arguments").and_then(Value::as_array);
        let mut reasons: Vec<String> = Vec::new();

        // crypto.pbkdf2(password, salt, iterations, keylen, digest, callback)
        let salt = arguments.and_then(|args| args.get(1));
        let iterations =
            resolve_numeric_value(arguments.and_then(|args| args.get(2)), literal_identifiers);
        let digest =
            resolve_string_value(arguments.and_then(|args| args.get(4)), literal_identifiers);

        if let Some(iterations) = iterations
            && iterations < min_iterations_for(digest)
        {
            reasons.push("low-iterations".to_owned());
        }

        if let Some(salt) = salt
            && is_string_literal(salt)
            && let Some(value) = salt.get("value").and_then(Value::as_str)
        {
            reasons.push(classify_hardcoded_secret(value, "salt", K_MIN_SALT_LENGTH));
        }

        if !reasons.is_empty() {
            ctx.source_file.warnings.push(generate_warning(
                "crypto.weak-pbkdf2",
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
