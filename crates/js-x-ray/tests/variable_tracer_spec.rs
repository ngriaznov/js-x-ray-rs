//! Upstream: `test/VariableTracer/VariableTracer.spec.ts`,
//! `test/VariableTracer/assignments.spec.ts`,
//! `test/VariableTracer/cryptoCreateHash.spec.ts`
//! (harness helpers: `test/VariableTracer/utils.ts`)

use serde_json::Value;

use js_x_ray::estree::{Position, SourceLocation};
use js_x_ray::parser::{JsSourceParser, SourceParser};
use js_x_ray::variable_tracer::{AssignmentKind, TraceOptions, TracerEvent, VariableTracer};
use js_x_ray::walker::walk_enter;

// --- test harness (upstream: test/VariableTracer/utils.ts) ------------------
//
// Upstream registers EventEmitter listeners on `tracer` before walking, so
// events are captured live as `tracer.walk` fires them. The Rust tracer has
// no emitter: it queues events and `drain_events` empties the queue. The
// harness below walks the whole body and drains the queue right after,
// which observes the exact same event stream in the exact same order.

struct Harness {
    tracer: VariableTracer,
}

impl Harness {
    fn new(enable_default_tracing: bool) -> Self {
        let mut tracer = VariableTracer::new();
        if enable_default_tracing {
            tracer = tracer.enable_default_tracing();
        }
        Self { tracer }
    }

    /// Upstream `walkOnCode`.
    fn walk_on_code(&mut self, code: &str) -> Vec<TracerEvent> {
        let body = JsSourceParser.parse(code).expect("upstream fixture must parse");
        let mut root = Value::Array(body);
        let tracer = &mut self.tracer;
        walk_enter(&mut root, |_ctx, node| {
            if node.is_array() {
                return;
            }
            tracer.walk(node);
        });
        self.tracer.drain_events()
    }
}

#[derive(Debug, Clone)]
struct AssignmentEvt {
    // Kept for parity with the upstream `AssignmentEventPayload` shape;
    // no upstream assignments.spec/VariableTracer.spec assertion reads it.
    #[allow(dead_code)]
    name: String,
    identifier_or_member_expr: String,
    id: String,
}

#[derive(Debug, Clone)]
struct ImportEvt {
    module_name: String,
    value: String,
    location: Option<SourceLocation>,
}

#[derive(Debug, Clone)]
struct ReturnValueEvt {
    name: String,
    identifier_or_member_expr: String,
    id: String,
    location: Option<SourceLocation>,
    arguments: Vec<Value>,
}

/// Upstream `getAssignmentArray`.
fn assignment_events(events: &[TracerEvent]) -> Vec<AssignmentEvt> {
    events
        .iter()
        .filter_map(|event| match event {
            TracerEvent::Assignment { name, identifier_or_member_expr, id, .. } => {
                Some(AssignmentEvt {
                    name: name.clone(),
                    identifier_or_member_expr: identifier_or_member_expr.clone(),
                    id: id.clone(),
                })
            }
            _ => None,
        })
        .collect()
}

/// Upstream `getImportArray`.
fn import_events(events: &[TracerEvent]) -> Vec<ImportEvt> {
    events
        .iter()
        .filter_map(|event| match event {
            TracerEvent::Import { module_name, value, location } => Some(ImportEvt {
                module_name: module_name.clone(),
                value: value.clone(),
                location: *location,
            }),
            _ => None,
        })
        .collect()
}

/// Upstream `getReturnValueArray`.
fn return_value_events(events: &[TracerEvent]) -> Vec<ReturnValueEvt> {
    events
        .iter()
        .filter_map(|event| match event {
            TracerEvent::ReturnValue { name, identifier_or_member_expr, id, location, arguments } => {
                Some(ReturnValueEvt {
                    name: name.clone(),
                    identifier_or_member_expr: identifier_or_member_expr.clone(),
                    id: id.clone(),
                    location: *location,
                    arguments: arguments.clone(),
                })
            }
            _ => None,
        })
        .collect()
}

/// `assignmentMemory` entries compared as `(kind, name)` pairs — `TracedIdentifierReport`
/// intentionally has no `PartialEq` derive, so tests compare fields explicitly instead.
fn memory_tuples(report: &js_x_ray::TracedIdentifierReport) -> Vec<(&'static str, &str)> {
    report
        .assignment_memory
        .iter()
        .map(|entry| {
            let kind = match entry.r#type {
                AssignmentKind::AliasBinding => "AliasBinding",
                AssignmentKind::ReturnValueAssignment => "ReturnValueAssignment",
            };
            (kind, entry.name.as_str())
        })
        .collect()
}

// =============================================================================
// VariableTracer.spec.ts
// =============================================================================

#[test]
fn get_data_from_identifier_must_return_primitive_null_if_there_is_no_known_traced_identifier() {
    let harness = Harness::new(true);

    assert!(harness.tracer.get_data_from_identifier("foobar", false).is_none());
}

