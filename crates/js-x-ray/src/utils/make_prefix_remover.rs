//! Upstream: `src/utils/makePrefixRemover.ts`

/// Returns a closure removing any of the given prefixes (plus the joining
/// dot) from a member expression path.
pub fn make_prefix_remover(prefixes: Vec<String>) -> impl Fn(&str) -> String {
    move |expr: &str| {
        if !expr.contains('.') {
            return expr.to_owned();
        }
        match prefixes.iter().find(|global_id| expr.starts_with(global_id.as_str())) {
            Some(matched_prefix) => expr
                .get(matched_prefix.len() + 1..)
                .unwrap_or("")
                .to_owned(),
            None => expr.to_owned(),
        }
    }
}
