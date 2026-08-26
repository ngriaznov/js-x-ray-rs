//! Analyse a file or inline code and print the report as JSON.
//!
//! ```bash
//! cargo run --example analyse -- path/to/file.js
//! echo 'eval("2+2")' | cargo run --example analyse
//! ```

use std::io::Read;

use js_x_ray_rs::ast_analyser::{
    AstAnalyser, AstAnalyserOptions, OptionalWarnings, RuntimeOptions,
};
use serde_json::json;

fn main() {
    let analyser = AstAnalyser::new(AstAnalyserOptions {
        optional_warnings: OptionalWarnings::All,
        ..Default::default()
    });

    let arg = std::env::args().nth(1);
    let source = match &arg {
        Some(path) => std::fs::read_to_string(path).expect("readable file"),
        None => {
            let mut buffer = String::new();
            std::io::stdin()
                .read_to_string(&mut buffer)
                .expect("readable stdin");
            buffer
        }
    };

    match analyser.analyse(&source, RuntimeOptions::default()) {
        Ok(report) => {
            let out = json!({
                "warnings": report.warnings,
                "dependencies": report.dependencies.keys().collect::<Vec<_>>(),
                "flags": report.flags.iter().collect::<Vec<_>>(),
                "idsLengthAvg": report.ids_length_avg,
                "stringScore": report.string_score,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&out).expect("serializable")
            );
        }
        Err(error) => {
            eprintln!("parsing error: {error}");
            std::process::exit(1);
        }
    }
}
