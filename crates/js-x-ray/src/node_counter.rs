//! Upstream: `src/NodeCounter.ts`
//!
//! PORT-TODO(stub): faithful port pending.

use serde_json::Value;

use crate::estree::Node;

pub struct NodeCounter {
    pub r#type: String,
    pub count: u32,
    pub properties: Value,
}

impl NodeCounter {
    pub fn new(r#type: impl Into<String>) -> Self {
        Self {
            r#type: r#type.into(),
            count: 0,
            properties: Value::Null,
        }
    }

    pub fn walk(&mut self, node: &Node) {
        // PORT-TODO(stub)
        let _ = node;
    }
}
