//! Upstream: `test/InlinedCallExpression.spec.ts`, `test/InlinedNew.spec.ts`,
//! `test/InlinedRequire.spec.ts`, `test/VirtualVariableIdentifier.spec.ts`.

use std::sync::{Mutex, MutexGuard};

use js_x_ray::estree::{Position, SourceLocation};
use js_x_ray::inlined::virtual_variable_identifier::VirtualVariableIdentifier;
use js_x_ray::inlined::{InlinedCallExpression, InlinedNew, InlinedRequire};
use js_x_ray::parser::{JsSourceParser, SourceParser};
use serde_json::Value;

/// `VirtualVariableIdentifier` is a process-global counter (mirroring
/// upstream's module-level `static` state), so tests touching it cannot
/// interleave under Rust's default parallel test runner the way upstream's
/// sequential `node:test` + `beforeEach(reset)` does. This guard serializes
/// every test in this file, standing in for that per-test reset.
static GLOBAL_STATE_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    GLOBAL_STATE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Upstream `getExpressionFromStatement`: unwraps an `ExpressionStatement`,
/// or yields `null` (here: `Value::Null`) for any other statement kind.
fn get_expression_from_statement(node: &Value) -> Value {
    if node["type"] == "ExpressionStatement" {
        node["expression"].clone()
    } else {
        Value::Null
    }
}

fn parse_statement(src: &str) -> Value {
    JsSourceParser
        .parse(src)
        .expect("valid JS")
        .into_iter()
        .next()
        .expect("at least one statement")
}

fn parse_expr(src: &str) -> Value {
    get_expression_from_statement(&parse_statement(src))
}

/// Minimal stand-in for upstream's `astring.generate()`, covering only the
/// node shapes these fixtures produce (no operator precedence, statements,
/// or non-empty object/template literals). Renders straight from the parsed
/// `raw` text for literals, exactly like astring does.
fn render(node: &Value) -> String {
    match node["type"].as_str().expect("typed node") {
        "Identifier" => node["name"].as_str().expect("Identifier.name").to_owned(),
        "Literal" => node["raw"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| node["value"].to_string()),
        "ArrayExpression" => {
            let elements: Vec<String> = node["elements"]
                .as_array()
                .expect("ArrayExpression.elements")
                .iter()
                .map(render)
                .collect();
            format!("[{}]", elements.join(", "))
        }
        "ObjectExpression" => {
            // Only empty object literals (`{}`) appear in these fixtures.
            assert!(node["properties"].as_array().is_some_and(Vec::is_empty));
            "{}".to_owned()
        }
        "CallExpression" => {
            let args: Vec<String> = node["arguments"]
                .as_array()
                .expect("CallExpression.arguments")
                .iter()
                .map(render)
                .collect();
            format!("{}({})", render(&node["callee"]), args.join(", "))
        }
        "NewExpression" => {
            let args: Vec<String> = node["arguments"]
                .as_array()
                .expect("NewExpression.arguments")
                .iter()
                .map(render)
                .collect();
            format!("new {}({})", render(&node["callee"]), args.join(", "))
        }
        "MemberExpression" => {
            let object = render(&node["object"]);
            let property = render(&node["property"]);
            if node["computed"].as_bool().unwrap_or(false) {
                format!("{object}[{property}]")
            } else {
                format!("{object}.{property}")
            }
        }
        other => panic!("render: unsupported node type {other}"),
    }
}

/// Renders a `const <id> = <init>;` `VariableDeclaration`, matching
/// `Inlined.buildSplitResult`'s `virtualDeclaration` shape.
fn render_declaration(declaration: &Value) -> String {
    assert_eq!(declaration["type"], "VariableDeclaration");
    assert_eq!(declaration["kind"], "const");
    let declarator = &declaration["declarations"][0];
    format!(
        "const {} = {};",
        render(&declarator["id"]),
        render(&declarator["init"])
    )
}

mod inlined_call_expression {
    use super::*;

