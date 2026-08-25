//! Upstream: `src/probes/isFetch.ts`

use serde_json::Value;

use crate::estree::Node;
use crate::probe::{Probe, ProbeCtx, ProbeReturn};
use crate::source_file::SourceFile;
use crate::variable_tracer::TraceOptions;

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
        source_file.tracer.trace(
            "fetch",
            TraceOptions {
                follow_consecutive_assignment: true,
                ..Default::default()
            },
        );
    }

    fn validate_node(&mut self, _node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        ctx.traced_data
            .is_some_and(|data| data.identifier_or_member_expr == "fetch")
            .then_some(Value::Null)
    }

    fn main(&mut self, _node: &Node, _data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        ctx.source_file.flags.insert("fetch".to_owned());

        ProbeReturn::Matched
    }
}
