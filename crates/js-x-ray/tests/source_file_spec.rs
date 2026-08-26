//! Upstream: `test/SourceFile.spec.ts`, `test/ProbeRunner.spec.ts`

use serde_json::{Map, Value, json};

use std::cell::RefCell;
use std::rc::Rc;

use js_x_ray_rs::collectable_set::{CollectableSetRegistry, DefaultCollectableSet};
use js_x_ray_rs::estree::{Node, Position, SourceLocation};
use js_x_ray_rs::probe::WalkAction;
use js_x_ray_rs::source_file::{SourceFileOptions, SourceFilePath};
use js_x_ray_rs::warnings::{Severity, Warning, WarningLocation};
use js_x_ray_rs::{Probe, ProbeCtx, ProbeReturn, ProbeRunner, SourceFile};

fn zero_location() -> SourceLocation {
    SourceLocation {
        start: Position { line: 0, column: 0 },
        end: Position { line: 0, column: 0 },
    }
}

fn new_source_file() -> SourceFile {
    SourceFile::new(
        None,
        SourceFileOptions {
            metadata: None,
            package_name: None,
            collectable_registry: None,
        },
    )
}

// --- SourceFile.spec.ts -----------------------------------------------------

#[test]
fn constructor_with_source_location_sets_the_path_location() {
    let source_file = SourceFile::new(
        Some("/path/to/file.js".to_owned()),
        SourceFileOptions {
            metadata: None,
            package_name: None,
            collectable_registry: None,
        },
    );

    assert_eq!(
        source_file.path.location.as_deref(),
        Some("/path/to/file.js")
    );
}

#[test]
fn add_dependency_without_an_unsafe_import_warning() {
    let mut metadata = Map::new();
    metadata.insert("spec".to_owned(), json!("react@19.0.1"));
    let registry = CollectableSetRegistry::new(vec![DefaultCollectableSet::new("dependency")]);

    let mut source_file = SourceFile::new(
        Some("file.js".to_owned()),
        SourceFileOptions {
            metadata: Some(metadata),
            package_name: None,
            collectable_registry: Some(registry),
        },
    );
    source_file.dependency_auto_warning = false;
    source_file.in_try_statement = true;
    source_file.add_dependency("package/", Some(zero_location()));

    assert_eq!(source_file.warnings, vec![]);

    let dependency_set = source_file
        .collectables_set_registry
        .get("dependency")
        .unwrap();
    let data = dependency_set.to_json();
    assert_eq!(data.entries.len(), 1);
    assert_eq!(data.entries[0].value, "package");
    assert_eq!(data.entries[0].locations.len(), 1);
    let location = &data.entries[0].locations[0];
    assert_eq!(location.file.as_deref(), Some("file.js"));
    assert_eq!(location.location, vec![[[0, 0], [0, 0]]]);
    let mut expected_metadata = Map::new();
    expected_metadata.insert("spec".to_owned(), json!("react@19.0.1"));
    expected_metadata.insert("inTry".to_owned(), json!(true));
    expected_metadata.insert("unsafe".to_owned(), json!(false));
    assert_eq!(location.metadata, Some(expected_metadata));
}

#[test]
fn add_dependency_with_an_unsafe_import_warning() {
    let mut metadata = Map::new();
    metadata.insert("spec".to_owned(), json!("react@19.0.1"));
    let registry = CollectableSetRegistry::new(vec![DefaultCollectableSet::new("dependency")]);

    let mut source_file = SourceFile::new(
        Some("file.js".to_owned()),
        SourceFileOptions {
            metadata: Some(metadata),
            package_name: None,
            collectable_registry: Some(registry),
        },
    );
    source_file.dependency_auto_warning = true;
    source_file.add_dependency("package", Some(zero_location()));

    assert_eq!(
        source_file.warnings,
        vec![Warning {
            kind: "unsafe-import".to_owned(),
            file: None,
            value: Some("package".to_owned()),
            source: "JS-X-Ray".to_owned(),
            location: WarningLocation::Single([[0, 0], [0, 0]]),
            i18n: "sast_warnings.unsafe_import".to_owned(),
            severity: Severity::Warning,
            experimental: Some(false),
        }]
    );

    let dependency_set = source_file
        .collectables_set_registry
        .get("dependency")
        .unwrap();
    let data = dependency_set.to_json();
    assert_eq!(data.entries[0].value, "package");
    let mut expected_metadata = Map::new();
    expected_metadata.insert("spec".to_owned(), json!("react@19.0.1"));
    expected_metadata.insert("inTry".to_owned(), json!(false));
    expected_metadata.insert("unsafe".to_owned(), json!(true));
    assert_eq!(
        data.entries[0].locations[0].metadata,
        Some(expected_metadata)
    );
}

