//! Upstream: `src/AstAnalyser.ts`

use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use crate::collectable_set::{CollectableSetRegistry, DefaultCollectableSet};
use crate::estree::{
    GetCallExpressionIdentifierOptions, Node, get_call_expression_identifier, is_call_expression,
    is_string_literal,
};
use crate::inlined::InlinedRequire;
use crate::obfuscators::trojan_source;
use crate::parser::{JsSourceParser, ParseError, SourceParser};
use crate::pipelines::{Inline, Pipeline, PipelineRunner};
use crate::probe::{Probe, ProbeRunner, WalkAction};
use crate::probes::{default_probes, optional_probe, optional_probe_names};
use crate::source_file::{Sensitivity, SourceFile, SourceFileOptions};
use crate::utils::{is_minified_code, is_one_line_expression_export};
use crate::walker::walk_enter;
use crate::warnings::{GenerateWarningOptions, Warning, generate_warning};

/// Upstream `AstAnalyserOptions.optionalWarnings`.
#[derive(Debug, Default, Clone)]
pub enum OptionalWarnings {
    #[default]
    Disabled,
    All,
    /// Explicit names; entries ending in `.*` are prefix patterns.
    Names(Vec<String>),
}

/// A probe factory: probes carry per-analysis state, so the analyser stores
/// constructors instead of instances (upstream reuses instances but resets
/// their `context` from `kProbeOriginalContext` in `ProbeRunner.finalize`).
pub type ProbeFactory = Box<dyn Fn() -> Box<dyn Probe>>;
pub type PipelineFactory = Box<dyn Fn() -> Box<dyn Pipeline>>;
/// Hook receiving the analysis' `SourceFile` (upstream `initialize`/`finalize`).
pub type SourceFileHook = Box<dyn FnOnce(&mut SourceFile)>;

#[derive(Default)]
pub struct AstAnalyserOptions {
    pub custom_probes: Vec<ProbeFactory>,
    pub skip_default_probes: bool,
    pub optional_warnings: OptionalWarnings,
    pub pipelines: Vec<PipelineFactory>,
    pub collectables: Vec<DefaultCollectableSet>,
    pub sensitivity: Sensitivity,
}

#[derive(Default)]
pub struct RuntimeOptions {
    pub location: Option<String>,
    pub remove_html_comments: bool,
    pub is_minified: bool,
    pub custom_parser: Option<Box<dyn SourceParser>>,
    pub initialize: Option<SourceFileHook>,
    pub finalize: Option<SourceFileHook>,
    pub metadata: Option<Map<String, Value>>,
    pub package_name: Option<String>,
}

/// Upstream `Report`.
#[derive(Debug)]
pub struct Report {
    pub warnings: Vec<Warning>,
    pub dependencies: indexmap::IndexMap<String, crate::source_file::Dependency>,
    pub flags: indexmap::IndexSet<String>,
    pub ids_length_avg: f64,
    pub string_score: f64,
    /// Milliseconds.
    pub execution_time: f64,
}

/// Upstream `ReportOnFile`.
#[derive(Debug)]
pub enum ReportOnFile {
    Ok {
        warnings: Vec<Warning>,
        dependencies: indexmap::IndexMap<String, crate::source_file::Dependency>,
        flags: indexmap::IndexSet<String>,
        execution_time: f64,
    },
    Failed {
        warnings: Vec<Warning>,
        execution_time: f64,
    },
}

/// Monotonic timer for `executionTime`; `Instant::now` traps on
/// wasm32-unknown-unknown, where the reported time is 0.
struct Stopwatch {
    #[cfg(not(target_arch = "wasm32"))]
    start: std::time::Instant,
}

impl Stopwatch {
    fn start() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            start: std::time::Instant::now(),
        }
    }

    fn elapsed_ms(&self) -> f64 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.start.elapsed().as_secs_f64() * 1000.0
        }
        #[cfg(target_arch = "wasm32")]
        {
            0.0
        }
    }
}

