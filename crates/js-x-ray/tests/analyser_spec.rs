//! Upstream: `test/AstAnalyser.spec.ts`, `test/warnings.spec.ts`
//!
//! `AstAnalyser` is a plain `Result`-returning API (no `EventEmitter`), and
//! `analyse_file` is the single synchronous method covering both upstream
//! `analyseFile` (async) and `analyseFileSync` — so the corresponding
//! upstream `describe` blocks are merged here rather than duplicated, and
//! the `ParsingError` event tests are omitted (their behavior — `ok: false`
//! plus a `parsing-error` warning — is covered directly below). Tests that
//! rely on `t.mock.method`/`t.mock.fn` to spy on method calls or on passing
//! a non-function where JS expects one are omitted too: Rust has no runtime
//! method mocking, and `RuntimeOptions.initialize`/`.finalize` are typed
//! closures, so passing something that "is not a function" cannot arise.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Map, Value, json};

use js_x_ray::ast_analyser::ProbeFactory;
use js_x_ray::collectable_set::DefaultCollectableSet;
use js_x_ray::estree::{Node, SourceLocation, root_location};
use js_x_ray::parser::{ParseError, SourceParser};
use js_x_ray::probe::{Probe, ProbeCtx, ProbeReturn};
use js_x_ray::utils::to_array_location;
use js_x_ray::warnings::{
    GenerateWarningOptions, Severity, Warning, WarningLocation, generate_warning,
};
use js_x_ray::{
    AstAnalyser, AstAnalyserOptions, OptionalWarnings, ReportOnFile, RuntimeOptions, SourceFile,
};

// --- shared fixtures and helpers --------------------------------------------

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/analyser_spec")
        .join(name)
}

fn warning_kinds(warnings: &[Warning]) -> Vec<String> {
    let mut kinds: Vec<String> = warnings.iter().map(|w| w.kind.clone()).collect();
    kinds.sort();
    kinds
}

/// A file under the OS temp dir, removed on drop. The pid + a per-process
/// counter keep names unique across the parallel test threads within this
/// binary and across sibling `cargo test` processes.
struct TempFile(PathBuf);

impl TempFile {
    fn new(name: &str, contents: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("jsxray-{}-{n}-{name}", std::process::id()));
        std::fs::write(&path, contents).expect("write temp file");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Upstream `test/helpers.ts` `customProbes`: flags a `const x = "danger"`
/// declaration directly (bypassing `generateWarning`, like the original).
struct CustomProbeUnsafeDanger;

impl Probe for CustomProbeUnsafeDanger {
    fn name(&self) -> &'static str {
        "customProbeUnsafeDanger"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["VariableDeclaration"])
    }

    fn validate_node(&mut self, node: &Node, _ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        (node
            .pointer("/declarations/0/init/value")
            .and_then(Value::as_str)
            == Some("danger"))
        .then_some(Value::Null)
    }

    fn main(&mut self, node: &Node, _data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        ctx.source_file.warnings.push(Warning {
            kind: "unsafe-danger".to_owned(),
            file: None,
            value: None,
            source: "JS-X-Ray Custom Probe".to_owned(),
            location: WarningLocation::Single(to_array_location(SourceLocation::from_node(node))),
            i18n: "sast_warnings.unsafe-danger".to_owned(),
            severity: Severity::Warning,
            experimental: None,
        });
        ProbeReturn::Skip
    }
}

fn custom_probes_factories() -> Vec<ProbeFactory> {
    vec![Box::new(|| {
        Box::new(CustomProbeUnsafeDanger) as Box<dyn Probe>
    })]
}

const K_INCRIMINATED_CODE_SAMPLE_CUSTOM_PROBE: &str =
    "const danger = 'danger'; const stream = eval('require')('stream');";

/// Upstream fixture `test/fixtures/FakeSourceParser.ts`: always returns the
/// same fixed, bogus AST regardless of input.
struct FakeSourceParser;

impl SourceParser for FakeSourceParser {
    fn parse(&self, _source: &str) -> Result<Vec<Node>, ParseError> {
        Ok(vec![json!({ "type": "LiteralExpression" })])
    }
}

// --- AstAnalyser::analyse ----------------------------------------------------

#[test]
fn analyse_returns_execution_time_as_a_non_negative_number() {
    let report = AstAnalyser::default()
        .analyse("const foo = 'bar';", RuntimeOptions::default())
        .unwrap();
    assert!(report.execution_time >= 0.0);
}

