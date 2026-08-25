//! Warning kinds, metadata, and generation.
//!
//! Upstream: `src/warnings.ts`. Serialization matches the Node.js JSON shape:
//! `location` is a `[[line, column], [line, column]]` pair for most warnings
//! and an *array* of such pairs for `encoded-literal`.

use serde::{Deserialize, Serialize};

use crate::estree::SourceLocation;
use crate::utils::{SourceArrayLocation, to_array_location};

pub const OPTIONAL_WARNING_NAMES: &[&str] = &[
    "synchronous-io",
    "log-usage",
    "insecure-random",
    "crypto.weak-scrypt",
    "crypto.unsafe-prehash",
    "crypto.weak-bcrypt",
    "crypto.password-shucking",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Information,
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WarningLocation {
    /// `null` (parsing errors carry no real location).
    Null,
    /// A single `[[line, column], [line, column]]`.
    Single(SourceArrayLocation),
    /// `encoded-literal`: every occurrence's location.
    Multiple(Vec<SourceArrayLocation>),
}

/// Upstream: `interface Warning`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Warning {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    pub value: Option<String>,
    pub source: String,
    pub location: WarningLocation,
    pub i18n: String,
    pub severity: Severity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<bool>,
}

/// Metadata table. Upstream: `export const warnings = Object.freeze({ ... })`.
pub const WARNINGS: &[(&str, &str, Severity, bool)] = &[
    ("parsing-error", "sast_warnings.parsing_error", Severity::Information, false),
    ("unsafe-import", "sast_warnings.unsafe_import", Severity::Warning, false),
    ("unsafe-regex", "sast_warnings.unsafe_regex", Severity::Warning, false),
    ("unsafe-stmt", "sast_warnings.unsafe_stmt", Severity::Warning, false),
    ("encoded-literal", "sast_warnings.encoded_literal", Severity::Information, false),
    ("short-identifiers", "sast_warnings.short_identifiers", Severity::Warning, false),
    ("suspicious-literal", "sast_warnings.suspicious_literal", Severity::Warning, false),
    ("suspicious-file", "sast_warnings.suspicious_file", Severity::Critical, false),
    ("obfuscated-code", "sast_warnings.obfuscated_code", Severity::Critical, true),
    ("crypto.weak-algorithm", "sast_warnings.weak_crypto", Severity::Information, false),
    ("shady-link", "sast_warnings.shady_link", Severity::Warning, false),
    ("unsafe-command", "sast_warnings.unsafe_command", Severity::Warning, true),
    ("synchronous-io", "sast_warnings.synchronous_io", Severity::Warning, true),
    ("serialize-environment", "sast_warnings.serialize_environment", Severity::Warning, false),
    ("data-exfiltration", "sast_warnings.data_exfiltration", Severity::Warning, false),
    ("log-usage", "sast_warnings.log_usage", Severity::Information, false),
    ("sql-injection", "sast_warnings.sql_injection", Severity::Warning, false),
    ("monkey-patch", "sast_warnings.monkey_patch", Severity::Warning, false),
    ("insecure-random", "sast_warnings.insecure_random", Severity::Information, false),
    ("prototype-pollution", "sast_warnings.prototype_pollution", Severity::Warning, false),
    ("crypto.weak-scrypt", "sast_warnings.weak_scrypt", Severity::Warning, true),
    ("crypto.unsafe-prehash", "sast_warnings.unsafe_prehash", Severity::Warning, true),
    ("crypto.weak-bcrypt", "sast_warnings.weak_bcrypt", Severity::Warning, true),
    ("crypto.password-shucking", "sast_warnings.password_shucking", Severity::Warning, true),
    ("unsafe-vm-context", "sast_warnings.unsafe_vm_context", Severity::Warning, false),
];

fn metadata(kind: &str) -> (&'static str, Severity, bool) {
    WARNINGS
        .iter()
        .find(|(name, ..)| *name == kind)
        .map(|&(_, i18n, severity, experimental)| (i18n, severity, experimental))
        .expect("unknown warning kind")
}

/// Options for [`generate_warning`]. Upstream: `GenerateWarningOptions`.
#[derive(Debug, Default, Clone)]
pub struct GenerateWarningOptions {
    pub location: Option<SourceLocation>,
    pub file: Option<String>,
    pub value: Option<String>,
    pub source: Option<String>,
    pub severity: Option<Severity>,
}

/// Upstream: `generateWarning`.
pub fn generate_warning(kind: &str, options: GenerateWarningOptions) -> Warning {
    let (i18n, default_severity, experimental) = metadata(kind);
    let source = options.source.unwrap_or_else(|| "JS-X-Ray".to_owned());
    let location = to_array_location(options.location);

    if kind == "encoded-literal" {
        return Warning {
            kind: kind.to_owned(),
            file: None,
            value: options.value,
            source,
            location: WarningLocation::Multiple(vec![location]),
            i18n: i18n.to_owned(),
            severity: default_severity,
            experimental: Some(experimental),
        };
    }

    Warning {
        kind: kind.to_owned(),
        file: options.file,
        value: options.value,
        source,
        location: WarningLocation::Single(location),
        i18n: i18n.to_owned(),
        severity: options.severity.unwrap_or(default_severity),
        experimental: Some(experimental),
    }
}
