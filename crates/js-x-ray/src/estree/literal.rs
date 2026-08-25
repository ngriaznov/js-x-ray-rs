//! Upstream: `src/estree/literal.ts`

use serde_json::Value;

/// JavaScript `String(value)` coercion for JSON values.
pub fn js_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => js_number_string(n),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn js_number_string(n: &serde_json::Number) -> String {
    if let Some(i) = n.as_i64() {
        return i.to_string();
    }
    if let Some(u) = n.as_u64() {
        return u.to_string();
    }
    let f = n.as_f64().unwrap_or(f64::NAN);
    if f.is_nan() {
        return "NaN".to_owned();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    if f == f.trunc() && f.abs() < 1e21 {
        // JS renders integral doubles without a fractional part.
        format!("{}", f as i128)
    } else {
        let mut buffer = dragonbox_ecma_format(f);
        if buffer.is_empty() {
            buffer = f.to_string();
        }
        buffer
    }
}

fn dragonbox_ecma_format(f: f64) -> String {
    // Shortest round-trip formatting; matches JS for the common cases.
    let s = format!("{f}");
    s
}

/// Upstream `toValue`: accepts a string or a Literal node and returns the
/// literal value coerced to a string.
pub fn to_value(str_or_literal: &Value) -> String {
    match str_or_literal {
        Value::String(s) => s.clone(),
        node => js_string(node.get("value").unwrap_or(&Value::Null)),
    }
}

/// Upstream `toRaw`: the `raw` source text of a Literal (or the string itself).
pub fn to_raw(str_or_literal: &Value) -> Option<String> {
    match str_or_literal {
        Value::String(s) => Some(s.clone()),
        node => node.get("raw").and_then(Value::as_str).map(str::to_owned),
    }
}
