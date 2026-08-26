//! Upstream: `src/utils/isEvilIdentifier.ts`

pub fn is_evil_identifier_path(identifier: &str) -> bool {
    is_function_prototype(identifier)
}

pub fn is_neutral_callable(identifier: &str) -> bool {
    identifier == "Function.prototype.call"
}

fn is_function_prototype(identifier: &str) -> bool {
    identifier.starts_with("Function.prototype") && {
        let lowered = identifier.to_lowercase();
        ["call", "apply", "bind"]
            .iter()
            .any(|needle| lowered.contains(needle))
    }
}