pub struct AstAnalyser {
    options: AstAnalyserOptions,
    /// Shared with each analysis run: taken before, restored after — upstream
    /// shares the registry object by reference across runs.
    collectable_registry: std::cell::RefCell<CollectableSetRegistry>,
}

impl Default for AstAnalyser {
    fn default() -> Self {
        Self::new(AstAnalyserOptions::default())
    }
}

impl AstAnalyser {
    pub fn new(options: AstAnalyserOptions) -> Self {
        let collectable_registry =
            std::cell::RefCell::new(CollectableSetRegistry::new(options.collectables.clone()));
        Self {
            options,
            collectable_registry,
        }
    }

    /// Upstream `AstAnalyser.probes`: the resolved probe list (defaults plus
    /// custom probes and any activated optional warnings). Exposed for
    /// introspection; each call builds fresh probe instances since probes
    /// carry per-analysis state.
    pub fn probes(&self) -> Vec<Box<dyn Probe>> {
        self.build_probes()
    }

    fn build_probes(&self) -> Vec<Box<dyn Probe>> {
        let mut probes =
            if !self.options.custom_probes.is_empty() && self.options.skip_default_probes {
                Vec::new()
            } else {
                default_probes()
            };
        for factory in &self.options.custom_probes {
            probes.push(factory());
        }

        match &self.options.optional_warnings {
            OptionalWarnings::Disabled => {}
            OptionalWarnings::All => {
                for name in optional_probe_names() {
                    probes.extend(optional_probe(name));
                }
            }
            OptionalWarnings::Names(names) => {
                for warning in names {
                    if let Some(prefix) = warning.strip_suffix(".*") {
                        for key in optional_probe_names() {
                            if key.starts_with(&format!("{prefix}.")) {
                                probes.extend(optional_probe(key));
                            }
                        }
                    } else {
                        probes.extend(optional_probe(warning));
                    }
                }
            }
        }

        probes
    }

    fn build_pipelines(&self) -> PipelineRunner {
        let mut pipelines: Vec<Box<dyn Pipeline>> = self
            .options
            .pipelines
            .iter()
            .map(|factory| factory())
            .collect();
        pipelines.push(Box::new(Inline));
        PipelineRunner::new(pipelines)
    }

    /// Upstream `analyse`.
    pub fn analyse(&self, str_: &str, options: RuntimeOptions) -> Result<Report, ParseError> {
        let start_time = Stopwatch::start();

        let RuntimeOptions {
            location,
            remove_html_comments,
            is_minified,
            custom_parser,
            initialize,
            finalize,
            metadata,
            package_name,
        } = options;

        let prepared = prepare_source(str_, remove_html_comments);
        let body = match &custom_parser {
            Some(parser) => parser.parse(&prepared)?,
            None => JsSourceParser.parse(&prepared)?,
        };

        let registry = std::mem::take(&mut *self.collectable_registry.borrow_mut());
        let mut source = SourceFile::new(
            location,
            SourceFileOptions {
                metadata,
                package_name,
                collectable_registry: Some(registry),
            },
        );
        source.sensitivity = self.options.sensitivity;

        if trojan_source::verify(str_) {
            source.warnings.push(generate_warning(
                "obfuscated-code",
                GenerateWarningOptions {
                    value: Some("trojan-source".to_owned()),
                    ..Default::default()
                },
            ));
        }

        let mut probe_runner = ProbeRunner::new(&mut source, self.build_probes());

        if let Some(initialize) = initialize {
            initialize(&mut source);
        }

        // Upstream evaluates isOneLineExpressionExport on the body array
        // AFTER the pipelines ran over it (sharing element references);
        // this port checked a pristine pre-pipeline clone, which is the same
        // observation — so check before handing the body to the pipelines
        // and skip the full-AST clone.
        let oneline_require = is_one_line_expression_export(&body);

        // We walk each AST node; this is purely synchronous.
        let reduced_body = self.build_pipelines().reduce(body);
        let mut reduced_root = Value::Array(reduced_body);
        let mut eval_error: Option<ParseError> = None;
        walk_ast(
            &mut reduced_root,
            &mut source,
            &mut probe_runner,
            &mut eval_error,
        );
        // Upstream: a malformed `eval("...")` body throws synchronously out
        // of `#walkEnter`, so `finalize`/`probeRunner.finalize`/the
        // oneline-require flag/`getResult` never run. Surface it the same
        // way here, before any of that happens.
        if let Some(err) = eval_error {
            // Still hand the (partially populated) collectable registry
            // back — upstream's registry is a shared-by-reference object
            // that keeps whatever the aborted walk already added to it, so
            // losing it here (it was `mem::take`n out above) would be a
            // self-inflicted regression, not a faithful port of the throw.
            *self.collectable_registry.borrow_mut() =
                std::mem::take(&mut source.collectables_set_registry);
            return Err(err);
        }

        if let Some(finalize) = finalize {
            finalize(&mut source);
        }
        probe_runner.finalize(&mut source);

        if oneline_require {
            source.flags.insert("oneline-require".to_owned());
        }

        let (ids_length_avg, string_score) = source.get_result(is_minified);

        // Hand the collectable registry (with this run's additions) back.
        *self.collectable_registry.borrow_mut() =
            std::mem::take(&mut source.collectables_set_registry);

        let execution_time = start_time.elapsed_ms();

        Ok(Report {
            warnings: source.warnings,
            dependencies: source.dependencies,
            flags: source.flags,
            ids_length_avg,
            string_score,
            execution_time,
        })
    }