#[test]
fn add_dependency_does_not_add_any_dependency_for_an_empty_string() {
    let mut metadata = Map::new();
    metadata.insert("spec".to_owned(), json!("react@19.0.1"));
    let registry = CollectableSetRegistry::new(vec![DefaultCollectableSet::new("dependency")]);

    let mut source_file = SourceFile::new(
        Some("file.js".to_owned()),
        SourceFileOptions {
            metadata: Some(metadata),
            package_name: None,
            collectable_registry: Some(registry),
        },
    );
    source_file.dependency_auto_warning = false;
    source_file.add_dependency("  ", Some(zero_location()));

    assert_eq!(source_file.warnings, vec![]);
    assert!(
        source_file
            .collectables_set_registry
            .get("dependency")
            .unwrap()
            .to_json()
            .entries
            .is_empty(),
        "a blank dependency name should not be recorded"
    );
}

#[test]
fn add_dependency_does_not_add_the_dependency_when_the_package_name_is_the_same() {
    let mut metadata = Map::new();
    metadata.insert("spec".to_owned(), json!("react@19.0.1"));
    let registry = CollectableSetRegistry::new(vec![DefaultCollectableSet::new("dependency")]);

    let mut source_file = SourceFile::new(
        Some("file.js".to_owned()),
        SourceFileOptions {
            metadata: Some(metadata),
            package_name: Some("package".to_owned()),
            collectable_registry: Some(registry),
        },
    );
    source_file.dependency_auto_warning = false;
    source_file.add_dependency("package", Some(zero_location()));

    assert_eq!(source_file.warnings, vec![]);
    assert!(
        source_file
            .collectables_set_registry
            .get("dependency")
            .unwrap()
            .to_json()
            .entries
            .is_empty(),
        "a dependency matching the file's own package name should not be recorded"
    );
}

// --- SourceFilePath ----------------------------------------------------------

#[test]
fn source_file_path_constructor_has_location_set_to_none_by_default() {
    let path = SourceFilePath::default();
    assert_eq!(path.location, None);
}

#[test]
fn source_file_path_use_sets_location_when_provided() {
    let mut path = SourceFilePath::default();
    path.use_location(Some("/foo/bar".to_owned()));
    assert_eq!(path.location.as_deref(), Some("/foo/bar"));
}

#[test]
fn source_file_path_use_sets_location_to_none_when_undefined() {
    let mut path = SourceFilePath::default();
    path.use_location(Some("/foo".to_owned()));
    path.use_location(None);
    assert_eq!(path.location, None);
}

#[test]
fn source_file_path_resolve_joins_parts_without_base_location() {
    let path = SourceFilePath::default();
    assert_eq!(path.resolve(&["foo", "bar.js"]), "foo/bar.js");
}

#[test]
fn source_file_path_resolve_joins_parts_with_base_location() {
    let mut path = SourceFilePath::default();
    path.use_location(Some("/base".to_owned()));
    assert_eq!(path.resolve(&["foo", "bar.js"]), "/base/foo/bar.js");
}

// --- ProbeRunner.spec.ts -----------------------------------------------------
//
// Upstream's ProbeRunner threads a single dynamic `context` value (built by
// a probe's `initialize`, deep-cloned per node, and passed to every hook)
// through a generic `ProbeCtx`, and supports `validateNode` as either a
// function or an array of validators, plus a `setEntryPoint` escape hatch
// that swaps `main` for a named sibling function. The Rust `Probe` trait
// has none of this machinery: a probe's state lives directly on its struct
// fields (mutated via `&mut self`), `validate_node` is always a single
// function, and an entry-point dispatch is just a match inside `main`. Tests
// that only exercise that dynamic-context machinery (context
// initialize/dispatch/clone-and-clear, `setEntryPoint`, `validateNode` as an
// array, and the malformed-probe-shape / frozen-object guards that Rust's
// trait system makes unrepresentable) have no Rust equivalent and are
// omitted; the rest are adapted to the trait-based API below.

#[derive(Default)]
struct RecordingState {
    validate_calls: u32,
    main_calls: u32,
    teardown_calls: u32,
    finalize_calls: u32,
    last_main_data: Option<Value>,
    last_source_file_ptr: Option<usize>,
}

/// A `Probe` whose validator and observed calls are exposed through a shared
/// handle, standing in for `t.mock.fn()` (there is no method-mocking
/// mechanism for trait objects in Rust).
type ValidateNodeFn = Box<dyn FnMut(&Node) -> Option<Value>>;

struct RecordingProbe {
    state: Rc<RefCell<RecordingState>>,
    validate_node_fn: ValidateNodeFn,
    main_return: ProbeReturn,
}

fn recording_probe(
    validate_node_fn: impl FnMut(&Node) -> Option<Value> + 'static,
    main_return: ProbeReturn,
) -> (Box<dyn Probe>, Rc<RefCell<RecordingState>>) {
    let state = Rc::new(RefCell::new(RecordingState::default()));
    let probe = RecordingProbe {
        state: state.clone(),
        validate_node_fn: Box::new(validate_node_fn),
        main_return,
    };
    (Box::new(probe), state)
}

