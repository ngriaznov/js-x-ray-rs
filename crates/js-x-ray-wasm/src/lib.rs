//! WASM bindings: `analyse(source, optionsJson)` returning the report as a
//! JSON string, mirroring `new AstAnalyser(options).analyse(source)`.

use js_x_ray_rs::ast_analyser::{
    AstAnalyser, AstAnalyserOptions, OptionalWarnings, RuntimeOptions,
};
use js_x_ray_rs::source_file::Sensitivity;
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

/// Analyse a JavaScript source. `options_json` is an optional JSON object:
/// `{ "optionalWarnings": true | ["log-usage", ...], "sensitivity": "aggressive",
///    "removeHTMLComments": true, "isMinified": false, "packageName": "..." }`
#[wasm_bindgen]
pub fn analyse(source: &str, options_json: Option<String>) -> Result<String, JsError> {
    let options: Value = match options_json {
        Some(raw) if !raw.trim().is_empty() => {
            serde_json::from_str(&raw).map_err(|e| JsError::new(&e.to_string()))?
        }
        _ => Value::Null,
    };

    let optional_warnings = match options.get("optionalWarnings") {
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
    let sensitivity = match options.get("sensitivity").and_then(Value::as_str) {
        Some("aggressive") => Sensitivity::Aggressive,
        _ => Sensitivity::Conservative,
    };

    let analyser = AstAnalyser::new(AstAnalyserOptions {
        optional_warnings,
        sensitivity,
        ..Default::default()
    });

    let runtime = RuntimeOptions {
        remove_html_comments: options
            .get("removeHTMLComments")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_minified: options
            .get("isMinified")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        package_name: options
            .get("packageName")
            .and_then(Value::as_str)
            .map(str::to_owned),
        ..Default::default()
    };

    match analyser.analyse(source, runtime) {
        Ok(report) => {
            let out = json!({
                "ok": true,
                "warnings": report.warnings,
                "dependencies": report.dependencies.iter().map(|(name, dep)| {
                    (name.clone(), json!({ "unsafe": dep.unsafe_, "inTry": dep.in_try }))
                }).collect::<serde_json::Map<String, Value>>(),
                "flags": report.flags.iter().collect::<Vec<_>>(),
                "idsLengthAvg": report.ids_length_avg,
                "stringScore": report.string_score,
                "executionTime": report.execution_time,
            });
            Ok(out.to_string())
        }
        Err(error) => {
            let out = json!({
                "ok": false,
                "warnings": [js_x_ray_rs::warnings::generate_warning(
                    "parsing-error",
                    js_x_ray_rs::warnings::GenerateWarningOptions {
                        value: Some(error.message),
                        ..Default::default()
                    },
                )],
            });
            Ok(out.to_string())
        }
    }
}