#[test]
fn analyse_returns_all_dependencies_required_at_runtime() {
    let report = AstAnalyser::default()
        .analyse(
            r#"
    const http = require("http");
    const net = require("net");
    const fs = require("fs").promises;

    require("assert").strictEqual;
    require("timers");
    require("./aFile.js");

    const myVar = "path";
    require(myVar);
  "#,
            RuntimeOptions::default(),
        )
        .unwrap();

    assert_eq!(report.warnings.len(), 0);
    let names: Vec<&str> = report.dependencies.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        vec![
            "http",
            "net",
            "fs",
            "assert",
            "timers",
            "./aFile.js",
            "path"
        ]
    );
}

#[test]
fn analyse_flags_a_suspicious_literal_warning() {
    let source = std::fs::read_to_string(fixture("suspect-string.js")).unwrap();
    let report = AstAnalyser::default()
        .analyse(&source, RuntimeOptions::default())
        .unwrap();

    assert_eq!(warning_kinds(&report.warnings), vec!["suspicious-literal"]);
    assert_eq!(report.string_score, 8.0);
}

#[test]
fn analyse_flags_a_suspicious_file_because_it_has_too_many_encoded_literal_warnings() {
    let source = std::fs::read_to_string(fixture("suspiciousFile.js")).unwrap();
    let report = AstAnalyser::default()
        .analyse(&source, RuntimeOptions::default())
        .unwrap();

    assert_eq!(warning_kinds(&report.warnings), vec!["suspicious-file"]);
}

#[test]
fn analyse_combines_the_same_encoded_literal_into_one_warning_with_multiple_locations() {
    let report = AstAnalyser::default()
        .analyse(
            r#"
    const foo = "18c15e5c5c9dac4d16f9311a92bb8331";
    const bar = "18c15e5c5c9dac4d16f9311a92bb8331";
    const xd = "18c15e5c5c9dac4d16f9311a92bb8331";
  "#,
            RuntimeOptions::default(),
        )
        .unwrap();

    assert_eq!(report.warnings.len(), 1);
    assert_eq!(warning_kinds(&report.warnings), vec!["encoded-literal"]);
    match &report.warnings[0].location {
        WarningLocation::Multiple(locations) => assert_eq!(locations.len(), 3),
        other => panic!("expected a Multiple location, got {other:?}"),
    }
}

#[test]
fn analyse_follows_malicious_code_with_hex_computation_and_reassignments() {
    let report = AstAnalyser::default()
        .analyse(
            r#"
    function unhex(r) {
      return Buffer.from(r, "hex").toString();
    }

    const g = eval("this");
    const p = g["pro" + "cess"];

    const evil = p["mainMod" + "ule"][unhex("72657175697265")];
    const work = evil(unhex("2e2f746573742f64617461"));
  "#,
            RuntimeOptions::default(),
        )
        .unwrap();

    assert_eq!(
        warning_kinds(&report.warnings),
        vec!["encoded-literal", "unsafe-import", "unsafe-stmt"]
    );
    let names: Vec<&str> = report.dependencies.keys().map(String::as_str).collect();
    assert_eq!(names, vec!["./test/data"]);
}

#[test]
fn analyse_flags_a_short_identifiers_warning() {
    let report = AstAnalyser::default()
        .analyse(
            r#"
    var a = 0, b, c, d;
    for (let i = 0; i < 10; i++) {
      a += i;
    }
    let de = "foo";
    let x, z;
  "#,
            RuntimeOptions::default(),
        )
        .unwrap();

    assert_eq!(warning_kinds(&report.warnings), vec!["short-identifiers"]);
}

#[test]
fn analyse_detects_a_dependency_required_under_a_try_statement() {
    let report = AstAnalyser::default()
        .analyse(
            r#"
    try {
      require("http");
    }
    catch {}
  "#,
            RuntimeOptions::default(),
        )
        .unwrap();

    assert!(
        report
            .dependencies
            .get("http")
            .is_some_and(|dep| dep.in_try)
    );
}

#[test]
fn analyse_sets_oneline_require_flag_given_a_single_line_cjs_export() {
    let report = AstAnalyser::default()
        .analyse(
            "module.exports = require('foo');",
            RuntimeOptions::default(),
        )
        .unwrap();

    assert!(report.flags.contains("oneline-require"));
    let names: Vec<&str> = report.dependencies.keys().map(String::as_str).collect();
    assert_eq!(names, vec!["foo"]);
}

