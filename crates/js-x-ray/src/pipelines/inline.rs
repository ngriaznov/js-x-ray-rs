//! Upstream: `src/pipelines/inline.ts`

use serde_json::{Value, json};

use crate::inlined::{InlinedCallExpression, InlinedNew};
use crate::walker::walk_enter;

use super::Pipeline;

#[derive(Debug, Default)]
pub struct Inline;

impl Pipeline for Inline {
    fn name(&self) -> &'static str {
        "inline"
    }

    fn walk(&mut self, body: Vec<Value>) -> Vec<Value> {
        let mut hoisted: Vec<Value> = Vec::new();
        let mut root = Value::Array(body);

        walk_enter(&mut root, |ctx, node| {
            if node.is_array() {
                return;
            }

            if let Some(split_new) = InlinedNew::split(node)
                && let Some(rebuild) = split_new.rebuild_expression
            {
                hoisted.push(split_new.virtual_declaration);
                ctx.replace_and_skip(rebuild);
                return;
            }

            if let Some(split_call) = InlinedCallExpression::split(node)
                && let Some(rebuild) = split_call.rebuild_expression
            {
                let block_statement = json!({
                    "type": "BlockStatement",
                    "body": [
                        split_call.virtual_declaration,
                        {
                            "type": "ExpressionStatement",
                            "expression": rebuild,
                        },
                    ],
                });
                ctx.replace_and_skip(block_statement);
            }
        });

        let Value::Array(body) = root else {
            return hoisted;
        };
        let mut result = hoisted;
        result.extend(body);
        result
    }
}
