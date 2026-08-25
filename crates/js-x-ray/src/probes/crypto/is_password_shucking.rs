//! Upstream: `src/probes/crypto/isPasswordShucking.ts`

use serde_json::Value;

use crate::estree::{
    Node, SourceLocation, get_member_call_expression, get_param_names, identifier_name,
    is_call_expression, is_function_node, is_identifier, is_member_expression,
};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::source_file::SourceFile;
use crate::variable_tracer::{TraceOptions, TracerEvent};
use crate::warnings::{GenerateWarningOptions, generate_warning};

const K_MODULE_NAME: &str = "bcryptjs";
const K_TRACED_FUNCTIONS: [&str; 2] = ["bcryptjs.hash", "bcryptjs.hashSync"];

/// `createHmac` is intentionally excluded: HMAC with a pepper is the
/// OWASP-safe pattern.
const K_HASH_DIGEST_CHAINS: [&str; 4] = [
    "crypto.createHash.update.digest",
    "crypto.createHash.update.digest.toString",
    "crypto.createHash.digest",
    "crypto.createHash.digest.toString",
];

fn is_create_hash_chain(node: Option<&Value>) -> bool {
    let mut current = node;
    while let Some(node) = current {
        if !is_call_expression(node) {
            break;
        }
        let Some(callee) = node.get("callee") else {
            break;
        };
        if !is_member_expression(callee) {
            break;
        }
        if callee
            .get("property")
            .is_some_and(|property| is_identifier(property) && identifier_name(property) == Some("createHash"))
        {
            return true;
        }
        current = callee.get("object");
    }

    false
}

fn has_digest_chain(hash_node: Option<&Value>) -> bool {
    let Some(hash_node) = hash_node else {
        return false;
    };
    if get_member_call_expression(hash_node, "digest").is_some() {
        return true;
    }

    let Some(to_string_call) = get_member_call_expression(hash_node, "toString") else {
        return false;
    };

    to_string_call
        .pointer("/callee/object")
        .is_some_and(|object| get_member_call_expression(object, "digest").is_some())
}

fn is_shucking_prehash(hash_node: Option<&Value>) -> bool {
    has_digest_chain(hash_node) && is_create_hash_chain(hash_node)
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
pub struct IsPasswordShucking {
    shucking_variables: indexmap::IndexSet<String>,
    ambiguous_variable_names: indexmap::IndexSet<String>,
    entry_point: EntryPoint,
}

impl IsPasswordShucking {
    fn mark_ambiguous_params(&mut self, data: &Value) -> ProbeReturn {
        for name in data.as_array().into_iter().flatten().filter_map(Value::as_str) {
            self.ambiguous_variable_names.insert(name.to_owned());
        }

        ProbeReturn::Matched
    }

    fn bcrypt_hash_call(&mut self, bcrypt_node: &Node, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let hash_argument = bcrypt_node.get("arguments").and_then(Value::as_array).and_then(|args| args.first());

        let is_variable_shucking = hash_argument.is_some_and(|arg| {
            is_identifier(arg)
                && identifier_name(arg).is_some_and(|name| {
                    !self.ambiguous_variable_names.contains(name) && self.shucking_variables.contains(name)
                })
        });

        if is_variable_shucking || is_shucking_prehash(hash_argument) {
            ctx.source_file.warnings.push(generate_warning(
                "crypto.password-shucking",
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

impl Probe for IsPasswordShucking {
    fn name(&self) -> &'static str {
        "isPasswordShucking"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["CallExpression", "FunctionDeclaration", "FunctionExpression", "ArrowFunctionExpression"])
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

        for chain in K_HASH_DIGEST_CHAINS {
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

    fn on_tracer_event(&mut self, event: &TracerEvent, _source_file: &mut SourceFile) {
        let TracerEvent::ReturnValue { identifier_or_member_expr, id, .. } = event else {
            return;
        };

        if K_HASH_DIGEST_CHAINS.contains(&identifier_or_member_expr.as_str()) {
            self.shucking_variables.insert(id.clone());
        }
    }

    fn validate_node(&mut self, node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        if !ctx.source_file.tracer.imported_modules.contains(K_MODULE_NAME)
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

            if param_names.iter().any(|name| self.shucking_variables.contains(name)) {
                self.entry_point = EntryPoint::MarkAmbiguousParams;
                return Some(Value::Array(param_names.into_iter().map(Value::String).collect()));
            }

            return None;
        }

        self.entry_point = EntryPoint::Default;
        ctx.traced_data
            .is_some_and(|data| K_TRACED_FUNCTIONS.contains(&data.identifier_or_member_expr.as_str()))
            .then_some(Value::Null)
    }

    fn main(&mut self, node: &Node, data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        match self.entry_point {
            EntryPoint::MarkAmbiguousParams => self.mark_ambiguous_params(data),
            EntryPoint::Default => self.bcrypt_hash_call(node, ctx),
        }
    }
}
