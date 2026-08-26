//! Upstream: `src/estree/types.ts`

use serde_json::Value;

use super::Node;

/// `isNode`: a JSON object with a string `type` field.
#[must_use]
pub fn is_node(value: &Value) -> bool {
    value
        .as_object()
        .is_some_and(|obj| obj.get("type").is_some_and(Value::is_string))
}

/// The `type` field of a node, when it is one.
#[must_use]
pub fn node_type(value: &Value) -> Option<&str> {
    value.as_object()?.get("type")?.as_str()
}

#[must_use]
pub fn is_type(value: &Value, ty: &str) -> bool {
    node_type(value) == Some(ty)
}

#[must_use]
pub fn is_literal(node: &Value) -> bool {
    is_type(node, "Literal")
}

#[must_use]
pub fn is_string_literal(node: &Value) -> bool {
    is_literal(node) && node.get("value").is_some_and(Value::is_string)
}

#[must_use]
pub fn is_numeric_literal(node: &Value) -> bool {
    is_literal(node) && node.get("value").is_some_and(Value::is_number)
}

#[must_use]
pub fn is_template_literal(node: &Value) -> bool {
    if !is_type(node, "TemplateLiteral") {
        return false;
    }
    let Some(first_quasi) = node.get("quasis").and_then(|q| q.get(0)) else {
        return false;
    };
    is_type(first_quasi, "TemplateElement")
        && first_quasi
            .pointer("/value/raw")
            .is_some_and(Value::is_string)
}

#[must_use]
pub fn is_function_node(node: &Value) -> bool {
    matches!(
        node_type(node),
        Some("FunctionDeclaration" | "FunctionExpression" | "ArrowFunctionExpression")
    )
}

#[must_use]
pub fn is_call_expression(node: &Value) -> bool {
    is_type(node, "CallExpression")
}

#[must_use]
pub fn is_identifier(node: &Value) -> bool {
    is_type(node, "Identifier")
}

#[must_use]
pub fn is_member_expression(node: &Value) -> bool {
    is_type(node, "MemberExpression")
}

/// The string `value` of a `Literal` node.
#[must_use]
pub fn literal_str(node: &Node) -> Option<&str> {
    if is_literal(node) {
        node.get("value")?.as_str()
    } else {
        None
    }
}

/// The `name` of an `Identifier` node.
#[must_use]
pub fn identifier_name(node: &Node) -> Option<&str> {
    if is_identifier(node) {
        node.get("name")?.as_str()
    } else {
        None
    }
}

/// Lookup used by `getCallExpressionIdentifier` and friends to resolve
/// identifiers through externally traced literal values.
/// Upstream: `externalIdentifierLookup` in `DefaultOptions` (estree/types.ts).
pub type ExternalIdentifierLookup<'a> = &'a dyn Fn(&str) -> Option<String>;

/// Upstream `noop`.
pub fn noop_lookup(_name: &str) -> Option<String> {
    None
}
