//! Upstream: `src/SourceFile.ts`

use std::sync::LazyLock;

use indexmap::IndexMap;
use regex::Regex;
use serde_json::{Map, Value};

use crate::collectable_set::CollectableSetRegistry;
use crate::deobfuscator::Deobfuscator;
use crate::estree::{Node, SourceLocation, node_type, root_location};
use crate::utils::{Base64Options, is_string_base64, is_svg, to_array_location};
use crate::variable_tracer::VariableTracer;
use crate::warnings::{GenerateWarningOptions, Warning, WarningLocation, generate_warning};

const MAXIMUM_ENCODED_LITERALS: usize = 10;

/// "fetch" | "oneline-require" | "is-minified"
pub type SourceFlag = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sensitivity {
    #[default]
    Conservative,
    Aggressive,
}

#[derive(Debug, Clone)]
pub struct Dependency {
    pub unsafe_: bool,
    pub in_try: bool,
    pub location: Option<SourceLocation>,
}

#[derive(Debug, Default, Clone)]
pub struct SourceFilePath {
    pub location: Option<String>,
}

impl SourceFilePath {
    pub fn use_location(&mut self, location: Option<String>) {
        self.location = location;
    }

    /// posix `path.join` over the stored location and the given parts.
    pub fn resolve(&self, parts: &[&str]) -> String {
        let mut all: Vec<&str> = Vec::new();
        if let Some(location) = &self.location {
            all.push(location);
        }
        all.extend(parts);
        posix_join(&all)
    }
}

fn posix_join(parts: &[&str]) -> String {
    let mut out: Vec<String> = Vec::new();
    for part in parts.iter().flat_map(|p| p.split('/')) {
        match part {
            "" | "." => {}
            ".." => {
                if matches!(out.last().map(String::as_str), None | Some("..")) {
                    out.push("..".to_owned());
                } else {
                    out.pop();
                }
            }
            other => out.push(other.to_owned()),
        }
    }
    let mut joined = out.join("/");
    if parts.first().is_some_and(|p| p.starts_with('/')) {
        joined = format!("/{joined}");
    }
    if joined.is_empty() {
        ".".to_owned()
    } else {
        joined
    }
}

pub struct SourceFile {
    pub tracer: VariableTracer,
    pub in_try_statement: bool,
    pub dependency_auto_warning: bool,
    pub deobfuscator: Deobfuscator,
    pub dependencies: IndexMap<String, Dependency>,
    pub encoded_literals: IndexMap<String, usize>,
    pub warnings: Vec<Warning>,
    pub flags: indexmap::IndexSet<SourceFlag>,
    pub path: SourceFilePath,
    pub sensitivity: Sensitivity,
    pub metadata: Option<Map<String, Value>>,
    pub collectables_set_registry: CollectableSetRegistry,
    pub package_name: Option<String>,
}

pub struct SourceFileOptions {
    pub metadata: Option<Map<String, Value>>,
    pub package_name: Option<String>,
    pub collectable_registry: Option<CollectableSetRegistry>,
}

impl SourceFile {
    pub fn new(source_location: Option<String>, options: SourceFileOptions) -> Self {
        let mut path = SourceFilePath::default();
        path.use_location(source_location);
        Self {
            tracer: VariableTracer::new().enable_default_tracing(),
            in_try_statement: false,
            dependency_auto_warning: false,
            deobfuscator: Deobfuscator::new(),
            dependencies: IndexMap::new(),
            encoded_literals: IndexMap::new(),
            warnings: Vec::new(),
            flags: indexmap::IndexSet::new(),
            path,
            sensitivity: Sensitivity::default(),
            metadata: options.metadata,
            collectables_set_registry: options.collectable_registry.unwrap_or_default(),
            package_name: options.package_name,
        }
    }

    /// Upstream `addDependency`.
    pub fn add_dependency(&mut self, name: &str, location: Option<SourceLocation>) {
        let unsafe_ = self.dependency_auto_warning;
        self.add_dependency_with(name, location, unsafe_);
    }

    pub fn add_dependency_with(
        &mut self,
        name: &str,
        location: Option<SourceLocation>,
        unsafe_: bool,
    ) {
        if name.trim().is_empty() {
            return;
        }

        let dependency_name = name.strip_suffix('/').unwrap_or(name);
        self.dependencies.insert(
            dependency_name.to_owned(),
            Dependency {
                unsafe_,
                in_try: self.in_try_statement,
                location,
            },
        );

        if self.package_name.as_deref() != Some(dependency_name) {
            let mut metadata = self.metadata.clone().unwrap_or_default();
            metadata.insert("inTry".to_owned(), Value::Bool(self.in_try_statement));
            metadata.insert("unsafe".to_owned(), Value::Bool(unsafe_));
            let file = self.path.location.clone();
            self.collectables_set_registry.add(
                "dependency",
                dependency_name,
                file,
                to_array_location(location),
                Some(metadata),
            );
        }

        if self.dependency_auto_warning {
            self.warnings.push(generate_warning(
                "unsafe-import",
                GenerateWarningOptions {
                    value: Some(dependency_name.to_owned()),
                    location,
                    ..Default::default()
                },
            ));
        }
    }

