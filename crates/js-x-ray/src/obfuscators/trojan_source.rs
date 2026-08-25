//! Upstream: `src/obfuscators/trojan-source.ts`
//!
//! Upstream is itself copy-pasted from
//! <https://github.com/lirantal/anti-trojan-source>.

/// Upstream `kConfusableRegex`: every dangerous confusable/invisible
/// character — explicit BMP confusables (bidirectional marks, zero-width
/// chars, variation selectors, etc.) plus the Variation Selectors
/// Supplement U+E0100..U+E01EF. A `char` range match replaces the regex.
fn is_confusable_char(char: char) -> bool {
    matches!(
        char,
        '\u{00A0}'
            | '\u{00AD}'
            | '\u{061C}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'
            | '\u{2063}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FEFF}'
            | '\u{E0100}'..='\u{E01EF}'
    )
}

pub fn verify(source_text_to_search: &str) -> bool {
    source_text_to_search.chars().any(is_confusable_char)
}