    /// Upstream `analyseFileSync` (also covering `analyseFile`).
    #[cfg(feature = "fs")]
    pub fn analyse_file(
        &self,
        path_to_file: &std::path::Path,
        mut options: RuntimeOptions,
    ) -> std::io::Result<ReportOnFile> {
        use crate::parser::TsSourceParser;

        let start_time = Stopwatch::start();
        let file_path_string = path_to_file.to_string_lossy().to_string();

        if file_path_string.contains("d.ts") {
            return Err(std::io::Error::other("Declaration files are not supported"));
        }

        if options.custom_parser.is_none()
            && path_to_file.extension().and_then(|e| e.to_str()) == Some("ts")
        {
            options.custom_parser = Some(Box::new(TsSourceParser));
        }

        let str_ = std::fs::read_to_string(path_to_file)?;
        let is_min = file_path_string.contains(".min") || is_minified_code(&str_);
        let location = path_to_file
            .parent()
            .map(|p| p.to_string_lossy().to_string());

        options.location = location;
        options.is_minified = is_min;

        match self.analyse(&str_, options) {
            Ok(data) => {
                let mut flags = data.flags;
                // Add is-minified flag if the file is minified and not a
                // one-line require.
                if !flags.contains("oneline-require") && is_min {
                    flags.insert("is-minified".to_owned());
                }
                Ok(ReportOnFile::Ok {
                    warnings: data.warnings,
                    dependencies: data.dependencies,
                    flags,
                    execution_time: start_time.elapsed_ms(),
                })
            }
            Err(error) => Ok(ReportOnFile::Failed {
                warnings: vec![generate_warning(
                    "parsing-error",
                    GenerateWarningOptions {
                        value: Some(error.message),
                        ..Default::default()
                    },
                )],
                execution_time: start_time.elapsed_ms(),
            }),
        }
    }

    pub fn get_collectable_set(&self, r#type: &str) -> Option<DefaultCollectableSet> {
        self.collectable_registry.borrow().get(r#type).cloned()
    }

    /// Upstream `prepareSource`.
    pub fn prepare_source(&self, source: &str, remove_html_comments: bool) -> String {
        prepare_source(source, remove_html_comments)
    }
}

/// Upstream `AstAnalyser.prepareSource` (free function form).
pub fn prepare_source(source: &str, remove_html_comments: bool) -> String {
    // If the file starts with a shebang we remove it because the parser
    // fails on it, e.g. #!/usr/bin/env node
    let raw_no_shebang = if source.starts_with('#') {
        match source.find('\n') {
            Some(idx) => &source[idx + 1..],
            // Upstream: `source.slice(source.indexOf("\n") + 1)`. When there is
            // no newline, `indexOf` returns -1 and `slice(0)` yields the whole
            // (unstripped) string back — not an empty string.
            None => source,
        }
    } else {
        source
    };

    if remove_html_comments {
        remove_html_comment(raw_no_shebang)
    } else {
        raw_no_shebang.to_owned()
    }
}

fn remove_html_comment(str_: &str) -> String {
    static HTML_COMMENT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?s)<!--.*?(?:-->)").expect("valid regex"));
    HTML_COMMENT.replace_all(str_, "").into_owned()
}