    /// Upstream `addEncodedLiteral`.
    pub fn add_encoded_literal(&mut self, value: &str, location: Option<SourceLocation>) {
        if self.encoded_literals.len() > MAXIMUM_ENCODED_LITERALS {
            return;
        }

        if let Some(&index) = self.encoded_literals.get(value) {
            let array_location = to_array_location(Some(location.unwrap_or_else(root_location)));
            if let Some(warning) = self.warnings.get_mut(index)
                && let WarningLocation::Multiple(locations) = &mut warning.location
            {
                locations.push(array_location);
            }
            return;
        }

        self.warnings.push(generate_warning(
            "encoded-literal",
            GenerateWarningOptions {
                value: Some(value.to_owned()),
                location,
                ..Default::default()
            },
        ));
        self.encoded_literals
            .insert(value.to_owned(), self.warnings.len() - 1);
    }

    /// Upstream `analyzeLiteral`.
    pub fn analyze_literal(&mut self, node: &Node, in_array_expr: bool) {
        let Some(value) = node.get("value").and_then(Value::as_str) else {
            return;
        };
        if is_svg(node) {
            return;
        }
        self.deobfuscator.analyze_string(value);

        static HEX_SEQ: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\\x[a-fA-F0-9]{2}").expect("valid regex"));
        static UNICODE_SEQ: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\\u[a-fA-F0-9]{4}").expect("valid regex"));

        let raw = node.get("raw").and_then(Value::as_str);
        let has_hexadecimal_sequence = raw.is_some_and(|raw| HEX_SEQ.is_match(raw));
        let has_unicode_sequence = raw.is_some_and(|raw| UNICODE_SEQ.is_match(raw));
        let is_base64 = is_string_base64(
            value,
            Base64Options {
                allow_empty: Some(false),
                ..Default::default()
            },
        );

        if (has_hexadecimal_sequence || has_unicode_sequence) && is_base64 {
            if in_array_expr {
                self.deobfuscator.encoded_array_value += 1;
            } else {
                self.add_encoded_literal(value, SourceLocation::from_node(node));
            }
        }
    }

    /// Upstream `getResult` (the flags/executionTime parts live on the
    /// analyser). Returns `(idsLengthAvg, stringScore)` after appending the
    /// summary warnings.
    pub fn get_result(&mut self, is_minified: bool) -> (f64, f64) {
        if let Some(obfuscator_name) = self.deobfuscator.assert_obfuscation() {
            self.warnings.push(generate_warning(
                "obfuscated-code",
                GenerateWarningOptions {
                    value: Some(obfuscator_name),
                    ..Default::default()
                },
            ));
        }

        let mut filtered_len = 0usize;
        let mut filtered_sum = 0usize;
        for value in &self.deobfuscator.identifiers {
            if value.r#type != "Property" {
                filtered_len += 1;
                filtered_sum += value.name.encode_utf16().count();
            }
        }
        let ids_length_avg = if filtered_len == 0 {
            0.0
        } else {
            filtered_sum as f64 / filtered_len as f64
        };
        let string_score = mean(&self.deobfuscator.literal_scores);

        if !is_minified && filtered_len > 5 && ids_length_avg <= 1.5 {
            self.warnings.push(generate_warning(
                "short-identifiers",
                GenerateWarningOptions {
                    value: Some(js_float_string(ids_length_avg)),
                    ..Default::default()
                },
            ));
        }
        if string_score >= 3.0 {
            self.warnings.push(generate_warning(
                "suspicious-literal",
                GenerateWarningOptions {
                    value: Some(js_float_string(string_score)),
                    ..Default::default()
                },
            ));
        }

        if self.encoded_literals.len() > MAXIMUM_ENCODED_LITERALS {
            self.warnings.push(generate_warning(
                "suspicious-file",
                GenerateWarningOptions::default(),
            ));
            self.warnings
                .retain(|warning| warning.kind != "encoded-literal");
        }

        (ids_length_avg, string_score)
    }

    /// The per-node bookkeeping part of upstream `SourceFile.walk` (the
    /// probe callback orchestration lives in `AstAnalyser`).
    pub fn walk_bookkeeping(&mut self, node: &Node) {
        self.tracer.walk(node);
        self.deobfuscator.walk(node);

        // Detect TryStatement / CatchClause to know which dependency is
        // required inside a try {} clause.
        match node_type(node) {
            Some("TryStatement") if !node.get("handler").is_none_or(Value::is_null) => {
                self.in_try_statement = true;
            }
            Some("CatchClause") => self.in_try_statement = false,
            _ => {}
        }
    }
}

fn mean(arr: &[u32]) -> f64 {
    if arr.is_empty() {
        0.0
    } else {
        arr.iter().map(|v| *v as f64).sum::<f64>() / arr.len() as f64
    }
}

/// `String(number)` for the warning values above.
pub fn js_float_string(value: f64) -> String {
    if value == value.trunc() && value.abs() < 1e21 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}