    mod split {
        use super::*;

        #[test]
        fn should_split_pino_info_hello() {
            let _guard = lock();
            VirtualVariableIdentifier::reset();

            let node = parse_expr("pino().info('hello');");
            let result = InlinedCallExpression::split(&node).expect("split result");

            assert_eq!(result.virtual_identifier, "__virtual_call_expression_0__");
            assert_eq!(
                render_declaration(&result.virtual_declaration),
                "const __virtual_call_expression_0__ = pino();"
            );
            let rebuild = result.rebuild_expression.expect("rebuild expression");
            assert_eq!(
                render(&rebuild),
                "__virtual_call_expression_0__.info('hello')"
            );
        }

        #[test]
        fn should_split_pino_object_info_hello() {
            let _guard = lock();
            VirtualVariableIdentifier::reset();

            let node = parse_expr("pino({}).info('hello');");
            let result = InlinedCallExpression::split(&node).expect("split result");

            assert_eq!(result.virtual_identifier, "__virtual_call_expression_0__");
            assert_eq!(
                render_declaration(&result.virtual_declaration),
                "const __virtual_call_expression_0__ = pino({});"
            );
            let rebuild = result.rebuild_expression.expect("rebuild expression");
            assert_eq!(
                render(&rebuild),
                "__virtual_call_expression_0__.info('hello')"
            );
        }

        #[test]
        fn should_increment_virtual_identifiers_across_multiple_splits() {
            let _guard = lock();
            VirtualVariableIdentifier::reset();

            let node1 = parse_expr("pino().info('hello');");
            let node2 = parse_expr("pino().error('hello');;");

            let result1 = InlinedCallExpression::split(&node1).expect("split result 1");
            let result2 = InlinedCallExpression::split(&node2).expect("split result 2");

            assert_eq!(result1.virtual_identifier, "__virtual_call_expression_0__");
            assert_eq!(result2.virtual_identifier, "__virtual_call_expression_1__");
        }
    }

    #[test]
    fn should_be_null_for_simple_function_call() {
        let _guard = lock();
        VirtualVariableIdentifier::reset();

        let node = parse_expr("pino();");
        assert!(InlinedCallExpression::split(&node).is_none());
    }

    #[test]
    fn should_be_null_for_property_access() {
        let _guard = lock();
        VirtualVariableIdentifier::reset();

        let node = parse_expr("pino().info;");
        assert!(InlinedCallExpression::split(&node).is_none());
    }

    #[test]
    fn should_be_null_for_simple_member_function_call() {
        let _guard = lock();
        VirtualVariableIdentifier::reset();

        let node = parse_expr("foo.bar();");
        assert!(InlinedCallExpression::split(&node).is_none());
    }

    #[test]
    fn should_be_null_for_inlined_new_expression() {
        let _guard = lock();
        VirtualVariableIdentifier::reset();

        let node = parse_expr("(new Foo()).bar();");
        assert!(InlinedCallExpression::split(&node).is_none());
    }

    #[test]
    fn should_be_null_for_require_expression() {
        let _guard = lock();
        VirtualVariableIdentifier::reset();

        let node = parse_expr(r#"require("child_process").spawn("csrutil", ["disable"]);"#);
        assert!(InlinedCallExpression::split(&node).is_none());
    }

    #[test]
    fn should_be_null_for_eval_expression() {
        let _guard = lock();
        VirtualVariableIdentifier::reset();

        // `ast.body[0]` is a `VariableDeclaration`, so
        // `getExpressionFromStatement` yields `null` here, same as upstream.
        let node = parse_expr("const stream = eval('require')('stream');");
        assert!(InlinedCallExpression::split(&node).is_none());
    }

    #[test]
    fn should_be_able_to_handle_chained_operations() {
        let _guard = lock();
        VirtualVariableIdentifier::reset();

        let node = parse_expr("fn().bar.foo().bar.foo().bar;");
        let result = InlinedCallExpression::split(&node).expect("split result");

        assert_eq!(result.virtual_identifier, "__virtual_call_expression_0__");
        assert_eq!(
            render_declaration(&result.virtual_declaration),
            "const __virtual_call_expression_0__ = fn();"
        );
        let rebuild = result.rebuild_expression.expect("rebuild expression");
        assert_eq!(
            render(&rebuild),
            "__virtual_call_expression_0__.bar.foo().bar.foo().bar"
        );
    }
}

mod inlined_new {
    use super::*;