#[test]
fn it_should_trace_re_assignment_from_a_module_import_using_promises() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "fs.readFile",
        TraceOptions {
            follow_consecutive_assignment: true,
            module_name: Some("fs".to_owned()),
            ..Default::default()
        },
    );
    harness.walk_on_code(
        r#"
        import { readFile } from "fs/promises";

        const foobar = readFile;
        const buf = await foobar("test.txt");
        console.log(buf);
        "#,
    );

    let report = harness
        .tracer
        .get_data_from_identifier("foobar", false)
        .expect("foobar must be traced");

    assert_eq!(report.identifier_or_member_expr, "fs.readFile");
    assert_eq!(report.name, "fs.readFile");
    assert_eq!(
        memory_tuples(&report),
        vec![("AliasBinding", "readFile"), ("AliasBinding", "foobar")]
    );
}

#[test]
fn it_should_trace_a_default_import_aliased_to_a_different_local_name() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "crypto.createHash",
        TraceOptions {
            follow_consecutive_assignment: true,
            module_name: Some("crypto".to_owned()),
            ..Default::default()
        },
    );
    harness.walk_on_code(
        r#"
        import c from "crypto";

        const h = c.createHash("md5");
        "#,
    );

    let report = harness
        .tracer
        .get_data_from_identifier("c.createHash", false)
        .expect("c.createHash must be traced");

    assert_eq!(report.identifier_or_member_expr, "crypto.createHash");
    assert_eq!(report.name, "crypto.createHash");
    assert_eq!(memory_tuples(&report), vec![("AliasBinding", "c")]);
}

#[test]
fn it_should_trace_a_namespace_import_aliased_to_a_different_local_name() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "crypto.createHash",
        TraceOptions {
            follow_consecutive_assignment: true,
            module_name: Some("crypto".to_owned()),
            ..Default::default()
        },
    );
    harness.walk_on_code(
        r#"
        import * as c from "crypto";

        const h = c.createHash("md5");
        "#,
    );

    let report = harness
        .tracer
        .get_data_from_identifier("c.createHash", false)
        .expect("c.createHash must be traced");

    assert_eq!(report.identifier_or_member_expr, "crypto.createHash");
    assert_eq!(report.name, "crypto.createHash");
    assert_eq!(memory_tuples(&report), vec![("AliasBinding", "c")]);
}

#[test]
fn it_should_be_able_to_trace_a_malicious_code_with_global_binaryexpr_assignments_and_hexadecimal() {
    let mut harness = Harness::new(true);

    let events = harness.walk_on_code(
        r#"
        var foo;
        const g = eval("this");
        const p = g["pro" + "cess"];

        const evil = p["mainMod" + "ule"][unhex("72657175697265")];
        const work = evil(unhex("2e2f746573742f64617461"))
        "#,
    );
    let assignments = assignment_events(&events);

    let evil = harness
        .tracer
        .get_data_from_identifier("evil", false)
        .expect("evil must be traced");
    assert_eq!(evil.name, "require");
    assert_eq!(evil.identifier_or_member_expr, "process.mainModule.require");
    assert_eq!(memory_tuples(&evil), vec![("AliasBinding", "p"), ("AliasBinding", "evil")]);

    assert_eq!(assignments.len(), 2);
    assert_eq!(assignments[0].identifier_or_member_expr, "process");
    assert_eq!(assignments[0].id, "p");
    assert_eq!(assignments[1].identifier_or_member_expr, "process.mainModule.require");
    assert_eq!(assignments[1].id, "evil");
}

#[test]
fn it_should_be_able_to_trace_a_malicious_callexpression_by_recombining_segments_of_the_memberexpression() {
    let mut harness = Harness::new(true);

    let events = harness.walk_on_code(
        r#"
        const g = global.process;
        const r = g.mainModule;
        const c = r.require;
        c("http");
        r.require("fs");
        "#,
    );
    let assignments = assignment_events(&events);

    let evil = harness
        .tracer
        .get_data_from_identifier("r.require", false)
        .expect("r.require must be traced");
    assert_eq!(evil.name, "require");
    assert_eq!(evil.identifier_or_member_expr, "process.mainModule.require");
    assert_eq!(
        memory_tuples(&evil),
        vec![("AliasBinding", "g"), ("AliasBinding", "r"), ("AliasBinding", "c")]
    );

    assert_eq!(assignments.len(), 3);
    assert_eq!(assignments[0].identifier_or_member_expr, "process");
    assert_eq!(assignments[0].id, "g");
    assert_eq!(assignments[1].identifier_or_member_expr, "process.mainModule");
    assert_eq!(assignments[1].id, "r");
    assert_eq!(assignments[2].identifier_or_member_expr, "process.mainModule.require");
    assert_eq!(assignments[2].id, "c");
}

#[test]
fn given_a_memberexpression_segment_that_doesnt_match_anything_then_it_should_return_null() {
    let harness = Harness::new(true);

    assert!(harness.tracer.get_data_from_identifier("foo.bar", false).is_none());
}

