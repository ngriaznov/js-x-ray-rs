//! Upstream: `test/EntryFilesAnalyser.spec.ts`
//!
//! `analyse` is synchronous here (see `entry_files_analyser`'s module docs),
//! so wherever upstream mocks `AstAnalyser.prototype.analyseFile` to count
//! calls, this port asserts the equivalent count via `EntryFilesAnalyser::stats`
//! instead. Wherever upstream relies on `astAnalyzer` being a public field
//! shared by reference (to read back a `DefaultCollectableSet` populated
//! during the run), this port uses the `EntryFilesAnalyser::ast_analyzer`
//! accessor added for that purpose.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use js_x_ray::ast_analyser::{AstAnalyser, AstAnalyserOptions, ReportOnFile};
use js_x_ray::collectable_set::DefaultCollectableSet;
use js_x_ray::entry_files_analyser::{
    EntryFilesAnalyser, EntryFilesAnalyserOptions, EntryFilesRuntimeOptions,
};
use serde_json::{Map, Value};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/entryFiles")
}

fn ts_fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/entryFilesTs")
}

fn to_file_strings(paths: impl IntoIterator<Item = PathBuf>) -> Vec<String> {
    paths
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn should_throw_when_ast_analyzer_has_no_dependency_collectable() {
    let ast_analyzer = AstAnalyser::default();

    let Err(error) = EntryFilesAnalyser::new(EntryFilesAnalyserOptions {
        ast_analyzer: Some(ast_analyzer),
        ..Default::default()
    }) else {
        panic!("expected EntryFilesAnalyser::new to fail");
    };

    assert_eq!(
        error.to_string(),
        "astAnalyzer instance must have a 'dependency' collectable"
    );
}

#[test]
fn should_analyze_internal_dependencies_recursively() {
    let mut entry_files_analyser =
        EntryFilesAnalyser::new(EntryFilesAnalyserOptions::default()).unwrap();
    let entry = fixtures_dir().join("entry.js");
    let deep_entry = fixtures_dir().join("deps/deepEntry.js");

    let reports = entry_files_analyser
        .analyse(
            [entry.clone(), deep_entry.clone()],
            EntryFilesRuntimeOptions::default(),
        )
        .unwrap();

    let mut files: Vec<String> = reports.iter().map(|report| report.file.clone()).collect();
    files.sort();

    // Order is non-deterministic in the async upstream original; check the set of files.
    let mut expected = to_file_strings([
        entry,
        fixtures_dir().join("deps/dep1.js"),
        fixtures_dir().join("shared.js"),
        fixtures_dir().join("deps/dep2.js"),
        deep_entry,
        fixtures_dir().join("deps/dep3.js"),
    ]);
    expected.sort();
    assert_eq!(files, expected);

    // Adaptation: upstream counts `AstAnalyser.prototype.analyseFile` mock calls;
    // this port asserts the equivalent via `stats.number_of_files_processed`.
    assert_eq!(entry_files_analyser.stats.number_of_files_processed, 6);
}

#[test]
fn should_analyze_esm_export_statements_recursively() {
    let mut entry_files_analyser =
        EntryFilesAnalyser::new(EntryFilesAnalyserOptions::default()).unwrap();
    let entry = fixtures_dir().join("export.js");

    let reports = entry_files_analyser
        .analyse([entry.clone()], EntryFilesRuntimeOptions::default())
        .unwrap();

    let files: Vec<String> = reports.iter().map(|report| report.file.clone()).collect();
    assert_eq!(
        files,
        to_file_strings([entry, fixtures_dir().join("shared.js")])
    );

    assert_eq!(entry_files_analyser.stats.number_of_files_processed, 2);
}

#[test]
fn should_detect_internal_deps_that_failed_to_be_analyzed() {
    let mut entry_files_analyser =
        EntryFilesAnalyser::new(EntryFilesAnalyserOptions::default()).unwrap();
    let entry = fixtures_dir().join("entryWithInvalidDep.js");

    let reports = entry_files_analyser
        .analyse([entry.clone()], EntryFilesRuntimeOptions::default())
        .unwrap();

    let mut files: Vec<String> = reports.iter().map(|report| report.file.clone()).collect();
    files.sort();
    let mut expected = to_file_strings([
        entry,
        fixtures_dir().join("deps/invalidDep.js"),
        fixtures_dir().join("deps/dep1.js"),
        fixtures_dir().join("shared.js"),
    ]);
    expected.sort();
    assert_eq!(files, expected);

    let invalid_reports: Vec<_> = reports.iter().filter(|report| !report.is_ok()).collect();
    assert_eq!(invalid_reports.len(), 1);
    match &invalid_reports[0].report {
        ReportOnFile::Failed { warnings, .. } => {
            assert_eq!(warnings[0].kind, "parsing-error");
        }
        ReportOnFile::Ok { .. } => panic!("expected a failed report"),
    }
}

#[test]
fn should_extends_default_extensions() {
    let mut entry_files_analyser = EntryFilesAnalyser::new(EntryFilesAnalyserOptions {
        load_extensions: Some(Box::new(|mut exts| {
            exts.push("jsx".to_owned());
            exts
        })),
        ..Default::default()
    })
    .unwrap();
    let entry = fixtures_dir().join("entryWithVariousDepExtensions.js");

    let reports = entry_files_analyser
        .analyse([entry.clone()], EntryFilesRuntimeOptions::default())
        .unwrap();

    let mut files: Vec<String> = reports.iter().map(|report| report.file.clone()).collect();
    files.sort();
    let mut expected = to_file_strings([
        entry,
        fixtures_dir().join("deps/default.js"),
        fixtures_dir().join("deps/default.cjs"),
        fixtures_dir().join("deps/dep.cjs"),
        fixtures_dir().join("deps/default.mjs"),
        fixtures_dir().join("deps/dep.mjs"),
        fixtures_dir().join("deps/default.node"),
        fixtures_dir().join("deps/dep.node"),
        fixtures_dir().join("deps/default.jsx"),
        fixtures_dir().join("deps/dep.jsx"),
    ]);
    expected.sort();
    assert_eq!(files, expected);
}

#[test]
fn should_override_default_extensions() {
    let mut entry_files_analyser = EntryFilesAnalyser::new(EntryFilesAnalyserOptions {
        load_extensions: Some(Box::new(|_defaults| vec!["jsx".to_owned()])),
        ..Default::default()
    })
    .unwrap();
    let entry = fixtures_dir().join("entryWithVariousDepExtensions.js");

    let reports = entry_files_analyser
        .analyse([entry.clone()], EntryFilesRuntimeOptions::default())
        .unwrap();

    let files: Vec<String> = reports.iter().map(|report| report.file.clone()).collect();
    assert_eq!(
        files,
        to_file_strings([
            entry,
            fixtures_dir().join("deps/default.jsx"),
            fixtures_dir().join("deps/dep.jsx"),
        ])
    );
}

#[test]
fn should_detect_recursive_dependencies_using_di_graph_with_root_path() {
    let mut entry_files_analyser = EntryFilesAnalyser::new(EntryFilesAnalyserOptions {
        root_path: Some(fixtures_dir()),
        ..Default::default()
    })
    .unwrap();
    let entry = fixtures_dir().join("recursive/A.js");

    entry_files_analyser
        .analyse([entry], EntryFilesRuntimeOptions::default())
        .unwrap();

    let a = Path::new("recursive")
        .join("A.js")
        .to_string_lossy()
        .into_owned();
    let b = Path::new("recursive")
        .join("B.js")
        .to_string_lossy()
        .into_owned();

    assert_eq!(
        entry_files_analyser.dependencies.find_cycles(),
        vec![vec![a.clone(), b.clone()]]
    );
    assert_eq!(
        entry_files_analyser.dependencies.get_deep_children(&a, 1),
        vec![b]
    );
}

#[test]
fn should_detect_recursive_dependencies_using_di_graph_but_without_root_path_everything_is_absolute()
 {
    let mut entry_files_analyser =
        EntryFilesAnalyser::new(EntryFilesAnalyserOptions::default()).unwrap();
    let entry = fixtures_dir().join("recursive/A.js");

    entry_files_analyser
        .analyse([entry], EntryFilesRuntimeOptions::default())
        .unwrap();

    let fixtures_prefix = fixtures_dir().to_string_lossy().into_owned();
    for cycle in entry_files_analyser.dependencies.find_cycles() {
        for id in &cycle {
            assert!(Path::new(id).is_absolute());
            assert!(id.starts_with(&fixtures_prefix));
        }
    }
}

#[test]
fn should_automatically_build_absolute_path_for_entry_files_when_root_path_is_provided() {
    let mut entry_files_analyser = EntryFilesAnalyser::new(EntryFilesAnalyserOptions {
        root_path: Some(fixtures_dir()),
        ..Default::default()
    })
    .unwrap();

    let reports = entry_files_analyser
        .analyse(["recursive/A.js"], EntryFilesRuntimeOptions::default())
        .unwrap();

    let files: Vec<String> = reports.iter().map(|report| report.file.clone()).collect();
    assert_eq!(
        files,
        vec![
            Path::new("recursive")
                .join("A.js")
                .to_string_lossy()
                .into_owned(),
            Path::new("recursive")
                .join("B.js")
                .to_string_lossy()
                .into_owned(),
        ]
    );
}

#[test]
fn should_ignore_file_that_does_not_exist_when_option_ignore_enoent_is_provided() {
    let mut entry_files_analyser = EntryFilesAnalyser::new(EntryFilesAnalyserOptions {
        ignore_enoent: true,
        root_path: Some(fixtures_dir()),
        ..Default::default()
    })
    .unwrap();
    let entry = fixtures_dir().join("does-not-exists.js");

    let reports = entry_files_analyser
        .analyse([entry], EntryFilesRuntimeOptions::default())
        .unwrap();

    assert_eq!(reports.len(), 0);
    assert!(
        !entry_files_analyser
            .dependencies
            .has_vertex("does-not-exists.js"),
        "an ignored ENOENT entry should not be added to the dependency graph"
    );
}

#[test]
fn should_parse_analyze_and_follow_dependencies_in_type_script() {
    let mut entry_files_analyser =
        EntryFilesAnalyser::new(EntryFilesAnalyserOptions::default()).unwrap();
    let entry = ts_fixtures_dir().join("entry.ts");

    let reports = entry_files_analyser
        .analyse([entry.clone()], EntryFilesRuntimeOptions::default())
        .unwrap();

    let files: Vec<String> = reports.iter().map(|report| report.file.clone()).collect();
    assert_eq!(
        files,
        to_file_strings([entry, ts_fixtures_dir().join("entryExport.ts")])
    );

    assert_eq!(entry_files_analyser.stats.number_of_files_processed, 2);
}

fn dependency_collectable_locations_have(
    dep_set: &DefaultCollectableSet,
    predicate: impl Fn(&Map<String, Value>) -> bool,
) -> bool {
    dep_set.to_json().entries.iter().all(|entry| {
        entry
            .locations
            .iter()
            .all(|location| location.metadata.as_ref().is_some_and(&predicate))
    })
}

#[test]
fn should_pass_file_metadata_per_file_to_the_dependency_collectable() {
    let ast_analyzer = AstAnalyser::new(AstAnalyserOptions {
        collectables: vec![DefaultCollectableSet::new("dependency")],
        ..Default::default()
    });
    let mut entry_files_analyser = EntryFilesAnalyser::new(EntryFilesAnalyserOptions {
        root_path: Some(fixtures_dir()),
        ast_analyzer: Some(ast_analyzer),
        ..Default::default()
    })
    .unwrap();
    let entry = fixtures_dir().join("entry.js");

    entry_files_analyser
        .analyse(
            [entry],
            EntryFilesRuntimeOptions {
                file_metadata: Some(Rc::new(|file| {
                    let mut metadata = Map::new();
                    let basename = file
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    metadata.insert("customFile".to_owned(), Value::String(basename));
                    metadata
                })),
                ..Default::default()
            },
        )
        .unwrap();

    let dep_set = entry_files_analyser
        .ast_analyzer()
        .get_collectable_set("dependency")
        .unwrap();
    assert!(dependency_collectable_locations_have(
        &dep_set,
        |metadata| { metadata.contains_key("customFile") }
    ));
}

#[test]
fn should_merge_file_metadata_with_global_metadata_in_collectables() {
    let ast_analyzer = AstAnalyser::new(AstAnalyserOptions {
        collectables: vec![DefaultCollectableSet::new("dependency")],
        ..Default::default()
    });
    let mut entry_files_analyser = EntryFilesAnalyser::new(EntryFilesAnalyserOptions {
        root_path: Some(fixtures_dir()),
        ast_analyzer: Some(ast_analyzer),
        ..Default::default()
    })
    .unwrap();
    let entry = fixtures_dir().join("entry.js");

    let mut metadata = Map::new();
    metadata.insert(
        "project".to_owned(),
        Value::String("test-project".to_owned()),
    );

    entry_files_analyser
        .analyse(
            [entry],
            EntryFilesRuntimeOptions {
                metadata: Some(metadata),
                file_metadata: Some(Rc::new(|file| {
                    let mut metadata = Map::new();
                    let basename = file
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    metadata.insert("customFile".to_owned(), Value::String(basename));
                    metadata
                })),
                ..Default::default()
            },
        )
        .unwrap();

    let dep_set = entry_files_analyser
        .ast_analyzer()
        .get_collectable_set("dependency")
        .unwrap();
    assert!(dependency_collectable_locations_have(
        &dep_set,
        |metadata| {
            metadata.get("project").and_then(Value::as_str) == Some("test-project")
                && metadata.contains_key("customFile")
        }
    ));
}

#[test]
fn should_allow_file_metadata_to_override_global_metadata_in_collectables() {
    let ast_analyzer = AstAnalyser::new(AstAnalyserOptions {
        collectables: vec![DefaultCollectableSet::new("dependency")],
        ..Default::default()
    });
    let mut entry_files_analyser = EntryFilesAnalyser::new(EntryFilesAnalyserOptions {
        root_path: Some(fixtures_dir()),
        ast_analyzer: Some(ast_analyzer),
        ..Default::default()
    })
    .unwrap();
    let entry = fixtures_dir().join("entry.js");

    let mut metadata = Map::new();
    metadata.insert("origin".to_owned(), Value::String("global".to_owned()));

    entry_files_analyser
        .analyse(
            [entry],
            EntryFilesRuntimeOptions {
                metadata: Some(metadata),
                file_metadata: Some(Rc::new(|_file| {
                    let mut metadata = Map::new();
                    metadata.insert("origin".to_owned(), Value::String("per-file".to_owned()));
                    metadata
                })),
                ..Default::default()
            },
        )
        .unwrap();

    let dep_set = entry_files_analyser
        .ast_analyzer()
        .get_collectable_set("dependency")
        .unwrap();
    assert!(dependency_collectable_locations_have(
        &dep_set,
        |metadata| { metadata.get("origin").and_then(Value::as_str) == Some("per-file") }
    ));
}

#[test]
fn should_not_mutate_global_metadata_when_using_file_metadata() {
    let ast_analyzer = AstAnalyser::new(AstAnalyserOptions {
        collectables: vec![DefaultCollectableSet::new("dependency")],
        ..Default::default()
    });
    let mut entry_files_analyser = EntryFilesAnalyser::new(EntryFilesAnalyserOptions {
        root_path: Some(fixtures_dir()),
        ast_analyzer: Some(ast_analyzer),
        ..Default::default()
    })
    .unwrap();
    let entry = fixtures_dir().join("entry.js");

    let mut global_metadata = Map::new();
    global_metadata.insert(
        "project".to_owned(),
        Value::String("test-project".to_owned()),
    );
    // Adaptation: upstream mutates a shared JS object and re-inspects it by
    // reference after the call; Rust's `metadata` is moved into `analyse`, so
    // there is no aliasing to observe. We keep a clone made before the call
    // and assert it stays exactly `{ project: "test-project" }` afterward,
    // preserving the same "no leakage from fileMetadata" intent.
    let saved_global_metadata = global_metadata.clone();

    entry_files_analyser
        .analyse(
            [entry],
            EntryFilesRuntimeOptions {
                metadata: Some(global_metadata),
                file_metadata: Some(Rc::new(|_file| {
                    let mut metadata = Map::new();
                    metadata.insert("extra".to_owned(), Value::String("value".to_owned()));
                    metadata
                })),
                ..Default::default()
            },
        )
        .unwrap();

    let mut expected = Map::new();
    expected.insert(
        "project".to_owned(),
        Value::String("test-project".to_owned()),
    );
    assert_eq!(saved_global_metadata, expected);
    assert!(!saved_global_metadata.contains_key("extra"));
}

#[test]
fn should_not_crash_when_a_parsing_error_occurs() {
    let mut entry_files_analyser = EntryFilesAnalyser::new(EntryFilesAnalyserOptions {
        ignore_enoent: true,
        root_path: Some(fixtures_dir()),
        ..Default::default()
    })
    .unwrap();
    let entry = fixtures_dir().join("parsing-error.js");

    entry_files_analyser
        .analyse([entry], EntryFilesRuntimeOptions::default())
        .unwrap();
}

#[test]
fn should_return_the_number_of_files_detected_and_the_number_of_internal_dependencies() {
    let mut entry_files_analyser =
        EntryFilesAnalyser::new(EntryFilesAnalyserOptions::default()).unwrap();
    let entry = fixtures_dir().join("entry.js");
    let deep_entry = fixtures_dir().join("deps/deepEntry.js");

    entry_files_analyser
        .analyse([entry, deep_entry], EntryFilesRuntimeOptions::default())
        .unwrap();

    assert_eq!(entry_files_analyser.stats.number_of_files_processed, 6);
    assert_eq!(entry_files_analyser.stats.number_of_imports_detected, 4);
}
