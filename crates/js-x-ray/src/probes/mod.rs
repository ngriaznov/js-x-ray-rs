//! Upstream: `src/probes/` — one Rust module per upstream probe file.
//!
//! `default_probes()` mirrors `ProbeRunner.Defaults` (order matters!) and
//! `optional_probes()` mirrors `ProbeRunner.Optionals`.

use crate::probe::Probe;

/// Upstream `ProbeRunner.Defaults` — the order of this table impacts probe
/// execution and must match upstream exactly.
pub fn default_probes() -> Vec<Box<dyn Probe>> {
    // PORT-TODO(stub): populate as probes land:
    // isFetch, isRequire, isESMExport, isUnsafeCallee, isLiteral,
    // isLiteralRegex, isRegexObject, isImportDeclaration, isWeakAlgorithm,
    // unsafeVmContext, isBinaryExpression, isArrayExpression, isUnsafeCommand,
    // isSerializeEnv, dataExfiltration, sqlInjection, isMonkeyPatch,
    // isPrototypePollution
    Vec::new()
}

/// Upstream `ProbeRunner.Optionals` keyed by `OptionalWarningName`.
pub fn optional_probe(name: &str) -> Option<Box<dyn Probe>> {
    // PORT-TODO(stub): synchronous-io, log-usage, insecure-random,
    // crypto.weak-scrypt, crypto.unsafe-prehash, crypto.weak-bcrypt,
    // crypto.password-shucking
    let _ = name;
    None
}

pub fn optional_probe_names() -> &'static [&'static str] {
    crate::warnings::OPTIONAL_WARNING_NAMES
}