#[test]
fn it_should_be_able_to_trace_a_require_using_function_prototype_call() {
    let mut harness = Harness::new(false);
    harness.tracer.trace("http", TraceOptions::default());

    let events = harness.walk_on_code(
        r#"
      const proto = Function.prototype.call.call(require, require, "http");
      "#,
    );
    let assignments = assignment_events(&events);

    assert!(harness.tracer.get_data_from_identifier("proto", false).is_none());
    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].identifier_or_member_expr, "http");
    assert_eq!(assignments[0].id, "proto");
}

#[test]
fn it_should_be_able_to_trace_an_unsafe_crypto_createhash_using_function_prototype_call_reassignment() {
    let mut harness = Harness::new(true);
    harness.tracer.trace(
        "crypto.createHash",
        TraceOptions {
            follow_consecutive_assignment: true,
            ..Default::default()
        },
    );

    let events = harness.walk_on_code(
        r#"
      const aA = Function.prototype.call;
      const bB = require;

      const crr = aA.call(bB, bB, "crypto");
      const createHashBis = crr.createHash;
      createHashBis("md5");
      "#,
    );
    let assignments = assignment_events(&events);

    let create_hash_bis = harness
        .tracer
        .get_data_from_identifier("createHashBis", false)
        .expect("createHashBis must be traced");
    assert_eq!(create_hash_bis.name, "crypto.createHash");
    assert_eq!(create_hash_bis.identifier_or_member_expr, "crypto.createHash");
    assert_eq!(
        memory_tuples(&create_hash_bis),
        vec![("AliasBinding", "crr"), ("AliasBinding", "createHashBis")]
    );

    assert!(harness.tracer.imported_modules.contains("crypto"));
    assert_eq!(assignments.len(), 3);
    assert_eq!(assignments[0].identifier_or_member_expr, "require");
    assert_eq!(assignments[0].id, "bB");
    assert_eq!(assignments[1].identifier_or_member_expr, "crypto");
    assert_eq!(assignments[1].id, "crr");
    assert_eq!(assignments[2].identifier_or_member_expr, "crypto.createHash");
    assert_eq!(assignments[2].id, "createHashBis");
}

#[test]
fn should_be_able_to_trace_the_return_value_of_a_traced_function() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "os.hostname",
        TraceOptions {
            follow_consecutive_assignment: true,
            follow_return_value_assignement: true,
            module_name: Some("os".to_owned()),
            ..Default::default()
        },
    );

    harness.walk_on_code(
        r#"
        import { hostname } from "os";

        const host = hostname();
        console.log(host);
        "#,
    );

    let report = harness.tracer.get_data_from_identifier("os.hostname", false).unwrap();
    assert_eq!(
        memory_tuples(&report),
        vec![("AliasBinding", "hostname"), ("ReturnValueAssignment", "host")]
    );
}

#[test]
fn should_be_able_to_follow_the_return_value_of_a_traced_function_in_an_object() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "os.hostname",
        TraceOptions {
            follow_consecutive_assignment: true,
            follow_return_value_assignement: true,
            module_name: Some("os".to_owned()),
            ..Default::default()
        },
    );

    harness.walk_on_code(
        r#"
        import { hostname } from "os";

        const host = {x: hostname()};
        console.log(host);
        "#,
    );

    let report = harness.tracer.get_data_from_identifier("os.hostname", false).unwrap();
    assert_eq!(
        memory_tuples(&report),
        vec![("AliasBinding", "hostname"), ("ReturnValueAssignment", "host")]
    );
}

#[test]
fn it_should_be_able_to_trace_the_return_value_of_a_traced_function_in_a_nested_object() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "os.hostname",
        TraceOptions {
            follow_consecutive_assignment: true,
            follow_return_value_assignement: true,
            module_name: Some("os".to_owned()),
            ..Default::default()
        },
    );

    harness.walk_on_code(
        r#"
        import { hostname } from "os";

        const host = {x: null, y: {z: hostname()}};
        console.log(host);
        "#,
    );

    let report = harness.tracer.get_data_from_identifier("os.hostname", false).unwrap();
    assert_eq!(
        memory_tuples(&report),
        vec![("AliasBinding", "hostname"), ("ReturnValueAssignment", "host")]
    );
}

#[test]
fn should_be_able_to_trace_the_return_value_of_a_traced_function_when_the_return_value_is_spreaded() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "os.userInfo",
        TraceOptions {
            follow_consecutive_assignment: true,
            follow_return_value_assignement: true,
            module_name: Some("os".to_owned()),
            ..Default::default()
        },
    );

    harness.walk_on_code(
        r#"
        import os from "os";

        const user = {...os.userInfo()};
        console.log(user);
        "#,
    );

    let report = harness.tracer.get_data_from_identifier("os.userInfo", false).unwrap();
    assert_eq!(memory_tuples(&report), vec![("ReturnValueAssignment", "user")]);
}

