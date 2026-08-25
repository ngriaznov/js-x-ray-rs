//! Upstream: `src/pipelines/deobfuscate.ts`

use serde_json::{Value, json};

use crate::estree::{is_call_expression, join_array_expression, noop};
use crate::walker::walk_enter;

use super::Pipeline;

#[derive(Debug, Default)]
pub struct Deobfuscate;

impl Deobfuscate {
    /// Upstream `#withCallExpression`.
    fn with_call_expression(node: &Value) -> Option<Value> {
        let value = join_array_expression(node, &noop)?;
        Some(json!({
            "type": "Literal",
            "value": value,
            "raw": value,
        }))
    }
}

impl Pipeline for Deobfuscate {
    fn name(&self) -> &'static str {
        "deobfuscate"
    }

    fn walk(&mut self, body: Vec<Value>) -> Vec<Value> {
        let mut root = Value::Array(body);

        walk_enter(&mut root, |ctx, node| {
            if node.is_array() {
                return;
            }

            if is_call_expression(node) {
                // Upstream calls `replaceAndSkip` unconditionally: children
                // are skipped even when there is nothing to replace.
                match Self::with_call_expression(node) {
                    Some(replacement) => ctx.replace_and_skip(replacement),
                    None => ctx.skip(),
                }
            }
        });

        match root {
            Value::Array(body) => body,
            other => vec![other],
        }
    }
}
