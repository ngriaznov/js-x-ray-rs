//! Upstream: `src/probes/isSyncIO.ts`
//!
//! Optional probe (warning kind `synchronous-io`): detect synchronous I/O
//! calls from `fs`, `crypto`, `child_process`, and `zlib`, e.g.
//! `fs.readFileSync(...)`.

use serde_json::Value;

use crate::estree::{Node, SourceLocation};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::probes::tracing::{TraceAllOptions, match_traced_call, trace_all_from_module};
use crate::source_file::SourceFile;
use crate::warnings::{GenerateWarningOptions, generate_warning};

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
        trace_all_from_module(
            &mut source_file.tracer,
            K_SYNC_IO_IDENTIFIER_OR_MEMBER_EXPS,
            TraceAllOptions::default(),
        );
    }

    fn validate_node(&mut self, _node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        match_traced_call(ctx, &K_SYNC_IO_IDENTIFIER_OR_MEMBER_EXPS)
    }

    fn main(&mut self, node: &Node, data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        let value = data
            .as_str()
            .and_then(|name| name.split('.').nth(1))
            .map(str::to_owned);

        let warning = generate_warning(
            "synchronous-io",
            GenerateWarningOptions {
                value,
                location: SourceLocation::from_node(node),
                ..Default::default()
            },
        );
        ctx.source_file.warnings.push(warning);

        ProbeReturn::Matched
    }
}