#[test]
fn analyse_extracts_dependency_names_for_esm() {
    let report = AstAnalyser::default()
        .analyse(
            r#"
    import * as http from "http";
    import fs from "fs";
    import { foo } from "xd";
  "#,
            RuntimeOptions::default(),
        )
        .unwrap();

    assert_eq!(report.warnings.len(), 0);
    let mut names: Vec<&str> = report.dependencies.keys().map(String::as_str).collect();
    names.sort_unstable();
    assert_eq!(names, vec!["fs", "http", "xd"]);
}

#[test]
fn analyse_appends_custom_probes_to_the_default_list() {
    let report = AstAnalyser::new(AstAnalyserOptions {
        custom_probes: custom_probes_factories(),
        ..Default::default()
    })
    .analyse(
        K_INCRIMINATED_CODE_SAMPLE_CUSTOM_PROBE,
        RuntimeOptions::default(),
    )
    .unwrap();

    assert_eq!(report.warnings[0].kind, "unsafe-danger");
    assert_eq!(report.warnings[1].kind, "unsafe-import");
    assert_eq!(report.warnings[2].kind, "unsafe-stmt");
    assert_eq!(report.warnings.len(), 3);
}

#[test]
fn analyse_replaces_the_probe_list_when_skip_default_probes_is_set() {
    let report = AstAnalyser::new(AstAnalyserOptions {
        custom_probes: custom_probes_factories(),
        skip_default_probes: true,
        ..Default::default()
    })
    .analyse(
        K_INCRIMINATED_CODE_SAMPLE_CUSTOM_PROBE,
        RuntimeOptions::default(),
    )
    .unwrap();

    assert_eq!(report.warnings[0].kind, "unsafe-danger");
    assert_eq!(report.warnings.len(), 1);
}

#[test]
fn analyse_initialize_hook_receives_a_working_source_file() {
    let calls: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let hook_calls = calls.clone();
    // Adaptation: upstream separately asserts the hook runs AND that its
    // argument is a `SourceFile` instance. Rust's `initialize` is typed as
    // `FnOnce(&mut SourceFile)`, so a working `SourceFile` argument is
    // guaranteed by the compiler; mutating it here and observing the effect
    // in the final report proves it was actually invoked with the live file.
    let report = AstAnalyser::default()
        .analyse(
            "const foo = 'bar';",
            RuntimeOptions {
                initialize: Some(Box::new(move |source_file: &mut SourceFile| {
                    hook_calls.borrow_mut().push("initialize");
                    source_file.flags.insert("marker".to_owned());
                })),
                ..Default::default()
            },
        )
        .unwrap();

    assert_eq!(*calls.borrow(), vec!["initialize"]);
    assert!(report.flags.contains("marker"));
}

#[test]
fn analyse_finalize_hook_receives_a_working_source_file() {
    let calls: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let hook_calls = calls.clone();
    let report = AstAnalyser::default()
        .analyse(
            "const foo = 'bar';",
            RuntimeOptions {
                finalize: Some(Box::new(move |source_file: &mut SourceFile| {
                    hook_calls.borrow_mut().push("finalize");
                    source_file.flags.insert("marker".to_owned());
                })),
                ..Default::default()
            },
        )
        .unwrap();

    assert_eq!(*calls.borrow(), vec!["finalize"]);
    assert!(report.flags.contains("marker"));
}

#[test]
fn analyse_calls_initialize_before_finalize() {
    let calls: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let init_calls = calls.clone();
    let finalize_calls = calls.clone();

    AstAnalyser::default()
        .analyse(
            "const foo = 'bar';",
            RuntimeOptions {
                initialize: Some(Box::new(move |_: &mut SourceFile| {
                    init_calls.borrow_mut().push("initialize")
                })),
                finalize: Some(Box::new(move |_: &mut SourceFile| {
                    finalize_calls.borrow_mut().push("finalize")
                })),
                ..Default::default()
            },
        )
        .unwrap();

    assert_eq!(*calls.borrow(), vec!["initialize", "finalize"]);
}

// --- AstAnalyser::analyse_file (covers upstream `analyseFile` + `analyseFileSync`) --

#[test]
fn analyse_file_returns_execution_time_as_a_non_negative_number_on_success() {
    let report = AstAnalyser::default()
        .analyse_file(
            &fixture("depName.js"),
            RuntimeOptions {
                package_name: Some("foobar".to_owned()),
                ..Default::default()
            },
        )
        .unwrap();

    match report {
        ReportOnFile::Ok { execution_time, .. } => assert!(execution_time >= 0.0),
        ReportOnFile::Failed { .. } => panic!("expected an Ok report"),
    }
}