#[test]
fn should_be_able_to_trace_a_property_access_on_the_return_value_of_a_traced_function() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "os.userInfo",
        TraceOptions {
            follow_consecutive_assignment: true,
            follow_return_value_assignement: true,
            module_name: Some("os".to_owned()),
            ..Default::default()
        },
    );

    harness.walk_on_code(
        r#"
        import os from "os";

        const user = {x: os.userInfo().name};

        console.log(user);
        "#,
    );

    let report = harness.tracer.get_data_from_identifier("os.userInfo", false).unwrap();
    assert_eq!(memory_tuples(&report), vec![("ReturnValueAssignment", "user")]);
}

#[test]
fn it_should_be_able_to_trace_the_return_value_of_a_traced_function_in_an_array() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "os.hostname",
        TraceOptions {
            follow_consecutive_assignment: true,
            follow_return_value_assignement: true,
            module_name: Some("os".to_owned()),
            ..Default::default()
        },
    );

    harness.walk_on_code(
        r#"
        import { hostname } from "os";

        const host = [hostname()];
        console.log(host);
        "#,
    );

    let report = harness.tracer.get_data_from_identifier("os.hostname", false).unwrap();
    assert_eq!(
        memory_tuples(&report),
        vec![("AliasBinding", "hostname"), ("ReturnValueAssignment", "host")]
    );
}

#[test]
fn should_be_able_to_trace_the_return_value_of_a_traced_function_in_a_nested_array() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "os.hostname",
        TraceOptions {
            follow_consecutive_assignment: true,
            follow_return_value_assignement: true,
            module_name: Some("os".to_owned()),
            ..Default::default()
        },
    );

    harness.walk_on_code(
        r#"
        import { hostname } from "os";

        const host = [null,[1, hostname()]];
        console.log(host);
        "#,
    );

    let report = harness.tracer.get_data_from_identifier("os.hostname", false).unwrap();
    assert_eq!(
        memory_tuples(&report),
        vec![("AliasBinding", "hostname"), ("ReturnValueAssignment", "host")]
    );
}

#[test]
fn should_be_able_to_trace_the_return_value_of_a_traced_function_in_an_array_when_the_return_value_is_spreaded() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "os.userInfo",
        TraceOptions {
            follow_consecutive_assignment: true,
            follow_return_value_assignement: true,
            module_name: Some("os".to_owned()),
            ..Default::default()
        },
    );

    harness.walk_on_code(
        r#"
        import os from "os";

        const user = [...os.userInfo()];
        console.log(user);
        "#,
    );

    let report = harness.tracer.get_data_from_identifier("os.userInfo", false).unwrap();
    assert_eq!(memory_tuples(&report), vec![("ReturnValueAssignment", "user")]);
}

#[test]
fn should_be_able_to_follow_re_assignment_on_traced_return_values() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "os.userInfo",
        TraceOptions {
            follow_consecutive_assignment: true,
            follow_return_value_assignement: true,
            module_name: Some("os".to_owned()),
            ..Default::default()
        },
    );

    harness.walk_on_code(
        r#"
        import os from "os";

        const user = [...os.userInfo()];
        const userBis = user;
        console.log(userBis);
        "#,
    );

    let report = harness.tracer.get_data_from_identifier("os.userInfo", false).unwrap();
    assert_eq!(
        memory_tuples(&report),
        vec![("ReturnValueAssignment", "user"), ("ReturnValueAssignment", "userBis")]
    );
}

#[test]
fn should_be_able_to_follow_re_assignment_on_multiple_consecutive_traced_return_values() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "os.userInfo",
        TraceOptions {
            follow_consecutive_assignment: true,
            follow_return_value_assignement: true,
            module_name: Some("os".to_owned()),
            ..Default::default()
        },
    );

    harness.walk_on_code(
        r#"
        import os from "os";

        const user = [...os.userInfo()];
        const userBis = {...user};
        const userTer = userBis;
        console.log(userTer);
        "#,
    );

    let report = harness.tracer.get_data_from_identifier("os.userInfo", false).unwrap();
    assert_eq!(
        memory_tuples(&report),
        vec![
            ("ReturnValueAssignment", "user"),
            ("ReturnValueAssignment", "userBis"),
            ("ReturnValueAssignment", "userTer"),
        ]
    );
}

#[test]
fn should_not_be_able_to_follow_re_assignment_of_a_traced_return_value_when_follow_consecutive_assignment_is_not_on() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "os.userInfo",
        TraceOptions {
            follow_return_value_assignement: true,
            module_name: Some("os".to_owned()),
            ..Default::default()
        },
    );

    harness.walk_on_code(
        r#"
        import os from "os";

        const user = [...os.userInfo()];
        const userBis = user;
        console.log(userBis);
        "#,
    );

    let report = harness.tracer.get_data_from_identifier("os.userInfo", false).unwrap();
    assert_eq!(memory_tuples(&report), vec![("ReturnValueAssignment", "user")]);
}

