//! Upstream: `src/probes/unsafe-vm-context.ts`

use std::collections::HashSet;

use serde_json::Value;

use crate::estree::{Node, SourceLocation, is_call_expression};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::source_file::SourceFile;
use crate::variable_tracer::{TraceOptions, TracerEvent};
use crate::warnings::{GenerateWarningOptions, generate_warning};

/// Upstream `setEntryPoint`/named `main` handlers (`default` | `script`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum EntryPoint {
    #[default]
    Default,
    Script,
}

#[derive(Debug, Default)]
pub struct UnsafeVmContext {
    /// Upstream `kRunInContextTracedFunctions` probe-local context set.
    run_in_context_traced_functions: HashSet<String>,
    entry_point: EntryPoint,
}

impl Probe for UnsafeVmContext {
    fn name(&self) -> &'static str {
        "unsafe-vm-context"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["CallExpression"])
    }

    fn initialize(&mut self, source_file: &mut SourceFile) {
        source_file.tracer.trace(
            "vm.runInNewContext",
            TraceOptions {
                follow_consecutive_assignment: true,
                module_name: Some("vm".to_owned()),
                ..Default::default()
            },
        );
        source_file.tracer.trace(
            "vm.Script",
            TraceOptions {
                follow_return_value_assignement: true,
                follow_consecutive_assignment: true,
                module_name: Some("vm".to_owned()),
                ..Default::default()
            },
        );
    }

    fn on_tracer_event(&mut self, event: &TracerEvent, source_file: &mut SourceFile) {
        let TracerEvent::ReturnValue { name, id, .. } = event else {
            return;
        };
        if name != "vm.Script" {
            return;
        }

        let traced_fn = format!("{id}.runInContext");
        self.run_in_context_traced_functions
            .insert(traced_fn.clone());
        source_file.tracer.trace(
            &traced_fn,
            TraceOptions {
                follow_consecutive_assignment: true,
                ..Default::default()
            },
        );
    }

    fn validate_node(&mut self, node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        if !is_call_expression(node) {
            return None;
        }
        if !ctx.source_file.tracer.imported_modules.contains("vm") {
            return None;
        }

        let identifier_or_member_expr = ctx
            .traced_data
            .map(|data| data.identifier_or_member_expr.as_str());

        if identifier_or_member_expr
            .is_some_and(|id| self.run_in_context_traced_functions.contains(id))
        {
            self.entry_point = EntryPoint::Script;
            return Some(Value::Bool(true));
        }

        self.entry_point = EntryPoint::Default;
        (identifier_or_member_expr == Some("vm.runInNewContext")).then_some(Value::Bool(true))
    }

    fn main(&mut self, node: &Node, _data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let value = match self.entry_point {
            EntryPoint::Script => "(new vm.Script(code, options)).runInContext",
            EntryPoint::Default => "vm.runInNewContext",
        };

        ctx.source_file.warnings.push(generate_warning(
            "unsafe-vm-context",
            GenerateWarningOptions {
                value: Some(value.to_owned()),
                location: SourceLocation::from_node(node),
                ..Default::default()
            },
        ));

        ProbeReturn::Matched
    }
}
