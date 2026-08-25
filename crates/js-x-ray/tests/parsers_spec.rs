//! Upstream: `test/parsers/JsSourceParser.spec.ts`, `test/parsers/TsSourceParser.spec.ts`
//!
//! Upstream's `parse` throws on failure; this port returns `Result`, so
//! every "should not crash" assertion here is adapted to `.is_ok()`.
//!
//! Omitted (no equivalent Rust API surface — both parsers are config-free
//! unit structs, `parse(source)` takes no options):
//! - JsSourceParser "should strip TypeScript types when the option is
//!   enabled": upstream's `stripTypeScriptTypes` constructor hook exists so
//!   meriyah (which cannot parse TS) can be handed pre-stripped source;
//!   oxc's `TsSourceParser` parses TypeScript natively, so the hook has no
//!   reason to exist in this port.
//! - TsSourceParser "should correctly parse with custom options": upstream's
//!   `parse(source, { loc, range })` toggles `loc`/`range` injection;
//!   `TsSourceParser::parse` always injects `loc` and never emits `range`,
//!   with no options parameter to override either.
//! - TsSourceParser "should crash parsing JSX if jsx: false": same reason —
//!   no `jsx` option to disable. (Fixed as part of this port: `parse`
//!   unconditionally enabled JSX parsing was previously *disabled* by
//!   default, contradicting upstream's `jsx: true` default; see
//!   `should_not_crash_parsing_jsx_by_default` below and `src/parser.rs`.)

use js_x_ray::{JsSourceParser, SourceParser, TsSourceParser};

const JSX_COMPONENT_SOURCE: &str = r#"
const Dropzone = forwardRef(({ children, ...params }, ref) => {
    const { open, ...props } = useDropzone(params);
    useImperativeHandle(ref, () => ({ open }), [open]);
    return <Fragment>{children({ ...props, open })}</Fragment>;
});
"#;

#[test]
fn js_source_parser_should_not_crash_when_using_import_keyword() {
    assert!(
        JsSourceParser
            .parse("import * as foo from \"foo\";")
            .is_ok()
    );
}

#[test]
fn js_source_parser_should_not_crash_when_using_export_keyword() {
    assert!(JsSourceParser.parse("export const foo = 5;").is_ok());
}

#[test]
fn js_source_parser_should_not_crash_with_a_source_code_containing_jsx() {
    assert!(JsSourceParser.parse(JSX_COMPONENT_SOURCE).is_ok());
}

#[test]
fn js_source_parser_should_not_crash_with_a_source_code_containing_import_attributes() {
    let code = r#"import data from "./data.json" with { type: "json" };
        export default data;"#;
    assert!(JsSourceParser.parse(code).is_ok());
}

#[test]
fn ts_source_parser_should_correctly_parse_with_default_options() {
    let body = TsSourceParser.parse("const x: number = 5;").unwrap();

    assert_eq!(body[0]["type"], "VariableDeclaration");
    assert!(body[0].get("loc").is_some());
    assert!(body[0].get("range").is_none());
}

#[test]
fn ts_source_parser_should_not_crash_parsing_jsx_by_default() {
    assert!(TsSourceParser.parse(JSX_COMPONENT_SOURCE).is_ok());
}
