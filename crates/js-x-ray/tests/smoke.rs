use js_x_ray_rs::parser::{JsSourceParser, SourceParser};

#[test]
fn parses_and_injects_loc() {
    let body = JsSourceParser
        .parse("const foo = require(\"bar\");\n")
        .unwrap();
    assert_eq!(body.len(), 1);
    let decl = &body[0];
    assert_eq!(decl["type"], "VariableDeclaration");
    assert_eq!(decl["loc"]["start"]["line"], 1);
    assert_eq!(decl["loc"]["start"]["column"], 0);
}

#[test]
fn commonjs_fallback_for_top_level_return() {
    let body = JsSourceParser.parse("if (a) { return; }\nmodule.exports = 1;\n");
    assert!(
        body.is_ok(),
        "top-level return should fall back to commonjs: {body:?}"
    );
}

#[test]
fn parse_error_surfaces() {
    assert!(JsSourceParser.parse("const const = 1;").is_err());
}
