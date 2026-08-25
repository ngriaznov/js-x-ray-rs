//! Deep-nesting robustness: minified/obfuscated code commonly nests far
//! beyond serde_json's default recursion limit of 128; the analysis must
//! behave like upstream (which handles thousands of levels) instead of
//! reporting a parsing error.

use js_x_ray::ast_analyser::{AstAnalyser, RuntimeOptions};

fn analyse_ok(code: &str) -> bool {
    AstAnalyser::default()
        .analyse(code, RuntimeOptions::default())
        .is_ok()
}

#[test]
fn deeply_nested_arrays_analyse() {
    let code = format!("const a = {}{};", "[".repeat(1000), "]".repeat(1000));
    assert!(analyse_ok(&code));
}

#[test]
fn deeply_chained_calls_analyse() {
    let code = format!("const y = {};", vec!["f()"; 500].join("."));
    assert!(analyse_ok(&code));
}

#[test]
fn deeply_nested_objects_analyse() {
    let code = format!("const o = {}1{};", "{a:".repeat(800), "}".repeat(800));
    assert!(analyse_ok(&code));
}

#[test]
fn deep_binary_expression_analyses() {
    let code = format!("const s = {};", vec!["\"x\""; 400].join(" + "));
    assert!(analyse_ok(&code));
}
