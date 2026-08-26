//! Upstream: `test/Deobfuscator.spec.ts`, `test/NodeCounter.spec.ts`,
//! `test/Pipelines.spec.ts`, `test/obfuscated.spec.ts`

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use serde_json::Value;

use js_x_ray_rs::deobfuscator::{Deobfuscator, ObfuscatedIdentifier};
use js_x_ray_rs::estree::{Node, is_identifier};
use js_x_ray_rs::node_counter::{NodeCounter, NodeCounterOptions};
use js_x_ray_rs::parser::{JsSourceParser, SourceParser};
use js_x_ray_rs::pipelines::{Deobfuscate, Pipeline};
use js_x_ray_rs::walker::walk_enter;
use js_x_ray_rs::{AstAnalyser, AstAnalyserOptions, ReportOnFile, RuntimeOptions, Warning};

/// Upstream's per-spec-file local `walkAst`: walks the parsed body, invoking
/// `callback` for every non-array node.
fn walk_ast_body(body: Vec<Value>, mut callback: impl FnMut(&Value)) {
    let mut root = Value::Array(body);
    walk_enter(&mut root, |_ctx, node| {
        if !node.is_array() {
            callback(&*node);
        }
    });
}

fn parse(code: &str) -> Vec<Value> {
    JsSourceParser.parse(code).expect("parses")
}

fn identifier(name: &str, r#type: &str) -> ObfuscatedIdentifier {
    ObfuscatedIdentifier {
        name: name.to_owned(),
        r#type: r#type.to_owned(),
    }
}

// ---------------------------------------------------------------------------
// Deobfuscator: identifiers and counters
// ---------------------------------------------------------------------------

#[test]
fn should_detect_two_identifiers_class_name_and_superclass_name() {
    let mut deobfuscator = Deobfuscator::new();
    let body = parse("class File extends Blob {}");
    walk_ast_body(body, |node| deobfuscator.walk(node));

    let counters = deobfuscator.aggregate_counters();
    assert_eq!(counters.identifiers, 2);
    assert_eq!(
        deobfuscator.identifiers,
        vec![
            identifier("File", "ClassDeclaration"),
            identifier("Blob", "ClassDeclaration")
        ]
    );
}

#[test]
fn should_detect_one_identifier_because_there_is_no_superclass() {
    let mut deobfuscator = Deobfuscator::new();
    let body = parse("class File {}");
    walk_ast_body(body, |node| deobfuscator.walk(node));

    assert_eq!(
        deobfuscator.identifiers,
        vec![identifier("File", "ClassDeclaration")]
    );
}

#[test]
fn should_detect_one_identifier_because_superclass_is_not_an_identifier_but_a_call_expression() {
    let mut deobfuscator = Deobfuscator::new();
    let body = parse("class File extends (foo()) {}");
    walk_ast_body(body, |node| deobfuscator.walk(node));

    assert_eq!(
        deobfuscator.identifiers,
        vec![identifier("File", "ClassDeclaration")]
    );
}

#[test]
fn should_detect_one_function_declaration_node() {
    let mut deobfuscator = Deobfuscator::new();
    let body = parse("function foo() {}");
    walk_ast_body(body, |node| deobfuscator.walk(node));

    let counters = deobfuscator.aggregate_counters();
    assert_eq!(counters.function_declaration, 1);
    assert_eq!(
        deobfuscator.identifiers,
        vec![identifier("foo", "FunctionDeclaration")]
    );
}

#[test]
fn should_detect_zero_function_declaration_because_foo_is_a_call_expression_node() {
    let mut deobfuscator = Deobfuscator::new();
    let body = parse("foo();");
    walk_ast_body(body, |node| deobfuscator.walk(node));

    let counters = deobfuscator.aggregate_counters();
    assert_eq!(counters.function_declaration, 0);
    assert_eq!(deobfuscator.identifiers.len(), 0);
}

#[test]
fn should_detect_zero_function_declaration_for_an_iife() {
    let mut deobfuscator = Deobfuscator::new();
    let body = parse("(function() {})()");
    walk_ast_body(body, |node| deobfuscator.walk(node));

    let counters = deobfuscator.aggregate_counters();
    assert_eq!(counters.function_declaration, 0);
    assert_eq!(deobfuscator.identifiers.len(), 0);
}