#[test]
fn analyse_file_returns_execution_time_as_a_non_negative_number_on_failure() {
    let report = AstAnalyser::default()
        .analyse_file(
            &fixture("parsingError.js"),
            RuntimeOptions {
                package_name: Some("foobar".to_owned()),
                ..Default::default()
            },
        )
        .unwrap();

    match report {
        ReportOnFile::Failed { execution_time, .. } => assert!(execution_time >= 0.0),
        ReportOnFile::Ok { .. } => panic!("expected a Failed report"),
    }
}

#[test]
fn analyse_file_detects_typescript_extension_and_uses_ts_source_parser_automatically() {
    let report = AstAnalyser::default()
        .analyse_file(
            &fixture("test.ts"),
            RuntimeOptions {
                package_name: Some("foobar".to_owned()),
                ..Default::default()
            },
        )
        .unwrap();

    match report {
        ReportOnFile::Ok { warnings, .. } => assert_eq!(warnings.len(), 0),
        ReportOnFile::Failed { .. } => panic!("expected an Ok report"),
    }
}

#[test]
fn analyse_file_errors_when_given_a_typescript_declaration_file() {
    // `test.d.ts` is checked by path before the file is read, so it need
    // not exist on disk (matching upstream, which never creates this file).
    let error = AstAnalyser::default()
        .analyse_file(
            &fixture("test.d.ts"),
            RuntimeOptions {
                package_name: Some("foobar".to_owned()),
                ..Default::default()
            },
        )
        .unwrap_err();

    assert_eq!(error.to_string(), "Declaration files are not supported");
}

#[test]
fn analyse_file_removes_the_package_name_from_the_dependencies_list() {
    let analyser = AstAnalyser::new(AstAnalyserOptions {
        collectables: vec![DefaultCollectableSet::new("dependency")],
        ..Default::default()
    });
    let report = analyser
        .analyse_file(
            &fixture("depName.js"),
            RuntimeOptions {
                package_name: Some("foobar".to_owned()),
                ..Default::default()
            },
        )
        .unwrap();

    match report {
        ReportOnFile::Ok { warnings, .. } => assert_eq!(warnings.len(), 0),
        ReportOnFile::Failed { .. } => panic!("expected an Ok report"),
    }
    let dependency_set = analyser.get_collectable_set("dependency").unwrap();
    let names: Vec<&str> = dependency_set.values().collect();
    assert_eq!(names, vec!["open"]);
}

#[test]
fn analyse_file_fails_with_a_parsing_error() {
    let report = AstAnalyser::default()
        .analyse_file(
            &fixture("parsingError.js"),
            RuntimeOptions {
                package_name: Some("foobar".to_owned()),
                ..Default::default()
            },
        )
        .unwrap();

    match report {
        ReportOnFile::Failed { warnings, .. } => {
            assert_eq!(warnings.len(), 1);
            assert_eq!(warnings[0].kind, "parsing-error");
        }
        ReportOnFile::Ok { .. } => panic!("expected a Failed report"),
    }
}

#[test]
fn analyse_file_collects_the_full_url_and_the_ip_address() {
    let temp = TempFile::new(
        "temp-oneline.js",
        "const IPv4URL = 'http://127.0.0.1:80/script'",
    );
    let expected_dir = temp.path().parent().unwrap().to_string_lossy().to_string();

    let mut metadata = Map::new();
    metadata.insert("spec".to_owned(), json!("react@19.0.1"));

    let analyser = AstAnalyser::new(AstAnalyserOptions {
        collectables: vec![
            DefaultCollectableSet::new("url"),
            DefaultCollectableSet::new("ip"),
            DefaultCollectableSet::new("hostname"),
        ],
        ..Default::default()
    });
    analyser
        .analyse_file(
            temp.path(),
            RuntimeOptions {
                metadata: Some(metadata),
                ..Default::default()
            },
        )
        .unwrap();

    let mut expected_metadata = Map::new();
    expected_metadata.insert("spec".to_owned(), json!("react@19.0.1"));

    let url_data = analyser.get_collectable_set("url").unwrap().to_json();
    assert_eq!(url_data.entries.len(), 1);
    assert_eq!(url_data.entries[0].value, "http://127.0.0.1/script");
    assert_eq!(
        url_data.entries[0].locations[0].file.as_deref(),
        Some(expected_dir.as_str())
    );
    assert_eq!(
        url_data.entries[0].locations[0].location,
        vec![[[1, 16], [1, 44]]]
    );
    assert_eq!(
        url_data.entries[0].locations[0].metadata,
        Some(expected_metadata.clone())
    );

    assert!(
        analyser
            .get_collectable_set("hostname")
            .unwrap()
            .to_json()
            .entries
            .is_empty()
    );

    let ip_data = analyser.get_collectable_set("ip").unwrap().to_json();
    assert_eq!(ip_data.entries.len(), 1);
    assert_eq!(ip_data.entries[0].value, "127.0.0.1");
    assert_eq!(
        ip_data.entries[0].locations[0].file.as_deref(),
        Some(expected_dir.as_str())
    );
    assert_eq!(
        ip_data.entries[0].locations[0].location,
        vec![[[1, 16], [1, 44]]]
    );
    assert_eq!(
        ip_data.entries[0].locations[0].metadata,
        Some(expected_metadata)
    );
}

