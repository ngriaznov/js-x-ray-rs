//! Upstream: `src/probes/crypto/isWeakBcrypt.ts`

use serde_json::Value;

use crate::estree::{Node, SourceLocation, is_string_literal};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::probes::crypto::resolve_numeric_value;
use crate::probes::tracing::{
    TraceAllOptions, has_imported_modules, match_traced_call, trace_all_from_module,
};
use crate::source_file::SourceFile;
use crate::warnings::{GenerateWarningOptions, generate_warning};

const K_MIN_ROUNDS: f64 = 10.0;
const K_MODULE_NAME: &str = "bcryptjs";

/// Maps a traced bcrypt function to the index of its work factor argument.
const K_WORK_FACTOR_ARG_INDEX: [(&str, usize); 4] = [
    ("bcryptjs.hash", 1),
    ("bcryptjs.hashSync", 1),
    ("bcryptjs.genSalt", 0),
    ("bcryptjs.genSaltSync", 0),
];

const K_TRACED_FUNCTIONS: [&str; K_WORK_FACTOR_ARG_INDEX.len()] = {
    let mut names = [""; K_WORK_FACTOR_ARG_INDEX.len()];
    let mut index = 0;
    while index < names.len() {
        names[index] = K_WORK_FACTOR_ARG_INDEX[index].0;
        index += 1;
    }
    names
};

fn arg_index_for(name: &str) -> Option<usize> {
    K_WORK_FACTOR_ARG_INDEX
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, index)| *index)
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
        trace_all_from_module(
            &mut source_file.tracer,
            K_TRACED_FUNCTIONS,
            TraceAllOptions::default(),
        );
    }

    fn validate_node(&mut self, _node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        if !has_imported_modules(ctx, &[K_MODULE_NAME]) {
            return None;
        }

        match_traced_call(ctx, &K_TRACED_FUNCTIONS)
    }

    fn main(&mut self, node: &Node, data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let Some(arg_index) = data.as_str().and_then(arg_index_for) else {
            return ProbeReturn::Matched;
        };
        let Some(arg) = node
            .get("arguments")
            .and_then(Value::as_array)
            .and_then(|args| args.get(arg_index))
        else {
            return ProbeReturn::Matched;
        };

        let num_value =
            resolve_numeric_value(Some(arg), &ctx.source_file.tracer.literal_identifiers);

        if let Some(value) = num_value {
            if value < K_MIN_ROUNDS {
                ctx.source_file.warnings.push(generate_warning(
                    "crypto.weak-bcrypt",
                    GenerateWarningOptions {
                        value: Some("low-work-factor".to_owned()),
                        location: SourceLocation::from_node(node),
                        ..Default::default()
                    },
                ));
            }
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
