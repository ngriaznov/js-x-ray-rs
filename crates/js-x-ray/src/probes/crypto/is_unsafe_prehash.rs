//! Upstream: `src/probes/crypto/isUnsafePrehash.ts`

use indexmap::IndexMap;
use serde_json::Value;

use crate::estree::{
    Node, SourceLocation, get_member_call_expression, get_param_names, identifier_name,
    is_call_expression, is_function_node, is_identifier, is_string_literal,
};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::source_file::SourceFile;
use crate::variable_tracer::{LiteralIdentifier, TraceOptions, TracerEvent};
use crate::warnings::{GenerateWarningOptions, generate_warning};

const K_MODULE_NAME: &str = "bcryptjs";
const K_TRACED_FUNCTIONS: [&str; 2] = ["bcryptjs.hash", "bcryptjs.hashSync"];

/// Digest encodings that produce ASCII-only output, avoiding the null-byte
/// truncation issue.
const K_SAFE_DIGEST_ENCODINGS: [&str; 3] = ["base64", "base64url", "hex"];

const K_DIGEST_CHAINS: [&str; 8] = [
    "crypto.createHash.update.digest",
    "crypto.createHash.update.digest.toString",
    "crypto.createHash.digest",
    "crypto.createHash.digest.toString",
    "crypto.createHmac.update.digest",
    "crypto.createHmac.update.digest.toString",
    "crypto.createHmac.digest",
    "crypto.createHmac.digest.toString",
];

/// Resolves both `x.digest(encoding)` and `x.digest().toString(encoding)`.
fn resolve_digest_encoding_arguments(hash_node: Option<&Value>) -> Option<&[Value]> {
    let hash_node = hash_node?;

    if let Some(digest_call) = get_member_call_expression(hash_node, "digest") {
        return digest_call
            .get("arguments")
            .and_then(Value::as_array)
            .map(Vec::as_slice);
    }

    let to_string_call = get_member_call_expression(hash_node, "toString")?;
    let inner_object = to_string_call.pointer("/callee/object")?;
    let inner_digest_call = get_member_call_expression(inner_object, "digest")?;
    let inner_args = inner_digest_call
        .get("arguments")
        .and_then(Value::as_array)?;

    if inner_args.is_empty() {
        to_string_call
            .get("arguments")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
    } else {
        Some(inner_args.as_slice())
    }
}

fn is_safe_encoding_arg(
    node: Option<&Value>,
    literal_identifiers: &IndexMap<String, LiteralIdentifier>,
) -> bool {
    let Some(node) = node else {
        return false;
    };

    if is_string_literal(node) {
        return node
            .get("value")
            .and_then(Value::as_str)
            .is_some_and(|value| K_SAFE_DIGEST_ENCODINGS.contains(&value));
    }
    if is_identifier(node) {
        let name = identifier_name(node).unwrap_or("");
        return literal_identifiers
            .get(name)
            .is_some_and(|literal| K_SAFE_DIGEST_ENCODINGS.contains(&literal.value.as_str()));
    }

    false
}

fn has_unsafe_digest_encoding(
    hash_node: Option<&Value>,
    literal_identifiers: &IndexMap<String, LiteralIdentifier>,
) -> bool {
    let Some(encoding_args) = resolve_digest_encoding_arguments(hash_node) else {
        return false;
    };

    !is_safe_encoding_arg(encoding_args.first(), literal_identifiers)
}

fn is_unsafe_hash_argument(
    hash_argument: Option<&Value>,
    ambiguous: &indexmap::IndexSet<String>,
    unsafe_digest: &indexmap::IndexSet<String>,
    literal_identifiers: &IndexMap<String, LiteralIdentifier>,
) -> bool {
    if let Some(arg) = hash_argument
        && is_identifier(arg)
    {
        let name = identifier_name(arg).unwrap_or("");
        return !ambiguous.contains(name) && unsafe_digest.contains(name);
    }

    if let Some(arg) = hash_argument
        && is_call_expression(arg)
        && let Some(callee) = arg.get("callee")
        && is_identifier(callee)
        && let Some(callee_name) = identifier_name(callee)
        && !ambiguous.contains(callee_name)
        && unsafe_digest.contains(callee_name)
    {
        let encoding_arg = arg
            .get("arguments")
            .and_then(Value::as_array)
            .and_then(|args| args.first());

        return !is_safe_encoding_arg(encoding_arg, literal_identifiers);
    }

    has_unsafe_digest_encoding(hash_argument, literal_identifiers)
}

