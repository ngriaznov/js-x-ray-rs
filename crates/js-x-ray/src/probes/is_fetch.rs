//! Upstream: `src/probes/isFetch.ts`
//!
//! PORT-TODO(stub): faithful port pending.

use serde_json::Value;

use crate::estree::Node;
use crate::probe::{Probe, ProbeCtx, ProbeReturn};

#[derive(Debug, Default)]
pub struct IsFetch;

impl Probe for IsFetch {
    fn name(&self) -> &'static str {
        "is-fetch"
    }

    fn validate_node(&mut self, _node: &Node, _ctx: &mut ProbeCtx<'_>) -> Option<Value> {
        // PORT-TODO(stub)
        None
    }

    fn main(&mut self, _node: &Node, _data: &Value, _ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
        ProbeReturn::Continue
    }
}
