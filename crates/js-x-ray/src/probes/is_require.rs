//! Upstream: `src/probes/isRequire/isRequire.ts` and
//! `src/probes/isRequire/RequireCallExpressionWalker.ts`

use indexmap::IndexSet;
use serde_json::Value;

use crate::estree::{
    GetCallExpressionIdentifierOptions, Node, SourceLocation, array_expression_to_string,
    concat_binary_expression_parts, get_call_expression_arguments, get_call_expression_identifier,
    get_member_expression_identifier, is_call_expression, is_member_expression, is_string_literal,
    is_type, node_type, noop,
};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::source_file::SourceFile;
use crate::utils::hex;
use crate::variable_tracer::VariableTracer;
use crate::walker::{WalkerContext, walk_enter};
use crate::warnings::{GenerateWarningOptions, Warning, generate_warning};

fn unsafe_import_warning(location: Option<SourceLocation>) -> Warning {
    generate_warning(
        "unsafe-import",
        GenerateWarningOptions {
            value: None,
            location,
            ..Default::default()
        },
    )
}

// const foo = "http"; require(foo);
fn validate_node_require(node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<String> {
    let id = get_call_expression_identifier(
        node,
        &GetCallExpressionIdentifierOptions {
            external_identifier_lookup: &noop,
            resolve_call_expression: false,
        },
    )?;

    let data = ctx.source_file.tracer.get_data_from_identifier(&id, true);
    data.is_some_and(|report| report.name == "require")
        .then_some(id)
}

// eval("require")("http")
fn validate_node_eval_require(node: &Node) -> Option<String> {
    let id = get_call_expression_identifier(node, &GetCallExpressionIdentifierOptions::default())?;
    if id != "eval" {
        return None;
    }

    let callee = node.get("callee")?;
    if !is_call_expression(callee) {
        return None;
    }

    let args = get_call_expression_arguments(callee, &noop)?;
    (args.first().map(String::as_str) == Some("require")).then_some(id)
}

#[derive(Debug, Default)]
pub struct IsRequire;

impl Probe for IsRequire {
    fn name(&self) -> &'static str {
        "isRequire"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["CallExpression"])
    }

    fn validate_node(&mut self, node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        validate_node_require(node, ctx)
            .or_else(|| validate_node_eval_require(node))
            .map(Value::String)
    }

    fn main(&mut self, node: &Node, data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let callee_name = data.as_str().unwrap_or("");
        let Some(arguments) = node.get("arguments").and_then(Value::as_array) else {
            return ProbeReturn::Matched;
        };
        let Some(arg) = arguments.first() else {
            return ProbeReturn::Matched;
        };

        if callee_name == "eval" {
            ctx.source_file.dependency_auto_warning = true;
        }
        let location = SourceLocation::from_node(node);

        match node_type(arg) {
            // const foo = "http"; require(foo);
            Some("Identifier") => {
                let name = arg.get("name").and_then(Value::as_str).unwrap_or("");
                if let Some(literal) = ctx.source_file.tracer.literal_identifiers.get(name) {
                    let value = literal.value.clone();
                    ctx.source_file.add_dependency(&value, location);
                } else {
                    ctx.source_file
                        .warnings
                        .push(unsafe_import_warning(location));
                }
            }

            // require("http")
            Some("Literal") => {
                if is_string_literal(arg) {
                    let value = arg.get("value").and_then(Value::as_str).unwrap_or("");
                    ctx.source_file.add_dependency(value, location);
                }
            }

            // require(["ht", "tp"])
            Some("ArrayExpression") => {
                let tracer = &ctx.source_file.tracer;
                let lookup = |name: &str| tracer.literal_identifier_lookup(name);
                let value = array_expression_to_string(arg, &lookup)
                    .concat()
                    .trim()
                    .to_owned();

                if value.is_empty() {
                    ctx.source_file
                        .warnings
                        .push(unsafe_import_warning(location));
                } else {
                    ctx.source_file.add_dependency(&value, location);
                }
            }

            // require("ht" + "tp");
            Some("BinaryExpression") => {
                if arg.get("operator").and_then(Value::as_str) != Some("+") {
                    ctx.source_file
                        .warnings
                        .push(unsafe_import_warning(location));
                } else {
                    let tracer = &ctx.source_file.tracer;
                    let lookup = |name: &str| tracer.literal_identifier_lookup(name);
                    match concat_binary_expression_parts(arg, &lookup, true) {
                        Some(parts) => ctx.source_file.add_dependency(&parts.concat(), location),
                        None => ctx
                            .source_file
                            .warnings
                            .push(unsafe_import_warning(location)),
                    }
                }
            }

            // require(Buffer.from("...", "hex").toString());
            Some("CallExpression") => {
                let (dependencies, trigger_warning) = {
                    let mut walker = RequireCallExpressionWalker::new(&ctx.source_file.tracer);
                    walker.walk(arg)
                };

                for dependency_name in &dependencies {
                    ctx.source_file
                        .add_dependency_with(dependency_name, location, true);
                }
                if trigger_warning {
                    ctx.source_file
                        .warnings
                        .push(unsafe_import_warning(location));
                }

                // We skip walking the tree to avoid anymore warnings...
                return ProbeReturn::Skip;
            }

            _ => {
                ctx.source_file
                    .warnings
                    .push(unsafe_import_warning(location));
            }
        }

        ProbeReturn::Matched
    }

    fn teardown(&mut self, source_file: &mut SourceFile) {
        source_file.dependency_auto_warning = false;
    }

    fn break_on_match(&self) -> bool {
        true
    }

    fn break_group(&self) -> Option<&'static str> {
        Some("import")
    }
}