/// Upstream `setEntryPoint`/named `main` handlers (`default` |
/// `markAmbiguousParams`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum EntryPoint {
    #[default]
    Default,
    MarkAmbiguousParams,
}

#[derive(Debug, Default)]
pub struct IsUnsafePrehash {
    unsafe_digest_variables: indexmap::IndexSet<String>,
    ambiguous_variable_names: indexmap::IndexSet<String>,
    entry_point: EntryPoint,
}

impl IsUnsafePrehash {
    fn mark_ambiguous_params(&mut self, data: &Value) -> ProbeReturn {
        for name in data
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            self.ambiguous_variable_names.insert(name.to_owned());
        }

        ProbeReturn::Matched
    }

    fn bcrypt_hash_call(&mut self, bcrypt_node: &Node, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let hash_argument = bcrypt_node
            .get("arguments")
            .and_then(Value::as_array)
            .and_then(|args| args.first());

        let is_unsafe = is_unsafe_hash_argument(
            hash_argument,
            &self.ambiguous_variable_names,
            &self.unsafe_digest_variables,
            &ctx.source_file.tracer.literal_identifiers,
        );

        if is_unsafe {
            ctx.source_file.warnings.push(generate_warning(
                "crypto.unsafe-prehash",
                GenerateWarningOptions {
                    value: None,
                    location: SourceLocation::from_node(bcrypt_node),
                    ..Default::default()
                },
            ));
        }

        ProbeReturn::Matched
    }
}

impl Probe for IsUnsafePrehash {
    fn name(&self) -> &'static str {
        "isUnsafePrehash"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&[
            "CallExpression",
            "FunctionDeclaration",
            "FunctionExpression",
            "ArrowFunctionExpression",
        ])
    }

    fn initialize(&mut self, source_file: &mut SourceFile) {
        for identifier_or_member_expr in K_TRACED_FUNCTIONS {
            source_file.tracer.trace(
                identifier_or_member_expr,
                TraceOptions {
                    follow_consecutive_assignment: true,
                    module_name: Some(K_MODULE_NAME.to_owned()),
                    ..Default::default()
                },
            );
        }

        for chain in K_DIGEST_CHAINS {
            source_file.tracer.trace(
                chain,
                TraceOptions {
                    follow_return_value_assignement: true,
                    follow_consecutive_assignment: true,
                    module_name: Some("crypto".to_owned()),
                    ..Default::default()
                },
            );
        }
    }

    fn on_tracer_event(&mut self, event: &TracerEvent, source_file: &mut SourceFile) {
        let TracerEvent::ReturnValue {
            identifier_or_member_expr,
            id,
            arguments,
            ..
        } = event
        else {
            return;
        };
        if !K_DIGEST_CHAINS.contains(&identifier_or_member_expr.as_str()) {
            return;
        }

        let encoding_arg = arguments.first();
        if !is_safe_encoding_arg(encoding_arg, &source_file.tracer.literal_identifiers) {
            self.unsafe_digest_variables.insert(id.clone());
        }
    }

    fn validate_node(&mut self, node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        if !ctx
            .source_file
            .tracer
            .imported_modules
            .contains(K_MODULE_NAME)
            || !ctx.source_file.tracer.imported_modules.contains("crypto")
        {
            return None;
        }

        if is_function_node(node) {
            let param_names = node
                .get("params")
                .and_then(Value::as_array)
                .map(|params| get_param_names(params))
                .unwrap_or_default();

            if param_names
                .iter()
                .any(|name| self.unsafe_digest_variables.contains(name))
            {
                self.entry_point = EntryPoint::MarkAmbiguousParams;
                return Some(Value::Array(
                    param_names.into_iter().map(Value::String).collect(),
                ));
            }

            return None;
        }

        self.entry_point = EntryPoint::Default;
        ctx.traced_data
            .is_some_and(|data| {
                K_TRACED_FUNCTIONS.contains(&data.identifier_or_member_expr.as_str())
            })
            .then_some(Value::Null)
    }

    fn main(&mut self, node: &Node, data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        match self.entry_point {
            EntryPoint::MarkAmbiguousParams => self.mark_ambiguous_params(data),
            EntryPoint::Default => self.bcrypt_hash_call(node, ctx),
        }
    }
}