    mod split {
        use super::*;

        #[test]
        fn should_split_new_vm_script_run_in_context_call() {
            let _guard = lock();
            VirtualVariableIdentifier::reset();

            let node = parse_expr("(new vm.Script(code, options)).runInContext(sandbox);");
            let result = InlinedNew::split(&node).expect("split result");

            assert_eq!(result.virtual_identifier, "__virtual_new_0__");
            assert_eq!(
                render_declaration(&result.virtual_declaration),
                "const __virtual_new_0__ = new vm.Script(code, options);"
            );
            let rebuild = result.rebuild_expression.expect("rebuild expression");
            assert_eq!(render(&rebuild), "__virtual_new_0__.runInContext(sandbox)");
        }

        #[test]
        fn should_split_new_vm_script_run_in_context_property_access() {
            let _guard = lock();
            VirtualVariableIdentifier::reset();

            let node = parse_expr("(new vm.Script(code, options)).runInContext;");
            let result = InlinedNew::split(&node).expect("split result");

            assert_eq!(result.virtual_identifier, "__virtual_new_0__");
            assert_eq!(
                render_declaration(&result.virtual_declaration),
                "const __virtual_new_0__ = new vm.Script(code, options);"
            );
            let rebuild = result.rebuild_expression.expect("rebuild expression");
            assert_eq!(render(&rebuild), "__virtual_new_0__.runInContext");
        }

        #[test]
        fn should_increment_virtual_identifiers_across_multiple_splits() {
            let _guard = lock();
            VirtualVariableIdentifier::reset();

            let node1 = parse_expr("(new vm.Script(code, options)).runInContext(sandbox);");
            let node2 = parse_expr("(new vm.Script(code, options)).runInContext(sandbox);");

            let result1 = InlinedNew::split(&node1).expect("split result 1");
            let result2 = InlinedNew::split(&node2).expect("split result 2");

            assert_eq!(result1.virtual_identifier, "__virtual_new_0__");
            assert_eq!(result2.virtual_identifier, "__virtual_new_1__");
        }
    }

    #[test]
    fn should_be_null_for_simple_new_call() {
        let _guard = lock();
        VirtualVariableIdentifier::reset();

        let node = parse_expr("new Foo();");
        assert!(InlinedNew::split(&node).is_none());
    }

    #[test]
    fn should_be_null_for_property_access() {
        let _guard = lock();
        VirtualVariableIdentifier::reset();

        let node = parse_expr("foo.bar;");
        assert!(InlinedNew::split(&node).is_none());
    }

    #[test]
    fn should_be_null_for_function_call() {
        let _guard = lock();
        VirtualVariableIdentifier::reset();

        let node = parse_expr("foo.bar();");
        assert!(InlinedNew::split(&node).is_none());
    }

    #[test]
    fn should_be_able_to_handle_chained_operations() {
        let _guard = lock();
        VirtualVariableIdentifier::reset();

        let node = parse_expr("(new Foo()).bar.foo().bar.foo().bar;");
        let result = InlinedNew::split(&node).expect("split result");

        assert_eq!(result.virtual_identifier, "__virtual_new_0__");
        assert_eq!(
            render_declaration(&result.virtual_declaration),
            "const __virtual_new_0__ = new Foo();"
        );
        let rebuild = result.rebuild_expression.expect("rebuild expression");
        assert_eq!(
            render(&rebuild),
            "__virtual_new_0__.bar.foo().bar.foo().bar"
        );
    }
}

mod inlined_require {
    use super::*;

