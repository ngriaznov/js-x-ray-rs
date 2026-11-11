//! Upstream: `src/probes/isRandom.ts`

use serde_json::Value;

use crate::estree::{Node, SourceLocation};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::probes::tracing::{TraceAllOptions, match_traced_call, trace_all};
use crate::source_file::SourceFile;
use crate::warnings::{GenerateWarningOptions, generate_warning};

const K_TRACED_FUNCTIONS: [&str; 1] = ["Math.random"];

#[derive(Debug, Default)]
pub struct IsRandom;

impl Probe for IsRandom {
    fn name(&self) -> &'static str {
        "isRandom"
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

    fn main(&mut self, node: &Node, _data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        ctx.source_file.warnings.push(generate_warning(
            "insecure-random",
            GenerateWarningOptions {
                value: None,
                location: SourceLocation::from_node(node),
                ..Default::default()
            },
        ));

        ProbeReturn::Matched
    }
}
