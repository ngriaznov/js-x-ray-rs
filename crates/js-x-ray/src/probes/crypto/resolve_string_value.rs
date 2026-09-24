//! Upstream: `src/probes/crypto/resolveStringValue.ts`

use indexmap::IndexMap;
use serde_json::Value;

use crate::estree::{identifier_name, literal_str};
use crate::variable_tracer::LiteralIdentifier;

/// Resolves a string literal, or an identifier tracked back to a string
/// literal assignment. An identifier tracked back to a template literal
/// resolves to `None`.
pub fn resolve_string_value<'a>(
    node: Option<&'a Value>,
    literal_identifiers: &'a IndexMap<String, LiteralIdentifier>,
) -> Option<&'a str> {
    let node = node?;
    if let Some(value) = literal_str(node) {
        return Some(value);
    }

    literal_identifiers
        .get(identifier_name(node)?)
        .filter(|literal| literal.r#type == "Literal")
        .map(|literal| literal.value.as_str())
}