#[test]
fn should_detect_three_identifiers_one_function_declaration_and_two_function_params() {
    let mut deobfuscator = Deobfuscator::new();
    let body = parse("function foo(err, result) {}");
    walk_ast_body(body, |node| deobfuscator.walk(node));

    let counters = deobfuscator.aggregate_counters();
    assert_eq!(counters.function_declaration, 1);
    assert_eq!(
        deobfuscator.identifiers,
        vec![
            identifier("err", "FunctionParams"),
            identifier("result", "FunctionParams"),
            identifier("foo", "FunctionDeclaration"),
        ]
    );
}

#[test]
fn should_detect_a_member_expression_with_two_no_computed_property() {
    let mut deobfuscator = Deobfuscator::new();
    let body = parse("process.mainModule.foo");
    walk_ast_body(body, |node| deobfuscator.walk(node));

    let counters = deobfuscator.aggregate_counters();
    assert_eq!(
        counters.member_expression,
        IndexMap::from([("false".to_owned(), 2u32)])
    );
}

#[test]
fn should_detect_a_member_expression_with_two_computed_properties_and_one_non_computed() {
    let mut deobfuscator = Deobfuscator::new();
    let body = parse("process.mainModule['foo']['bar']");
    walk_ast_body(body, |node| deobfuscator.walk(node));

    let counters = deobfuscator.aggregate_counters();
    assert_eq!(
        counters.member_expression,
        IndexMap::from([("true".to_owned(), 2u32), ("false".to_owned(), 1u32)])
    );
}

#[test]
fn should_detect_no_member_expression_at_all() {
    let mut deobfuscator = Deobfuscator::new();
    let body = parse("process");
    walk_ast_body(body, |node| deobfuscator.walk(node));

    let counters = deobfuscator.aggregate_counters();
    assert_eq!(counters.member_expression, IndexMap::<String, u32>::new());
}

#[test]
fn should_detect_three_identifiers_one_class_declaration_and_two_method_definition() {
    let mut deobfuscator = Deobfuscator::new();
    let body = parse(
        "class File {
          constructor() {}
          foo() {}
        }",
    );
    walk_ast_body(body, |node| deobfuscator.walk(node));

    assert_eq!(
        deobfuscator.identifiers,
        vec![
            identifier("File", "ClassDeclaration"),
            identifier("constructor", "MethodDefinition"),
            identifier("foo", "MethodDefinition"),
        ]
    );
}

#[test]
fn should_detect_four_identifiers_class_two_method_definition_and_one_function_params() {
    let mut deobfuscator = Deobfuscator::new();
    let body = parse(
        "class File {
          get foo() {}
          set bar(value) {}
        }",
    );
    walk_ast_body(body, |node| deobfuscator.walk(node));

    assert_eq!(
        deobfuscator.identifiers,
        vec![
            identifier("File", "ClassDeclaration"),
            identifier("foo", "MethodDefinition"),
            identifier("bar", "MethodDefinition"),
            identifier("value", "FunctionParams"),
        ]
    );
}

#[test]
fn should_detect_one_assignment_expression_with_two_identifiers() {
    let mut deobfuscator = Deobfuscator::new();
    let body = parse("obj = { foo: 1 }");
    walk_ast_body(body, |node| deobfuscator.walk(node));

    let counters = deobfuscator.aggregate_counters();
    assert_eq!(counters.assignment_expression, 1);
    assert_eq!(
        deobfuscator.identifiers,
        vec![
            identifier("obj", "AssignmentExpression"),
            identifier("foo", "Property")
        ]
    );
}

#[test]
fn should_detect_zero_assignment_expression_but_one_identifier() {
    let mut deobfuscator = Deobfuscator::new();
    let body = parse("Object.assign(obj, { foo: 1 })");
    walk_ast_body(body, |node| deobfuscator.walk(node));

    let counters = deobfuscator.aggregate_counters();
    assert_eq!(counters.assignment_expression, 0);
    assert_eq!(
        deobfuscator.identifiers,
        vec![identifier("foo", "Property")]
    );
}

