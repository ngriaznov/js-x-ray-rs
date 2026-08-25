//! Upstream: `src/estree/functions/*.ts` — one section per upstream file to
//! keep diffs against the Node.js implementation easy to review.

use serde_json::Value;

use super::literal::js_string;
use super::types::{
    is_call_expression, is_identifier, is_member_expression, is_node, is_string_literal, is_type,
    node_type,
};
use crate::utils::hex;

/// Identifier lookup shared by most helpers.
/// Upstream: `DefaultOptions.externalIdentifierLookup`.
pub type Lookup<'a> = &'a dyn Fn(&str) -> Option<String>;

pub fn noop(_: &str) -> Option<String> {
    None
}

// ---------------------------------------------------------------------------
// getCallExpressionIdentifier.ts
// ---------------------------------------------------------------------------

pub struct GetCallExpressionIdentifierOptions<'a> {
    pub external_identifier_lookup: Lookup<'a>,
    /// Resolve the callee if it is itself a CallExpression (`require('x')()`).
    pub resolve_call_expression: bool,
}

impl Default for GetCallExpressionIdentifierOptions<'_> {
    fn default() -> Self {
        Self {
            external_identifier_lookup: &noop,
            resolve_call_expression: true,
        }
    }
}

pub fn get_call_expression_identifier(
    node: &Value,
    options: &GetCallExpressionIdentifierOptions<'_>,
) -> Option<String> {
    if !matches!(node_type(node), Some("CallExpression" | "NewExpression")) {
        return None;
    }
    let callee = node.get("callee")?;

    if is_identifier(callee) {
        return callee.get("name")?.as_str().map(str::to_owned);
    }
    if is_member_expression(callee) {
        let member_object = callee.get("object")?;
        let mut last_id = String::new();
        for part in get_member_expression_identifier(callee, options.external_identifier_lookup) {
            if last_id.is_empty() {
                last_id = part;
            } else {
                last_id = format!("{last_id}.{part}");
            }
        }

        if options.resolve_call_expression && is_call_expression(member_object) {
            // Upstream concatenates even when the inner lookup returns null,
            // producing e.g. "null.foo" — keep that behavior.
            let inner = get_call_expression_identifier(
                member_object,
                &GetCallExpressionIdentifierOptions::default(),
            );
            return Some(format!("{}.{last_id}", inner.as_deref().unwrap_or("null")));
        }

        return Some(last_id);
    }

    if options.resolve_call_expression {
        get_call_expression_identifier(
            callee,
            &GetCallExpressionIdentifierOptions {
                external_identifier_lookup: options.external_identifier_lookup,
                resolve_call_expression: true,
            },
        )
    } else {
        None
    }
}

/// Convenience wrapper with default options.
pub fn call_expression_identifier(node: &Value) -> Option<String> {
    get_call_expression_identifier(node, &GetCallExpressionIdentifierOptions::default())
}

// ---------------------------------------------------------------------------
// getMemberExpressionIdentifier.ts
// ---------------------------------------------------------------------------

/// Return the complete identifier parts of a MemberExpression.
pub fn get_member_expression_identifier(node: &Value, lookup: Lookup<'_>) -> Vec<String> {
    let mut parts = Vec::new();
    collect_member_expression_identifier(node, lookup, &mut parts);
    parts
}

