//! Upstream: `src/utils/isEvilIdentifier.ts`

pub fn is_evil_identifier_path(identifier: &str) -> bool {
    is_function_prototype(identifier)
}

pub fn is_neutral_callable(identifier: &str) -> bool {
    identifier == "Function.prototype.call"
}

fn is_function_prototype(identifier: &str) -> bool {
    identifier.starts_with("Function.prototype")
        && ["call", "apply", "bind"]
            .iter()
            .any(|needle| identifier.to_lowercase().contains(needle))
}
