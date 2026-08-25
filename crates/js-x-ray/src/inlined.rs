//! Upstream: `src/Inlined.ts`, `src/InlinedCallExpression.ts`,
//! `src/InlinedNew.ts`, and `src/probes/isRequire/InlinedRequire.ts`.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Value, json};

use crate::estree::{
    Node, SourceLocation, call_expression_identifier, is_call_expression, is_identifier,
    is_member_expression, is_type,
};

// `lib.rs` does not declare `virtual_variable_identifier` as a crate module
// (out of scope for this port: shared file). Mount the already-written
// sibling file here instead, since `Inlined` is its only upstream consumer.
// `pub`: `VirtualVariableIdentifier.spec.ts` needs `js_x_ray::inlined::virtual_variable_identifier`
// reachable from the integration-test binary.
#[path = "virtual_variable_identifier.rs"]
pub mod virtual_variable_identifier;
use virtual_variable_identifier::VirtualVariableIdentifier;

#[derive(Debug, Clone)]
pub struct SplitResult {
    pub virtual_identifier: String,
    pub virtual_declaration: Node,
    pub rebuild_expression: Option<Node>,
}

// ---------------------------------------------------------------------------
// Inlined.ts
// ---------------------------------------------------------------------------

fn build_split_result(node: &Node, target: &Node, identifier: &str) -> SplitResult {
    let virtual_identifier =
        VirtualVariableIdentifier::generate(identifier, SourceLocation::from_node(node));

    let virtual_declaration = json!({
        "type": "VariableDeclaration",
        "kind": "const",
        "declarations": [
            {
                "type": "VariableDeclarator",
                "id": { "type": "Identifier", "name": virtual_identifier },
                "init": target
            }
        ]
    });
    let rebuild_expression = rebuild_with_virtual_identifier(node, target, &virtual_identifier);

    SplitResult {
        virtual_identifier,
        virtual_declaration,
        rebuild_expression,
    }
}

fn rebuild_with_virtual_identifier(
    node: &Node,
    target: &Node,
    virtual_identifier: &str,
) -> Option<Node> {
    if std::ptr::eq(node, target) {
        return None;
    }

    let virtual_id = json!({ "type": "Identifier", "name": virtual_identifier });
    Some(clone_and_replace(node, target, &virtual_id))
}

fn clone_and_replace(node: &Node, target: &Node, replacement: &Node) -> Node {
    if std::ptr::eq(node, target) {
        return replacement.clone();
    }

    if is_call_expression(node) {
        let mut cloned = node.clone();
        if let Some((callee, arguments)) = node.get("callee").zip(node.get("arguments")) {
            let callee = clone_and_replace(callee, target, replacement);
            let arguments = arguments
                .as_array()
                .map(|args| {
                    args.iter()
                        .map(|arg| clone_and_replace(arg, target, replacement))
                        .collect()
                })
                .unwrap_or_default();
            let obj = cloned.as_object_mut().expect("CallExpression is an object");
            obj.insert("callee".to_owned(), callee);
            obj.insert("arguments".to_owned(), Value::Array(arguments));
        }
        return cloned;
    }

    if is_member_expression(node) {
        let mut cloned = node.clone();
        if let Some(object) = node.get("object") {
            let object = clone_and_replace(object, target, replacement);
            cloned
                .as_object_mut()
                .expect("MemberExpression is an object")
                .insert("object".to_owned(), object);
        }
        return cloned;
    }

    node.clone()
}

// ---------------------------------------------------------------------------
// InlinedCallExpression.ts
// ---------------------------------------------------------------------------

pub struct InlinedCallExpression;

impl InlinedCallExpression {
    pub fn split(node: &Node) -> Option<SplitResult> {
        if !is_call_expression(node) && !is_member_expression(node) {
            return None;
        }
        let call_expression = Self::find_call_expression(node, None, 0)?;
        if std::ptr::eq(call_expression, node) {
            return None;
        }

        Some(build_split_result(node, call_expression, "call_expression"))
    }

    fn find_call_expression<'a>(
        node: &'a Node,
        result: Option<&'a Node>,
        extra_call_count: u32,
    ) -> Option<&'a Node> {
        let object = if is_member_expression(node) {
            node.get("object")
        } else {
            node.get("callee")
        };

        if let Some(object) = object
            && is_call_expression(object)
            && object.get("callee").is_some_and(|callee| {
                is_identifier(callee)
                    && matches!(
                        callee.get("name").and_then(Value::as_str),
                        Some("require" | "eval")
                    )
            })
        {
            return None;
        }

        if is_member_expression(node) {
            return Self::find_call_expression(object?, Some(node), extra_call_count);
        }
        if is_call_expression(node) {
            return Self::find_call_expression(object?, Some(node), extra_call_count + 1);
        }

        result.filter(|r| is_call_expression(r) && extra_call_count > 1)
    }
}

// ---------------------------------------------------------------------------
// InlinedNew.ts
// ---------------------------------------------------------------------------

pub struct InlinedNew;

impl InlinedNew {
    pub fn split(node: &Node) -> Option<SplitResult> {
        if !is_call_expression(node) && !is_member_expression(node) {
            return None;
        }
        let new_expression = Self::find_new_call(node)?;

        Some(build_split_result(node, new_expression, "new"))
    }

    fn find_new_call(node: &Node) -> Option<&Node> {
        if is_call_expression(node) {
            return Self::find_new_call(node.get("callee")?);
        }
        if is_member_expression(node) {
            return Self::find_new_call(node.get("object")?);
        }
        if is_type(node, "NewExpression") {
            return Some(node);
        }

        None
    }
}

// ---------------------------------------------------------------------------
// probes/isRequire/InlinedRequire.ts
// ---------------------------------------------------------------------------

pub struct InlinedRequire;

impl InlinedRequire {
    pub fn assert_node(node: &Node) -> bool {
        static REQUIRE_PATTERN: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"(?i)^require..*$").expect("valid regex"));

        is_call_expression(node)
            && call_expression_identifier(node).is_some_and(|id| REQUIRE_PATTERN.is_match(&id))
    }

    pub fn split(node: &Node) -> Option<SplitResult> {
        if !Self::assert_node(node) {
            return None;
        }
        let require_call = Self::find_require_call(node)?;

        Some(build_split_result(node, require_call, "require"))
    }

    fn find_require_call(node: &Node) -> Option<&Node> {
        let object = if is_member_expression(node) {
            node.get("object")?
        } else {
            node.get("callee")?
        };

        if is_call_expression(object)
            && object.get("callee").is_some_and(|callee| {
                is_identifier(callee)
                    && callee.get("name").and_then(Value::as_str) == Some("require")
            })
        {
            return Some(object);
        }

        if is_member_expression(object) || is_call_expression(object) {
            return Self::find_require_call(object);
        }

        None
    }
}