fn collect_member_expression_identifier(node: &Value, lookup: Lookup<'_>, out: &mut Vec<String>) {
    let Some(object) = node.get("object") else {
        return;
    };
    match node_type(object) {
        Some("MemberExpression") => collect_member_expression_identifier(object, lookup, out),
        Some("Identifier") => {
            if let Some(name) = object.get("name").and_then(Value::as_str) {
                out.push(name.to_owned());
            }
        }
        // Literal is used when the property is computed
        Some("Literal") => {
            if let Some(value) = object.get("value").and_then(Value::as_str) {
                out.push(value.to_owned());
            }
        }
        _ => {}
    }

    let Some(property) = node.get("property") else {
        return;
    };
    match node_type(property) {
        Some("Identifier") => {
            let name = property.get("name").and_then(Value::as_str).unwrap_or("");
            match lookup(name) {
                Some(identifier_value) => out.push(identifier_value),
                None => out.push(name.to_owned()),
            }
        }
        // Literal is used when the property is computed
        Some("Literal") => {
            if let Some(value) = property.get("value").and_then(Value::as_str) {
                out.push(value.to_owned());
            }
        }
        // foo.bar[callexpr()]
        Some("CallExpression") => {
            let args = property.get("arguments").and_then(Value::as_array);
            if let Some(args) = args
                && let Some(first) = args.first()
                && is_type(first, "Literal")
                && let Some(value) = first.get("value").and_then(Value::as_str)
                && hex::is_hex_str(value)
            {
                out.push(hex::decode_hex_lossy(value));
            }
        }
        // foo.bar["k" + "e" + "y"]
        Some("BinaryExpression") => {
            let literal: String = concat_binary_expression_parts(property, lookup, false)
                .unwrap_or_default()
                .concat();
            if !literal.trim().is_empty() {
                out.push(literal);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// concatBinaryExpression.ts
// ---------------------------------------------------------------------------

const BINARY_EXPR_TYPES: &[&str] = &[
    "Literal",
    "BinaryExpression",
    "ArrayExpression",
    "Identifier",
];

/// Upstream `concatBinaryExpression`. Returns `None` when
/// `stop_on_unsupported_node` is set and an unsupported node is found
/// (upstream throws).
pub fn concat_binary_expression_parts(
    node: &Value,
    lookup: Lookup<'_>,
    stop_on_unsupported_node: bool,
) -> Option<Vec<String>> {
    let mut parts = Vec::new();
    concat_binary_inner(node, lookup, stop_on_unsupported_node, &mut parts)?;
    Some(parts)
}

fn concat_binary_inner(
    node: &Value,
    lookup: Lookup<'_>,
    stop_on_unsupported_node: bool,
    out: &mut Vec<String>,
) -> Option<()> {
    let left = node.get("left")?;
    let right = node.get("right")?;

    if stop_on_unsupported_node {
        for child in [left, right] {
            let ty = node_type(child).unwrap_or("");
            if !BINARY_EXPR_TYPES.contains(&ty) {
                return None;
            }
        }
    }

    for child_node in [left, right] {
        match node_type(child_node) {
            Some("BinaryExpression") => {
                concat_binary_inner(child_node, lookup, stop_on_unsupported_node, out)?;
            }
            Some("ArrayExpression") => {
                out.extend(array_expression_to_string_with(child_node, lookup, true));
            }
            Some("Literal") => {
                if let Some(value) = child_node.get("value").and_then(Value::as_str) {
                    out.push(value.to_owned());
                }
            }
            Some("Identifier") => {
                let name = child_node.get("name").and_then(Value::as_str).unwrap_or("");
                // Upstream uses a truthiness check: empty strings are skipped.
                if let Some(identifier) = lookup(name)
                    && !identifier.is_empty()
                {
                    out.push(identifier);
                }
            }
            _ => {}
        }
    }
    Some(())
}

// ---------------------------------------------------------------------------
// toLiteral.ts
// ---------------------------------------------------------------------------

/// Upstream `toLiteral`: flatten a TemplateLiteral into a single string where
/// each interpolation is replaced by `${i}`.
pub fn to_literal(template_literal: &Value) -> String {
    let Some(quasis) = template_literal.get("quasis").and_then(Value::as_array) else {
        return String::new();
    };
    quasis
        .iter()
        .enumerate()
        .map(|(i, quasi)| {
            let raw = quasi
                .pointer("/value/raw")
                .and_then(Value::as_str)
                .unwrap_or("");
            let tail = quasi.get("tail").and_then(Value::as_bool).unwrap_or(false);
            if tail {
                raw.to_owned()
            } else {
                format!("{raw}${{{i}}}")
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// getCallExpressionArguments.ts
// ---------------------------------------------------------------------------

pub fn get_call_expression_arguments(node: &Value, lookup: Lookup<'_>) -> Option<Vec<String>> {
    if !is_call_expression(node) {
        return None;
    }
    let args = node.get("arguments")?.as_array()?;
    if args.is_empty() {
        return None;
    }

    let mut literals_node: Vec<String> = Vec::new();
    for arg in args {
        match node_type(arg) {
            Some("Identifier") => {
                let name = arg.get("name").and_then(Value::as_str).unwrap_or("");
                if let Some(identifier_value) = lookup(name) {
                    literals_node.push(identifier_value);
                }
            }
            Some("Literal") => {
                if let Some(value) = arg.get("value").and_then(Value::as_str) {
                    literals_node.push(hex_to_string(value));
                }
            }
            Some("TemplateLiteral") => {
                literals_node.push(to_literal(arg));
            }
            Some("BinaryExpression") => {
                let concatenated = concat_binary_expression_parts(arg, lookup, false)
                    .unwrap_or_default()
                    .concat();
                if !concatenated.is_empty() {
                    literals_node.push(concatenated);
                }
            }
            _ => {}
        }
    }

    if literals_node.is_empty() {
        None
    } else {
        Some(literals_node)
    }
}

fn hex_to_string(value: &str) -> String {
    if hex::is_hex_str(value) {
        hex::decode_hex_lossy(value)
    } else {
        value.to_owned()
    }
}

// ---------------------------------------------------------------------------
// arrayExpression.ts
// ---------------------------------------------------------------------------

/// Upstream `arrayExpressionToString`.
pub fn array_expression_to_string(node: &Value, lookup: Lookup<'_>) -> Vec<String> {
    array_expression_to_string_with(node, lookup, true)
}

pub fn array_expression_to_string_with(
    node: &Value,
    lookup: Lookup<'_>,
    resolve_char_code: bool,
) -> Vec<String> {
    let mut out = Vec::new();
    if !is_node(node) || !is_type(node, "ArrayExpression") {
        return out;
    }
    let Some(elements) = node.get("elements").and_then(Value::as_array) else {
        return out;
    };

    for row in elements {
        if row.is_null() {
            continue;
        }
        match node_type(row) {
            Some("Literal") => {
                let value = row.get("value").unwrap_or(&Value::Null);
                if value.as_str() == Some("") {
                    continue;
                }
                if resolve_char_code {
                    // `Number(row.value)` then `String.fromCharCode` when numeric.
                    let numeric = match value {
                        Value::Number(n) => n.as_f64(),
                        Value::String(s) => {
                            let t = s.trim();
                            if t.is_empty() {
                                Some(0.0)
                            } else {
                                t.parse::<f64>().ok()
                            }
                        }
                        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
                        Value::Null => Some(0.0),
                        _ => None,
                    };
                    match numeric {
                        Some(code) => {
                            // String.fromCharCode with a single UTF-16 unit;
                            // an unpaired surrogate becomes U+FFFD in Rust.
                            let unit = (code as i64).rem_euclid(65536) as u16;
                            let s: String = char::decode_utf16([unit])
                                .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
                                .collect();
                            out.push(s);
                        }
                        None => out.push(js_string(value)),
                    }
                } else {
                    out.push(js_string(value));
                }
            }
            Some("Identifier") => {
                let name = row.get("name").and_then(Value::as_str).unwrap_or("");
                if let Some(identifier) = lookup(name) {
                    out.push(identifier);
                }
            }
            Some("CallExpression") => {
                if let Some(value) = join_array_expression(row, lookup) {
                    out.push(value);
                }
            }
            _ => {}
        }
    }
    out
}

/// Upstream `joinArrayExpression`: resolves `[...].join("sep")`.
pub fn join_array_expression(node: &Value, lookup: Lookup<'_>) -> Option<String> {
    if !is_call_expression(node) {
        return None;
    }
    let args = node.get("arguments")?.as_array()?;
    let callee = node.get("callee")?;
    if args.len() != 1
        || !is_type(callee, "MemberExpression")
        || !is_type(callee.get("object")?, "ArrayExpression")
    {
        return None;
    }

    let id = get_member_expression_identifier(callee, &noop).join(".");
    if id != "join" || !is_string_literal(&args[0]) {
        return None;
    }
    let separator = args[0].get("value")?.as_str()?;

    let parts = array_expression_to_string_with(callee.get("object")?, lookup, false);
    if parts.is_empty() {
        return Some(String::new());
    }
    Some(parts.join(separator))
}

// ---------------------------------------------------------------------------
// extractLogicalExpression.ts
// ---------------------------------------------------------------------------

/// Flatten a LogicalExpression into `(operator, node)` leaves.
pub fn extract_logical_expression(node: &Value) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    extract_logical_inner(node, &mut out);
    out
}

fn extract_logical_inner(node: &Value, out: &mut Vec<(String, Value)>) {
    if !is_type(node, "LogicalExpression") {
        return;
    }
    let operator = node
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    for side in ["left", "right"] {
        let Some(child) = node.get(side) else {
            continue;
        };
        if is_type(child, "LogicalExpression") {
            extract_logical_inner(child, out);
        } else {
            out.push((operator.clone(), child.clone()));
        }
    }
}

// ---------------------------------------------------------------------------
// getMemberCallExpression.ts
// ---------------------------------------------------------------------------

/// Return the node when it is a `<expr>.<methodName>(...)` call.
pub fn get_member_call_expression<'a>(node: &'a Value, method_name: &str) -> Option<&'a Value> {
    if is_call_expression(node) {
        let callee = node.get("callee")?;
        if is_member_expression(callee)
            && callee.get("computed").and_then(Value::as_bool) != Some(true)
            && is_identifier(callee.get("property")?)
            && callee.pointer("/property/name").and_then(Value::as_str) == Some(method_name)
        {
            return Some(node);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// getParamNames.ts
// ---------------------------------------------------------------------------

pub fn get_param_names(params: &[Value]) -> Vec<String> {
    let mut names = Vec::new();
    for param in params {
        for (_, assignment_id) in get_variable_declaration_identifiers(param, None) {
            if let Some(name) = assignment_id.get("name").and_then(Value::as_str) {
                names.push(name.to_owned());
            }
        }
    }
    names
}

// ---------------------------------------------------------------------------
// getVariableDeclarationIdentifiers.ts
// ---------------------------------------------------------------------------

/// Yields `(name, assignmentId)` pairs; `assignmentId` is a cloned Identifier
/// node.
pub fn get_variable_declaration_identifiers(
    node: &Value,
    prefix: Option<&str>,
) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    collect_variable_declaration_identifiers(node, prefix, &mut out);
    out
}

fn collect_variable_declaration_identifiers(
    node: &Value,
    prefix: Option<&str>,
    out: &mut Vec<(String, Value)>,
) {
    match node_type(node) {
        Some("VariableDeclaration") => {
            for declarator in node
                .get("declarations")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                collect_variable_declaration_identifiers(declarator, prefix, out);
            }
        }
        Some("VariableDeclarator") => {
            if let Some(id) = node.get("id") {
                collect_variable_declaration_identifiers(id, prefix, out);
            }
            if let Some(init) = node.get("init")
                && !init.is_null()
            {
                collect_variable_declaration_identifiers(init, prefix, out);
            }
        }
        Some("Identifier") => {
            let name = node.get("name").and_then(Value::as_str).unwrap_or("");
            out.push((auto_prefix(name, prefix), node.clone()));
        }
        Some("Property") => {
            if node.get("kind").and_then(Value::as_str) != Some("init")
                || !is_type(node.get("key").unwrap_or(&Value::Null), "Identifier")
            {
                return;
            }
            let key = &node["key"];
            let key_name = key.get("name").and_then(Value::as_str).unwrap_or("");
            let value = node.get("value").unwrap_or(&Value::Null);

            if matches!(node_type(value), Some("ObjectPattern" | "ArrayPattern")) {
                let new_prefix = auto_prefix(key_name, prefix);
                collect_variable_declaration_identifiers(value, Some(&new_prefix), out);
                return;
            }

            let assignment_id = if is_identifier(value) {
                value
            } else if is_type(value, "AssignmentPattern")
                && is_identifier(value.get("left").unwrap_or(&Value::Null))
            {
                &value["left"]
            } else {
                key
            };

            out.push((auto_prefix(key_name, prefix), assignment_id.clone()));
        }
        Some("RestElement") => {
            if let Some(argument) = node.get("argument")
                && is_identifier(argument)
            {
                let name = argument.get("name").and_then(Value::as_str).unwrap_or("");
                out.push((auto_prefix(name, prefix), argument.clone()));
            }
        }
        Some("ObjectExpression") => {
            for property in node
                .get("properties")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                collect_variable_declaration_identifiers(property, prefix, out);
            }
        }
        Some("SequenceExpression") => {
            for expr in node
                .get("expressions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                collect_variable_declaration_identifiers(expr, prefix, out);
            }
        }
        Some("AssignmentExpression") => {
            if let Some(left) = node.get("left") {
                collect_variable_declaration_identifiers(left, prefix, out);
            }
        }
        Some("AssignmentPattern") => {
            let left = node.get("left").unwrap_or(&Value::Null);
            if is_identifier(left) {
                let name = left.get("name").and_then(Value::as_str).unwrap_or("");
                out.push((name.to_owned(), left.clone()));
            } else {
                collect_variable_declaration_identifiers(left, prefix, out);
            }
        }
        Some("ArrayPattern") => {
            for element in node
                .get("elements")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if !element.is_null() {
                    collect_variable_declaration_identifiers(element, prefix, out);
                }
            }
        }
        Some("ObjectPattern") => {
            for property in node
                .get("properties")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if !property.is_null() {
                    collect_variable_declaration_identifiers(property, prefix, out);
                }
            }
        }
        _ => {}
    }
}

fn auto_prefix(name: &str, prefix: Option<&str>) -> String {
    match prefix {
        Some(prefix) => format!("{prefix}.{name}"),
        None => name.to_owned(),
    }
}