#[test]
fn should_detect_an_object_expression_with_two_property_node() {
    let mut deobfuscator = Deobfuscator::new();
    let body = parse(
        "const obj = {
          log: ['a', 'b', 'c'],
          get latest() {
            return this.log[this.log.length - 1];
          }
        };",
    );
    walk_ast_body(body, |node| deobfuscator.walk(node));

    let counters = deobfuscator.aggregate_counters();
    assert_eq!(counters.variable_declarator, 1);
    assert_eq!(counters.property, 2);
    assert_eq!(
        counters.member_expression,
        IndexMap::from([("true".to_owned(), 1u32), ("false".to_owned(), 3u32)])
    );
    assert_eq!(
        deobfuscator.identifiers,
        vec![
            identifier("obj", "VariableDeclarator"),
            identifier("log", "Property"),
            identifier("latest", "Property"),
        ]
    );
}

#[test]
fn should_detect_one_unary_array() {
    let mut deobfuscator = Deobfuscator::new();
    let body = parse("!![]");
    walk_ast_body(body, |node| deobfuscator.walk(node));

    let counters = deobfuscator.aggregate_counters();
    assert_eq!(counters.double_unary_expression, 1);
}

#[test]
fn should_detect_zero_unary_array() {
    let mut deobfuscator = Deobfuscator::new();
    let body = parse("![]");
    walk_ast_body(body, |node| deobfuscator.walk(node));

    let counters = deobfuscator.aggregate_counters();
    assert_eq!(counters.double_unary_expression, 0);
}

#[test]
fn should_detect_all_variable_declaration_kinds() {
    let mut deobfuscator = Deobfuscator::new();
    let body = parse("var foo; const a = 5; let b = 'foo';");
    walk_ast_body(body, |node| deobfuscator.walk(node));

    let counters = deobfuscator.aggregate_counters();
    assert_eq!(counters.variable_declarator, 3);
    assert_eq!(
        counters.variable_declaration,
        IndexMap::from([
            ("var".to_owned(), 1u32),
            ("const".to_owned(), 1u32),
            ("let".to_owned(), 1u32),
        ])
    );
    assert_eq!(
        deobfuscator.identifiers,
        vec![
            identifier("foo", "VariableDeclarator"),
            identifier("a", "VariableDeclarator"),
            identifier("b", "VariableDeclarator"),
        ]
    );
}

#[test]
fn should_count_the_number_of_variable_declarator() {
    let mut deobfuscator = Deobfuscator::new();
    let body = parse("let a,b,c;");
    walk_ast_body(body, |node| deobfuscator.walk(node));

    let counters = deobfuscator.aggregate_counters();
    assert_eq!(counters.variable_declarator, 3);
    assert_eq!(
        counters.variable_declaration,
        IndexMap::from([("let".to_owned(), 1u32)])
    );
    assert_eq!(
        deobfuscator.identifiers,
        vec![
            identifier("a", "VariableDeclarator"),
            identifier("b", "VariableDeclarator"),
            identifier("c", "VariableDeclarator"),
        ]
    );
}

// ---------------------------------------------------------------------------
// Deobfuscator: analyzeString
// ---------------------------------------------------------------------------

#[test]
fn should_detect_static_dictionary_string() {
    let mut deobfuscator = Deobfuscator::new();
    assert!(!deobfuscator.has_dictionary_string);

    deobfuscator.analyze_string("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789");

    assert!(deobfuscator.has_dictionary_string);
}

#[test]
fn should_detect_morse() {
    let mut deobfuscator = Deobfuscator::new();
    assert_eq!(deobfuscator.morse_literals.len(), 0);

    let morse_str = "--.- --.--";
    deobfuscator.analyze_string(morse_str);

    assert_eq!(deobfuscator.morse_literals.len(), 1);
    assert!(deobfuscator.morse_literals.contains(morse_str));
}

// ---------------------------------------------------------------------------
// NodeCounter
// ---------------------------------------------------------------------------

#[test]
fn should_use_name_option_instead_of_type() {
    let nc = NodeCounter::with_options(
        "UnaryExpression",
        NodeCounterOptions {
            name: Some("DoubleUnaryExpression"),
            ..Default::default()
        },
    );

    assert_eq!(nc.name, "DoubleUnaryExpression");
}

