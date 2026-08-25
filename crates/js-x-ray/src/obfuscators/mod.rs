//! Upstream: `src/obfuscators/` (trojan-source.ts, jsfuck.ts, jjencode.ts,
//! freejsobfuscator.ts, obfuscator-io.ts).
//!
//! PORT-TODO(stub): only trojan-source is sketched; the rest pending.

pub mod trojan_source {
    //! Upstream: `src/obfuscators/trojan-source.ts`

    /// Unicode control characters used in trojan-source attacks.
    const DANGEROUS: &[char] = &[
        '\u{202A}', '\u{202B}', '\u{202D}', '\u{202E}', '\u{2066}', '\u{2067}', '\u{2068}',
        '\u{2069}', '\u{200E}', '\u{200F}', '\u{061C}',
    ];

    pub fn verify(source: &str) -> bool {
        source.chars().any(|c| DANGEROUS.contains(&c))
    }
}
