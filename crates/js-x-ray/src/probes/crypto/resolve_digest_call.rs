//! Upstream: `src/probes/crypto/resolveDigestCall.ts`

use serde_json::Value;

use crate::estree::get_member_call_expression;

fn call_arguments(call: &Value) -> Option<&[Value]> {
    call.get("arguments")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
}

/// The encoding arguments of `x.digest(...)` or `x.digest().toString(...)`.
/// digest's own argument takes precedence over toString's whenever digest
/// received one.
pub fn resolve_digest_call(hash_node: Option<&Value>) -> Option<&[Value]> {
    let hash_node = hash_node?;
    if let Some(digest_call) = get_member_call_expression(hash_node, "digest") {
        return call_arguments(digest_call);
    }

    let to_string_call = get_member_call_expression(hash_node, "toString")?;
    let inner_digest_call =
        get_member_call_expression(to_string_call.pointer("/callee/object")?, "digest")?;
    let inner_arguments = call_arguments(inner_digest_call)?;

    if inner_arguments.is_empty() {
        call_arguments(to_string_call)
    } else {
        Some(inner_arguments)
    }
}
