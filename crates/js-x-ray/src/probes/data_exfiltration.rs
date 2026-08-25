//! Upstream: `src/probes/data-exfiltration.ts`

use std::sync::LazyLock;

use indexmap::IndexMap;
use regex::Regex;
use serde_json::Value;

use crate::estree::{
    GetCallExpressionIdentifierOptions, Node, SourceLocation, get_call_expression_identifier,
    is_call_expression, is_string_literal,
};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::source_file::{Sensitivity, SourceFile};
use crate::utils::{SourceArrayLocation, to_array_location};
use crate::variable_tracer::{TraceOptions, TracerEvent};
use crate::warnings::{GenerateWarningOptions, WarningLocation, generate_warning};

const K_SENSITIVE_MODULES: [&str; 2] = ["os", "dns"];

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
    context: IndexMap<String, Vec<SourceArrayLocation>>,
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
        self.add_in_context(value.to_owned(), SourceLocation::from_node(node));

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

        if let Some(data) = ctx.source_file.tracer.get_data_from_identifier(&id, false) {
            let is_sensitive = K_SENSITIVE_METHODS.iter().any(|method| {
                data.identifier_or_member_expr == *method
                    && ctx
                        .source_file
                        .tracer
                        .imported_modules
                        .contains(method.split('.').next().unwrap_or(""))
            });
            if is_sensitive {
                self.add_in_context(
                    data.identifier_or_member_expr.clone(),
                    SourceLocation::from_node(first_arg),
                );
            }
        }

        ProbeReturn::Matched
    }

    /// Upstream `addInContext`.
    fn add_in_context(&mut self, value: String, location: Option<SourceLocation>) {
        self.context
            .entry(value)
            .or_default()
            .push(to_array_location(location));
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
        source_file
            .tracer
            .trace(
                "JSON.stringify",
                TraceOptions {
                    follow_consecutive_assignment: true,
                    ..Default::default()
                },
            )
            .trace(
                "os.userInfo",
                TraceOptions {
                    module_name: Some("os".to_owned()),
                    follow_consecutive_assignment: true,
                    ..Default::default()
                },
            )
            .trace(
                "os.networkInterfaces",
                TraceOptions {
                    module_name: Some("os".to_owned()),
                    follow_consecutive_assignment: true,
                    ..Default::default()
                },
            )
            .trace(
                "os.cpus",
                TraceOptions {
                    module_name: Some("os".to_owned()),
                    follow_consecutive_assignment: true,
                    ..Default::default()
                },
            )
            .trace(
                "dns.getServers",
                TraceOptions {
                    module_name: Some("dns".to_owned()),
                    follow_consecutive_assignment: true,
                    ..Default::default()
                },
            );
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

        if K_SENSITIVE_MODULES.contains(&module_name.as_str())
            && !self.context.contains_key(module_name)
        {
            self.context
                .insert(module_name.clone(), vec![to_array_location(*location)]);
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
        if self.context.is_empty() {
            return;
        }

        let value = self.context.keys().cloned().collect::<Vec<_>>().join(", ");
        let mut warning = generate_warning(
            "data-exfiltration",
            GenerateWarningOptions {
                value: Some(value),
                ..Default::default()
            },
        );
        warning.location =
            WarningLocation::Multiple(self.context.values().flatten().copied().collect());
        source_file.warnings.push(warning);
    }
}
