//! Upstream: `src/probes/isRandom.ts`

use serde_json::Value;

use crate::estree::{Node, SourceLocation};
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::source_file::SourceFile;
use crate::variable_tracer::TraceOptions;
use crate::warnings::{GenerateWarningOptions, generate_warning};

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
        source_file.tracer.trace(
            "Math.random",
            TraceOptions {
                follow_consecutive_assignment: true,
                ..Default::default()
            },
        );
    }

    fn validate_node(&mut self, _node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        ctx.traced_data
            .is_some_and(|data| data.name == "Math.random")
            .then_some(Value::Null)
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
