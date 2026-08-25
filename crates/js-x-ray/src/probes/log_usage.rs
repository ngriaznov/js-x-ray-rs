//! Upstream: `src/probes/log-usage.ts`
//!
//! Upstream registers dynamic `VariableTracer.ReturnValueEvent` listeners
//! from `initialize` (closures capturing `logUsages`/factory-tracked-function
//! sets). Here those listeners become [`LogUsage::on_tracer_event`], reacting
//! to queued [`TracerEvent::ReturnValue`] events and mutating probe-local
//! state (the Rust replacement for the upstream probe `context` object and
//! the closures' captured sets/maps).

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use indexmap::IndexMap;
use regex::Regex;
use serde_json::Value;

use crate::estree::{Node, SourceLocation, identifier_name, is_identifier, is_type};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::source_file::SourceFile;
use crate::utils::{SourceArrayLocation, to_array_location};
use crate::variable_tracer::{TraceOptions, TracerEvent};
use crate::warnings::{GenerateWarningOptions, WarningLocation, generate_warning};

const K_PINO_LOG_METHODS: [&str; 6] = ["info", "warn", "error", "fatal", "debug", "trace"];
const K_WINSTON_LOG_METHODS: [&str; 8] = [
    "info", "warn", "error", "http", "debug", "verbose", "silly", "log",
];

#[derive(Debug, Default)]
pub struct LogUsage {
    /// Upstream `logUsages` (grows as new logger instances are discovered).
    logger_names: HashSet<String>,
    /// Upstream probe `context` (`LogUsageContextDef`).
    context: IndexMap<String, Vec<SourceArrayLocation>>,
    /// Upstream `createWinstonTracerListener`'s `winstonLoggerFactoryTracedFunctions`.
    winston_child_sources: HashSet<String>,
    /// Upstream `createWinstonCreateLoggerTracerListener`'s
    /// `winstonCreateLoggerFactoryTracedFunctions`.
    winston_create_logger_sources: HashSet<String>,
    /// Upstream `winstonCreateLoggerChildLoggerFunctions`.
    winston_create_logger_methods: HashMap<String, Vec<String>>,
    /// Upstream `createPinoTracerListener`'s `pinoLoggerFactoryTracedFunctions`.
    pino_sources: HashSet<String>,
    /// Upstream `pinoLoggerChildLoggerFunctions`.
    pino_methods: HashMap<String, Vec<String>>,
}

