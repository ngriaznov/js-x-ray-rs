//! Upstream: `src/probes/isLiteral.ts`

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use crate::estree::{Node, SourceLocation};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::shady_link::{IsIpAddressSafeOptions, IsUrlSafeOptions, ShadyLink};
use crate::utils::hex;
use crate::warnings::{GenerateWarningOptions, Severity, generate_warning};

/// `require("node:module").builtinModules` (no `node:`-prefixed entries).
const NODE_BUILTIN_MODULES: &[&str] = &[
    "_http_agent",
    "_http_client",
    "_http_common",
    "_http_incoming",
    "_http_outgoing",
    "_http_server",
    "_stream_duplex",
    "_stream_passthrough",
    "_stream_readable",
    "_stream_transform",
    "_stream_wrap",
    "_stream_writable",
    "_tls_common",
    "_tls_wrap",
    "assert",
    "assert/strict",
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "diagnostics_channel",
    "dns",
    "dns/promises",
    "domain",
    "events",
    "fs",
    "fs/promises",
    "http",
    "http2",
    "https",
    "inspector",
    "inspector/promises",
    "module",
    "net",
    "os",
    "path",
    "path/posix",
    "path/win32",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "readline/promises",
    "repl",
    "stream",
    "stream/consumers",
    "stream/promises",
    "stream/web",
    "string_decoder",
    "sys",
    "timers",
    "timers/promises",
    "tls",
    "trace_events",
    "tty",
    "url",
    "util",
    "util/types",
    "v8",
    "vm",
    "wasi",
    "worker_threads",
    "zlib",
];

static EMAIL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[^.\s@:](?:[^\s@:]*[^\s@:.])?@[^.\s@]+(?:\.[^.\s@]+)*$").expect("valid regex")
});

#[derive(Debug, Default)]
pub struct IsLiteral;

impl Probe for IsLiteral {
    fn name(&self) -> &'static str {
        "is-literal"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["Literal"])
    }

    fn validate_node(&mut self, node: &Node, _ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        (node.get("type").and_then(Value::as_str) == Some("Literal")
            && node.get("value").is_some_and(Value::is_string))
        .then_some(Value::Null)
    }

    fn main(&mut self, node: &Node, _data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let source_file = &mut *ctx.source_file;
        let value = node.get("value").and_then(Value::as_str).unwrap_or("");
        let location = SourceLocation::from_node(node);

        if hex::is_hex_str(value) {
            let decoded = hex::decode_hex_lossy(value);
            source_file.deobfuscator.analyze_string(&decoded);

            if NODE_BUILTIN_MODULES.contains(&decoded.as_str()) {
                source_file.add_dependency(&decoded, location);
                source_file.warnings.push(generate_warning(
                    "unsafe-import",
                    GenerateWarningOptions {
                        location,
                        ..Default::default()
                    },
                ));
            } else if decoded == "require" || !hex::is_safe(&Value::String(value.to_owned())) {
                source_file.add_encoded_literal(value, location);
            }
        } else if source_file.collectables_set_registry.has("email") && EMAIL_REGEX.is_match(value)
        {
            let file = source_file.path.location.clone();
            let metadata = source_file.metadata.clone();
            source_file.collectables_set_registry.add(
                "email",
                value,
                file,
                crate::utils::to_array_location(location),
                metadata,
            );
            return ProbeReturn::Matched;
        } else if ShadyLink::is_valid_ip_address(value) {
            let file = source_file.path.location.clone();
            let metadata = source_file.metadata.clone();
            let result = ShadyLink::is_ip_address_safe(
                value,
                IsIpAddressSafeOptions {
                    collectable_set_registry: &mut source_file.collectables_set_registry,
                    file: file.as_deref(),
                    location,
                    metadata: metadata.as_ref(),
                },
            );
            if !result.safe {
                source_file.warnings.push(generate_warning(
                    "shady-link",
                    GenerateWarningOptions {
                        value: Some(value.to_owned()),
                        location,
                        severity: Some(Severity::Information),
                        ..Default::default()
                    },
                ));
                return ProbeReturn::Matched;
            }
        } else {
            let file = source_file.path.location.clone();
            let metadata = source_file.metadata.clone();
            let result = ShadyLink::is_url_safe(
                value,
                IsUrlSafeOptions {
                    collectable_set_registry: &mut source_file.collectables_set_registry,
                    file: file.as_deref(),
                    location,
                    metadata: metadata.as_ref(),
                },
            );
            if !result.safe {
                source_file.warnings.push(generate_warning(
                    "shady-link",
                    GenerateWarningOptions {
                        value: Some(value.to_owned()),
                        location,
                        severity: Some(if result.is_local_address {
                            Severity::Information
                        } else {
                            Severity::Warning
                        }),
                        ..Default::default()
                    },
                ));
                return ProbeReturn::Matched;
            }

            source_file.analyze_literal(node, false);
        }

        ProbeReturn::Matched
    }
}
