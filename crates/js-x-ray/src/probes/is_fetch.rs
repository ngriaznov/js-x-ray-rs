//! Upstream: `src/probes/isFetch.ts`

use serde_json::Value;

use crate::estree::Node;
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::probes::tracing::{TraceAllOptions, match_traced_call, trace_all};
use crate::source_file::SourceFile;

const K_TRACED_FUNCTIONS: [&str; 1] = ["fetch"];

#[derive(Debug, Default)]
pub struct IsFetch;

impl Probe for IsFetch {
    fn name(&self) -> &'static str {
        "isFetch"
    }

    fn node_types(&self) -> Option<&'static [&'static str]> {
        Some(&["CallExpression"])
    }

    fn initialize(&mut self, source_file: &mut SourceFile) {
        trace_all(
            &mut source_file.tracer,
            K_TRACED_FUNCTIONS,
            TraceAllOptions::default(),
        );
    }

    fn validate_node(&mut self, _node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        match_traced_call(ctx, &K_TRACED_FUNCTIONS)
    }

    fn main(&mut self, _node: &Node, _data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        ctx.source_file.flags.insert("fetch".to_owned());

        ProbeReturn::Matched
    }
}