impl Probe for RecordingProbe {
    fn name(&self) -> &'static str {
        "recording-probe"
    }

    fn validate_node(&mut self, node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        let mut state = self.state.borrow_mut();
        state.validate_calls += 1;
        state.last_source_file_ptr = Some(std::ptr::from_ref(ctx.source_file) as usize);
        drop(state);
        (self.validate_node_fn)(node)
    }

    fn main(&mut self, _node: &Node, data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let mut state = self.state.borrow_mut();
        state.main_calls += 1;
        state.last_main_data = Some(data.clone());
        state.last_source_file_ptr = Some(std::ptr::from_ref(ctx.source_file) as usize);
        self.main_return
    }

    fn teardown(&mut self, _source_file: &mut SourceFile) {
        self.state.borrow_mut().teardown_calls += 1;
    }

    fn finalize(&mut self, _source_file: &mut SourceFile) {
        self.state.borrow_mut().finalize_calls += 1;
    }
}

fn source_file_ptr(source_file: &SourceFile) -> usize {
    std::ptr::from_ref(source_file) as usize
}

#[test]
fn constructor_uses_the_default_probes_when_given_that_list() {
    let mut source_file = new_source_file();
    // Adaptation: `ProbeRunner::new` always takes an explicit probe list
    // (there is no "Defaults when none given" overload); this checks that
    // `probes::default_probes()` populates `ProbeRunner.probes` faithfully.
    let expected_names: Vec<&'static str> = js_x_ray_rs::probes::default_probes()
        .iter()
        .map(|p| p.name())
        .collect();
    let runner = ProbeRunner::new(&mut source_file, js_x_ray_rs::probes::default_probes());

    let names: Vec<&'static str> = runner.probes.iter().map(|p| p.name()).collect();
    assert_eq!(names, expected_names);
}

#[test]
fn constructor_keeps_the_provided_probe_list() {
    let mut source_file = new_source_file();
    let (probe, _state) = recording_probe(|_node| Some(Value::Null), ProbeReturn::Matched);
    let runner = ProbeRunner::new(&mut source_file, vec![probe]);

    assert_eq!(runner.probes.len(), 1);
    assert_eq!(runner.probes[0].name(), "recording-probe");
}

#[test]
fn walk_passes_validate_node_then_calls_main_and_teardown() {
    let mut source_file = new_source_file();
    let expected_ptr = source_file_ptr(&source_file);
    let (probe, state) = recording_probe(
        |node| (node.get("type").and_then(Value::as_str) == Some("Literal")).then_some(Value::Null),
        ProbeReturn::Matched,
    );
    let mut runner = ProbeRunner::new(&mut source_file, vec![probe]);

    let node = json!({ "type": "Literal", "value": "test" });
    let action = runner.walk(&node, &mut source_file);

    assert_eq!(action, WalkAction::None);
    let state = state.borrow();
    assert_eq!(state.validate_calls, 1);
    assert_eq!(state.main_calls, 1);
    assert_eq!(state.teardown_calls, 1);
    assert_eq!(state.last_main_data, Some(Value::Null));
    assert_eq!(state.last_source_file_ptr, Some(expected_ptr));
}

#[test]
fn walk_forwards_validate_node_data_to_main() {
    let mut source_file = new_source_file();
    let payload = json!({ "test": "data" });
    let (probe, state) = recording_probe(
        {
            let payload = payload.clone();
            move |_node| Some(payload.clone())
        },
        ProbeReturn::Skip,
    );
    let mut runner = ProbeRunner::new(&mut source_file, vec![probe]);

    let node = json!({ "type": "Literal", "value": "test" });
    runner.walk(&node, &mut source_file);
    runner.finalize(&mut source_file);

    assert_eq!(state.borrow().last_main_data, Some(payload));
}

#[test]
fn walk_triggers_and_returns_a_skip_signal() {
    let mut source_file = new_source_file();
    let (probe, state) = recording_probe(
        |node| (node.get("type").and_then(Value::as_str) == Some("Literal")).then_some(Value::Null),
        ProbeReturn::Skip,
    );
    let mut runner = ProbeRunner::new(&mut source_file, vec![probe]);

    let node = json!({ "type": "Literal", "value": "test" });
    let action = runner.walk(&node, &mut source_file);

    assert_eq!(action, WalkAction::Skip);
    assert_eq!(state.borrow().teardown_calls, 1);
}

#[test]
fn finalize_calls_the_finalize_method_of_every_probe() {
    let mut source_file = new_source_file();
    let (skip_probe, skip_state) = recording_probe(|_| Some(Value::Null), ProbeReturn::Skip);
    let (break_probe, break_state) = recording_probe(|_| Some(Value::Null), ProbeReturn::Break);
    let (other_skip_probe, other_skip_state) =
        recording_probe(|_| Some(Value::Null), ProbeReturn::Skip);
    let mut runner = ProbeRunner::new(
        &mut source_file,
        vec![skip_probe, break_probe, other_skip_probe],
    );

    runner.finalize(&mut source_file);

    for state in [&skip_state, &break_state, &other_skip_state] {
        assert_eq!(state.borrow().finalize_calls, 1);
    }
}
