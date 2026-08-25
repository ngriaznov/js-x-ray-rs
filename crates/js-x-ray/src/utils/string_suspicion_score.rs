//! Upstream: `src/utils/stringSuspicionScore.ts`

use std::collections::HashSet;

/// Rough grapheme-aware string width (upstream uses `Intl.Segmenter`).
fn string_length(string: &str) -> usize {
    if string.is_empty() {
        return 0;
    }
    let utf16_len = string.encode_utf16().count();
    // Fast path mirrors upstream: short or ASCII-only strings use char count.
    if utf16_len <= 128 || string.is_ascii() {
        return utf16_len;
    }
    // Approximation of grapheme segmentation: count Unicode scalar values,
    // merging combining marks into their base character.
    let mut length = 0usize;
    let mut previous_was_base = false;
    for c in string.chars() {
        let is_combining = matches!(u32::from(c), 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F);
        if is_combining && previous_was_base {
            continue;
        }
        length += 1;
        previous_was_base = true;
    }
    length
}

/// Upstream `stringCharDiversity`: number of unique UTF-16 code points.
pub fn string_char_diversity(str_: &str, chars_to_exclude: &[char]) -> usize {
    let mut data: HashSet<char> = str_.chars().collect();
    for char_ in chars_to_exclude {
        data.remove(char_);
    }
    data.len()
}

const MAX_SAFE_STRING_LEN: usize = 45;
const MAX_SAFE_STRING_CHAR_DIVERSITY: usize = 70;
const MIN_UNSAFE_STRING_LEN_THRESHOLD: usize = 200;
const SCORE_STRING_LENGTH_THRESHOLD: f64 = 750.0;

/// Upstream `stringSuspicionScore`.
pub fn string_suspicion_score(str_: &str) -> u32 {
    let str_len = string_length(str_);
    if str_len < MAX_SAFE_STRING_LEN {
        return 0;
    }

    let include_space = str_.contains(' ');
    let include_space_at_start = if include_space {
        // `str.slice(0, 45)` operates on UTF-16 units; ASCII space search is
        // equivalent on the char prefix.
        str_.chars().take(MAX_SAFE_STRING_LEN).any(|c| c == ' ')
    } else {
        false
    };

    let mut suspect_score: u32 = if include_space_at_start { 0 } else { 1 };
    if str_len > MIN_UNSAFE_STRING_LEN_THRESHOLD {
        suspect_score += (str_len as f64 / SCORE_STRING_LENGTH_THRESHOLD).ceil() as u32;
    }

    if string_char_diversity(str_, &[]) >= MAX_SAFE_STRING_CHAR_DIVERSITY {
        suspect_score + 2
    } else {
        suspect_score
    }
}