#[test]
fn analyse_collects_the_full_url_and_the_ip_address_with_a_null_file() {
    // Same scenario, driven through `analyse` (no path) rather than
    // `analyse_file`: the collected locations carry `file: None`.
    let mut metadata = Map::new();
    metadata.insert("spec".to_owned(), json!("react@19.0.1"));

    let analyser = AstAnalyser::new(AstAnalyserOptions {
        collectables: vec![
            DefaultCollectableSet::new("url"),
            DefaultCollectableSet::new("ip"),
            DefaultCollectableSet::new("hostname"),
        ],
        ..Default::default()
    });
    analyser
        .analyse(
            "const IPv4URL = 'http://127.0.0.1:80/script'",
            RuntimeOptions {
                metadata: Some(metadata),
                ..Default::default()
            },
        )
        .unwrap();

    let mut expected_metadata = Map::new();
    expected_metadata.insert("spec".to_owned(), json!("react@19.0.1"));

    let url_data = analyser.get_collectable_set("url").unwrap().to_json();
    assert_eq!(url_data.entries[0].value, "http://127.0.0.1/script");
    assert_eq!(url_data.entries[0].locations[0].file, None);
    assert_eq!(
        url_data.entries[0].locations[0].location,
        vec![[[1, 16], [1, 44]]]
    );
    assert_eq!(
        url_data.entries[0].locations[0].metadata,
        Some(expected_metadata.clone())
    );

    assert!(
        analyser
            .get_collectable_set("hostname")
            .unwrap()
            .to_json()
            .entries
            .is_empty()
    );

    let ip_data = analyser.get_collectable_set("ip").unwrap().to_json();
    assert_eq!(ip_data.entries[0].value, "127.0.0.1");
    assert_eq!(ip_data.entries[0].locations[0].file, None);
}

#[test]
fn analyse_file_implements_new_custom_probes_while_keeping_default_probes() {
    let report = AstAnalyser::new(AstAnalyserOptions {
        custom_probes: custom_probes_factories(),
        skip_default_probes: false,
        ..Default::default()
    })
    .analyse_file(&fixture("customProbe.js"), RuntimeOptions::default())
    .unwrap();

    match report {
        ReportOnFile::Ok { warnings, .. } => {
            assert_eq!(warnings[0].kind, "unsafe-danger");
            assert_eq!(warnings[1].kind, "unsafe-import");
            assert_eq!(warnings[2].kind, "unsafe-stmt");
            assert_eq!(warnings.len(), 3);
        }
        ReportOnFile::Failed { .. } => panic!("expected an Ok report"),
    }
}

#[test]
fn analyse_file_implements_new_custom_probes_while_skipping_default_probes() {
    let report = AstAnalyser::new(AstAnalyserOptions {
        custom_probes: custom_probes_factories(),
        skip_default_probes: true,
        ..Default::default()
    })
    .analyse_file(&fixture("customProbe.js"), RuntimeOptions::default())
    .unwrap();

    match report {
        ReportOnFile::Ok { warnings, .. } => {
            assert_eq!(warnings[0].kind, "unsafe-danger");
            assert_eq!(warnings.len(), 1);
        }
        ReportOnFile::Failed { .. } => panic!("expected an Ok report"),
    }
}

/// Upstream's inline probes with `initialize`/`finalize` methods, used to
/// prove every probe's hooks run once around an `analyseFile` call.
struct HookedProbe {
    calls: Rc<RefCell<Vec<&'static str>>>,
}

