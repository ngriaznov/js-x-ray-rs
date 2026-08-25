//! Upstream: `src/pipelines/deobfuscate.ts`
//!
//! PORT-TODO(stub): faithful port pending.

use serde_json::Value;

use super::Pipeline;

#[derive(Debug, Default)]
pub struct Deobfuscate;

impl Pipeline for Deobfuscate {
    fn name(&self) -> &'static str {
        "deobfuscate"
    }

    fn walk(&mut self, body: Vec<Value>) -> Vec<Value> {
        // PORT-TODO(stub)
        body
    }
}
