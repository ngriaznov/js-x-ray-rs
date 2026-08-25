//! Upstream: `src/probes/isMonkeyPatch.ts`

use std::collections::HashSet;
use std::sync::LazyLock;

use serde_json::Value;

use crate::estree::{
    Node, SourceLocation, call_expression_identifier, get_member_expression_identifier,
    is_member_expression, node_type,
};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::source_file::SourceFile;
use crate::variable_tracer::TraceOptions;
use crate::warnings::{GenerateWarningOptions, generate_warning};

static JS_TYPES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "AggregateError",
        "Array",
        "ArrayBuffer",
        "BigInt",
        "BigInt64Array",
        "BigUint64Array",
        "Boolean",
        "DataView",
        "Date",
        "Error",
        "EvalError",
        "FinalizationRegistry",
        "Float32Array",
        "Float64Array",
        "Function",
        "Int16Array",
        "Int32Array",
        "Int8Array",
        "Map",
        "Number",
        "Object",
        "Promise",
        "Proxy",
        "RangeError",
        "ReferenceError",
        "Reflect",
        "RegExp",
        "Set",
        "SharedArrayBuffer",
        "String",
        "Symbol",
        "SyntaxError",
        "TypeError",
        "Uint16Array",
        "Uint32Array",
        "Uint8Array",
        "Uint8ClampedArray",
        "URIError",
        "WeakMap",
        "WeakRef",
        "WeakSet",
    ]
    .into_iter()
    .collect()
});

/// Search for monkey patching of built-in prototypes, e.g.
/// `Array.prototype.map = function() {};`
fn validate_node_assignment(node: &Value, source_file: &SourceFile) -> Option<String> {
    let left = node.get("left")?;
    if !is_member_expression(left) {
        return None;
    }
    validate_member_expression(left, source_file)
}

fn resolve_define_property_identifier(node: &Value, source_file: &SourceFile) -> Option<String> {
    let id = call_expression_identifier(node)?;
    if id == "Object.defineProperty" || id == "Reflect.defineProperty" {
        return Some(id);
    }
    if !id.contains('.') {
        return None;
    }

    let mut parts = id.splitn(2, '.');
    let object_part = parts.next()?;
    let method_name = parts.next().unwrap_or("");
    if method_name != "defineProperty" {
        return None;
    }

    let resolved = resolve_js_type_name(object_part, source_file)?;
    (resolved == "Object" || resolved == "Reflect").then(|| format!("{resolved}.defineProperty"))
}

fn validate_define_property(node: &Value, source_file: &SourceFile) -> Option<String> {
    resolve_define_property_identifier(node, source_file)?;

    // TODO: detect aliased prototype target in defineProperty,
    // e.g. const ap = Array.prototype; Object.defineProperty(ap, ...)
    let first_arg = node.get("arguments")?.as_array()?.first()?;
    if !is_member_expression(first_arg) {
        return None;
    }

    validate_member_expression(first_arg, source_file)
}

fn resolve_js_type_name(name: &str, source_file: &SourceFile) -> Option<String> {
    if JS_TYPES.contains(name) {
        return Some(name.to_owned());
    }

    let traced = source_file.tracer.get_data_from_identifier(name, false)?;
    JS_TYPES
        .contains(traced.identifier_or_member_expr.as_str())
        .then_some(traced.identifier_or_member_expr)
}

fn validate_member_expression(node: &Value, source_file: &SourceFile) -> Option<String> {
    let parts = {
        let literal_identifiers = &source_file.tracer.literal_identifiers;
        let lookup = |name: &str| literal_identifiers.get(name).map(|literal| literal.value.clone());
        get_member_expression_identifier(node, &lookup)
    };

    let raw_name = parts.first()?;
    let js_type_name = resolve_js_type_name(raw_name, source_file)?;

    (parts.get(1).map(String::as_str) == Some("prototype"))
        .then(|| format!("{js_type_name}.prototype"))
}

#[derive(Debug, Default)]
pub struct IsMonkeyPatch;

impl Probe for IsMonkeyPatch {
    fn name(&self) -> &'static str {
        "isMonkeyPatch"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["AssignmentExpression", "CallExpression"])
    }

    fn initialize(&mut self, source_file: &mut SourceFile) {
        for js_type in JS_TYPES.iter().copied() {
            source_file.tracer.trace(
                js_type,
                TraceOptions {
                    follow_consecutive_assignment: true,
                    ..Default::default()
                },
            );
        }
    }

    fn validate_node(&mut self, node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        match node_type(node) {
            Some("AssignmentExpression") => validate_node_assignment(node, ctx.source_file),
            Some("CallExpression") => validate_define_property(node, ctx.source_file),
            _ => None,
        }
        .map(Value::String)
    }

    fn main(&mut self, node: &Node, data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        ctx.source_file.warnings.push(generate_warning(
            "monkey-patch",
            GenerateWarningOptions {
                value: data.as_str().map(str::to_owned),
                location: SourceLocation::from_node(node),
                ..Default::default()
            },
        ));

        ProbeReturn::Matched
    }
}
