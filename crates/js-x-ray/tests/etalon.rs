//! Etalon (reference-output) tests.
//!
//! `tests/etalon/corpus/**.json` holds inputs; `tests/etalon/snapshots/**.json`
//! holds the output of the ORIGINAL Node.js js-x-ray for each input
//! (regenerate with `node --experimental-strip-types tools/etalon/generate.mjs`).
//! This test replays every corpus case through the Rust port and compares.
//!
//! Environment variables:
//! - `ETALON_FILTER=<substring>` — only run matching cases.
//! - `ETALON_VERBOSE=1` — print per-case diffs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use js_x_ray::ast_analyser::{AstAnalyser, AstAnalyserOptions, OptionalWarnings, RuntimeOptions};
use js_x_ray::collectable_set::DefaultCollectableSet;
use js_x_ray::source_file::Sensitivity;

fn repo_root() -> PathBuf {
    // crates/js-x-ray → repo root
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

fn collect_cases(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_cases(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
            out.push(path);
        }
    }
}

struct CaseResult {
    name: String,
    failure: Option<String>,
}

fn build_analyser(case: &Value) -> AstAnalyser {
    let analyser_options = case.get("analyserOptions").cloned().unwrap_or(Value::Null);
    let optional_warnings = match analyser_options.get("optionalWarnings") {
        Some(Value::Bool(true)) => OptionalWarnings::All,
        Some(Value::Array(names)) => OptionalWarnings::Names(
            names
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
        ),
        _ => OptionalWarnings::Disabled,
    };
    let sensitivity = match analyser_options.get("sensitivity").and_then(Value::as_str) {
        Some("aggressive") => Sensitivity::Aggressive,
        _ => Sensitivity::Conservative,
    };
    AstAnalyser::new(AstAnalyserOptions {
        optional_warnings,
        sensitivity,
        collectables: vec![DefaultCollectableSet::new("dependency")],
        ..Default::default()
    })
}

fn runtime_options(case: &Value) -> RuntimeOptions {
    let runtime = case.get("runtimeOptions").cloned().unwrap_or(Value::Null);
    RuntimeOptions {
        remove_html_comments: runtime
            .get("removeHTMLComments")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_minified: runtime
            .get("isMinified")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        package_name: runtime
            .get("packageName")
            .and_then(Value::as_str)
            .map(str::to_owned),
        ..Default::default()
    }
}

