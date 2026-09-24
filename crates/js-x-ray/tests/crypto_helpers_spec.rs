//! Upstream: `test/probes/crypto/resolveDigestCall.spec.ts`,
//! `test/probes/crypto/resolveNumericValue.spec.ts`,
//! `test/probes/crypto/resolveStringValue.spec.ts`

use indexmap::IndexMap;
use serde_json::Value;

use js_x_ray_rs::parser::{JsSourceParser, SourceParser};
use js_x_ray_rs::probes::crypto::{
    resolve_digest_call, resolve_numeric_value, resolve_string_value,
};
use js_x_ray_rs::variable_tracer::LiteralIdentifier;

/// Upstream `parseScript` + `[astNode] = ...body` — parses and returns the
/// first top-level statement.
fn parse_first(code: &str) -> Value {
    JsSourceParser
        .parse(code)
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
}

/// Upstream `getExpressionFromStatementIf`: unwraps an `ExpressionStatement`,
/// or returns the node itself otherwise.
fn expression_from_statement_if(node: &Value) -> Value {
    if node["type"] == "ExpressionStatement" {
        node["expression"].clone()
    } else {
        node.clone()
    }
}

fn get_expression(code: &str) -> Value {
    expression_from_statement_if(&parse_first(code))
}

fn literal(value: &str, r#type: &'static str) -> LiteralIdentifier {
    LiteralIdentifier {
        value: value.to_owned(),
        r#type,
    }
}

// ---------------------------------------------------------------------------
// Upstream: test/probes/crypto/resolveDigestCall.spec.ts
// ---------------------------------------------------------------------------

#[test]
fn digest_call_returns_none_when_node_is_not_a_digest_call() {
    let node = get_expression("hash.update('data')");

    assert!(resolve_digest_call(Some(&node)).is_none());
}

#[test]
fn digest_call_resolves_a_direct_digest_encoding_call() {
    let node = get_expression("hash.digest('hex')");

    let result = resolve_digest_call(Some(&node)).expect("should resolve");

    assert_eq!(result.len(), 1);
    assert_eq!(result[0]["value"], "hex");
}

#[test]
fn digest_call_resolves_digest_tostring_encoding_using_tostring_arguments_when_digest_has_none() {
    let node = get_expression("hash.digest().toString('base64')");

    let result = resolve_digest_call(Some(&node)).expect("should resolve");

    assert_eq!(result.len(), 1);
    assert_eq!(result[0]["value"], "base64");
}

#[test]
fn digest_call_resolves_digest_encoding_tostring_preferring_digests_own_arguments() {
    let node = get_expression("hash.digest('hex').toString()");

    let result = resolve_digest_call(Some(&node)).expect("should resolve");

    assert_eq!(result.len(), 1);
    assert_eq!(result[0]["value"], "hex");
}

#[test]
fn digest_call_returns_none_for_a_tostring_call_not_wrapping_a_digest_call() {
    let node = get_expression("hash.foo().toString('hex')");

    assert!(resolve_digest_call(Some(&node)).is_none());
}

#[test]
fn digest_call_returns_none_for_none() {
    assert!(resolve_digest_call(None).is_none());
}

// ---------------------------------------------------------------------------
// Upstream: test/probes/crypto/resolveNumericValue.spec.ts
// ---------------------------------------------------------------------------

#[test]
fn numeric_value_returns_the_value_of_a_numeric_literal_node() {
    let node = get_expression("42");

    assert_eq!(
        resolve_numeric_value(Some(&node), &IndexMap::new()),
        Some(42.0)
    );
}

#[test]
fn numeric_value_resolves_an_identifier_tracked_back_to_a_stringified_numeric_literal() {
    let node = get_expression("rounds");
    let literal_identifiers: IndexMap<String, LiteralIdentifier> =
        IndexMap::from([("rounds".to_owned(), literal("10", "Literal"))]);

    assert_eq!(
        resolve_numeric_value(Some(&node), &literal_identifiers),
        Some(10.0)
    );
}

#[test]
fn numeric_value_returns_none_when_the_tracked_identifier_does_not_resolve_to_a_number() {
    let node = get_expression("salt");
    let literal_identifiers: IndexMap<String, LiteralIdentifier> =
        IndexMap::from([("salt".to_owned(), literal("not-a-number", "Literal"))]);

    assert_eq!(
        resolve_numeric_value(Some(&node), &literal_identifiers),
        None
    );
}