impl Probe for LogUsage {
    fn name(&self) -> &'static str {
        "log-usage"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["CallExpression"])
    }

    fn initialize(&mut self, source_file: &mut SourceFile) {
        for method in [
            "console.log",
            "console.info",
            "console.warn",
            "console.error",
            "console.debug",
        ] {
            self.logger_names.insert(method.to_owned());
            source_file.tracer.trace(
                method,
                TraceOptions {
                    follow_consecutive_assignment: true,
                    ..Default::default()
                },
            );
        }

        for (identifier, module_name) in [
            ("winston.createLogger", "winston"),
            ("winston", "winston"),
            ("pino", "pino"),
        ] {
            source_file.tracer.trace(
                identifier,
                TraceOptions {
                    follow_return_value_assignement: true,
                    follow_consecutive_assignment: true,
                    module_name: Some(module_name.to_owned()),
                    ..Default::default()
                },
            );
        }

        source_file.tracer.trace(
            "winston.child",
            TraceOptions {
                follow_return_value_assignement: true,
                module_name: Some("winston".to_owned()),
                ..Default::default()
            },
        );
        for method in K_WINSTON_LOG_METHODS {
            let logger_method = format!("winston.{method}");
            self.logger_names.insert(logger_method.clone());
            source_file.tracer.trace(
                &logger_method,
                TraceOptions {
                    follow_consecutive_assignment: true,
                    module_name: Some("winston".to_owned()),
                    ..Default::default()
                },
            );
        }

        self.winston_child_sources
            .insert("winston.child".to_owned());
        self.winston_create_logger_sources
            .insert("winston.createLogger".to_owned());
        self.pino_sources.insert("pino".to_owned());
    }

    fn on_tracer_event(&mut self, event: &TracerEvent, source_file: &mut SourceFile) {
        let TracerEvent::ReturnValue {
            name,
            id,
            arguments,
            ..
        } = event
        else {
            return;
        };

        if self.winston_child_sources.contains(name) {
            self.handle_winston_child(id, source_file);
        }
        if self.winston_create_logger_sources.contains(name) {
            self.handle_winston_create_logger(name, id, arguments, source_file);
        }
        if self.pino_sources.contains(name) {
            self.handle_pino(name, id, arguments, source_file);
        }
    }

    fn validate_node(&mut self, _node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        let identifier_or_member_expr = ctx.traced_data?.identifier_or_member_expr.as_str();

        self.logger_names
            .contains(identifier_or_member_expr)
            .then(|| Value::String(identifier_or_member_expr.to_owned()))
    }

    fn main(&mut self, node: &Node, data: &Value, _ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        if let Some(log_identifier) = data.as_str() {
            let location = to_array_location(SourceLocation::from_node(node));
            self.context
                .entry(log_identifier.to_owned())
                .or_default()
                .push(location);
        }

        ProbeReturn::Matched
    }

    fn finalize(&mut self, source_file: &mut SourceFile) {
        if self.context.is_empty() {
            return;
        }

        static VIRTUAL_CALL_RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"__virtual_call_.*\d+__\.").expect("valid regex"));

        let value = self
            .context
            .keys()
            .map(|method| VIRTUAL_CALL_RE.replace(method, "").into_owned())
            .collect::<Vec<_>>()
            .join(", ");

        let mut warning = generate_warning(
            "log-usage",
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

impl LogUsage {
    /// Upstream `createWinstonTracerListener`'s dynamic listener body.
    fn handle_winston_child(&mut self, id: &str, source_file: &mut SourceFile) {
        for method in K_WINSTON_LOG_METHODS {
            let traced_fn = format!("{id}.{method}");
            self.logger_names.insert(traced_fn.clone());
            source_file.tracer.trace(
                &traced_fn,
                TraceOptions {
                    follow_consecutive_assignment: true,
                    module_name: Some("winston".to_owned()),
                    ..Default::default()
                },
            );
        }

        let child_logger = format!("{id}.child");
        source_file.tracer.trace(
            &child_logger,
            TraceOptions {
                follow_return_value_assignement: true,
                module_name: Some("winston".to_owned()),
                ..Default::default()
            },
        );
        self.winston_child_sources.insert(child_logger);
    }

    /// Upstream `createWinstonCreateLoggerTracerListener`'s dynamic listener body.
    fn handle_winston_create_logger(
        &mut self,
        name: &str,
        id: &str,
        arguments: &[Value],
        source_file: &mut SourceFile,
    ) {
        let mut methods = self
            .winston_create_logger_methods
            .get(name)
            .cloned()
            .unwrap_or_else(|| {
                K_WINSTON_LOG_METHODS
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect()
            });

        if name == "winston.createLogger"
            && let Some(levels) = resolve_call_context(arguments, source_file)
                .and_then(|context| find_object_property(context, "levels"))
        {
            methods.clear();
            add_log_methods(Some(levels), &mut methods);
        }

        for method in &methods {
            let traced_fn = format!("{id}.{method}");
            self.logger_names.insert(traced_fn.clone());
            source_file.tracer.trace(
                &traced_fn,
                TraceOptions {
                    follow_consecutive_assignment: true,
                    module_name: Some("winston".to_owned()),
                    ..Default::default()
                },
            );
        }

        let child_logger = format!("{id}.child");
        source_file.tracer.trace(
            &child_logger,
            TraceOptions {
                follow_return_value_assignement: true,
                module_name: Some("winston".to_owned()),
                ..Default::default()
            },
        );
        self.winston_create_logger_methods
            .insert(child_logger.clone(), methods);
        self.winston_create_logger_sources.insert(child_logger);
    }

    /// Upstream `createPinoTracerListener`'s dynamic listener body.
    fn handle_pino(
        &mut self,
        name: &str,
        id: &str,
        arguments: &[Value],
        source_file: &mut SourceFile,
    ) {
        let mut methods = self
            .pino_methods
            .get(name)
            .cloned()
            .unwrap_or_else(|| K_PINO_LOG_METHODS.iter().map(|s| (*s).to_owned()).collect());

        if name == "pino"
            && let Some(context) = resolve_call_context(arguments, source_file)
        {
            let custom_levels = find_object_property(context, "customLevels");
            let use_only_custom_levels = find_object_property(context, "useOnlyCustomLevels")
                .and_then(|property| resolve_property_text(property, source_file));

            if use_only_custom_levels.as_deref() == Some("true") {
                methods.clear();
            }
            add_log_methods(custom_levels, &mut methods);
        }

        for method in &methods {
            let traced_fn = format!("{id}.{method}");
            self.logger_names.insert(traced_fn.clone());
            source_file.tracer.trace(
                &traced_fn,
                TraceOptions {
                    follow_consecutive_assignment: true,
                    module_name: Some("pino".to_owned()),
                    ..Default::default()
                },
            );
        }

        let child_logger = format!("{id}.child");
        source_file.tracer.trace(
            &child_logger,
            TraceOptions {
                follow_return_value_assignement: true,
                module_name: Some("pino".to_owned()),
                ..Default::default()
            },
        );
        self.pino_methods.insert(child_logger.clone(), methods);
        self.pino_sources.insert(child_logger);
    }
}

/// Resolves a traced function's first call argument to an `ObjectExpression`,
/// following an `Identifier` back through `tracer.objectIdentifiers` when needed.
fn resolve_call_context<'a>(
    arguments: &'a [Value],
    source_file: &'a SourceFile,
) -> Option<&'a Value> {
    let arg = arguments.first()?;
    let context = if is_identifier(arg) {
        source_file
            .tracer
            .object_identifiers
            .get(identifier_name(arg)?)?
    } else {
        arg
    };

    is_type(context, "ObjectExpression").then_some(context)
}

