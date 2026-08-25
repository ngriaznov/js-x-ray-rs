//! Upstream: `src/utils/stripNodePrefix.ts`

const NODE_MODULE_PREFIX: &str = "node:";

pub fn strip_node_prefix(value: &str) -> &str {
    value.strip_prefix(NODE_MODULE_PREFIX).unwrap_or(value)
}
