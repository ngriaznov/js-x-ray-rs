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
//! Divergence: upstream's two-parse-stage split means a pattern that is
//! valid JS but unsupported by `regexp-tree` (e.g. lookaround, which
//! `regexp-tree` 0.1.x cannot parse either) is caught by a per-analyzer
//! `try/catch` and silently treated as *not vulnerable* — only a pattern
//! that fails the first-stage `new RegExp(pattern)` construction returns
//! `false` outright. We have a single parser (`regex-syntax`, which shares
//! `regexp-tree`'s inability to parse lookaround/backreferences), so we
//! cannot distinguish the two failure modes; we mirror the more common
//! outcome and treat any parse failure as "safe" (permissive fallback).

use regex_syntax::ast::{self, Ast};

const DEFAULT_SAFE_REP_LIMIT: u32 = 25;

/// Returns `true` when the pattern is considered safe.
pub fn is_safe_regex(pattern: &str) -> bool {
    let Ok(ast) = ast::parse::Parser::new().parse(pattern) else {
        return true;
    };

    let mut max_star_height = 0;
    let mut repetition_count = 0;
    walk(&ast, 0, &mut max_star_height, &mut repetition_count);

    max_star_height <= 1 && repetition_count <= DEFAULT_SAFE_REP_LIMIT
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
    fn falls_back_permissively_on_unsupported_syntax() {
        // Lookaround is valid JS but unsupported by `regex-syntax` (and by
        // `regexp-tree`, upstream's parser) — see the module doc comment.
        assert!(is_safe_regex("(?=foo)bar"));
    }

    #[test]
    fn treats_unparseable_pattern_permissively() {
        assert!(is_safe_regex("("));
    }
}
