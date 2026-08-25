//! Source parsing: oxc → ESTree-compatible JSON.
//!
//! Upstream: `src/parsers/JsSourceParser.ts` (meriyah) and
//! `src/parsers/TsSourceParser.ts` (typescript-estree). oxc parses both
//! grammars natively; the AST is serialized to the same ESTree JSON shape
//! meriyah produces, then enriched with `loc` objects (1-based lines,
//! 0-based UTF-16 columns) to match `loc: true` in the Node.js parsers.

use oxc_allocator::Allocator;
use oxc_parser::{ParseOptions, Parser};
use oxc_span::SourceType;
use serde_json::Value;

use crate::estree::Node;

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct ParseError {
    pub message: String,
}

/// Trait mirroring upstream `SourceParser.parse(source) -> ESTree.Statement[]`.
pub trait SourceParser {
    /// Returns the Program body as a JSON array value.
    fn parse(&self, source: &str) -> Result<Vec<Node>, ParseError>;
}

/// Upstream: `JsSourceParser` — parses as an ES module first and falls back
/// to CommonJS (top-level `return` allowed) exactly like meriyah's
/// `sourceType: "commonjs"` fallback.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsSourceParser;

impl JsSourceParser {
    pub const FILE_EXTENSIONS: &'static [&'static str] = &[".js", ".cjs", ".mjs", ".jsx"];
}

impl SourceParser for JsSourceParser {
    fn parse(&self, source: &str) -> Result<Vec<Node>, ParseError> {
        let module = SourceType::mjs().with_jsx(true);
        match parse_with(source, module, false) {
            Ok(body) => Ok(body),
            Err(error) => {
                // meriyah: "Illegal return statement" / oxc: "A 'return'
                // statement can only be used within a function body."
                let lowered = error.message.to_lowercase();
                let is_illegal_return =
                    lowered.contains("return statement") || lowered.contains("'return' statement");
                if is_illegal_return {
                    let script = SourceType::cjs().with_jsx(true);
                    parse_with(source, script, true)
                } else {
                    Err(error)
                }
            }
        }
    }
}

/// Upstream: `TsSourceParser` — TypeScript sources.
#[derive(Debug, Default, Clone, Copy)]
pub struct TsSourceParser;

impl TsSourceParser {
    pub const FILE_EXTENSIONS: &'static [&'static str] = &[".ts", ".mts", ".cts", ".tsx"];
}

impl SourceParser for TsSourceParser {
    fn parse(&self, source: &str) -> Result<Vec<Node>, ParseError> {
        // Upstream's `kTypeScriptParsingOptions` sets `jsx: true` unconditionally
        // (regardless of a `.ts` vs `.tsx` extension), so JSX parses by default here too.
        let ts = SourceType::ts().with_jsx(true);
        parse_with(source, ts, false)
    }
}

fn parse_with(
    source: &str,
    source_type: SourceType,
    allow_return_outside_function: bool,
) -> Result<Vec<Node>, ParseError> {
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, source, source_type)
        .with_options(ParseOptions {
            allow_return_outside_function,
            // meriyah emits no ParenthesizedExpression nodes.
            preserve_parens: false,
            ..ParseOptions::default()
        })
        .parse();

    if let Some(diagnostic) = ret.diagnostics.first() {
        let message = diagnostic.message.to_string();
        return Err(ParseError { message });
    }
    if ret.panicked {
        return Err(ParseError {
            message: "Failed to parse source".to_owned(),
        });
    }

    let json = ret.program.to_estree_json(false, false);
    let mut program: Value = serde_json::from_str(&json).map_err(|error| ParseError {
        message: format!("Failed to deserialize AST: {error}"),
    })?;

    let table = LineTable::new(source);
    inject_loc(&mut program, &table);

    match program.get_mut("body") {
        Some(Value::Array(body)) => Ok(std::mem::take(body)),
        _ => Ok(Vec::new()),
    }
}

/// Byte offset → (1-based line, 0-based UTF-16 column) mapping.
///
/// Columns count UTF-16 code units (meriyah/JS semantics). ASCII-only lines
/// take an O(1) fast path; other lines fall back to scanning the line prefix,
/// so pathological non-ASCII one-liners are the only quadratic case.
pub struct LineTable {
    /// Byte offset at which each line starts.
    line_starts: Vec<usize>,
    /// Whether the line holds only ASCII bytes (column == byte offset).
    line_is_ascii: Vec<bool>,
    source: Vec<u8>,
}

impl LineTable {
    pub fn new(source: &str) -> Self {
        let bytes = source.as_bytes();
        let mut line_starts = vec![0usize];
        let mut line_is_ascii = Vec::new();
        let mut current_ascii = true;
        for (idx, &byte) in bytes.iter().enumerate() {
            if byte == b'\n' {
                line_is_ascii.push(current_ascii);
                line_starts.push(idx + 1);
                current_ascii = true;
            } else if !byte.is_ascii() {
                current_ascii = false;
            }
        }
        line_is_ascii.push(current_ascii);
        Self {
            line_starts,
            line_is_ascii,
            source: bytes.to_vec(),
        }
    }

    pub fn position(&self, offset: usize) -> (u64, u64) {
        let offset = offset.min(self.source.len());
        let line_idx = match self.line_starts.binary_search(&offset) {
            Ok(idx) => idx,
            Err(idx) => idx - 1,
        };
        let line_start = self.line_starts[line_idx];
        let column = if self.line_is_ascii[line_idx] {
            (offset - line_start) as u64
        } else {
            match std::str::from_utf8(&self.source[line_start..offset]) {
                Ok(s) => s.encode_utf16().count() as u64,
                Err(_) => (offset - line_start) as u64,
            }
        };
        ((line_idx + 1) as u64, column)
    }
}

/// Add a meriyah-style `loc` field to every node carrying start/end offsets.
fn inject_loc(value: &mut Value, table: &LineTable) {
    match value {
        Value::Object(map) => {
            let start = map.get("start").and_then(Value::as_u64);
            let end = map.get("end").and_then(Value::as_u64);
            let is_node = map.get("type").is_some_and(Value::is_string);
            if is_node && let (Some(start), Some(end)) = (start, end) {
                let (start_line, start_column) = table.position(start as usize);
                let (end_line, end_column) = table.position(end as usize);
                map.insert(
                    "loc".to_owned(),
                    serde_json::json!({
                        "start": { "line": start_line, "column": start_column },
                        "end": { "line": end_line, "column": end_column },
                    }),
                );
            }
            for (_key, child) in map.iter_mut() {
                inject_loc(child, table);
            }
        }
        Value::Array(items) => {
            for item in items {
                inject_loc(item, table);
            }
        }
        _ => {}
    }
}
