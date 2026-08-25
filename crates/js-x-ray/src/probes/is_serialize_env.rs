//! Upstream: `src/probes/isSerializeEnv.ts`
//!
//! Detect serialization of `process.env`, which could indicate environment
//! variable exfiltration, e.g. `JSON.stringify(process.env)`.

use serde_json::Value;

use crate::estree::{
    Node, get_member_expression_identifier, identifier_name, is_identifier, is_member_expression,
    noop,
};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::source_file::{Sensitivity, SourceFile};
use crate::variable_tracer::TraceOptions;
use crate::warnings::{GenerateWarningOptions, generate_warning};

/// Upstream `setEntryPoint`/named `main` handlers (`default` | `process.env`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum EntryPoint {
    #[default]
    Default,
    ProcessEnv,
}

#[derive(Debug, Default)]
pub struct IsSerializeEnv {
    entry_point: EntryPoint,
}

impl IsSerializeEnv {
    /// Upstream `validateJsonStringify`.
    fn validate_json_stringify(node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        let data = ctx.traced_data?;
        if data.identifier_or_member_expr != "JSON.stringify" {
            return None;
        }

        let first_arg = node.get("arguments")?.as_array()?.first()?;

        if is_member_expression(first_arg) {
            let member_expr_id = get_member_expression_identifier(first_arg, &noop).join(".");
            if member_expr_id == "process.env" {
                return Some(Value::Null);
            }
        }

        if is_identifier(first_arg) {
            let name = identifier_name(first_arg).unwrap_or("");
            if ctx
                .source_file
                .tracer
                .get_data_from_identifier(name, false)
                .is_some()
            {
                return Some(Value::Null);
            }
        }

        None
    }

    /// Upstream `validateProcessEnv`.
    fn validate_process_env(&mut self, node: &Node, _ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        if !is_member_expression(node) {
            return None;
        }

        let member_expr_id = get_member_expression_identifier(node, &noop).join(".");
        if member_expr_id != "process.env" {
            return None;
        }

        self.entry_point = EntryPoint::ProcessEnv;
        Some(Value::Null)
    }

    /// Upstream `defaultHandler`.
    fn default_handler(node: &Node, source_file: &mut SourceFile) -> ProbeReturn {
        let warning = generate_warning(
            "serialize-environment",
            GenerateWarningOptions {
                value: Some("JSON.stringify(process.env)".to_owned()),
                location: crate::estree::SourceLocation::from_node(node),
                ..Default::default()
            },
        );
        source_file.warnings.push(warning);

        ProbeReturn::Skip
    }

    /// Upstream `processEnvHandler`.
    fn process_env_handler(node: &Node, source_file: &mut SourceFile) -> ProbeReturn {
        if source_file.sensitivity != Sensitivity::Aggressive {
            return ProbeReturn::Continue;
        }

        let warning = generate_warning(
            "serialize-environment",
            GenerateWarningOptions {
                value: Some("process.env".to_owned()),
                location: crate::estree::SourceLocation::from_node(node),
                ..Default::default()
            },
        );
        source_file.warnings.push(warning);

        ProbeReturn::Skip
    }
}

impl Probe for IsSerializeEnv {
    fn name(&self) -> &'static str {
        "isSerializeEnv"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["CallExpression", "MemberExpression"])
    }

    fn initialize(&mut self, source_file: &mut SourceFile) {
        source_file.tracer.trace(
            "process.env",
            TraceOptions {
                follow_consecutive_assignment: true,
                ..Default::default()
            },
        );
        source_file.tracer.trace(
            "JSON.stringify",
            TraceOptions {
                follow_consecutive_assignment: true,
                ..Default::default()
            },
        );
    }

    fn validate_node(&mut self, node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        self.entry_point = EntryPoint::Default;

        if let Some(data) = Self::validate_json_stringify(node, ctx) {
            return Some(data);
        }

        self.validate_process_env(node, ctx)
    }

    fn main(&mut self, node: &Node, _data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        match self.entry_point {
            EntryPoint::Default => Self::default_handler(node, ctx.source_file),
            EntryPoint::ProcessEnv => Self::process_env_handler(node, ctx.source_file),
        }
    }
}