#[test]
fn should_get_an_importevent_when_an_import_declaration_is_encountered() {
    let mut harness = Harness::new(false);
    harness.tracer.trace("os.userInfo", TraceOptions::default());

    // Upstream asserts an exact `loc`, so this snippet keeps upstream's
    // literal 4-space indentation instead of matching the surrounding Rust.
    let events = harness.walk_on_code(
        "
    import os from \"node:os\";

    const foo = os.userInfo();

    console.log(foo);
    ",
    );
    let imports = import_events(&events);

    assert_eq!(imports.len(), 1);
    assert_eq!(imports[0].module_name, "os");
    assert_eq!(imports[0].value, "node:os");
    assert_eq!(
        imports[0].location,
        Some(SourceLocation {
            start: Position { line: 2, column: 4 },
            end: Position { line: 2, column: 29 },
        })
    );
}

#[test]
fn should_get_an_importevent_when_a_require_call_is_encountered() {
    let mut harness = Harness::new(false);
    harness.tracer.trace("os.userInfo", TraceOptions::default());

    // Upstream asserts an exact `loc`, so this snippet keeps upstream's
    // literal 4-space indentation instead of matching the surrounding Rust.
    let events = harness.walk_on_code(
        "
    const os = require(\"node:os\");

    const foo = os.userInfo();

    console.log(foo);
    ",
    );
    let imports = import_events(&events);

    assert_eq!(imports.len(), 1);
    assert_eq!(imports[0].module_name, "os");
    assert_eq!(imports[0].value, "node:os");
    assert_eq!(
        imports[0].location,
        Some(SourceLocation {
            start: Position { line: 2, column: 23 },
            end: Position { line: 2, column: 32 },
        })
    );
}

#[test]
fn should_get_a_returnvalueevent_when_a_function_stores_a_result_into_a_variable() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "fn",
        TraceOptions {
            follow_return_value_assignement: true,
            ..Default::default()
        },
    );

    let events = harness.walk_on_code(
        r#"
       function fn(something) {
         return "foo" + something;
       }

      const fooAndBar = fn(" and bar");
      const fooAndFoo = fn(" and foo");
        "#,
    );
    let return_values = return_value_events(&events);

    assert_eq!(return_values.len(), 2);

    assert_eq!(return_values[0].id, "fooAndBar");
    assert_eq!(return_values[0].name, "fn");
    assert_eq!(return_values[0].identifier_or_member_expr, "fn");
    assert_eq!(return_values[0].arguments.len(), 1);
    assert_eq!(return_values[0].arguments[0]["type"], "Literal");
    assert_eq!(return_values[0].arguments[0]["value"], " and bar");
    assert!(return_values[0].location.is_some());

    assert_eq!(return_values[1].id, "fooAndFoo");
    assert_eq!(return_values[1].name, "fn");
    assert_eq!(return_values[1].identifier_or_member_expr, "fn");
    assert_eq!(return_values[1].arguments.len(), 1);
    assert_eq!(return_values[1].arguments[0]["type"], "Literal");
    assert_eq!(return_values[1].arguments[0]["value"], " and foo");
    assert!(return_values[1].location.is_some());
}

#[test]
fn should_get_a_returnvalueevent_when_a_class_stores_a_class_instance_in_a_variable() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "Foo",
        TraceOptions {
            follow_return_value_assignement: true,
            follow_consecutive_assignment: true,
            ..Default::default()
        },
    );

    let events = harness.walk_on_code(
        r#"
      class Foo {
        constructor(something) {
          this.something = something;
        }
      };

      const Bar = Foo;

      const fooAndBar = new Foo(" and bar");
      const fooAndFoo = new Bar(" and foo");
        "#,
    );
    let return_values = return_value_events(&events);

    assert_eq!(return_values.len(), 2);

    assert_eq!(return_values[0].id, "fooAndBar");
    assert_eq!(return_values[0].name, "Foo");
    assert_eq!(return_values[0].identifier_or_member_expr, "Foo");
    assert_eq!(return_values[0].arguments.len(), 1);
    assert_eq!(return_values[0].arguments[0]["type"], "Literal");
    assert_eq!(return_values[0].arguments[0]["value"], " and bar");
    assert!(return_values[0].location.is_some());

    assert_eq!(return_values[1].id, "fooAndFoo");
    assert_eq!(return_values[1].name, "Foo");
    assert_eq!(return_values[1].identifier_or_member_expr, "Foo");
    assert_eq!(return_values[1].arguments.len(), 1);
    assert_eq!(return_values[1].arguments[0]["type"], "Literal");
    assert_eq!(return_values[1].arguments[0]["value"], " and foo");
    assert!(return_values[1].location.is_some());
}