fn find_object_property<'a>(object_expr: &'a Value, key_name: &str) -> Option<&'a Value> {
    object_expr
        .get("properties")?
        .as_array()?
        .iter()
        .find(|property| {
            is_type(property, "Property")
                && property
                    .get("key")
                    .is_some_and(|key| is_identifier(key) && identifier_name(key) == Some(key_name))
        })
}

/// Resolves a `Property` node's value to its literal source text, following
/// a bare `Identifier` value back through `tracer.literalIdentifiers`.
fn resolve_property_text(property: &Value, source_file: &SourceFile) -> Option<String> {
    let value = property.get("value")?;
    if is_type(value, "Literal") {
        value.get("raw").and_then(Value::as_str).map(str::to_owned)
    } else if is_identifier(value) {
        source_file
            .tracer
            .literal_identifiers
            .get(identifier_name(value)?)
            .map(|literal| literal.value.clone())
    } else {
        None
    }
}

/// Upstream `addLogMethods`.
fn add_log_methods(property: Option<&Value>, methods: &mut Vec<String>) {
    let Some(property) = property else { return };
    if !is_type(property, "Property") {
        return;
    }
    let Some(value) = property.get("value") else {
        return;
    };
    if !is_type(value, "ObjectExpression") {
        return;
    }

    for level in value
        .get("properties")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if is_type(level, "Property")
            && let Some(key) = level.get("key")
            && is_identifier(key)
            && let Some(name) = identifier_name(key)
        {
            methods.push(name.to_owned());
        }
    }
}
