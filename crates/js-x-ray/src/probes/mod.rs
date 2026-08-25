//! Upstream: `src/probes/` — one Rust module per upstream probe file.
//!
//! `default_probes()` mirrors `ProbeRunner.Defaults` (order matters!) and
//! `optional_probe()` mirrors `ProbeRunner.Optionals`.

pub mod crypto;
pub mod data_exfiltration;
pub mod is_array_expression;
pub mod is_binary_expression;
pub mod is_esm_export;
pub mod is_fetch;
pub mod is_import_declaration;
pub mod is_literal;
pub mod is_literal_regex;
pub mod is_monkey_patch;
pub mod is_prototype_pollution;
pub mod is_random;
pub mod is_regex_object;
pub mod is_require;
pub mod is_serialize_env;
pub mod is_sync_io;
pub mod is_unsafe_callee;
pub mod is_unsafe_command;
pub mod log_usage;
pub mod sql_injection;
pub mod unsafe_vm_context;

use crate::probe::Probe;

/// Upstream `ProbeRunner.Defaults` — the order of this table impacts probe
/// execution and must match upstream exactly.
pub fn default_probes() -> Vec<Box<dyn Probe>> {
    vec![
        Box::new(is_fetch::IsFetch),
        Box::new(is_require::IsRequire),
        Box::new(is_esm_export::IsEsmExport),
        Box::new(is_unsafe_callee::IsUnsafeCallee),
        Box::new(is_literal::IsLiteral),
        Box::new(is_literal_regex::IsLiteralRegex),
        Box::new(is_regex_object::IsRegexObject),
        Box::new(is_import_declaration::IsImportDeclaration),
        Box::new(crypto::IsWeakAlgorithm),
        Box::new(unsafe_vm_context::UnsafeVmContext::default()),
        Box::new(is_binary_expression::IsBinaryExpression),
        Box::new(is_array_expression::IsArrayExpression),
        Box::new(is_unsafe_command::IsUnsafeCommand),
        Box::new(is_serialize_env::IsSerializeEnv::default()),
        Box::new(data_exfiltration::DataExfiltration::default()),
        Box::new(sql_injection::SqlInjection),
        Box::new(is_monkey_patch::IsMonkeyPatch),
        Box::new(is_prototype_pollution::IsPrototypePollution),
    ]
}

/// Upstream `ProbeRunner.Optionals` keyed by `OptionalWarningName`.
pub fn optional_probe(name: &str) -> Option<Box<dyn Probe>> {
    match name {
        "synchronous-io" => Some(Box::new(is_sync_io::IsSyncIo)),
        "log-usage" => Some(Box::new(log_usage::LogUsage::default())),
        "insecure-random" => Some(Box::new(is_random::IsRandom)),
        "crypto.weak-scrypt" => Some(Box::new(crypto::IsWeakScrypt)),
        "crypto.unsafe-prehash" => Some(Box::new(crypto::IsUnsafePrehash::default())),
        "crypto.weak-bcrypt" => Some(Box::new(crypto::IsWeakBcrypt)),
        "crypto.password-shucking" => Some(Box::new(crypto::IsPasswordShucking::default())),
        _ => None,
    }
}

pub fn optional_probe_names() -> &'static [&'static str] {
    crate::warnings::OPTIONAL_WARNING_NAMES
}
