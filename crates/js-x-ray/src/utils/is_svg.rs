//! Upstream: `src/utils/isSvg.ts`, which delegates document validation to
//! the `is-svg` npm package (v6, itself backed by `@file-type/xml`'s
//! `XmlTextDetector`, a SAX-based full-document scan). This port hand-rolls
//! an equivalent well-formedness scanner: matching nested tags, quoted
//! attributes, comments/PIs/DOCTYPE/CDATA skipped, at most one root element,
//! no non-whitespace text outside it — then checks the root element name.

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

/// Port of the `is-svg` package: full-document well-formedness scan whose
/// root element must be `svg`.
pub fn is_string_svg(input: &str) -> bool {
    let trimmed = input.trim_matches(is_xml_space);
    if trimmed.is_empty() {
        return false;
    }

    XmlScanner::new(trimmed)
        .parse()
        .is_some_and(|root_name| root_name.eq_ignore_ascii_case("svg"))
}

/// Upstream `isSvgPath`.
pub fn is_svg_path(str_: &str) -> bool {
    let trim_str = str_.trim();
    trim_str.len() > 4 && SVG_PATH_START.is_match(trim_str) && SVG_PATH_END.is_match(trim_str)
}

fn is_xml_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn is_name_start_char(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == ':'
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':')
}

/// A minimal recursive-descent well-formedness scanner over a trimmed XML
/// string, tracking only what's needed to answer "is this one well-formed
/// document whose root element is named X".
struct XmlScanner<'a> {
    rest: &'a str,
    stack: Vec<&'a str>,
    root_name: Option<&'a str>,
}

impl<'a> XmlScanner<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            rest: input,
            stack: Vec::new(),
            root_name: None,
        }
    }

    /// Returns the root element name if `rest` is a single well-formed XML
    /// document, `None` otherwise.
    fn parse(mut self) -> Option<&'a str> {
        while !self.rest.is_empty() {
            if self.consume_prefix("<!--") {
                self.skip_until("-->")?;
            } else if self.consume_prefix("<?") {
                self.skip_until("?>")?;
            } else if self.rest.starts_with("<!DOCTYPE") || self.rest.starts_with("<!doctype") {
                self.rest = &self.rest[9..];
                self.skip_doctype()?;
            } else if self.consume_prefix("<![CDATA[") {
                self.skip_until("]]>")?;
            } else if self.rest.starts_with("</") {
                self.parse_end_tag()?;
            } else if self.rest.starts_with('<') {
                if self.stack.is_empty() && self.root_name.is_some() {
                    return None; // a second root element
                }
                self.parse_start_tag()?;
            } else {
                let text = self.take_text();
                let outside_root = self.stack.is_empty();
                if outside_root && !text.trim_matches(is_xml_space).is_empty() {
                    return None;
                }
            }
        }

        self.stack.is_empty().then_some(self.root_name).flatten()
    }

    fn consume_prefix(&mut self, prefix: &str) -> bool {
        let Some(rest) = self.rest.strip_prefix(prefix) else {
            return false;
        };
        self.rest = rest;
        true
    }

    fn skip_whitespace(&mut self) {
        self.rest = self.rest.trim_start_matches(is_xml_space);
    }

    fn skip_until(&mut self, marker: &str) -> Option<()> {
        let idx = self.rest.find(marker)?;
        self.rest = &self.rest[idx + marker.len()..];
        Some(())
    }

    /// Skips a `DOCTYPE` declaration, honoring an internal subset's own
    /// `>` characters inside `[...]` (e.g. entity declarations).
    fn skip_doctype(&mut self) -> Option<()> {
        let mut depth = 0i32;
        for (i, c) in self.rest.char_indices() {
            match c {
                '[' => depth += 1,
                ']' => depth -= 1,
                '>' if depth <= 0 => {
                    self.rest = &self.rest[i + 1..];
                    return Some(());
                }
                _ => {}
            }
        }
        None
    }

    fn take_text(&mut self) -> &'a str {
        let end = self.rest.find('<').unwrap_or(self.rest.len());
        let (text, rest) = self.rest.split_at(end);
        self.rest = rest;
        text
    }

    fn parse_name(&mut self) -> Option<&'a str> {
        let mut chars = self.rest.char_indices();
        let (_, first) = chars.next()?;
        if !is_name_start_char(first) {
            return None;
        }
        let end = chars
            .find(|&(_, c)| !is_name_char(c))
            .map_or(self.rest.len(), |(i, _)| i);
        let (name, rest) = self.rest.split_at(end);
        self.rest = rest;
        Some(name)
    }

    /// Consumes `name="value"` / `name='value'` pairs; rejects unquoted
    /// attribute values.
    fn skip_attributes(&mut self) -> Option<()> {
        loop {
            self.skip_whitespace();
            match self.rest.chars().next() {
                None | Some('/' | '>') => return Some(()),
                _ => {}
            }
            self.parse_name()?;
            self.skip_whitespace();
            if !self.consume_prefix("=") {
                return None;
            }
            self.skip_whitespace();
            let quote = self.rest.chars().next()?;
            if quote != '"' && quote != '\'' {
                return None;
            }
            self.rest = &self.rest[quote.len_utf8()..];
            let end = self.rest.find(quote)?;
            self.rest = &self.rest[end + quote.len_utf8()..];
        }
    }

    fn parse_start_tag(&mut self) -> Option<()> {
        self.rest = &self.rest[1..]; // '<'
        let name = self.parse_name()?;
        self.skip_attributes()?;
        self.skip_whitespace();
        let self_closing = self.consume_prefix("/>");
        if !self_closing && !self.consume_prefix(">") {
            return None;
        }

        if self.root_name.is_none() {
            self.root_name = Some(name);
        }
        if !self_closing {
            self.stack.push(name);
        }
        Some(())
    }

    fn parse_end_tag(&mut self) -> Option<()> {
        self.rest = &self.rest[2..]; // "</"
        let name = self.parse_name()?;
        self.skip_whitespace();
        if !self.consume_prefix(">") {
            return None;
        }
        (self.stack.pop() == Some(name)).then_some(())
    }
}

#[cfg(test)]
mod tests {
    use super::is_string_svg;

    #[test]
    fn accepts_svg_document() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg"
          width="150" height="100" viewBox="0 0 3 2">

          <rect width="1" height="2" x="0" fill="#008d46" />
          <rect width="1" height="2" x="1" fill="#ffffff" />
          <rect width="1" height="2" x="2" fill="#d2232c" />
      </svg>"##;
        assert!(is_string_svg(svg));
    }

    #[test]
    fn rejects_lone_closing_tag() {
        assert!(!is_string_svg("</a>"));
    }

    #[test]
    fn rejects_unclosed_tag() {
        assert!(!is_string_svg("<svg><rect></svg>"));
    }

    #[test]
    fn rejects_unquoted_attribute() {
        assert!(!is_string_svg("<svg width=150></svg>"));
    }

    #[test]
    fn rejects_mismatched_case_close() {
        assert!(!is_string_svg("<svg></SVG>"));
    }

    #[test]
    fn accepts_self_closing_root() {
        assert!(is_string_svg("<svg/>"));
    }

    #[test]
    fn rejects_non_svg_root() {
        assert!(!is_string_svg("<rss><channel></channel></rss>"));
    }

    #[test]
    fn rejects_multiple_roots() {
        assert!(!is_string_svg("<svg></svg><svg></svg>"));
    }

    #[test]
    fn skips_prolog_and_comments() {
        let doc = "<?xml version=\"1.0\"?><!-- comment --><svg></svg>";
        assert!(is_string_svg(doc));
    }
}
