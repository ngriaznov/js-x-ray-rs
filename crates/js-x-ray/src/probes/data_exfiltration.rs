//! Upstream: `src/probes/data-exfiltration.ts`

use std::sync::LazyLock;

use indexmap::IndexSet;
use regex::Regex;
use serde_json::Value;

use crate::estree::{
    GetCallExpressionIdentifierOptions, Node, SourceLocation, get_call_expression_identifier,
    is_call_expression, is_string_literal,
};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::probes::tracing::{TraceAllOptions, trace_all, trace_all_from_module};
use crate::probes::warning_aggregator::{
    AggregatedLocations, collect_location, push_aggregated_warning,
};
use crate::source_file::{Sensitivity, SourceFile};
use crate::variable_tracer::TracerEvent;

const K_SENSITIVE_METHODS: [&str; 4] = [
    "os.userInfo",
    "os.networkInterfaces",
    "os.cpus",
    "dns.getServers",
];

static SENSITIVE_PATH_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"~/\.(ssh|aws|npmrc|gitconfig|bashrc)(/[^\s"'`]+)?"#).expect("valid regex")
});

/// Upstream `setEntryPoint`/named `main` handlers (`default` | `literal`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum EntryPoint {
    #[default]
    Default,
    Literal,
}

#[derive(Debug, Default)]
pub struct DataExfiltration {
    /// Upstream probe-local `context: DataExfiltrationContextDef`.
    context: AggregatedLocations,
    /// Upstream `sensitiveModules` captured by the aggressive import listener.
    sensitive_modules: IndexSet<String>,
    entry_point: EntryPoint,
}

impl DataExfiltration {
    /// Upstream `validateJSONStringify`.
    fn validate_json_stringify(node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        if ctx.source_file.sensitivity == Sensitivity::Aggressive {
            return None;
        }

        let data = ctx.traced_data?;
        if data.identifier_or_member_expr != "JSON.stringify" {
            return None;
        }

        let arguments = node.get("arguments")?.as_array()?;
        if arguments.is_empty() {
            return None;
        }

        Some(Value::Null)
    }

    /// Upstream `validateLiteral`.
    fn validate_literal(&mut self, node: &Node) -> Option<Value> {
        if !is_string_literal(node) {
            return None;
        }
        let value = node.get("value")?.as_str()?;
        if !SENSITIVE_PATH_REGEX.is_match(value) {
            return None;
        }

        self.entry_point = EntryPoint::Literal;
        Some(Value::Null)
    }

    /// Upstream `sensitiveLiteralHandler`.
    fn sensitive_literal_handler(&mut self, node: &Node) -> ProbeReturn {
        let value = node.get("value").and_then(Value::as_str).unwrap_or("");
        collect_location(&mut self.context, value, SourceLocation::from_node(node));

        ProbeReturn::Matched
    }

    /// Upstream `sensitiveMethodsHandler`.
    fn sensitive_methods_handler(&mut self, node: &Node, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let Some(first_arg) = node
            .get("arguments")
            .and_then(Value::as_array)
            .and_then(|args| args.first())
        else {
            return ProbeReturn::Matched;
        };
        if !is_call_expression(first_arg) {
            return ProbeReturn::Matched;
        }

        let Some(id) = get_call_expression_identifier(
            first_arg,
            &GetCallExpressionIdentifierOptions::default(),
        ) else {
            return ProbeReturn::Matched;
        };

        let data = ctx.source_file.tracer.get_data_from_identifier(&id, false);
        if let Some(data) = data
            && K_SENSITIVE_METHODS.contains(&data.identifier_or_member_expr.as_str())
        {
            collect_location(
                &mut self.context,
                &data.identifier_or_member_expr,
                SourceLocation::from_node(first_arg),
            );
        }

        ProbeReturn::Matched
    }
}

impl Probe for DataExfiltration {
    fn name(&self) -> &'static str {
        "dataExfiltration"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["CallExpression", "Literal"])
    }

    fn initialize(&mut self, source_file: &mut SourceFile) {
        let tracer = &mut source_file.tracer;

        trace_all(tracer, ["JSON.stringify"], TraceAllOptions::default());
        self.sensitive_modules =
            trace_all_from_module(tracer, K_SENSITIVE_METHODS, TraceAllOptions::default());
    }

    fn on_tracer_event(&mut self, event: &TracerEvent, source_file: &mut SourceFile) {
        if source_file.sensitivity != Sensitivity::Aggressive {
            return;
        }
        let TracerEvent::Import {
            module_name,
            location,
            ..
        } = event
        else {
            return;
        };

        // Only the import itself is reported, not every later usage.
        if self.sensitive_modules.contains(module_name) && !self.context.contains_key(module_name) {
            collect_location(&mut self.context, module_name, *location);
        }
    }

    fn validate_node(&mut self, node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        self.entry_point = EntryPoint::Default;

        if let Some(data) = Self::validate_json_stringify(node, ctx) {
            return Some(data);
        }

        self.validate_literal(node)
    }

    fn main(&mut self, node: &Node, _data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        match self.entry_point {
            EntryPoint::Default => self.sensitive_methods_handler(node, ctx),
            EntryPoint::Literal => self.sensitive_literal_handler(node),
        }
    }

    fn finalize(&mut self, source_file: &mut SourceFile) {
        push_aggregated_warning(&self.context, source_file, "data-exfiltration", None);
    }
}
