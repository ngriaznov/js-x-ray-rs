//! Upstream: `src/probes/isImportDeclaration.ts`
//!
//! Search for ESM `ImportDeclaration`/`ImportExpression`, e.g.
//! `import * as foo from "bar";`, `import fs from "fs";`,
//! `import "make-promises-safe";`.
//! <https://github.com/estree/estree/blob/master/es2015.md#importdeclaration>

use serde_json::Value;

use crate::estree::{Node, SourceLocation, node_type};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::warnings::{GenerateWarningOptions, generate_warning};

/// Upstream: dangerous import prefixes.
/// - `data:text/javascript;...`: eval via import, see
///   <https://2ality.com/2019/10/eval-via-import.html>
/// - `file:...`: file inclusion vulnerability, see
///   <https://en.wikipedia.org/wiki/File_inclusion_vulnerability>
const SUSPICIOUS_IMPORT_PREFIXES: [&str; 2] = ["data:text/javascript", "file:"];

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

        let source = node.get("source")?;
        (source.get("type").and_then(Value::as_str) == Some("Literal")
            && source.get("value").is_some_and(Value::is_string))
        .then_some(Value::Null)
    }

    fn main(&mut self, node: &Node, _data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let value = node
            .pointer("/source/value")
            .and_then(Value::as_str)
            .unwrap_or_default();
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
