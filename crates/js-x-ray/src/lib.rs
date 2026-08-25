//! # js-x-ray
//!
//! JavaScript AST X-Ray analysis — a Rust port of
//! [`@nodesecure/js-x-ray`](https://github.com/NodeSecure/js-x-ray), powered
//! by [oxc](https://oxc.rs).
//!
//! The port is a behavioral clone: sources are parsed with oxc and serialized
//! to the same ESTree JSON shape meriyah produces, and every analysis module
//! maps 1:1 to an upstream TypeScript file (see each module's `Upstream:`
//! header) to keep synchronization with upstream releases mechanical.
//!
//! # Example
//!
//! ```
//! use js_x_ray::{AstAnalyser, RuntimeOptions};
//!
//! let analyser = AstAnalyser::default();
//! let report = analyser
//!     .analyse(
//!         r#"const stream = require("node:stream"); eval("console.log('hello')");"#,
//!         RuntimeOptions::default(),
//!     )
//!     .expect("source parses");
//!
//! for warning in &report.warnings {
//!     println!("{}: {:?}", warning.kind, warning.value);
//! }
//! println!("dependencies: {:?}", report.dependencies.keys().collect::<Vec<_>>());
//! ```

pub mod ast_analyser;
pub mod collectable_set;
pub mod deobfuscator;
#[cfg(feature = "fs")]
pub mod entry_files_analyser;
pub mod estree;
pub mod inlined;
pub mod node_counter;
pub mod obfuscators;
pub mod parser;
pub mod pipelines;
pub mod probe;
pub mod probes;
pub mod shady_link;
pub mod source_file;
pub mod utils;
pub mod variable_tracer;
pub mod walker;
pub mod warnings;

pub use ast_analyser::{
    AstAnalyser, AstAnalyserOptions, OptionalWarnings, Report, ReportOnFile, RuntimeOptions,
};
pub use collectable_set::{CollectableSetRegistry, DefaultCollectableSet};
#[cfg(feature = "fs")]
pub use entry_files_analyser::EntryFilesAnalyser;
pub use parser::{JsSourceParser, ParseError, SourceParser, TsSourceParser};
pub use probe::{Probe, ProbeCtx, ProbeReturn, ProbeRunner};
pub use source_file::{Dependency, Sensitivity, SourceFile};
pub use variable_tracer::{TracedIdentifierReport, TracerEvent, VariableTracer};
pub use warnings::{Severity, Warning, WarningLocation, generate_warning};