// Adaptation: upstream's `filter`/`match` are independent mock callbacks
// tracked via `node:test`'s `mock.fn()`. The Rust `NodeCounter` only keeps a
// `filter` (a plain `fn` pointer, so its call count needs a static counter
// here) — the `match` callback has no equivalent field at all (see the
// `node_counter` module docs), so its "was it invoked" signal is read off
// `NodeCounter::walk`'s boolean return instead.
static TRIGGER_TEST_FILTER_CALLS: AtomicUsize = AtomicUsize::new(0);

fn trigger_test_filter(_node: &Node) -> bool {
    TRIGGER_TEST_FILTER_CALLS.fetch_add(1, Ordering::SeqCst);
    true
}

#[test]
fn should_trigger_filter_and_match_functions_when_node_type_is_matching() {
    let mut nc = NodeCounter::with_options(
        "FunctionDeclaration",
        NodeCounterOptions {
            filter: Some(trigger_test_filter),
            ..Default::default()
        },
    );

    let body = parse("function foo() {};");
    let mut match_calls = 0usize;
    walk_ast_body(body, |node| {
        if nc.walk(node) {
            match_calls += 1;
        }
    });

    assert_eq!(TRIGGER_TEST_FILTER_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(match_calls, 1);
    assert_eq!(nc.count(), 1);
    assert!(nc.properties().is_empty());
}

#[test]
fn should_count_one_for_a_function_declaration_with_an_identifier() {
    let mut nc = NodeCounter::new("FunctionDeclaration");
    assert_eq!(nc.r#type, "FunctionDeclaration");
    assert_eq!(nc.name, "FunctionDeclaration");
    assert_eq!(nc.lookup, None);

    let body = parse("function foo() {};");
    // Adaptation: the id-collecting `match` callback becomes a manual check
    // on `NodeCounter::walk`'s return value (see the previous test's note).
    let mut ids: Vec<String> = Vec::new();
    walk_ast_body(body, |node| {
        if nc.walk(node)
            && let Some(name) = node.pointer("/id/name").and_then(Value::as_str)
        {
            ids.push(name.to_owned());
        }
    });

    assert_eq!(nc.count(), 1);
    assert!(nc.properties().is_empty());
    assert_eq!(ids, vec!["foo".to_owned()]);
}

fn function_expression_has_identifier(node: &Node) -> bool {
    is_identifier(node.get("id").unwrap_or(&Value::Null))
}

#[test]
fn should_count_zero_for_a_function_expression_with_no_identifier() {
    let mut nc = NodeCounter::with_options(
        "FunctionExpression",
        NodeCounterOptions {
            filter: Some(function_expression_has_identifier),
            ..Default::default()
        },
    );
    assert_eq!(nc.r#type, "FunctionExpression");
    assert_eq!(nc.lookup, None);

    let body = parse("const foo = function() {};");
    walk_ast_body(body, |node| {
        nc.walk(node);
    });

    assert_eq!(nc.count(), 0);
    assert!(nc.properties().is_empty());
}

#[test]
fn should_count_variable_declaration_kinds_property() {
    let mut nc = NodeCounter::new("VariableDeclaration[kind]");
    assert_eq!(nc.r#type, "VariableDeclaration");
    assert_eq!(nc.lookup, Some("kind".to_owned()));

    let body = parse(
        "let foo, xd = 5;
      const yo = 2;
      const mdr = 5;",
    );
    walk_ast_body(body, |node| {
        nc.walk(node);
    });

    assert_eq!(nc.count(), 3);
    assert_eq!(
        *nc.properties(),
        IndexMap::from([("let".to_owned(), 1u32), ("const".to_owned(), 2u32)])
    );
}

#[test]
fn should_count_member_expression_computed_property() {
    let mut nc = NodeCounter::new("MemberExpression[computed]");
    assert_eq!(nc.name, "MemberExpression");
    assert_eq!(nc.r#type, "MemberExpression");
    assert_eq!(nc.lookup, Some("computed".to_owned()));

    let body = parse("yoo.xd[\"damn\"].oh;");
    walk_ast_body(body, |node| {
        nc.walk(node);
    });

    assert_eq!(nc.count(), 3);
    assert_eq!(
        *nc.properties(),
        IndexMap::from([("true".to_owned(), 1u32), ("false".to_owned(), 2u32)])
    );
}

// ---------------------------------------------------------------------------
// AstAnalyser pipelines
// ---------------------------------------------------------------------------

/// Upstream's `mock.fn()` pipeline, recording every `body` it is walked
/// with. `name` is fixed so that two instances registered under the same
/// name get deduplicated by `PipelineRunner`, exactly like the upstream test
/// registering the same mock object twice.
struct SpyPipeline {
    calls: Arc<Mutex<Vec<Vec<Value>>>>,
}

impl Pipeline for SpyPipeline {
    fn name(&self) -> &'static str {
        "test-pipeline"
    }

    fn walk(&mut self, body: Vec<Value>) -> Vec<Value> {
        self.calls.lock().unwrap().push(body.clone());
        body
    }
}

#[test]
fn should_iterate_once_on_the_pipeline() {
    let calls: Arc<Mutex<Vec<Vec<Value>>>> = Arc::new(Mutex::new(Vec::new()));

    let make_factory = |calls: &Arc<Mutex<Vec<Vec<Value>>>>| {
        let calls = Arc::clone(calls);
        move || {
            Box::new(SpyPipeline {
                calls: Arc::clone(&calls),
            }) as Box<dyn Pipeline>
        }
    };

    let analyser = AstAnalyser::new(AstAnalyserOptions {
        pipelines: vec![
            Box::new(make_factory(&calls)),
            Box::new(make_factory(&calls)),
        ],
        ..Default::default()
    });

    let code = "return \"Hello World\";";
    analyser
        .analyse(code, RuntimeOptions::default())
        .expect("analyse");

    let recorded = calls.lock().unwrap();
    assert_eq!(recorded.len(), 1);

    // The exact AST shape is an implementation detail of the parser (oxc
    // here, meriyah upstream); assert against what `JsSourceParser` itself
    // produces for the same source instead of a hand-written literal tree.
    let expected_body = parse(code);
    assert_eq!(recorded[0], expected_body);
}

fn warning_kinds(warnings: &[Warning]) -> Vec<String> {
    let mut kinds: Vec<String> = warnings
        .iter()
        .map(|warning| warning.kind.clone())
        .collect();
    kinds.sort();
    kinds
}

#[test]
fn pipelines_deobfuscate_should_find_a_shady_url_by_deobfuscating_a_joined_array_expression() {
    let analyser = AstAnalyser::new(AstAnalyserOptions {
        pipelines: vec![Box::new(|| Box::new(Deobfuscate) as Box<dyn Pipeline>)],
        ..Default::default()
    });

    let code = r#"
      const URL = ["http://", ["77", "244", "210", "1"].join("."), "/script"].join("");
    "#;
    let report = analyser
        .analyse(code, RuntimeOptions::default())
        .expect("analyse");

    assert_eq!(
        warning_kinds(&report.warnings),
        vec!["shady-link".to_owned()]
    );
}

// ---------------------------------------------------------------------------
// obfuscated.spec: end-to-end obfuscation detection
// ---------------------------------------------------------------------------

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/obfuscated")
        .join(name)
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(fixture_path(name))
        .unwrap_or_else(|error| panic!("failed to read fixture {name}: {error}"))
}

#[test]
fn should_detect_jsfuck_obfuscation() {
    let source = fixture("jsfuck.js");
    let report = AstAnalyser::default()
        .analyse(&source, RuntimeOptions::default())
        .expect("analyse");

    assert_eq!(report.warnings.len(), 1);
    assert_eq!(
        warning_kinds(&report.warnings),
        vec!["obfuscated-code".to_owned()]
    );
    assert_eq!(report.warnings[0].value, Some("jsfuck".to_owned()));
}

#[test]
fn should_detect_morse_obfuscation() {
    let source = fixture("morse.js");
    let report = AstAnalyser::default()
        .analyse(&source, RuntimeOptions::default())
        .expect("analyse");

    assert_eq!(report.warnings.len(), 1);
    assert_eq!(
        warning_kinds(&report.warnings),
        vec!["obfuscated-code".to_owned()]
    );
    assert_eq!(report.warnings[0].value, Some("morse".to_owned()));
}

#[test]
fn should_not_detect_morse_obfuscation() {
    let source = fixture("notMorse.js");
    let report = AstAnalyser::default()
        .analyse(&source, RuntimeOptions::default())
        .expect("analyse");

    assert_eq!(report.warnings.len(), 0);
}

#[test]
fn should_not_detect_morse_obfuscation_for_high_number_of_doubles_morse_symbols() {
    let repeated = "'.' + '..' +".repeat(37);
    let code = format!("const a = {repeated} '.'");
    let report = AstAnalyser::default()
        .analyse(&code, RuntimeOptions::default())
        .expect("analyse");

    assert_eq!(report.warnings.len(), 0);
}

#[test]
fn should_detect_jjencode_obfuscation() {
    let source = fixture("jjencode.js");
    let report = AstAnalyser::default()
        .analyse(&source, RuntimeOptions::default())
        .expect("analyse");

    assert_eq!(report.warnings.len(), 1);
    assert_eq!(
        warning_kinds(&report.warnings),
        vec!["obfuscated-code".to_owned()]
    );
    assert_eq!(report.warnings[0].value, Some("jjencode".to_owned()));
}

#[test]
fn should_detect_freejsobfuscator_obfuscation() {
    let source = fixture("freejsobfuscator.js");
    let report = AstAnalyser::default()
        .analyse(&source, RuntimeOptions::default())
        .expect("analyse");

    let mut expected = vec![
        "encoded-literal".to_owned(),
        "encoded-literal".to_owned(),
        "obfuscated-code".to_owned(),
    ];
    expected.sort();
    assert_eq!(warning_kinds(&report.warnings), expected);
    assert_eq!(
        report.warnings[2].value,
        Some("freejsobfuscator".to_owned())
    );
}

#[test]
fn should_detect_obfuscator_io_obfuscation_with_hexadecimal_generator() {
    let source = fixture("obfuscatorio-hexa.js");
    let report = AstAnalyser::default()
        .analyse(&source, RuntimeOptions::default())
        .expect("analyse");

    assert_eq!(report.warnings.len(), 1);
    assert_eq!(
        warning_kinds(&report.warnings),
        vec!["obfuscated-code".to_owned()]
    );
    assert_eq!(report.warnings[0].value, Some("obfuscator.io".to_owned()));
}

#[test]
fn should_not_detect_trojan_source_when_providing_safe_control_character() {
    // The `` backspace is an actual embedded control character in the
    // analysed source (as it would be after the upstream TS template
    // literal interpolates its own `` escape), not the two-character
    // text `\`+`u0008`.
    let code = "\n    const simpleStringWithControlCharacters = \"Its only a \u{8}backspace\";\n  ";
    let report = AstAnalyser::default()
        .analyse(code, RuntimeOptions::default())
        .expect("analyse");

    assert!(report.warnings.is_empty());
}

#[test]
fn should_detect_trojan_source_when_there_is_one_unsafe_unicode_control_char() {
    let code = "\n    const role = \"ROLE_ADMIN\u{2066}\" // Dangerous control char;\n  ";
    let report = AstAnalyser::default()
        .analyse(code, RuntimeOptions::default())
        .expect("analyse");

    assert_eq!(report.warnings.len(), 1);
    assert_eq!(
        warning_kinds(&report.warnings),
        vec!["obfuscated-code".to_owned()]
    );
    assert_eq!(report.warnings[0].value, Some("trojan-source".to_owned()));
}

#[test]
fn should_detect_trojan_source_when_there_is_at_least_one_unsafe_unicode_control_char() {
    let path = fixture_path("unsafe-unicode-chars.js");
    let report = AstAnalyser::default()
        .analyse_file(&path, RuntimeOptions::default())
        .expect("analyse_file");

    let ReportOnFile::Ok { warnings, .. } = report else {
        panic!("expected a successfully parsed file, got {report:?}");
    };
    assert_eq!(warnings.len(), 1);
    assert_eq!(warning_kinds(&warnings), vec!["obfuscated-code".to_owned()]);
}
