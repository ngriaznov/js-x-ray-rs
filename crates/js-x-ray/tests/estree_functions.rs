//! Upstream: `test/estree/arrayExpression.spec.ts`,
//! `test/estree/concatBinaryExpression.spec.ts`,
//! `test/estree/extractLogicalExpression.spec.ts`,
//! `test/estree/getCallExpressionArguments.spec.ts`,
//! `test/estree/getCallExpressionIdentifier.spec.ts`,
//! `test/estree/getMemberCallExpression.spec.ts`,
//! `test/estree/getMemberExpressionIdentifier.spec.ts`,
//! `test/estree/getVariableDeclarationIdentifiers.spec.ts`,
//! `test/estree/toLiteral.spec.ts`

use js_x_ray_rs::estree::{
    GetCallExpressionIdentifierOptions, array_expression_to_string, concat_binary_expression_parts,
    extract_logical_expression, get_call_expression_arguments, get_call_expression_identifier,
    get_member_call_expression, get_member_expression_identifier,
    get_variable_declaration_identifiers, join_array_expression, noop, to_literal,
};
use js_x_ray_rs::parser::{JsSourceParser, SourceParser};
use serde_json::{Value, json};

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

/// Upstream `getExpressionFromStatement`: unwraps an `ExpressionStatement`,
/// or returns `null` for any other node type.
fn expression_from_statement(node: &Value) -> Value {
    if node["type"] == "ExpressionStatement" {
        node["expression"].clone()
    } else {
        Value::Null
    }
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

fn lookup_from(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: std::collections::HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    move |name: &str| map.get(name).cloned()
}

// ---------------------------------------------------------------------------
// estree.arrayExpressionToString
// ---------------------------------------------------------------------------

#[test]
fn array_expression_to_string_two_literals_returns_them_one_by_one() {
    let ast_node = parse_first("['foo', 'bar']");
    let result = array_expression_to_string(&expression_from_statement(&ast_node), &noop);

    assert_eq!(result, vec!["foo", "bar"]);
}

#[test]
fn array_expression_to_string_two_identifiers_returns_tracer_values() {
    let lookup = lookup_from(&[("foo", "1"), ("bar", "2")]);

    let ast_node = parse_first("[foo, bar]");
    let result = array_expression_to_string(&expression_from_statement(&ast_node), &lookup);

    assert_eq!(result, vec!["1", "2"]);
}

#[test]
fn array_expression_to_string_two_numbers_converts_to_char_code() {
    let ast_node = parse_first("[65, 66]");
    let result = array_expression_to_string(&expression_from_statement(&ast_node), &noop);

    assert_eq!(result, vec!["A", "B"]);
}

#[test]
fn array_expression_to_string_empty_literals_returns_no_values() {
    let ast_node = parse_first("['', '']");
    let result = array_expression_to_string(&expression_from_statement(&ast_node), &noop);

    assert!(result.is_empty());
}

#[test]
fn array_expression_to_string_non_array_expression_returns_immediately() {
    let ast_node = parse_first("const foo = 5;");
    let result = array_expression_to_string(&ast_node, &noop);

    assert!(result.is_empty());
}

// ---------------------------------------------------------------------------
// joinArrayExpression
// ---------------------------------------------------------------------------

#[test]
fn join_array_expression_returns_none_when_node_is_not_a_call_expression() {
    let ast = parse_first("const a = 1;");
    assert_eq!(
        join_array_expression(&expression_from_statement_if(&ast), &noop),
        None
    );
}

#[test]
fn join_array_expression_should_combine_and_return_the_ip() {
    let ast = parse_first(r#"["127","0","0","1"].join(".");"#);
    assert_eq!(
        join_array_expression(&expression_from_statement_if(&ast), &noop),
        Some("127.0.0.1".to_owned())
    );
}

#[test]
fn join_array_expression_should_combine_multiple_depth_of_joined_array_expression() {
    let ast = parse_first(
        r#"[
      ["hello", "world"].join(" "),
      "0",
      "0",
      "1"
    ].join(".");"#,
    );
    assert_eq!(
        join_array_expression(&expression_from_statement_if(&ast), &noop),
        Some("hello world.0.0.1".to_owned())
    );
}

#[test]
fn join_array_expression_should_look_for_external_identifiers() {
    let lookup = lookup_from(&[("a", "1"), ("b", "2")]);

    let ast = parse_first("[a, b].join('.');");
    assert_eq!(
        join_array_expression(&expression_from_statement_if(&ast), &lookup),
        Some("1.2".to_owned())
    );
}

// ---------------------------------------------------------------------------
// estree.concatBinaryExpression
// ---------------------------------------------------------------------------

#[test]
fn concat_binary_expression_two_literals_returns_literal_values() {
    let ast_node = parse_first("'foo' + 'bar' + 'xd'");
    let result =
        concat_binary_expression_parts(&expression_from_statement(&ast_node), &noop, false)
            .unwrap();

    assert_eq!(result, vec!["foo", "bar", "xd"]);
}

#[test]
fn concat_binary_expression_two_array_expressions_returns_array_values_as_string() {
    let ast_node = parse_first("['A'] + ['B']");
    let result =
        concat_binary_expression_parts(&expression_from_statement(&ast_node), &noop, false)
            .unwrap();

    assert_eq!(result, vec!["A", "B"]);
}

#[test]
fn concat_binary_expression_two_identifiers_returns_tracer_values() {
    let lookup = lookup_from(&[("foo", "A"), ("bar", "B")]);

    let ast_node = parse_first("foo + bar");
    let result =
        concat_binary_expression_parts(&expression_from_statement(&ast_node), &lookup, false)
            .unwrap();

    assert_eq!(result, vec!["A", "B"]);
}

// Adaptation: upstream throws `Error("concatBinaryExpression:: Unsupported
// node detected")` on the first `iter.next()`; the Rust port has no
// exceptions to raise from an iterator, so `stop_on_unsupported_node` makes
// the function return `None` instead.
#[test]
fn concat_binary_expression_one_level_unsupported_node_returns_none() {
    let ast_node = parse_first("evil() + 's'");
    let result = concat_binary_expression_parts(&expression_from_statement(&ast_node), &noop, true);

    assert_eq!(result, None);
}

#[test]
fn concat_binary_expression_deep_unsupported_node_returns_none() {
    let ast_node = parse_first("'a' + evil() + 's'");
    let result = concat_binary_expression_parts(&expression_from_statement(&ast_node), &noop, true);

    assert_eq!(result, None);
}

// ---------------------------------------------------------------------------
// estree.extractLogicalExpression
// ---------------------------------------------------------------------------

#[test]
fn extract_logical_expression_extracts_two_nodes_with_two_operands() {
    let ast_node = parse_first("5 || 10");
    let result = extract_logical_expression(&expression_from_statement(&ast_node));

    assert_eq!(result.len(), 2);

    assert_eq!(result[0].0, "||");
    assert_eq!(result[0].1["type"], "Literal");
    assert_eq!(result[0].1["value"], 5);

    assert_eq!(result[1].0, "||");
    assert_eq!(result[1].1["type"], "Literal");
    assert_eq!(result[1].1["value"], 10);
}

#[test]
fn extract_logical_expression_extracts_all_nodes_and_adds_up_all_literal_values() {
    let ast_node = parse_first("5 || 10 || 15 || 20");
    let result = extract_logical_expression(&expression_from_statement(&ast_node));

    let total: f64 = result
        .iter()
        .map(|(_, node)| {
            if node["type"] == "Literal" && node["value"].is_number() {
                node["value"].as_f64().unwrap()
            } else {
                0.0
            }
        })
        .sum();

    assert_eq!(total, 50.0);
}

#[test]
fn extract_logical_expression_extracts_nodes_with_different_operators() {
    let ast_node = parse_first("5 || 10 && 55");
    let result = extract_logical_expression(&expression_from_statement(&ast_node));

    let mut operators: Vec<String> = Vec::new();
    for (operator, _) in &result {
        if !operators.contains(operator) {
            operators.push(operator.clone());
        }
    }
    assert_eq!(operators, vec!["||", "&&"]);
}

// ---------------------------------------------------------------------------
// estree.getCallExpressionArguments
// ---------------------------------------------------------------------------

#[test]
fn get_call_expression_arguments_returns_none_when_node_is_not_a_call_expression() {
    let ast_node = parse_first("const a = 1;");
    assert_eq!(get_call_expression_arguments(&ast_node, &noop), None);
}

#[test]
fn get_call_expression_arguments_returns_first_literal_node_of_eval_call_expression() {
    let ast_node = parse_first("eval(\"this\");");
    let args = get_call_expression_arguments(&expression_from_statement(&ast_node), &noop);

    assert_eq!(args, Some(vec!["this".to_owned()]));
}

#[test]
fn get_call_expression_arguments_returns_all_literal_nodes_from_the_call_expression() {
    let ast_node = parse_first("eval('1', foo(), '2', 10);");
    let args = get_call_expression_arguments(&expression_from_statement(&ast_node), &noop);

    assert_eq!(args, Some(vec!["1".to_owned(), "2".to_owned()]));
}

#[test]
fn get_call_expression_arguments_resolves_binary_expression() {
    let ast_node = parse_first("foo('1' + '2');");
    let args = get_call_expression_arguments(&expression_from_statement(&ast_node), &noop);

    assert_eq!(args, Some(vec!["12".to_owned()]));
}

#[test]
fn get_call_expression_arguments_resolves_identifier_using_external_lookup() {
    let lookup = lookup_from(&[("myVar", "hello world")]);

    let ast_node = parse_first("foo(myVar);");
    let args = get_call_expression_arguments(&expression_from_statement(&ast_node), &lookup);

    assert_eq!(args, Some(vec!["hello world".to_owned()]));
}

#[test]
fn get_call_expression_arguments_resolves_template_literal() {
    let ast_node = parse_first("foo(`hello ${name}`);");
    let args = get_call_expression_arguments(&expression_from_statement(&ast_node), &noop);

    assert_eq!(args, Some(vec!["hello ${0}".to_owned()]));
}

// ---------------------------------------------------------------------------
// estree.getCallExpressionIdentifier
// ---------------------------------------------------------------------------

#[test]
fn get_call_expression_identifier_eval_returns_eval() {
    let ast_node = parse_first("eval(\"this\");");
    let identifier = get_call_expression_identifier(
        &expression_from_statement(&ast_node),
        &GetCallExpressionIdentifierOptions::default(),
    );

    assert_eq!(identifier.as_deref(), Some("eval"));
}

#[test]
fn get_call_expression_identifier_double_call_expression_returns_function_literal_identifier() {
    let ast_node = parse_first("Function(\"return this\")();");
    let identifier = get_call_expression_identifier(
        &expression_from_statement(&ast_node),
        &GetCallExpressionIdentifierOptions::default(),
    );

    assert_eq!(identifier.as_deref(), Some("Function"));
}

#[test]
fn get_call_expression_identifier_double_call_expression_with_resolve_disabled_returns_none() {
    let ast_node = parse_first("Function(\"return this\")();");
    let identifier = get_call_expression_identifier(
        &expression_from_statement(&ast_node),
        &GetCallExpressionIdentifierOptions {
            external_identifier_lookup: &noop,
            resolve_call_expression: false,
        },
    );

    assert_eq!(identifier, None);
}

#[test]
fn get_call_expression_identifier_new_class_expression() {
    let ast_node = parse_first("new Foo('something');");
    let identifier = get_call_expression_identifier(
        &expression_from_statement(&ast_node),
        &GetCallExpressionIdentifierOptions::default(),
    );

    assert_eq!(identifier.as_deref(), Some("Foo"));
}

#[test]
fn get_call_expression_identifier_assignment_expression_returns_none() {
    let ast_node = parse_first("foo = 10;");
    let identifier = get_call_expression_identifier(
        &expression_from_statement(&ast_node),
        &GetCallExpressionIdentifierOptions::default(),
    );

    assert_eq!(identifier, None);
}

#[test]
fn get_call_expression_identifier_require_iife_with_resolve_enabled_returns_require() {
    let ast_node = parse_first("require('foo')();");
    let identifier = get_call_expression_identifier(
        &expression_from_statement(&ast_node),
        &GetCallExpressionIdentifierOptions {
            external_identifier_lookup: &noop,
            resolve_call_expression: true,
        },
    );

    assert_eq!(identifier.as_deref(), Some("require"));
}

#[test]
fn get_call_expression_identifier_require_iife_with_resolve_disabled_returns_none() {
    let ast_node = parse_first("require('foo')();");
    let identifier = get_call_expression_identifier(
        &expression_from_statement(&ast_node),
        &GetCallExpressionIdentifierOptions {
            external_identifier_lookup: &noop,
            resolve_call_expression: false,
        },
    );

    assert_eq!(identifier, None);
}

#[test]
fn get_call_expression_identifier_member_expression_then_call_returns_full_path() {
    let ast_node = parse_first("foo.bar().yo();");
    let identifier = get_call_expression_identifier(
        &expression_from_statement(&ast_node),
        &GetCallExpressionIdentifierOptions {
            external_identifier_lookup: &noop,
            resolve_call_expression: true,
        },
    );

    assert_eq!(identifier.as_deref(), Some("foo.bar.yo"));
}

// ---------------------------------------------------------------------------
// estree.getMemberCallExpression
// ---------------------------------------------------------------------------

// Adaptation: JS distinguishes `null` and `undefined`; ESTree JSON built from
// serde_json has only `Value::Null` for both, so a single case covers it.
#[test]
fn get_member_call_expression_returns_none_for_null() {
    assert_eq!(get_member_call_expression(&Value::Null, "digest"), None);
}

#[test]
fn get_member_call_expression_returns_none_when_node_is_not_a_call_expression() {
    let ast_node = parse_first("foo.bar");
    assert_eq!(
        get_member_call_expression(&expression_from_statement_if(&ast_node), "bar"),
        None
    );
}

#[test]
fn get_member_call_expression_returns_none_when_callee_is_a_plain_identifier() {
    let ast_node = parse_first("digest()");
    assert_eq!(
        get_member_call_expression(&expression_from_statement_if(&ast_node), "digest"),
        None
    );
}

#[test]
fn get_member_call_expression_returns_none_when_method_name_does_not_match() {
    let ast_node = parse_first("hash.update('data')");
    assert_eq!(
        get_member_call_expression(&expression_from_statement_if(&ast_node), "digest"),
        None
    );
}

#[test]
fn get_member_call_expression_returns_none_when_property_is_computed() {
    let ast_node = parse_first("hash['digest']()");
    assert_eq!(
        get_member_call_expression(&expression_from_statement_if(&ast_node), "digest"),
        None
    );
}

#[test]
fn get_member_call_expression_returns_the_node_when_method_name_matches() {
    let ast_node = parse_first("hash.digest('hex')");
    let node = expression_from_statement_if(&ast_node);

    let result = get_member_call_expression(&node, "digest").unwrap();

    assert_eq!(result["type"], "CallExpression");
    assert_eq!(result["callee"]["type"], "MemberExpression");
    assert_eq!(result["callee"]["property"]["name"], "digest");
}

#[test]
fn get_member_call_expression_returned_node_carries_the_original_arguments() {
    let ast_node = parse_first("hash.digest('hex')");
    let node = expression_from_statement_if(&ast_node);

    let result = get_member_call_expression(&node, "digest").unwrap();

    assert_eq!(result["arguments"].as_array().unwrap().len(), 1);
    assert_eq!(result["arguments"][0]["value"], "hex");
}

#[test]
fn get_member_call_expression_returns_the_node_for_chained_member_call() {
    let ast_node = parse_first("crypto.createHash('sha256').digest('hex')");
    let node = expression_from_statement_if(&ast_node);

    let result = get_member_call_expression(&node, "digest").unwrap();

    assert_eq!(result["callee"]["property"]["name"], "digest");
}

// ---------------------------------------------------------------------------
// estree.getMemberExpressionIdentifier
// ---------------------------------------------------------------------------

#[test]
fn get_member_expression_identifier_returns_all_literals_of_the_member_expression() {
    let ast_node = parse_first("foo.bar.xd");
    let result = get_member_expression_identifier(&expression_from_statement(&ast_node), &noop);

    assert_eq!(result, vec!["foo", "bar", "xd"]);
}

#[test]
fn get_member_expression_identifier_returns_all_computed_properties() {
    let ast_node = parse_first("foo['bar']['xd']");
    let result = get_member_expression_identifier(&expression_from_statement(&ast_node), &noop);

    assert_eq!(result, vec!["foo", "bar", "xd"]);
}

#[test]
fn get_member_expression_identifier_resolves_computed_binary_expression() {
    let ast_node = parse_first("foo.bar[\"k\" + \"e\" + \"y\"]");
    let result = get_member_expression_identifier(&expression_from_statement(&ast_node), &noop);

    assert_eq!(result, vec!["foo", "bar", "key"]);
}

#[test]
fn get_member_expression_identifier_resolves_computed_identifiers_from_tracer() {
    let lookup = lookup_from(&[("foo", "hello"), ("yo", "bar")]);

    let ast_node = parse_first("hey[foo][yo]");
    let result = get_member_expression_identifier(&expression_from_statement(&ast_node), &lookup);

    assert_eq!(result, vec!["hey", "hello", "bar"]);
}

// ---------------------------------------------------------------------------
// estree.getVariableDeclarationIdentifiers
// ---------------------------------------------------------------------------

fn identifier_names(node: &Value, prefix: Option<&str>) -> Vec<String> {
    get_variable_declaration_identifiers(node, prefix)
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

#[test]
fn get_variable_declaration_identifiers_returns_empty_when_not_a_variable_declaration() {
    let ast_node = parse_first("foobar();");
    let names = identifier_names(&expression_from_statement_if(&ast_node), None);

    assert!(names.is_empty());
}

#[test]
fn get_variable_declaration_identifiers_returns_the_identifier_from_variable_declaration() {
    let ast_node = parse_first("const a = 1;");
    let names = identifier_names(&expression_from_statement_if(&ast_node), None);

    assert_eq!(names, vec!["a"]);
}

#[test]
fn get_variable_declaration_identifiers_returns_all_identifiers_from_variable_declaration() {
    let ast_node = parse_first("const a = 1, b = 2;");
    let names = identifier_names(&expression_from_statement_if(&ast_node), None);

    assert_eq!(names, vec!["a", "b"]);
}

#[test]
fn get_variable_declaration_identifiers_returns_foo_from_various_patterns() {
    let cases = [
        "const [...foo] = []",
        "const { ...foo } = {}",
        "const [foo] = []",
        "const { foo } = {}",
        "const [{ foo }] = []",
        "const [foo = 10] = []",
    ];

    for code in cases {
        let ast_node = parse_first(code);
        let names = identifier_names(&expression_from_statement_if(&ast_node), None);

        assert_eq!(names, vec!["foo"], "case: {code}");
    }
}

#[test]
fn get_variable_declaration_identifiers_returns_multiple_identifiers_of_pattern() {
    let cases = ["const [ foo, bar ] = []", "const { foo, bar } = {}"];

    for code in cases {
        let ast_node = parse_first(code);
        let names = identifier_names(&expression_from_statement_if(&ast_node), None);

        assert_eq!(names, vec!["foo", "bar"], "case: {code}");
    }
}

#[test]
fn get_variable_declaration_identifiers_returns_deeply_destructured_identifier() {
    let ast_node = parse_first("const { hello: { world } } = {}");
    let names = identifier_names(&expression_from_statement_if(&ast_node), None);

    assert_eq!(names, vec!["hello.world"]);
}

#[test]
fn get_variable_declaration_identifiers_returns_the_identifier_in_an_assignment_expression() {
    let ast_node = parse_first("(foo = 5)");
    let names = identifier_names(&expression_from_statement_if(&ast_node), None);

    assert_eq!(names, vec!["foo"]);
}

#[test]
fn get_variable_declaration_identifiers_returns_all_identifiers_of_a_sequence_expression() {
    let ast_node = parse_first("(foo = 5, bar = null)");
    let names = identifier_names(&expression_from_statement_if(&ast_node), None);

    assert_eq!(names, vec!["foo", "bar"]);
}

#[test]
fn get_variable_declaration_identifiers_returns_property_identifiers_of_object_expression() {
    let ast_node = parse_first("({ foo: 1, bar: 2 });");
    let names = identifier_names(&expression_from_statement_if(&ast_node), None);

    assert_eq!(names, vec!["foo", "bar"]);
}

#[test]
fn get_variable_declaration_identifiers_returns_identifiers_of_declarator_id_and_init() {
    let ast_node = parse_first("const hello = { foo: 1, bar: 2 };");
    let names = identifier_names(&expression_from_statement_if(&ast_node), None);

    assert_eq!(names, vec!["hello", "foo", "bar"]);
}

// ---------------------------------------------------------------------------
// estree.toLiteral
// ---------------------------------------------------------------------------

#[test]
fn to_literal_transforms_a_template_literal_to_a_literal() {
    assert_eq!(
        to_literal(&json!({
            "type": "TemplateLiteral",
            "quasis": [],
            "expressions": []
        })),
        ""
    );

    assert_eq!(
        to_literal(&json!({
            "type": "TemplateLiteral",
            "quasis": [{
                "type": "TemplateElement",
                "value": { "raw": "hello", "cooked": null },
                "tail": true
            }],
            "expressions": []
        })),
        "hello"
    );

    assert_eq!(
        to_literal(&json!({
            "type": "TemplateLiteral",
            "quasis": [
                {
                    "type": "TemplateElement",
                    "value": { "raw": "hello ", "cooked": null },
                    "tail": false
                },
                {
                    "type": "TemplateElement",
                    "value": { "raw": " world", "cooked": null },
                    "tail": true
                }
            ],
            "expressions": [{ "type": "Literal", "value": 1, "raw": "1" }]
        })),
        "hello ${0} world"
    );

    assert_eq!(
        to_literal(&json!({
            "type": "TemplateLiteral",
            "quasis": [
                {
                    "type": "TemplateElement",
                    "value": { "raw": "hello ", "cooked": null },
                    "tail": false
                },
                {
                    "type": "TemplateElement",
                    "value": { "raw": " world ", "cooked": null },
                    "tail": false
                },
                {
                    "type": "TemplateElement",
                    "value": { "raw": " ", "cooked": null },
                    "tail": true
                }
            ],
            "expressions": [
                { "type": "Literal", "value": 1, "raw": "1" },
                { "type": "Literal", "value": 1, "raw": "1" }
            ]
        })),
        "hello ${0} world ${1} "
    );
}
