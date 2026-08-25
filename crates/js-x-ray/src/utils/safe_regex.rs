//! Port of the `safe-regex` npm package (used by the isLiteralRegex and
//! isRegexObject probes to flag exponential-backtracking regexes).
//!
//! Upstream builds a `RegExp` object from the pattern (throwing, hence
//! "unsafe", on a JS-invalid pattern), then parses that with `regexp-tree`
//! and walks its AST: each `Repetition` node increments a running "star
//! height" counter on entry and decrements it on exit, and a running
//! repetition count. The pattern is flagged vulnerable when the maximum
//! observed star height exceeds 1 or the repetition count exceeds a limit
//! of 25 — see `lib/heuristic-analyzer.js` in the npm package.
//!
//! `regex-syntax` (unlike `regexp-tree`) refuses to parse lookaround
//! (`(?=`, `(?!`, `(?<=`, `(?<!`) and backreferences (`\1`, `\k<name>`) at
//! all, even though they're valid JS regex syntax that `new RegExp` accepts
//! and `regexp-tree` parses fine (verified against both live). Since those
//! constructs are common ReDoS vectors in their own right (e.g.
//! `/(?=(a+)+)b/`), silently treating every unparseable pattern as "safe"
//! would blanket-miss them. Instead: an error naming one of those specific,
//! *valid-but-unsupported* constructs gets that exact span neutralized
//! (turned into an equivalent, parseable stand-in that preserves the
//! surrounding repetition structure) and the pattern is re-parsed; any other
//! parse error means the pattern would also fail `new RegExp()` upstream, so
//! it is treated as unsafe (matching upstream's outer `try/catch`, which
//! returns "vulnerable" for a JS-invalid pattern).

use regex_syntax::ast::{self, Ast, Error, ErrorKind};

const DEFAULT_SAFE_REP_LIMIT: u32 = 25;
/// Guard against pathological inputs; each successful patch removes exactly
/// one unsupported construct, so real patterns terminate in a handful of
/// iterations.
const MAX_PATCH_ATTEMPTS: u32 = 64;

/// Returns `true` when the pattern is considered safe.
pub fn is_safe_regex(pattern: &str) -> bool {
    let mut working = pattern.to_owned();

    for _ in 0..MAX_PATCH_ATTEMPTS {
        match ast::parse::Parser::new().parse(&working) {
            Ok(ast) => {
                let mut max_star_height = 0;
                let mut repetition_count = 0;
                walk(&ast, 0, &mut max_star_height, &mut repetition_count);

                return max_star_height <= 1 && repetition_count <= DEFAULT_SAFE_REP_LIMIT;
            }
            Err(err) => match patch_unsupported_construct(&working, &err) {
                Some(patched) => working = patched,
                // A genuine syntax error: `new RegExp(pattern)` would throw
                // upstream too, which safe-regex's outer catch treats as
                // vulnerable.
                None => return false,
            },
        }
    }

    // Could not resolve after repeated patching (should not happen for real
    // patterns) — fall back to the same conservative "unsafe" default.
    false
}

/// Neutralizes one valid-JS-but-`regex-syntax`-unsupported construct named by
/// `err`, returning the patched pattern, or `None` if `err` is a genuine
/// syntax error rather than an unsupported-construct error.
fn patch_unsupported_construct(pattern: &str, err: &Error) -> Option<String> {
    let span = err.span();
    let start = span.start.offset;
    let end = span.end.offset;

    match err.kind() {
        // Span covers exactly the assertion opener (`(?=`, `(?!`, `(?<=` or
        // `(?<!`); turning it into a plain non-capturing group preserves the
        // nested repetition structure the heuristic cares about.
        ErrorKind::UnsupportedLookAround => {
            Some(format!("{}(?:{}", &pattern[..start], &pattern[end..]))
        }
        // Span covers `\` followed by one or more digits; a single literal
        // atom preserves position for any following quantifier.
        ErrorKind::UnsupportedBackreference => {
            Some(format!("{}a{}", &pattern[..start], &pattern[end..]))
        }
        // `regex-syntax` doesn't recognize named backreferences (`\k<name>`)
        // at all, reporting just the `\k` as an unrecognized escape. Replace
        // the whole `\k<name>` token with a single literal atom.
        ErrorKind::EscapeUnrecognized if &pattern[start..end] == "\\k" => {
            let rest = &pattern[end..];
            let name_end = rest.strip_prefix('<').and_then(|s| s.find('>'))?;
            // `+2` accounts for the stripped leading `<` and the `>` itself.
            Some(format!("{}a{}", &pattern[..start], &rest[name_end + 2..]))
        }
        _ => None,
    }
}

fn walk(ast: &Ast, depth: u32, max_depth: &mut u32, repetition_count: &mut u32) {
    match ast {
        Ast::Repetition(rep) => {
            *repetition_count += 1;
            let depth = depth + 1;
            *max_depth = (*max_depth).max(depth);
            walk(&rep.ast, depth, max_depth, repetition_count);
        }
        Ast::Group(group) => walk(&group.ast, depth, max_depth, repetition_count),
        Ast::Concat(concat) => concat
            .asts
            .iter()
            .for_each(|node| walk(node, depth, max_depth, repetition_count)),
        Ast::Alternation(alt) => alt
            .asts
            .iter()
            .for_each(|node| walk(node, depth, max_depth, repetition_count)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::is_safe_regex;

    #[test]
    fn flags_nested_repetition_as_unsafe() {
        assert!(!is_safe_regex("(a+){10}"));
    }

    #[test]
    fn allows_plain_pattern() {
        assert!(is_safe_regex("^hello"));
    }

    #[test]
    fn allows_sibling_repetitions() {
        assert!(is_safe_regex("(a+)(b+)"));
    }

    #[test]
    fn flags_excessive_repetition_count() {
        let pattern: String = "a?".repeat(26);
        assert!(!is_safe_regex(&pattern));
    }

    #[test]
    fn allows_lookaround_with_no_nested_repetition() {
        // Lookaround is valid JS but unparseable by `regex-syntax` directly;
        // it must still be structurally analyzed rather than blanket-allowed.
        assert!(is_safe_regex("(?=foo)bar"));
    }

    #[test]
    fn flags_nested_repetition_hidden_behind_lookahead() {
        // A classic ReDoS pattern smuggled behind a lookahead assertion —
        // confirmed unsafe against the live `safe-regex` npm package.
        assert!(!is_safe_regex("(?=(a+)+)b"));
    }

    #[test]
    fn flags_nested_repetition_hidden_behind_lookbehind() {
        assert!(!is_safe_regex("(?<=(a+)+)b"));
    }

    #[test]
    fn allows_plain_backreference() {
        assert!(is_safe_regex("(a)(b)\\1\\2"));
    }

    #[test]
    fn flags_nested_repetition_hidden_behind_named_backreference() {
        assert!(!is_safe_regex("(?<name>(a+)+)\\k<name>"));
    }

    #[test]
    fn allows_plain_named_backreference() {
        assert!(is_safe_regex("(?<name>a)\\k<name>"));
    }

    #[test]
    fn treats_unparseable_pattern_as_unsafe() {
        // Genuinely invalid regex syntax would also throw from `new
        // RegExp(pattern)` upstream, which safe-regex's outer catch treats
        // as vulnerable (confirmed against the live npm package).
        assert!(!is_safe_regex("("));
        assert!(!is_safe_regex("a{2,1}"));
        assert!(!is_safe_regex("["));
    }
}