    mod assert_node {
        use super::*;

        #[test]
        fn should_return_false_for_a_require_call_expression() {
            let node = parse_expr(r#"require("fs");"#);
            assert!(!InlinedRequire::assert_node(&node));
        }

        #[test]
        fn should_return_false_for_a_require_call_expression_with_property_access() {
            let node = parse_expr(r#"require("fs").promises;"#);
            assert!(!InlinedRequire::assert_node(&node));
        }

        #[test]
        fn should_return_true_for_an_inlined_require_call_expression() {
            let node = parse_expr(r#"require("child_process").spawn("csrutil", ["disable"]);"#);
            assert!(InlinedRequire::assert_node(&node));
        }
    }

    mod split {
        use super::*;

        #[test]
        fn should_return_null_for_a_simple_require_call() {
            let _guard = lock();
            VirtualVariableIdentifier::reset();

            let node = parse_expr(r#"require("fs");"#);
            assert!(InlinedRequire::split(&node).is_none());
        }

        #[test]
        fn should_return_null_for_a_require_with_property_access_but_no_method_call() {
            let _guard = lock();
            VirtualVariableIdentifier::reset();

            let node = parse_expr(r#"require("fs").promises;"#);
            assert!(InlinedRequire::split(&node).is_none());
        }

        #[test]
        fn should_split_require_child_process_spawn_into_virtual_declaration_and_rebuilt_expression()
         {
            let _guard = lock();
            VirtualVariableIdentifier::reset();

            let node = parse_expr(r#"require("child_process").spawn("csrutil", ["disable"]);"#);
            let result = InlinedRequire::split(&node).expect("split result");

            assert_eq!(result.virtual_identifier, "__virtual_require_0__");
            assert_eq!(
                render_declaration(&result.virtual_declaration),
                r#"const __virtual_require_0__ = require("child_process");"#
            );
            let rebuild = result.rebuild_expression.expect("rebuild expression");
            assert_eq!(
                render(&rebuild),
                r#"__virtual_require_0__.spawn("csrutil", ["disable"])"#
            );
        }

        #[test]
        fn should_return_null_for_require_fs_promises_read_file_because_callee_is_member_expression()
         {
            let _guard = lock();
            VirtualVariableIdentifier::reset();

            // `require("fs").promises.readFile()` has callee =
            // `require("fs").promises` (a MemberExpression): assertNode only
            // matches `require.something()`, not `require().something.method()`.
            let node = parse_expr(r#"require("fs").promises.readFile("./package.json");"#);
            assert!(InlinedRequire::split(&node).is_none());
        }

        #[test]
        fn should_split_require_fs_read_file_sync_correctly() {
            let _guard = lock();
            VirtualVariableIdentifier::reset();

            let node = parse_expr(r#"require("fs").readFileSync("./package.json");"#);
            let result = InlinedRequire::split(&node).expect("split result");

            assert_eq!(result.virtual_identifier, "__virtual_require_0__");
            assert_eq!(
                render_declaration(&result.virtual_declaration),
                r#"const __virtual_require_0__ = require("fs");"#
            );
            let rebuild = result.rebuild_expression.expect("rebuild expression");
            assert_eq!(
                render(&rebuild),
                r#"__virtual_require_0__.readFileSync("./package.json")"#
            );
        }

        #[test]
        fn should_increment_virtual_identifiers_across_multiple_splits() {
            let _guard = lock();
            VirtualVariableIdentifier::reset();

            let node1 = parse_expr(r#"require("fs").readFileSync("a.txt");"#);
            let node2 = parse_expr(r#"require("path").join("a", "b");"#);

            let result1 = InlinedRequire::split(&node1).expect("split result 1");
            let result2 = InlinedRequire::split(&node2).expect("split result 2");

            assert_eq!(result1.virtual_identifier, "__virtual_require_0__");
            assert_eq!(result2.virtual_identifier, "__virtual_require_1__");
        }

        #[test]
        fn should_handle_require_with_computed_property_access() {
            let _guard = lock();
            VirtualVariableIdentifier::reset();

            let node = parse_expr(r#"require("obj")["method"]("arg");"#);
            let result = InlinedRequire::split(&node).expect("split result");

            assert_eq!(
                render_declaration(&result.virtual_declaration),
                r#"const __virtual_require_0__ = require("obj");"#
            );
            let rebuild = result.rebuild_expression.expect("rebuild expression");
            assert_eq!(
                render(&rebuild),
                r#"__virtual_require_0__["method"]("arg")"#
            );
        }

        #[test]
        fn should_return_rebuild_expression_as_null_when_the_node_is_the_require_call_itself() {
            let _guard = lock();
            VirtualVariableIdentifier::reset();

            // `require.resolve("fs")` doesn't have a nested `require()` call
            // (its callee is the `require.resolve` MemberExpression, not a
            // CallExpression), so `split` returns `None`.
            let node = parse_expr(r#"require.resolve("fs");"#);
            assert!(InlinedRequire::split(&node).is_none());
        }

        #[test]
        fn should_return_null_for_non_call_expression_node() {
            let _guard = lock();
            VirtualVariableIdentifier::reset();

            // Unlike the other cases, upstream passes `ast.body[0]` directly
            // here (a `VariableDeclaration`), skipping `getExpressionFromStatement`.
            let node = parse_statement("const x = 5;");
            assert!(InlinedRequire::split(&node).is_none());
        }

        #[test]
        fn should_return_null_for_call_expression_that_is_not_a_require_pattern() {
            let _guard = lock();
            VirtualVariableIdentifier::reset();

            let node = parse_expr(r#"console.log("hello");"#);
            assert!(InlinedRequire::split(&node).is_none());
        }
    }
}

mod virtual_variable_identifier {
    use super::*;

    mod generate {
        use super::*;

        #[test]
        fn should_generate_a_virtual_identifier_with_incrementing_counter() {
            let _guard = lock();
            VirtualVariableIdentifier::reset();

            let id1 = VirtualVariableIdentifier::generate("foo", None);
            let id2 = VirtualVariableIdentifier::generate("bar", None);

            assert_eq!(id1, "__virtual_foo_0__");
            assert_eq!(id2, "__virtual_bar_1__");
        }

        #[test]
        fn should_store_location_when_provided() {
            let _guard = lock();
            VirtualVariableIdentifier::reset();

            let location = SourceLocation {
                start: Position { line: 1, column: 0 },
                end: Position {
                    line: 1,
                    column: 10,
                },
            };
            let id = VirtualVariableIdentifier::generate("test", Some(location));

            assert_eq!(VirtualVariableIdentifier::get_location(&id), Some(location));
        }
    }

    mod get_location {
        use super::*;

        #[test]
        fn should_return_none_for_unknown_virtual_id() {
            let _guard = lock();
            VirtualVariableIdentifier::reset();

            assert_eq!(VirtualVariableIdentifier::get_location("unknown"), None);
        }

        #[test]
        fn should_return_none_when_no_location_was_provided() {
            let _guard = lock();
            VirtualVariableIdentifier::reset();

            let id = VirtualVariableIdentifier::generate("noLoc", None);
            assert_eq!(VirtualVariableIdentifier::get_location(&id), None);
        }
    }

    mod reset {
        use super::*;

        #[test]
        fn should_reset_counter_and_clear_stored_locations() {
            let _guard = lock();
            VirtualVariableIdentifier::reset();

            let id1 = VirtualVariableIdentifier::generate("before", None);
            assert_eq!(id1, "__virtual_before_0__");

            VirtualVariableIdentifier::reset();

            let id2 = VirtualVariableIdentifier::generate("after", None);
            assert_eq!(id2, "__virtual_after_0__");
            assert_eq!(VirtualVariableIdentifier::get_location(&id1), None);
        }
    }
}
