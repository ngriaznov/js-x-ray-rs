//! Upstream: `src/utils/isMinifiedCode.ts` (itself imported from
//! `is-minified-code` by Martin Kolarik).

use std::sync::LazyLock;

use regex::Regex;

static COMMENT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)/\*.*?\*/\r?\n?|//.{0,200}?(?:\r?\n|$)").expect("valid regex")
});

pub fn is_minified_code(code: &str) -> bool {
    let cleaned = COMMENT_PATTERN.replace_all(code, "");

    // Strip a single trailing newline (upstream: /\r?\n$/).
    let cleaned = cleaned
        .strip_suffix('\n')
        .map(|s| s.strip_suffix('\r').unwrap_or(s))
        .unwrap_or(&cleaned);

    let mut lines: Vec<usize> = cleaned
        .split('\n')
        .map(|line| line.encode_utf16().count())
        .filter(|len| *len > 0)
        .collect();

    lines.len() <= 1 || median(&mut lines) > 200.0
}

fn median(values: &mut [usize]) -> f64 {
    values.sort_unstable();
    let half = values.len() / 2;
    if values.len() % 2 == 1 {
        values[half] as f64
    } else {
        (values[half - 1] + values[half]) as f64 / 2.0
    }
}
