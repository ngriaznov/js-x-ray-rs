//! Upstream: `src/probes/isSyncIO.ts`
//!
//! Optional probe (warning kind `synchronous-io`): detect synchronous I/O
//! calls from `fs`, `crypto`, `child_process`, and `zlib`, e.g.
//! `fs.readFileSync(...)`.

use serde_json::Value;

use crate::estree::{Node, identifier_name};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::source_file::SourceFile;
use crate::variable_tracer::TraceOptions;
use crate::warnings::{GenerateWarningOptions, generate_warning};

const K_TRACED_NODE_CORE_MODULES: [&str; 4] = ["fs", "crypto", "child_process", "zlib"];

const K_SYNC_IO_IDENTIFIER_OR_MEMBER_EXPS: [&str; 32] = [
    "crypto.pbkdf2Sync",
    "crypto.scryptSync",
    "crypto.generateKeyPairSync",
    "crypto.generateKeySync",
    "crypto.hkdfSync",
    "crypto.randomFillSync",
    "crypto.checkPrimeSync",
    "crypto.argon2Sync",
    "fs.readFileSync",
    "fs.writeFileSync",
    "fs.appendFileSync",
    "fs.readSync",
    "fs.writeSync",
    "fs.readdirSync",
    "fs.statSync",
    "fs.mkdirSync",
    "fs.renameSync",
    "fs.unlinkSync",
    "fs.symlinkSync",
    "fs.openSync",
    "fs.fstatSync",
    "fs.linkSync",
    "fs.realpathSync",
    "child_process.execSync",
    "child_process.spawnSync",
    "child_process.execFileSync",
    "zlib.deflateSync",
    "zlib.inflateSync",
    "zlib.gzipSync",
    "zlib.gunzipSync",
    "zlib.brotliCompressSync",
    "zlib.brotliDecompressSync",
];

#[derive(Debug, Default)]
pub struct IsSyncIo;

impl Probe for IsSyncIo {
    fn name(&self) -> &'static str {
        "isSyncIO"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["CallExpression"])
    }

    fn initialize(&mut self, source_file: &mut SourceFile) {
        for identifier_or_member_exp in K_SYNC_IO_IDENTIFIER_OR_MEMBER_EXPS {
            let module_name = identifier_or_member_exp
                .split('.')
                .next()
                .unwrap_or_default()
                .to_owned();
            source_file.tracer.trace(
                identifier_or_member_exp,
                TraceOptions {
                    follow_consecutive_assignment: true,
                    module_name: Some(module_name),
                    ..Default::default()
                },
            );
        }
    }

    fn validate_node(&mut self, _node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        let tracer = &ctx.source_file.tracer;
        if !K_TRACED_NODE_CORE_MODULES
            .iter()
            .any(|module_name| tracer.imported_modules.contains(*module_name))
        {
            return None;
        }

        let data = ctx.traced_data?;
        data.identifier_or_member_expr
            .ends_with("Sync")
            .then_some(Value::Null)
    }

    fn main(&mut self, node: &Node, _data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let value = node
            .get("callee")
            .and_then(identifier_name)
            .map(str::to_owned);

        let warning = generate_warning(
            "synchronous-io",
            GenerateWarningOptions {
                value,
                location: crate::estree::SourceLocation::from_node(node),
                ..Default::default()
            },
        );
        ctx.source_file.warnings.push(warning);

        ProbeReturn::Matched
    }
}
