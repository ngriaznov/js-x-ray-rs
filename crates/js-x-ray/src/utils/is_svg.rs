//! Upstream: `src/utils/isSvg.ts` (which delegates to the `is-svg` npm
//! package for full documents). This port validates `<svg>` documents with a
//! lightweight XML well-formedness scan.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use crate::estree::to_value;

static SVG_PATH_START: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^[mzlhvcsqta]\s*[-+.0-9][^mlhvzcsqta]+").expect("valid regex")
});
static SVG_PATH_END: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)[\dz]$").expect("valid regex"));

pub fn is_svg(str_or_literal: &Value) -> bool {
    let value = to_value(str_or_literal);
    let trimmed = value.trim_start();
    (trimmed.starts_with('<') && is_string_svg(&value)) || is_svg_path(&value)
}

/// Port of the `is-svg` package: strip comments/entities, require a
/// well-formed XML document whose root element is `svg`.
pub fn is_string_svg(input: &str) -> bool {
    let input = input.trim();
    if input.is_empty() {
        return false;
    }
    // Strip DTD entities and comments like is-svg does before validating.
    static ENTITY_DECL: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?s)<!Entity\s+\S*\s*(?:"[^"]*"|'[^']*')\s*>"#).expect("valid regex")
    });
    static HTML_COMMENTS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?s)<!--.*?-->").expect("valid regex"));
    let cleaned = ENTITY_DECL.replace_all(input, "");
    let cleaned = HTML_COMMENTS.replace_all(&cleaned, "");
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return false;
    }

    xml_root_is_svg(cleaned)
}

/// Minimal XML scan: skips prologue/doctype, then requires the first element
/// to be `<svg …>` and the document to close it.
fn xml_root_is_svg(mut s: &str) -> bool {
    loop {
        s = s.trim_start();
        if let Some(rest) = s.strip_prefix("<?") {
            match rest.find("?>") {
                Some(idx) => s = &rest[idx + 2..],
                None => return false,
            }
        } else if s.starts_with("<!DOCTYPE") || s.starts_with("<!doctype") {
            match s.find('>') {
                Some(idx) => s = &s[idx + 1..],
                None => return false,
            }
        } else {
            break;
        }
    }

    let s = s.trim_start();
    let Some(rest) = s.strip_prefix('<') else {
        return false;
    };
    let name: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == ':' || *c == '-')
        .collect();
    if !name.eq_ignore_ascii_case("svg") {
        return false;
    }

    // Self-closing root or a matching closing tag at the end.
    let trimmed_end = s.trim_end();
    trimmed_end.ends_with("/>") || {
        let lower = trimmed_end.to_ascii_lowercase();
        lower.ends_with("</svg>")
    }
}

/// Upstream `isSvgPath`.
pub fn is_svg_path(str_: &str) -> bool {
    let trim_str = str_.trim();
    trim_str.len() > 4 && SVG_PATH_START.is_match(trim_str) && SVG_PATH_END.is_match(trim_str)
}