#[test]
fn should_trace_a_method_call_on_an_aliased_return_value() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "foo.bar",
        TraceOptions {
            follow_consecutive_assignment: true,
            follow_return_value_assignement: true,
            module_name: Some("foo".to_owned()),
            ..Default::default()
        },
    );
    harness.tracer.trace(
        "x.baz",
        TraceOptions {
            follow_return_value_assignement: true,
            ..Default::default()
        },
    );

    let events = harness.walk_on_code(
        r#"
        import { bar } from "foo";
        const x = bar();
        const y = x;
        const result = y.baz();
        "#,
    );
    let return_values = return_value_events(&events);

    assert_eq!(return_values.len(), 2);
    assert_eq!(return_values[0].name, "foo.bar");
    assert_eq!(return_values[0].id, "x");
    assert_eq!(return_values[1].name, "x.baz");
    assert_eq!(return_values[1].id, "result");
}

// =============================================================================
// assignments.spec.ts
// =============================================================================

#[test]
fn it_should_be_able_to_trace_a_require_assignment_using_a_global_variable() {
    let mut harness = Harness::new(true);

    let events = harness.walk_on_code(
        r#"
        const test = globalThis;
        const foo = test.require;
        foo("http");
        "#,
    );
    let assignments = assignment_events(&events);

    let foo = harness.tracer.get_data_from_identifier("foo", false).unwrap();
    assert_eq!(foo.name, "require");
    assert_eq!(foo.identifier_or_member_expr, "require");
    assert_eq!(memory_tuples(&foo), vec![("AliasBinding", "foo")]);

    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].identifier_or_member_expr, "require");
    assert_eq!(assignments[0].id, "foo");
}

#[test]
fn it_should_be_able_to_trace_a_require_assignment_using_a_memberexpression() {
    let mut harness = Harness::new(true);

    let events = harness.walk_on_code(
        r#"
        const foo = require.resolve;
        foo("http");
        "#,
    );
    let assignments = assignment_events(&events);

    let foo = harness.tracer.get_data_from_identifier("foo", false).unwrap();
    assert_eq!(foo.name, "require");
    assert_eq!(foo.identifier_or_member_expr, "require.resolve");
    assert_eq!(memory_tuples(&foo), vec![("AliasBinding", "foo")]);

    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].identifier_or_member_expr, "require.resolve");
    assert_eq!(assignments[0].id, "foo");
}

#[test]
fn it_should_be_able_to_trace_a_global_assignment_using_an_estree_objectpattern() {
    let mut harness = Harness::new(true);

    let events = harness.walk_on_code(
        r#"
        const { process: yoo } = globalThis;

        const boo = yoo.mainModule.require;
        "#,
    );
    let assignments = assignment_events(&events);

    let boo = harness.tracer.get_data_from_identifier("boo", false).unwrap();
    assert_eq!(boo.name, "require");
    assert_eq!(boo.identifier_or_member_expr, "process.mainModule.require");
    assert_eq!(memory_tuples(&boo), vec![("AliasBinding", "yoo"), ("AliasBinding", "boo")]);

    assert_eq!(assignments.len(), 2);
    assert_eq!(assignments[0].identifier_or_member_expr, "process");
    assert_eq!(assignments[0].id, "yoo");
    assert_eq!(assignments[1].identifier_or_member_expr, "process.mainModule.require");
    assert_eq!(assignments[1].id, "boo");
}

#[test]
fn it_should_be_able_to_trace_an_unsafe_function_assignment_using_an_estree_objectpattern() {
    let mut harness = Harness::new(true);

    let events = harness.walk_on_code(
        r#"
        const { process: yoo } = Function("return this")();

        const boo = yoo.mainModule.require;
        "#,
    );
    let assignments = assignment_events(&events);

    let boo = harness.tracer.get_data_from_identifier("boo", false).unwrap();
    assert_eq!(boo.name, "require");
    assert_eq!(boo.identifier_or_member_expr, "process.mainModule.require");
    assert_eq!(memory_tuples(&boo), vec![("AliasBinding", "yoo"), ("AliasBinding", "boo")]);

    assert_eq!(assignments.len(), 2);
    assert_eq!(assignments[0].identifier_or_member_expr, "process");
    assert_eq!(assignments[0].id, "yoo");
    assert_eq!(assignments[1].identifier_or_member_expr, "process.mainModule.require");
    assert_eq!(assignments[1].id, "boo");
}

#[test]
fn it_should_be_able_to_trace_a_require_assignment_with_atob() {
    let mut harness = Harness::new(true);

    let events = harness.walk_on_code(
        r#"
        const xo = atob;
        const yo = 'b3M=';
        const ff = xo(yo);
        "#,
    );
    let assignments = assignment_events(&events);

    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].identifier_or_member_expr, "atob");
    assert_eq!(assignments[0].id, "xo");

    let ff = harness.tracer.literal_identifiers.get("ff").expect("ff must be a literal identifier");
    assert_eq!(ff.value, "os");
    assert_eq!(ff.r#type, "Literal");
}