/// Upstream `AstAnalyser.#walkEnter` + `SourceFile.walk` fused together.
///
/// Upstream parses `eval("...")` bodies inline and lets a parse failure
/// throw straight out of `#walkEnter` (uncaught), aborting `analyse()`
/// before `finalize`/`probeRunner.finalize`/the oneline-require check ever
/// run. `walk_enter`'s closure can't itself return a `Result`, so a failure
/// is recorded into `eval_error` instead; once set, further probe work is
/// skipped (matching the "nothing after the throw executes" behavior) and
/// the caller checks `eval_error` immediately after `walk_ast` returns.
fn walk_ast(
    root: &mut Value,
    source: &mut SourceFile,
    probe_runner: &mut ProbeRunner,
    eval_error: &mut Option<ParseError>,
) {
    walk_enter(root, |ctx, node| {
        if node.is_array() || eval_error.is_some() {
            return;
        }

        // Upstream SourceFile.walk: InlinedRequire split runs first.
        if let Some(split) = InlinedRequire::split(node) {
            source.tracer.walk(&split.virtual_declaration);
            dispatch_tracer_events(source, probe_runner);
            if let Some(rebuild) = &split.rebuild_expression {
                source.tracer.walk(rebuild);
                dispatch_tracer_events(source, probe_runner);
            }

            probe_and_recurse(&split.virtual_declaration, source, probe_runner, ctx, eval_error);
            if let Some(rebuild) = &split.rebuild_expression {
                probe_and_recurse(rebuild, source, probe_runner, ctx, eval_error);
            }
        }

        source.walk_bookkeeping(node);
        dispatch_tracer_events(source, probe_runner);

        probe_and_recurse(node, source, probe_runner, ctx, eval_error);
    });
}

fn dispatch_tracer_events(source: &mut SourceFile, probe_runner: &mut ProbeRunner) {
    let events = source.tracer.drain_events();
    if !events.is_empty() {
        probe_runner.dispatch_events(&events, source);
    }
}

/// The probe callback body of upstream `#walkEnter`, including the recursive
/// analysis of `eval("...")` string bodies.
fn probe_and_recurse(
    probe_node: &Node,
    source: &mut SourceFile,
    probe_runner: &mut ProbeRunner,
    ctx: &mut crate::walker::WalkerContext,
    eval_error: &mut Option<ParseError>,
) {
    let action = probe_runner.walk(probe_node, source);
    if action == WalkAction::Skip {
        ctx.skip();
    }

    if is_eval_call_expr(probe_node)
        && let Some(first_arg) = probe_node.pointer("/arguments/0")
        && is_string_literal(first_arg)
        && let Some(eval_source) = first_arg.get("value").and_then(Value::as_str)
    {
        match JsSourceParser.parse(eval_source) {
            Ok(eval_body) => {
                let mut eval_root = Value::Array(eval_body);
                walk_ast(&mut eval_root, source, probe_runner, eval_error);
            }
            // Upstream: this parse call is unguarded, so a malformed
            // `eval(...)` body throws out of `analyse()` entirely.
            Err(err) => *eval_error = Some(err),
        }
    }
}

fn is_eval_call_expr(node: &Node) -> bool {
    is_call_expression(node)
        && get_call_expression_identifier(node, &GetCallExpressionIdentifierOptions::default())
            .as_deref()
            == Some("eval")
}