/// Reduce a report to the snapshot shape (see tools/etalon/generate.mjs).
fn run_case(case: &Value, etalon_dir: &Path) -> Value {
    let analyser = build_analyser(case);
    let runtime = runtime_options(case);

    let outcome = if let Some(file) = case.get("file").and_then(Value::as_str) {
        let path = etalon_dir.join(file);
        match analyser.analyse_file(&path, runtime) {
            Ok(js_x_ray::ReportOnFile::Ok {
                warnings, flags, ..
            }) => Ok((warnings, flags, None, None)),
            Ok(js_x_ray::ReportOnFile::Failed { warnings, .. }) => Err(warnings),
            Err(io) => Err(vec![js_x_ray::generate_warning(
                "parsing-error",
                js_x_ray::warnings::GenerateWarningOptions {
                    value: Some(io.to_string()),
                    ..Default::default()
                },
            )]),
        }
    } else {
        let code = case.get("code").and_then(Value::as_str).unwrap_or_default();
        match analyser.analyse(code, runtime) {
            Ok(report) => Ok((
                report.warnings,
                report.flags,
                Some(report.ids_length_avg),
                Some(report.string_score),
            )),
            Err(_error) => Err(vec![]),
        }
    };

    match outcome {
        Ok((warnings, flags, ids_length_avg, string_score)) => {
            let mut warnings_json: Vec<Value> = warnings
                .iter()
                .map(|w| normalize_warning(&serde_json::to_value(w).expect("serializable")))
                .collect();
            warnings_json.sort_by_key(|w| w.to_string());

            let mut flags_json: Vec<&str> = flags.iter().map(String::as_str).collect();
            flags_json.sort_unstable();

            let dependencies: BTreeMap<String, Value> = analyser
                .get_collectable_set("dependency")
                .map(|set| {
                    set.to_json()
                        .entries
                        .into_iter()
                        .map(|entry| {
                            let meta = entry
                                .locations
                                .first()
                                .and_then(|l| l.metadata.clone())
                                .unwrap_or_default();
                            (
                                entry.value,
                                json!({
                                    "unsafe": meta.get("unsafe").cloned().unwrap_or(Value::Bool(false)),
                                    "inTry": meta.get("inTry").cloned().unwrap_or(Value::Bool(false)),
                                }),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();

            let mut out = Map::new();
            out.insert("ok".into(), Value::Bool(true));
            out.insert("warnings".into(), Value::Array(warnings_json));
            out.insert(
                "flags".into(),
                Value::Array(flags_json.into_iter().map(Value::from).collect()),
            );
            if let Some(avg) = ids_length_avg {
                out.insert("idsLengthAvg".into(), round4(avg));
            }
            if let Some(score) = string_score {
                out.insert("stringScore".into(), round4(score));
            }
            out.insert(
                "dependencies".into(),
                Value::Object(dependencies.into_iter().collect()),
            );
            Value::Object(out)
        }
        Err(_warnings) => json!({
            "ok": false,
            "warnings": [{ "kind": "parsing-error" }],
        }),
    }
}

fn round4(value: f64) -> Value {
    let rounded = (value * 10_000.0).round() / 10_000.0;
    serde_json::Number::from_f64(rounded)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

/// Mirror the harness normalization: parsing-error keeps only the kind.
fn normalize_warning(warning: &Value) -> Value {
    if warning.get("kind").and_then(Value::as_str) == Some("parsing-error") {
        return json!({ "kind": "parsing-error" });
    }
    warning.clone()
}

/// Normalize a stored snapshot for comparison (numbers → round4, warning sort).
fn normalize_snapshot(snapshot: &Value) -> Value {
    let mut snapshot = snapshot.clone();
    if let Some(warnings) = snapshot.get_mut("warnings").and_then(Value::as_array_mut) {
        let mut normalized: Vec<Value> = warnings.iter().map(normalize_warning).collect();
        normalized.sort_by_key(|w| w.to_string());
        *warnings = normalized;
    }
    for key in ["idsLengthAvg", "stringScore"] {
        if let Some(number) = snapshot.get_mut(key)
            && let Some(f) = number.as_f64()
        {
            *number = round4(f);
        }
    }
    if let Some(flags) = snapshot.get_mut("flags").and_then(Value::as_array_mut) {
        flags.sort_by_key(|f| f.to_string());
    }
    snapshot
}

#[test]
fn etalon_corpus_matches_upstream_snapshots() {
    let root = repo_root();
    let etalon_dir = root.join("tests/etalon");
    let corpus_dir = etalon_dir.join("corpus");
    let snapshots_dir = etalon_dir.join("snapshots");

    let mut cases = Vec::new();
    collect_cases(&corpus_dir, &mut cases);
    assert!(
        !cases.is_empty(),
        "no corpus cases found under {corpus_dir:?} — generate them first"
    );

    let filter = std::env::var("ETALON_FILTER").unwrap_or_default();
    let verbose = std::env::var("ETALON_VERBOSE").is_ok();

    let mut results: Vec<CaseResult> = Vec::new();
    let mut skipped_missing_snapshot = 0usize;

    for case_path in &cases {
        let rel = case_path.strip_prefix(&corpus_dir).expect("under corpus");
        let rel_str = rel.to_string_lossy().to_string();
        if !filter.is_empty() && !rel_str.contains(&filter) {
            continue;
        }

        let snapshot_path = snapshots_dir.join(rel);
        let Ok(snapshot_raw) = std::fs::read_to_string(&snapshot_path) else {
            skipped_missing_snapshot += 1;
            continue;
        };
        let snapshot: Value = match serde_json::from_str(&snapshot_raw) {
            Ok(v) => v,
            Err(e) => {
                results.push(CaseResult {
                    name: rel_str,
                    failure: Some(format!("unreadable snapshot: {e}")),
                });
                continue;
            }
        };

        let case: Value = serde_json::from_str(
            &std::fs::read_to_string(case_path).expect("corpus file readable"),
        )
        .expect("corpus file is JSON");

        let actual = run_case(&case, &etalon_dir);
        let expected = normalize_snapshot(&snapshot);
        let actual = normalize_snapshot(&actual);

        let failure = if actual == expected {
            None
        } else {
            let diff = if verbose {
                format!(
                    "\n  expected: {}\n  actual:   {}",
                    serde_json::to_string(&expected).unwrap(),
                    serde_json::to_string(&actual).unwrap()
                )
            } else {
                summarize_diff(&expected, &actual)
            };
            Some(diff)
        };
        results.push(CaseResult {
            name: rel_str,
            failure,
        });
    }

    let failed: Vec<&CaseResult> = results.iter().filter(|r| r.failure.is_some()).collect();
    let passed = results.len() - failed.len();
    eprintln!(
        "etalon: {passed}/{} cases match upstream ({skipped_missing_snapshot} without snapshots skipped)",
        results.len()
    );
    if !failed.is_empty() {
        let mut message = format!("{} etalon case(s) diverge from upstream:\n", failed.len());
        for result in failed.iter().take(50) {
            message.push_str(&format!(
                "  - {}{}\n",
                result.name,
                result.failure.as_deref().unwrap_or("")
            ));
        }
        if failed.len() > 50 {
            message.push_str(&format!("  ... and {} more\n", failed.len() - 50));
        }
        panic!("{message}");
    }
}

fn summarize_diff(expected: &Value, actual: &Value) -> String {
    let mut parts = Vec::new();
    for key in ["ok", "warnings", "flags", "idsLengthAvg", "stringScore", "dependencies"] {
        let (e, a) = (expected.get(key), actual.get(key));
        if e != a {
            match key {
                "warnings" => {
                    let kinds = |v: Option<&Value>| -> Vec<String> {
                        v.and_then(Value::as_array)
                            .map(|arr| {
                                arr.iter()
                                    .map(|w| {
                                        w.get("kind")
                                            .and_then(Value::as_str)
                                            .unwrap_or("?")
                                            .to_owned()
                                    })
                                    .collect()
                            })
                            .unwrap_or_default()
                    };
                    parts.push(format!(
                        "warnings expected {:?} got {:?}",
                        kinds(e),
                        kinds(a)
                    ));
                }
                _ => parts.push(format!(
                    "{key} expected {} got {}",
                    e.map(|v| v.to_string()).unwrap_or_else(|| "∅".into()),
                    a.map(|v| v.to_string()).unwrap_or_else(|| "∅".into())
                )),
            }
        }
    }
    format!(" [{}]", parts.join("; "))
}