impl Probe for HookedProbe {
    fn name(&self) -> &'static str {
        "name"
    }

    fn initialize(&mut self, _source_file: &mut SourceFile) {
        self.calls.borrow_mut().push("initialize");
    }

    fn validate_node(&mut self, _node: &Node, _ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        Some(Value::Null)
    }

    fn main(&mut self, _node: &Node, _data: &Value, _ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        ProbeReturn::Skip
    }

    fn finalize(&mut self, _source_file: &mut SourceFile) {
        self.calls.borrow_mut().push("finalize");
    }
}

struct ClassicProbe;

impl Probe for ClassicProbe {
    fn name(&self) -> &'static str {
        "classic probe"
    }

    fn validate_node(&mut self, _node: &Node, _ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        Some(Value::Null)
    }

    fn main(&mut self, _node: &Node, _data: &Value, _ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        ProbeReturn::Continue
    }
}

#[test]
fn analyse_file_calls_initialize_and_finalize_of_every_probe_at_the_end() {
    let calls: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let hooked_calls = calls.clone();
    let custom_probes: Vec<ProbeFactory> = vec![
        Box::new(move || {
            Box::new(HookedProbe {
                calls: hooked_calls.clone(),
            }) as Box<dyn Probe>
        }),
        Box::new(|| Box::new(ClassicProbe) as Box<dyn Probe>),
    ];

    AstAnalyser::new(AstAnalyserOptions {
        custom_probes,
        skip_default_probes: true,
        ..Default::default()
    })
    .analyse_file(&fixture("customProbe.js"), RuntimeOptions::default())
    .unwrap();

    assert_eq!(*calls.borrow(), vec!["initialize", "finalize"]);
}

#[test]
fn analyse_file_initialize_hook_receives_a_working_source_file() {
    let calls: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let hook_calls = calls.clone();
    let report = AstAnalyser::default()
        .analyse_file(
            &fixture("depName.js"),
            RuntimeOptions {
                initialize: Some(Box::new(move |source_file: &mut SourceFile| {
                    hook_calls.borrow_mut().push("initialize");
                    source_file.flags.insert("marker".to_owned());
                })),
                ..Default::default()
            },
        )
        .unwrap();

    assert_eq!(*calls.borrow(), vec!["initialize"]);
    match report {
        ReportOnFile::Ok { flags, .. } => assert!(flags.contains("marker")),
        ReportOnFile::Failed { .. } => panic!("expected an Ok report"),
    }
}

#[test]
fn analyse_file_finalize_hook_receives_a_working_source_file() {
    let calls: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let hook_calls = calls.clone();
    let report = AstAnalyser::default()
        .analyse_file(
            &fixture("depName.js"),
            RuntimeOptions {
                finalize: Some(Box::new(move |source_file: &mut SourceFile| {
                    hook_calls.borrow_mut().push("finalize");
                    source_file.flags.insert("marker".to_owned());
                })),
                ..Default::default()
            },
        )
        .unwrap();

    assert_eq!(*calls.borrow(), vec!["finalize"]);
    match report {
        ReportOnFile::Ok { flags, .. } => assert!(flags.contains("marker")),
        ReportOnFile::Failed { .. } => panic!("expected an Ok report"),
    }
}

#[test]
fn analyse_file_calls_initialize_before_finalize() {
    let calls: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let init_calls = calls.clone();
    let finalize_calls = calls.clone();

    AstAnalyser::default()
        .analyse_file(
            &fixture("depName.js"),
            RuntimeOptions {
                initialize: Some(Box::new(move |_: &mut SourceFile| {
                    init_calls.borrow_mut().push("initialize")
                })),
                finalize: Some(Box::new(move |_: &mut SourceFile| {
                    finalize_calls.borrow_mut().push("finalize")
                })),
                ..Default::default()
            },
        )
        .unwrap();

    assert_eq!(*calls.borrow(), vec!["initialize", "finalize"]);
}

#[test]
fn analyse_file_adds_is_minified_flag_for_minified_files() {
    let content = "var a=require(\"fs\"),b=require(\"http\");\
        a.readFile(\"test.txt\",function(c,d){b.createServer().listen(3000)});";
    let temp = TempFile::new("temp-test.min.js", content);

    let report = AstAnalyser::default()
        .analyse_file(temp.path(), RuntimeOptions::default())
        .unwrap();

    match report {
        ReportOnFile::Ok { flags, .. } => {
            assert!(flags.contains("is-minified"));
            assert!(!flags.contains("oneline-require"));
        }
        ReportOnFile::Failed { .. } => panic!("expected an Ok report"),
    }
}

