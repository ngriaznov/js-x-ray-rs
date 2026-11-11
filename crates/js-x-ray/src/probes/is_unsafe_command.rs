//! Upstream: `src/probes/isUnsafeCommand.ts`
//!
//! Detect spawn/exec unsafe commands, e.g.
//! `child_process.spawn("csrutil", ["status"])`.

use serde_json::Value;

use crate::estree::{
    Node, SourceLocation, is_string_literal, is_template_literal, literal_to_string, node_type,
};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::probes::tracing::{TraceAllOptions, match_traced_call, trace_all_from_module};
use crate::source_file::{Sensitivity, SourceFile};
use crate::warnings::{GenerateWarningOptions, generate_warning};

const K_UNSAFE_COMMANDS: [&str; 4] = ["csrutil", "uname", "ping", "curl"];

const K_SPAWN_FUNCTIONS: [&str; 2] = ["child_process.spawn", "child_process.spawnSync"];
const K_TRACED_FUNCTIONS: [&str; 4] = [
    "child_process.spawn",
    "child_process.spawnSync",
    "child_process.exec",
    "child_process.execSync",
];

fn is_unsafe_command(command: &str) -> bool {
    K_UNSAFE_COMMANDS
        .iter()
        .any(|unsafe_command| command.contains(unsafe_command))
}

fn concat_array_args(command: String, node: &Node) -> String {
    let Some(arr_expr) = node
        .get("arguments")
        .and_then(Value::as_array)
        .and_then(|args| args.get(1))
    else {
        return command;
    };
    if node_type(arr_expr) != Some("ArrayExpression") {
        return command;
    }

    arr_expr
        .get("elements")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|element| is_string_literal(element))
        .fold(command, |mut acc, element| {
            let value = element
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default();
            acc.push(' ');
            acc.push_str(value);
            acc
        })
}

#[derive(Debug, Default)]
pub struct IsUnsafeCommand;

impl Probe for IsUnsafeCommand {
    fn name(&self) -> &'static str {
        "isUnsafeCommand"
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
        match_traced_call(ctx, &K_TRACED_FUNCTIONS)
    }

    fn main(&mut self, node: &Node, data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let traced_function = data.as_str().unwrap_or_default();

        let Some(command_arg) = node
            .get("arguments")
            .and_then(Value::as_array)
            .and_then(|args| args.first())
        else {
            return ProbeReturn::Continue;
        };
        if !is_string_literal(command_arg) && !is_template_literal(command_arg) {
            return ProbeReturn::Continue;
        }

        let mut command = literal_to_string(command_arg);
        let is_spawn = K_SPAWN_FUNCTIONS.contains(&traced_function);

        let matched = match ctx.source_file.sensitivity {
            Sensitivity::Aggressive => true,
            Sensitivity::Conservative => is_unsafe_command(&command),
        };
        if !matched {
            return ProbeReturn::Continue;
        }

        if is_spawn {
            command = concat_array_args(command, node);
        }

        let warning = generate_warning(
            "unsafe-command",
            GenerateWarningOptions {
                value: Some(command),
                location: SourceLocation::from_node(node),
                ..Default::default()
            },
        );
        ctx.source_file.warnings.push(warning);

        ProbeReturn::Skip
    }
}