#[test]
fn it_should_be_able_to_trace_template_literals_who_has_being_assigned() {
    let mut harness = Harness::new(false);

    harness.walk_on_code(
        r#"
        const x = `hello ${name}`;
        "#,
    );

    let x = harness.tracer.literal_identifiers.get("x").expect("x must be a literal identifier");
    assert_eq!(x.value, "hello ${0}");
    assert_eq!(x.r#type, "TemplateLiteral");
}

#[test]
fn it_should_be_able_to_resolve_an_identifier_assigned_an_object_literal() {
    let mut harness = Harness::new(false);

    harness.walk_on_code(
        r#"
        const opts = { useOnlyCustomLevels: true, foo: "bar" };
        "#,
    );

    let object_node = harness
        .tracer
        .object_identifiers
        .get("opts")
        .expect("opts must be an object identifier");
    assert_eq!(object_node["type"], "ObjectExpression");
    assert_eq!(object_node["properties"].as_array().unwrap().len(), 2);
}

#[test]
fn it_should_not_resolve_a_non_top_level_object_literal_as_an_identifier() {
    let mut harness = Harness::new(false);

    harness.walk_on_code(
        r#"
        const opts = { nested: { useOnlyCustomLevels: true } };
        "#,
    );

    assert!(harness.tracer.object_identifiers.contains_key("opts"));
    assert!(!harness.tracer.object_identifiers.contains_key("nested"));
}

#[test]
fn it_should_be_able_to_trace_a_global_assignment_using_a_logicalexpression() {
    let mut harness = Harness::new(true);

    let events = harness.walk_on_code(
        r#"
        var root = freeGlobal || freeSelf || Function('return this')();
        const foo = root.require;
        foo("http");
        "#,
    );
    let assignments = assignment_events(&events);

    let foo = harness.tracer.get_data_from_identifier("foo", false).unwrap();
    assert_eq!(foo.name, "require");
    assert_eq!(foo.identifier_or_member_expr, "require");
    assert_eq!(memory_tuples(&foo), vec![("AliasBinding", "foo")]);

    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].identifier_or_member_expr, "require");
    assert_eq!(assignments[0].id, "foo");
}

#[test]
fn it_should_be_able_to_trace_assignment_of_process_get_builtin_module() {
    let mut harness = Harness::new(true);

    let events = harness.walk_on_code(
        r#"
        if (globalThis.process?.getBuiltinModule) {
          const foo = globalThis.process.getBuiltinModule;
          const fs = foo('fs');
        }
        "#,
    );
    let assignments = assignment_events(&events);

    let foo = harness.tracer.get_data_from_identifier("foo", false).unwrap();
    assert_eq!(foo.name, "require");
    assert_eq!(foo.identifier_or_member_expr, "process.getBuiltinModule");
    assert_eq!(memory_tuples(&foo), vec![("AliasBinding", "foo")]);

    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].identifier_or_member_expr, "process.getBuiltinModule");
    assert_eq!(assignments[0].id, "foo");

    assert!(
        harness
            .tracer
            .get_data_from_identifier("globalThis.process.getBuiltinModule", false)
            .is_none()
    );

    let get_builtin_module = harness
        .tracer
        .get_data_from_identifier("globalThis.process.getBuiltinModule", true)
        .expect("must resolve once the globalThis prefix is stripped");
    assert_eq!(get_builtin_module.name, "require");
    assert_eq!(get_builtin_module.identifier_or_member_expr, "process.getBuiltinModule");
    assert_eq!(memory_tuples(&get_builtin_module), vec![("AliasBinding", "foo")]);
}

// =============================================================================
// cryptoCreateHash.spec.ts
// =============================================================================

#[test]
fn it_should_be_able_to_trace_crypto_createhash_when_imported_with_an_importnamespacespecifier_esm() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "crypto.createHash",
        TraceOptions {
            follow_consecutive_assignment: true,
            module_name: Some("crypto".to_owned()),
            ..Default::default()
        },
    );

    let events = harness.walk_on_code(
        r#"
        import fs from "fs";
        import * as cryptoBis from "crypto";

        const createHashBis = cryptoBis.createHash;
        createHashBis("md5");
        "#,
    );
    let assignments = assignment_events(&events);

    let create_hash_bis = harness.tracer.get_data_from_identifier("createHashBis", false).unwrap();
    assert_eq!(create_hash_bis.name, "crypto.createHash");
    assert_eq!(create_hash_bis.identifier_or_member_expr, "crypto.createHash");
    assert_eq!(
        memory_tuples(&create_hash_bis),
        vec![("AliasBinding", "cryptoBis"), ("AliasBinding", "createHashBis")]
    );

    assert_eq!(assignments.len(), 2);
    assert_eq!(assignments[0].identifier_or_member_expr, "crypto");
    assert_eq!(assignments[0].id, "cryptoBis");
    assert_eq!(assignments[1].identifier_or_member_expr, "crypto.createHash");
    assert_eq!(assignments[1].id, "createHashBis");
}