#[test]
fn analyse_file_adds_oneline_require_flag_for_one_line_exports() {
    let temp = TempFile::new("temp-oneline-export.js", "module.exports = require('foo');");

    let analyser = AstAnalyser::new(AstAnalyserOptions {
        collectables: vec![DefaultCollectableSet::new("dependency")],
        ..Default::default()
    });
    let report = analyser
        .analyse_file(temp.path(), RuntimeOptions::default())
        .unwrap();

    match report {
        ReportOnFile::Ok { flags, .. } => {
            assert!(flags.contains("oneline-require"));
            assert!(!flags.contains("is-minified"));
        }
        ReportOnFile::Failed { .. } => panic!("expected an Ok report"),
    }
    let dependency_set = analyser.get_collectable_set("dependency").unwrap();
    let names: Vec<&str> = dependency_set.values().collect();
    assert_eq!(names, vec!["foo"]);
}

// --- AstAnalyser::prepare_source ---------------------------------------------

#[test]
fn prepare_source_removes_a_shebang_at_the_start_of_the_file() {
    let prepared = AstAnalyser::default()
        .prepare_source("#!/usr/bin/env node\nconst hello = \"world\";", false);
    assert_eq!(prepared, "const hello = \"world\";");
}

#[test]
fn prepare_source_does_not_remove_a_shebang_that_is_not_at_the_start() {
    let source = "const hello = \"world\";\n#!/usr/bin/env node";
    let prepared = AstAnalyser::default().prepare_source(source, false);
    assert_eq!(prepared, source);
}

#[test]
fn prepare_source_removes_a_singleline_html_comment_when_enabled() {
    let prepared = AstAnalyser::default().prepare_source("<!-- const yo = 5; -->", true);
    assert_eq!(prepared, "");
}

#[test]
fn prepare_source_removes_a_multiline_html_comment_when_enabled() {
    let prepared = AstAnalyser::default().prepare_source(
        "\n      <!--\n    // == fake comment == //\n\n    const yo = 5;\n    //-->\n    ",
        true,
    );
    assert_eq!(prepared.trim(), "");
}

#[test]
fn prepare_source_removes_multiple_html_comments() {
    let prepared = AstAnalyser::default().prepare_source(
        "<!-- const yo = 5; -->\nconst yo = 'foo'\n<!-- const yo = 5; -->",
        true,
    );
    assert_eq!(prepared, "\nconst yo = 'foo'\n");
}

// --- constructor --------------------------------------------------------------

#[test]
fn constructor_does_not_error_without_a_custom_parser() {
    let report = AstAnalyser::default()
        .analyse("const foo = 'bar';", RuntimeOptions::default())
        .unwrap();
    assert!(report.dependencies.is_empty());
}

#[test]
fn constructor_instantiates_with_the_default_probe_list() {
    // Adaptation: upstream compares `analyser.probes` to `ProbeRunner.Defaults`
    // by reference; Rust's `AstAnalyser::probes()` builds a fresh probe list
    // per call (probes carry per-analysis state), so names/order are
    // compared instead.
    let analyser = AstAnalyser::default();
    let names: Vec<&str> = analyser.probes().iter().map(|p| p.name()).collect();
    let expected: Vec<&str> = js_x_ray::probes::default_probes()
        .iter()
        .map(|p| p.name())
        .collect();
    assert_eq!(names, expected);
}

