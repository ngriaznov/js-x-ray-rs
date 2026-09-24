//! Upstream: `src/probes/crypto/resolveNumericValue.ts`

use indexmap::IndexMap;
use serde_json::Value;

use crate::estree::{identifier_name, is_numeric_literal};
use crate::variable_tracer::LiteralIdentifier;

/// Resolves a numeric literal, or an identifier tracked back to a numeric
/// literal assignment.
pub fn resolve_numeric_value(
    node: Option<&Value>,
    literal_identifiers: &IndexMap<String, LiteralIdentifier>,
) -> Option<f64> {
    let node = node?;
    if is_numeric_literal(node) {
        return node.get("value")?.as_f64();
    }

    let literal = literal_identifiers.get(identifier_name(node)?)?;
    let value = js_string_to_number(&literal.value);

    (!value.is_nan()).then_some(value)
}

/// ECMAScript `StringToNumber` (what `Number(string)` does).
fn js_string_to_number(input: &str) -> f64 {
    let text = input.trim_matches(is_js_whitespace);
    if text.is_empty() {
        return 0.0;
    }

    let radix = match text.get(..2) {
        Some("0x" | "0X") => 16,
        Some("0o" | "0O") => 8,
        Some("0b" | "0B") => 2,
        _ => 10,
    };
    if radix != 10 {
        let digits = &text[2..];
        return if !digits.is_empty() && digits.chars().all(|c| c.is_digit(radix)) {
            digits.chars().fold(0.0, |acc, c| {
                acc * f64::from(radix) + f64::from(c.to_digit(radix).unwrap_or(0))
            })
        } else {
            f64::NAN
        };
    }

    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    if unsigned == "Infinity" {
        return if text.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }

    // Rust's float grammar is StrDecimalLiteral plus `inf`/`nan` spellings,
    // which JS rejects; every JS-valid decimal is digits, `.`, and one exponent.
    if unsigned
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))
    {
        text.parse().unwrap_or(f64::NAN)
    } else {
        f64::NAN
    }
}

/// JS `WhiteSpace` and `LineTerminator` code points (what `String.prototype.trim` strips).
fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n'
            | '\u{0B}'
            | '\u{0C}'
            | '\r'
            | ' '
            | '\u{A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    )
}

#[cfg(test)]
mod tests {
    use super::js_string_to_number;

    #[test]
    fn follows_ecmascript_string_to_number() {
        for (input, expected) in [
            ("12", 12.0),
            ("  12\u{FEFF}\n", 12.0),
            ("", 0.0),
            ("   ", 0.0),
            ("0x1F", 31.0),
            ("0o17", 15.0),
            ("0b101", 5.0),
            ("1e3", 1000.0),
            ("1e+21", 1e21),
            (".5", 0.5),
            ("5.", 5.0),
            ("+5", 5.0),
            ("-5", -5.0),
            ("-Infinity", f64::NEG_INFINITY),
        ] {
            assert_eq!(js_string_to_number(input), expected, "{input:?}");
        }

        for input in [
            "abc", "inf", "nan", "NaN", "infinity", "1_000", "-0x10", "0x", "1e", "12px", "true",
            "\u{85}1",
        ] {
            assert!(js_string_to_number(input).is_nan(), "{input:?}");
        }
    }
}