struct RequireCallExpressionWalker<'a> {
    tracer: &'a VariableTracer,
    dependencies: IndexSet<String>,
    trigger_warning: bool,
}

impl<'a> RequireCallExpressionWalker<'a> {
    fn new(tracer: &'a VariableTracer) -> Self {
        Self {
            tracer,
            dependencies: IndexSet::new(),
            trigger_warning: true,
        }
    }

    fn walk(&mut self, call_expr_node: &Node) -> (IndexSet<String>, bool) {
        self.dependencies.clear();
        self.trigger_warning = true;

        let mut node_clone = call_expr_node.clone();
        walk_enter(&mut node_clone, |wctx, node| self.enter(wctx, node));

        (std::mem::take(&mut self.dependencies), self.trigger_warning)
    }

    fn enter(&mut self, wctx: &mut WalkerContext, node: &Value) {
        if !is_call_expression(node) {
            return;
        }
        let Some(args) = node.get("arguments").and_then(Value::as_array) else {
            return;
        };
        let Some(root_argument) = args.first() else {
            return;
        };

        if is_type(root_argument, "Literal")
            && let Some(value) = root_argument.get("value").and_then(Value::as_str)
            && hex::is_hex_str(value)
        {
            self.dependencies.insert(hex::decode_hex_lossy(value));
            wctx.skip();
            return;
        }

        let callee = node.get("callee");
        let full_name = match callee.filter(|c| is_member_expression(c)) {
            Some(member) => get_member_expression_identifier(member, &noop).join("."),
            None => callee
                .and_then(|c| c.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
        };

        let traced_full_name = self
            .tracer
            .get_data_from_identifier(&full_name, false)
            .map(|report| report.identifier_or_member_expr)
            .unwrap_or(full_name);

        match traced_full_name.as_str() {
            "atob" => self.handle_atob(node),
            "Buffer.from" => self.handle_buffer_from(node),
            "require.resolve" => self.handle_require_resolve(root_argument),
            "path.join" | "path.resolve" => self.handle_path_join(node),
            _ => {}
        }
    }

    fn handle_atob(&mut self, node: &Value) {
        let tracer = self.tracer;
        let lookup = |name: &str| tracer.literal_identifier_lookup(name);
        let Some(arguments) = get_call_expression_arguments(node, &lookup) else {
            return;
        };
        if let Some(first) = arguments.first() {
            self.dependencies.insert(base64_decode_to_string(first));
        }
    }

    fn handle_buffer_from(&mut self, node: &Value) {
        let Some(element) = node
            .get("arguments")
            .and_then(Value::as_array)
            .and_then(|args| args.first())
        else {
            return;
        };

        if is_type(element, "ArrayExpression") {
            let dependency_name = array_expression_to_string(element, &noop)
                .concat()
                .trim()
                .to_owned();
            self.dependencies.insert(dependency_name);
        }
    }

    fn handle_require_resolve(&mut self, node: &Value) {
        if is_string_literal(node) {
            let value = node.get("value").and_then(Value::as_str).unwrap_or("");
            self.dependencies.insert(value.to_owned());
        }
    }

    fn handle_path_join(&mut self, node: &Value) {
        let Some(args) = node.get("arguments").and_then(Value::as_array) else {
            return;
        };
        if !args.iter().all(is_string_literal) {
            return;
        }

        let parts: Vec<&str> = args
            .iter()
            .map(|arg| arg.get("value").and_then(Value::as_str).unwrap_or(""))
            .collect();
        self.dependencies.insert(posix_join(&parts));
        self.trigger_warning = false;
    }
}

/// `path.posix.join`, restricted to the plain-segment case this probe needs
/// (no cwd/absolute resolution): join with `/`, then collapse `.`/`..`
/// segments. Upstream calls `path.posix.join` for both `path.join` and
/// `path.resolve` call patterns (not an actual `path.resolve`).
fn posix_join(parts: &[&str]) -> String {
    let joined = parts
        .iter()
        .copied()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/");
    if joined.is_empty() {
        return ".".to_owned();
    }

    let is_absolute = joined.starts_with('/');
    let mut segments: Vec<&str> = Vec::new();
    for segment in joined.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if !is_absolute && segments.last().is_none_or(|&s| s == "..") {
                    segments.push("..");
                } else {
                    segments.pop();
                }
            }
            other => segments.push(other),
        }
    }

    let normalized = segments.join("/");
    if is_absolute {
        format!("/{normalized}")
    } else if normalized.is_empty() {
        ".".to_owned()
    } else {
        normalized
    }
}

/// Node.js `Buffer.from(input, "base64").toString()` emulation (lenient:
/// accepts the standard and url-safe alphabets, ignores anything else
/// including padding), followed by lossy UTF-8 decoding. Duplicated from
/// `VariableTracer::base64_decode_to_string`, which is private to that file.
fn base64_decode_to_string(input: &str) -> String {
    let mut bytes: Vec<u8> = Vec::with_capacity(input.len() / 4 * 3);
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;

    for ch in input.bytes() {
        let sextet = match ch {
            b'A'..=b'Z' => (ch - b'A') as u32,
            b'a'..=b'z' => (ch - b'a' + 26) as u32,
            b'0'..=b'9' => (ch - b'0' + 52) as u32,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => continue,
        };
        buffer = (buffer << 6) | sextet;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((buffer >> bits) as u8);
        }
    }

    String::from_utf8_lossy(&bytes).into_owned()
}
