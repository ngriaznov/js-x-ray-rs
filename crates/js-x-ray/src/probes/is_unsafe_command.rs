//! Upstream: `src/probes/isUnsafeCommand.ts`
//!
//! Detect spawn/exec unsafe commands, e.g.
//! `child_process.spawn("csrutil", ["status"])`.

use serde_json::Value;

use crate::estree::{Node, SourceLocation, is_string_literal, is_template_literal, node_type, to_literal};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::source_file::{Sensitivity, SourceFile};
use crate::variable_tracer::TraceOptions;
use crate::warnings::{GenerateWarningOptions, generate_warning};

const K_UNSAFE_COMMANDS: [&str; 4] = ["csrutil", "uname", "ping", "curl"];

const K_IDENTIFIER_OR_MEMBER_EXPS: [&str; 4] = [
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

fn get_command(command_arg: &Node) -> String {
    match node_type(command_arg) {
        Some("Literal") => command_arg
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        Some("TemplateLiteral") => to_literal(command_arg),
        _ => String::new(),
    }
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
            let value = element.get("value").and_then(Value::as_str).unwrap_or_default();
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
        for identifier_or_member_exp in K_IDENTIFIER_OR_MEMBER_EXPS {
            let module_name = identifier_or_member_exp
                .split('.')
                .next()
                .unwrap_or_default()
                .to_owned();
            source_file.tracer.trace(
                identifier_or_member_exp,
                TraceOptions {
                    follow_consecutive_assignment: true,
                    module_name: Some(module_name),
                    ..Default::default()
                },
            );
        }
    }

    fn validate_node(&mut self, _node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        let data = ctx.traced_data?;

        K_IDENTIFIER_OR_MEMBER_EXPS
            .contains(&data.name.as_str())
            .then(|| Value::String(data.name["child_process.".len()..].to_owned()))
    }

    fn main(&mut self, node: &Node, data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let method_name = data.as_str().unwrap_or_default();

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

        let mut command = get_command(command_arg);
        let is_spawn = matches!(method_name, "spawn" | "spawnSync");

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