#[test]
fn it_should_be_able_to_trace_createhash_when_required_commonjs_and_destructured_with_an_estree_objectpattern() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "crypto.createHash",
        TraceOptions {
            follow_consecutive_assignment: true,
            module_name: Some("crypto".to_owned()),
            ..Default::default()
        },
    );

    // This is an ObjectPattern: `const { createHash } = ...`
    let events = harness.walk_on_code(
        r#"
        const { createHash } = require("crypto");

        const createHashBis = createHash;
        createHashBis("md5");
        "#,
    );
    let assignments = assignment_events(&events);

    let create_hash_bis = harness.tracer.get_data_from_identifier("createHashBis", false).unwrap();
    assert_eq!(create_hash_bis.name, "crypto.createHash");
    assert_eq!(create_hash_bis.identifier_or_member_expr, "crypto.createHash");
    assert_eq!(
        memory_tuples(&create_hash_bis),
        vec![("AliasBinding", "createHash"), ("AliasBinding", "createHashBis")]
    );

    assert_eq!(assignments.len(), 2);
    assert_eq!(assignments[0].identifier_or_member_expr, "crypto.createHash");
    assert_eq!(assignments[0].id, "createHash");
    assert_eq!(assignments[1].identifier_or_member_expr, "crypto.createHash");
    assert_eq!(assignments[1].id, "createHashBis");
}

#[test]
fn it_should_be_able_to_trace_crypto_createhash_when_imported_with_an_importspecifier_esm() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "crypto.createHash",
        TraceOptions {
            follow_consecutive_assignment: true,
            module_name: Some("crypto".to_owned()),
            ..Default::default()
        },
    );

    let events = harness.walk_on_code(
        r#"
        import { createHash } from "node:crypto";

        const createHashBis = createHash;
        createHashBis("md5");
        "#,
    );
    let assignments = assignment_events(&events);

    let create_hash_bis = harness.tracer.get_data_from_identifier("createHashBis", false).unwrap();
    assert_eq!(create_hash_bis.name, "crypto.createHash");
    assert_eq!(create_hash_bis.identifier_or_member_expr, "crypto.createHash");
    assert_eq!(
        memory_tuples(&create_hash_bis),
        vec![("AliasBinding", "createHash"), ("AliasBinding", "createHashBis")]
    );

    assert_eq!(assignments.len(), 2);
    assert_eq!(assignments[0].identifier_or_member_expr, "crypto.createHash");
    assert_eq!(assignments[0].id, "createHash");
    assert_eq!(assignments[1].identifier_or_member_expr, "crypto.createHash");
    assert_eq!(assignments[1].id, "createHashBis");
}

#[test]
fn it_should_be_able_to_trace_crypto_createhash_with_commonjs_require_and_a_computed_method_with_a_literal() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "crypto.createHash",
        TraceOptions {
            follow_consecutive_assignment: true,
            module_name: Some("crypto".to_owned()),
            ..Default::default()
        },
    );

    let events = harness.walk_on_code(
        r#"
        const fs = require("fs");
        const crypto = require("node:crypto");

        const id = "createHash";
        const createHashBis = crypto[id];
        createHashBis("md5");
        "#,
    );

    assert!(harness.tracer.imported_modules.contains("crypto"));

    let create_hash_bis = harness.tracer.get_data_from_identifier("createHashBis", false).unwrap();
    assert_eq!(create_hash_bis.name, "crypto.createHash");
    assert_eq!(create_hash_bis.identifier_or_member_expr, "crypto.createHash");
    assert_eq!(memory_tuples(&create_hash_bis), vec![("AliasBinding", "createHashBis")]);

    let assignments = assignment_events(&events);
    assert_eq!(assignments.len(), 2);
    assert_eq!(assignments[0].identifier_or_member_expr, "crypto");
    assert_eq!(assignments[0].id, "crypto");
    assert_eq!(assignments[1].identifier_or_member_expr, "crypto.createHash");
    assert_eq!(assignments[1].id, "createHashBis");
}

#[test]
fn it_should_not_detect_variable_assignment_since_the_crypto_module_is_not_imported() {
    let mut harness = Harness::new(false);
    harness.tracer.trace(
        "crypto.createHash",
        TraceOptions {
            follow_consecutive_assignment: true,
            module_name: Some("crypto".to_owned()),
            ..Default::default()
        },
    );

    let events = harness.walk_on_code(
        r#"
        const crypto = {
          createHash() {}
        }
        const _t = crypto.createHash;
        _t("md5");
        "#,
    );

    assert!(!harness.tracer.imported_modules.contains("crypto"));
    assert_eq!(assignment_events(&events).len(), 0);
}

#[test]
fn it_should_return_null_because_crypto_createhash_is_not_imported_from_a_module() {
    let mut harness = Harness::new(true);
    harness.tracer.trace(
        "crypto.createHash",
        TraceOptions {
            follow_consecutive_assignment: true,
            module_name: Some("crypto".to_owned()),
            ..Default::default()
        },
    );

    harness.walk_on_code(
        r#"
        const crypto = {
          createHash() {}
        }
        const evil = crypto.createHash;
        evil('md5');
        "#,
    );

    assert!(harness.tracer.get_data_from_identifier("crypto.createHash", false).is_none());
}
