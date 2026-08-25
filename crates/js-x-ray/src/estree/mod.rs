//! ESTree node helpers over `serde_json::Value` trees.
//!
//! Upstream: `src/estree/` (types.ts, literal.ts, index.ts). The analysis
//! layer works over ESTree-shaped JSON produced by oxc, mirroring how the
//! Node.js implementation works over meriyah's ESTree objects.

mod functions;
mod literal;
pub mod types;

pub use functions::*;
pub use literal::*;
pub use types::*;

use serde_json::Value;

/// An ESTree AST node (or any JSON fragment of one).
pub type Node = Value;

/// A `SourceLocation` as `{ start: { line, column }, end: { line, column } }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Position {
    pub line: u64,
    pub column: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceLocation {
    pub start: Position,
    pub end: Position,
}

impl SourceLocation {
    /// Extract the `loc` field of a node, when present and well-formed.
    pub fn from_node(node: &Node) -> Option<Self> {
        serde_json::from_value(node.get("loc")?.clone()).ok()
    }
}

/// Upstream `rootLocation()`: an all-zero location.
pub fn root_location() -> SourceLocation {
    SourceLocation {
        start: Position { line: 0, column: 0 },
        end: Position { line: 0, column: 0 },
    }
}
