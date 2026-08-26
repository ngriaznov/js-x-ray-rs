//! Benchmark the Rust port over the etalon corpus (mirror of
//! `tools/bench/bench-upstream.mjs`).
//!
//! ```bash
//! cargo run --release --example bench -- [iterations]
//! ```

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::{Value, json};

use js_x_ray_rs::ast_analyser::{
    AstAnalyser, AstAnalyserOptions, OptionalWarnings, RuntimeOptions,
};
use js_x_ray_rs::collectable_set::DefaultCollectableSet;
use js_x_ray_rs::source_file::Sensitivity;

struct Case {
    code: String,
    analyser_options: Value,
    runtime_options: Value,
}

fn load_cases(dir: &Path, etalon_root: &Path, out: &mut Vec<Case>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("corpus dir readable")
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            load_cases(&path, etalon_root, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let case: Value =
                serde_json::from_str(&std::fs::read_to_string(&path).expect("readable"))
                    .expect("valid JSON");
            let code = match case.get("code").and_then(Value::as_str) {
                Some(code) => code.to_owned(),
                None => {
                    let file = case
                        .get("file")
                        .and_then(Value::as_str)
                        .expect("code or file");
                    std::fs::read_to_string(etalon_root.join(file)).expect("fixture readable")
                }
            };
            out.push(Case {
                code,
                analyser_options: case.get("analyserOptions").cloned().unwrap_or(Value::Null),
                runtime_options: case.get("runtimeOptions").cloned().unwrap_or(Value::Null),
            });
        }
    }
}

fn run_all(cases: &[Case]) -> usize {
    let mut warnings = 0usize;
    for case in cases {
        let optional_warnings = match case.analyser_options.get("optionalWarnings") {
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
        let sensitivity = match case
            .analyser_options
            .get("sensitivity")
            .and_then(Value::as_str)
        {
            Some("aggressive") => Sensitivity::Aggressive,
            _ => Sensitivity::Conservative,
        };
        let analyser = AstAnalyser::new(AstAnalyserOptions {
            optional_warnings,
            sensitivity,
            collectables: vec![DefaultCollectableSet::new("dependency")],
            ..Default::default()
        });
        let runtime = RuntimeOptions {
            remove_html_comments: case
                .runtime_options
                .get("removeHTMLComments")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_minified: case
                .runtime_options
                .get("isMinified")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            package_name: case
                .runtime_options
                .get("packageName")
                .and_then(Value::as_str)
                .map(str::to_owned),
            ..Default::default()
        };
        match analyser.analyse(&case.code, runtime) {
            Ok(report) => warnings += report.warnings.len(),
            Err(_) => warnings += 1,
        }
    }
    warnings
}

fn main() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root");
    let etalon_root = repo_root.join("tests/etalon");

    let mut cases = Vec::new();
    load_cases(&etalon_root.join("corpus"), &etalon_root, &mut cases);

    let iterations: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(10);

    run_all(&cases); // warmup

    let mut times = Vec::with_capacity(iterations);
    for i in 0..iterations {
        let t0 = Instant::now();
        let warnings = run_all(&cases);
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
        if i == 0 {
            eprintln!("sanity: {warnings} warnings over {} cases", cases.len());
        }
    }
    times.sort_by(|a, b| a.total_cmp(b));
    let stats = json!({
        "impl": "rust-native",
        "cases": cases.len(),
        "iterations": iterations,
        "best_ms": times[0],
        "median_ms": times[times.len() / 2],
        "mean_ms": times.iter().sum::<f64>() / times.len() as f64,
    });
    println!("{stats}");
}
