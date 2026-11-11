//! Upstream: `src/probes/isImportDeclaration.ts`
//!
//! Search for ESM `ImportDeclaration`/`ImportExpression`, e.g.
//! `import * as foo from "bar";`, `import fs from "fs";`,
//! `import "make-promises-safe";`, `import(`bar`);`.
//! <https://github.com/estree/estree/blob/master/es2015.md#importdeclaration>

use serde_json::Value;

use crate::estree::{Node, SourceLocation, is_string_literal, node_type};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::warnings::{GenerateWarningOptions, generate_warning};

/// Upstream: dangerous import prefixes.
/// - `data:text/javascript;...`: eval via import, see
///   <https://2ality.com/2019/10/eval-via-import.html>
/// - `file:...`: file inclusion vulnerability, see
///   <https://en.wikipedia.org/wiki/File_inclusion_vulnerability>
const SUSPICIOUS_IMPORT_PREFIXES: [&str; 2] = ["data:text/javascript", "file:"];

/// Upstream `getSpecifier`: `import(`bar`)` is the same specifier as
/// `import("bar")`.
fn get_specifier(source: &Node) -> Option<String> {
    if is_string_literal(source) {
        return source.get("value")?.as_str().map(str::to_owned);
    }

    if node_type(source) == Some("TemplateLiteral")
        && source
            .get("expressions")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
    {
        return source
            .pointer("/quasis/0/value/cooked")?
            .as_str()
            .map(str::to_owned);
    }

    None
}

#[derive(Debug, Default)]
pub struct IsImportDeclaration;

impl Probe for IsImportDeclaration {
    fn name(&self) -> &'static str {
        "isImportDeclaration"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["ImportDeclaration", "ImportExpression"])
    }

    fn validate_node(&mut self, node: &Node, _ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        if !matches!(
            node_type(node),
            Some("ImportDeclaration" | "ImportExpression")
        ) {
            return None;
        }

        get_specifier(node.get("source")?).map(Value::String)
    }

    fn main(&mut self, node: &Node, data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let value = data.as_str().unwrap_or_default();
        let location = SourceLocation::from_node(node);

        if SUSPICIOUS_IMPORT_PREFIXES
            .iter()
            .any(|prefix| value.starts_with(prefix))
        {
            ctx.source_file.warnings.push(generate_warning(
                "unsafe-import",
                GenerateWarningOptions {
                    value: Some(value.to_owned()),
                    location,
                    ..Default::default()
                },
            ));
        }
        ctx.source_file.add_dependency(value, location);

        ProbeReturn::Matched
    }

    fn break_on_match(&self) -> bool {
        true
    }

    fn break_group(&self) -> Option<&'static str> {
        Some("import")
    }
}
