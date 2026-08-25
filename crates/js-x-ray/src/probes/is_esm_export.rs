//! Upstream: `src/probes/isESMExport.ts`
//!
//! Search for ESM Export, e.g. `export { bar } from "./foo.js";` or
//! `export * from "./bar.js";`.

use serde_json::Value;

use crate::estree::{Node, SourceLocation, is_type, node_type};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};

#[derive(Debug, Default)]
pub struct IsEsmExport;

impl Probe for IsEsmExport {
    fn name(&self) -> &'static str {
        "isESMExport"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["ExportNamedDeclaration", "ExportAllDeclaration"])
    }

    fn validate_node(&mut self, node: &Node, _ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        if !matches!(
            node_type(node),
            Some("ExportNamedDeclaration" | "ExportAllDeclaration")
        ) {
            return None;
        }

        let source = node.get("source")?;
        (!source.is_null() && is_type(source, "Literal") && source.get("value")?.is_string())
            .then_some(Value::Null)
    }

    fn main(&mut self, node: &Node, _data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let value = node
            .pointer("/source/value")
            .and_then(Value::as_str)
            .unwrap_or_default();
        ctx.source_file
            .add_dependency(value, SourceLocation::from_node(node));

        ProbeReturn::Matched
    }

    fn break_on_match(&self) -> bool {
        true
    }
}