#[test]
fn constructor_uses_the_default_or_custom_parser_via_analyse_file() {
    // Adaptation: there is no method-call-count mock for trait objects in
    // Rust, so the default-vs-custom parser choice is proven behaviorally:
    // `parsingError.js` fails to parse with the real (default) parser, but
    // `FakeSourceParser` always returns a fixed dummy AST regardless of
    // input — succeeding here proves the custom parser was actually used.
    let ok = AstAnalyser::default()
        .analyse_file(
            &fixture("depName.js"),
            RuntimeOptions {
                package_name: Some("foobar".to_owned()),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(matches!(ok, ReportOnFile::Ok { .. }));

    let via_fake_parser = AstAnalyser::default()
        .analyse_file(
            &fixture("parsingError.js"),
            RuntimeOptions {
                package_name: Some("foobar2".to_owned()),
                custom_parser: Some(Box::new(FakeSourceParser)),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(matches!(via_fake_parser, ReportOnFile::Ok { .. }));
}

#[test]
fn constructor_uses_the_default_or_custom_parser_via_analyse() {
    // Same behavioral proof as above, via `analyse`: the default parser
    // finds the real `require("http")` dependency, while `FakeSourceParser`'s
    // fixed dummy AST contains no such call.
    let default_report = AstAnalyser::default()
        .analyse(
            "const http = require(\"http\");",
            RuntimeOptions {
                remove_html_comments: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(default_report.dependencies.contains_key("http"));

    let fake_parser_report = AstAnalyser::default()
        .analyse(
            "const fs = require(\"fs\");",
            RuntimeOptions {
                remove_html_comments: false,
                custom_parser: Some(Box::new(FakeSourceParser)),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(fake_parser_report.dependencies.is_empty());
}

// --- optional warnings --------------------------------------------------------

#[test]
fn optional_warnings_does_not_crash_on_an_unknown_name() {
    let result = AstAnalyser::new(AstAnalyserOptions {
        optional_warnings: OptionalWarnings::Names(vec!["unknown".to_owned()]),
        ..Default::default()
    })
    .analyse("", RuntimeOptions::default());

    assert!(result.is_ok());
}

#[test]
fn optional_warnings_activates_all_crypto_probes_with_a_glob_pattern() {
    let analyser = AstAnalyser::new(AstAnalyserOptions {
        optional_warnings: OptionalWarnings::Names(vec!["crypto.*".to_owned()]),
        ..Default::default()
    });
    let names: Vec<&str> = analyser.probes().iter().map(|p| p.name()).collect();

    for expected in [
        "isWeakScrypt",
        "isUnsafePrehash",
        "isWeakBcrypt",
        "isPasswordShucking",
    ] {
        assert!(
            names.contains(&expected),
            "missing probe {expected} (have {names:?})"
        );
    }
}

// --- warnings.spec.ts ---------------------------------------------------------

#[test]
fn generate_warning_for_an_encoded_literal_kind_uses_a_deep_location_array() {
    let result = generate_warning(
        "encoded-literal",
        GenerateWarningOptions {
            value: None,
            location: Some(root_location()),
            ..Default::default()
        },
    );

    assert_eq!(
        result,
        Warning {
            experimental: Some(false),
            kind: "encoded-literal".to_owned(),
            file: None,
            value: None,
            source: "JS-X-Ray".to_owned(),
            location: WarningLocation::Multiple(vec![[[0, 0], [0, 0]]]),
            i18n: "sast_warnings.encoded_literal".to_owned(),
            severity: Severity::Information,
        }
    );
}

#[test]
fn generate_warning_for_a_weak_crypto_kind_uses_a_simple_location_and_experimental_flag() {
    let result = generate_warning(
        "crypto.weak-algorithm",
        GenerateWarningOptions {
            value: Some("md5".to_owned()),
            location: Some(root_location()),
            file: Some("hello.js".to_owned()),
            ..Default::default()
        },
    );

    assert_eq!(
        result,
        Warning {
            kind: "crypto.weak-algorithm".to_owned(),
            value: Some("md5".to_owned()),
            file: Some("hello.js".to_owned()),
            source: "JS-X-Ray".to_owned(),
            location: WarningLocation::Single([[0, 0], [0, 0]]),
            i18n: "sast_warnings.weak_crypto".to_owned(),
            severity: Severity::Information,
            experimental: Some(false),
        }
    );
}

#[test]
fn analyse_surfaces_a_parse_error_from_inside_an_eval_body() {
    // Upstream `#walkEnter` parses `eval("...")` bodies with an unguarded
    // `AstAnalyser.DefaultParser.parse(...)` call: a malformed eval body
    // throws straight out of `analyse()` (before finalize/oneline-require/
    // getResult ever run), rather than being silently skipped.
    let analyser = AstAnalyser::default();
    let result = analyser.analyse(
        r#"eval("this ) is not : valid js");"#,
        RuntimeOptions::default(),
    );

    assert!(
        result.is_err(),
        "expected a ParseError from the malformed eval body, got {result:?}"
    );
}

#[test]
fn generate_warning_severity_option_overrides_the_default_severity() {
    let warning_a = generate_warning(
        "parsing-error",
        GenerateWarningOptions {
            value: Some("test".to_owned()),
            ..Default::default()
        },
    );
    let warning_b = generate_warning(
        "parsing-error",
        GenerateWarningOptions {
            value: Some("test".to_owned()),
            severity: Some(Severity::Critical),
            ..Default::default()
        },
    );

    assert_eq!(warning_a.severity, Severity::Information);
    assert_ne!(warning_a.severity, warning_b.severity);
}