#[test]
fn numeric_value_returns_none_for_an_identifier_that_is_not_tracked() {
    let node = get_expression("rounds");

    assert_eq!(resolve_numeric_value(Some(&node), &IndexMap::new()), None);
}

#[test]
fn numeric_value_returns_none_for_a_node_that_is_neither_a_literal_nor_an_identifier() {
    let node = get_expression("foo()");

    assert_eq!(resolve_numeric_value(Some(&node), &IndexMap::new()), None);
}

#[test]
fn numeric_value_returns_none_for_a_string_literal() {
    let node = get_expression("'10'");

    assert_eq!(resolve_numeric_value(Some(&node), &IndexMap::new()), None);
}

#[test]
fn numeric_value_resolves_an_identifier_tracked_to_a_stringified_zero_as_zero_not_none() {
    let node = get_expression("rounds");
    let literal_identifiers: IndexMap<String, LiteralIdentifier> =
        IndexMap::from([("rounds".to_owned(), literal("0", "Literal"))]);

    assert_eq!(
        resolve_numeric_value(Some(&node), &literal_identifiers),
        Some(0.0)
    );
}

#[test]
fn numeric_value_returns_the_value_of_a_direct_numeric_literal_zero() {
    let node = get_expression("0");

    assert_eq!(
        resolve_numeric_value(Some(&node), &IndexMap::new()),
        Some(0.0)
    );
}

#[test]
fn numeric_value_known_limitation_a_negative_literal_is_a_unary_expression_so_it_resolves_to_none()
{
    let node = get_expression("-10");

    assert_eq!(node["type"], "UnaryExpression");
    assert_eq!(resolve_numeric_value(Some(&node), &IndexMap::new()), None);
}

// ---------------------------------------------------------------------------
// Upstream: test/probes/crypto/resolveStringValue.spec.ts
// ---------------------------------------------------------------------------

#[test]
fn string_value_returns_the_value_of_a_string_literal_node() {
    let node = get_expression("'hello'");

    assert_eq!(
        resolve_string_value(Some(&node), &IndexMap::new()),
        Some("hello")
    );
}

#[test]
fn string_value_resolves_an_identifier_tracked_back_to_a_literal_assignment() {
    let node = get_expression("foo");
    let literal_identifiers: IndexMap<String, LiteralIdentifier> =
        IndexMap::from([("foo".to_owned(), literal("bar", "Literal"))]);

    assert_eq!(
        resolve_string_value(Some(&node), &literal_identifiers),
        Some("bar")
    );
}

#[test]
fn string_value_returns_none_for_an_identifier_that_is_not_tracked() {
    let node = get_expression("foo");

    assert_eq!(resolve_string_value(Some(&node), &IndexMap::new()), None);
}

#[test]
fn string_value_returns_none_for_a_node_that_is_neither_a_literal_nor_an_identifier() {
    let node = get_expression("foo()");

    assert_eq!(resolve_string_value(Some(&node), &IndexMap::new()), None);
}

#[test]
fn string_value_returns_none_for_a_numeric_literal() {
    let node = get_expression("42");

    assert_eq!(resolve_string_value(Some(&node), &IndexMap::new()), None);
}

#[test]
fn string_value_returns_the_value_of_a_direct_empty_string_literal() {
    let node = get_expression("''");

    assert_eq!(
        resolve_string_value(Some(&node), &IndexMap::new()),
        Some("")
    );
}

#[test]
fn string_value_resolves_an_identifier_tracked_to_an_empty_string_not_none() {
    let node = get_expression("foo");
    let literal_identifiers: IndexMap<String, LiteralIdentifier> =
        IndexMap::from([("foo".to_owned(), literal("", "Literal"))]);

    assert_eq!(
        resolve_string_value(Some(&node), &literal_identifiers),
        Some("")
    );
}

#[test]
fn string_value_returns_none_for_an_identifier_tracked_back_to_a_template_literal() {
    let node = get_expression("foo");
    let literal_identifiers: IndexMap<String, LiteralIdentifier> =
        IndexMap::from([("foo".to_owned(), literal("${0}", "TemplateLiteral"))]);

    assert_eq!(
        resolve_string_value(Some(&node), &literal_identifiers),
        None
    );
}
