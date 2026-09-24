# Changelog

All notable changes to `js-x-ray-rs` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow
[SemVer](https://semver.org/).

## [0.2.0] — 2026-09-24

Tracks `@nodesecure/js-x-ray` **16.1.0** ([pinned commit](UPSTREAM.lock)).

### Added

- `crypto.weak-argon2` opt-in warning: `crypto.argon2()`/`argon2Sync()` with
  the argon2d variant, memory/passes below OWASP's recommended combinations,
  or a short or hardcoded nonce.
- ``require(`http`)`` (template literal, no expressions) records the
  dependency instead of reporting `unsafe-import`.
- `require(Buffer.from("aHR0cA==", "base64").toString())` decodes to `http`,
  like the existing `hex` and `atob` forms.
- `estree::find_property_match`, `estree::is_object_expression`.

### Changed

- `crypto.weak-scrypt` reports one warning per call with every failing
  check joined (`"low-cost, short-salt"`), and measures the salt in UTF-8
  bytes.
- Option lookups in `crypto.weak-scrypt` and winston's `levels` now match
  quoted and computed string keys (`{ "N": 1024 }`, `{ ["levels"]: … }`).
- `crypto.unsafe-prehash` no longer treats an encoding held in a
  template-literal variable as safe, and resolves the digest encoding from
  the assigned call itself.
- **Breaking:** `TracerEvent::ReturnValue` carries the assigned call as
  `node`; `VariableTracer::literal_identifier_lookup` is renamed
  `resolve_literal_identifier` (upstream's name).

### Verified

- 749/749 etalon cases match upstream 16.1.0 byte-for-byte; ~470 unit
  tests, including upstream's new specs for `findPropertyMatch`,
  `resolveLiteralIdentifier` and the crypto resolvers.

## [0.1.0] — 2026-08-26

First release. A verified behavioral clone of `@nodesecure/js-x-ray` v16
([pinned commit](UPSTREAM.lock)) in Rust, powered by [oxc](https://oxc.rs).

### Added

- Static security analysis of JavaScript/TypeScript/JSX via `AstAnalyser`:
  the full upstream v16 warning catalogue, dependency collection, source
  flags, and obfuscation scores.
- Optional probes (`synchronous-io`, `log-usage`, `insecure-random`, and the
  `crypto.*` family) and `conservative`/`aggressive` sensitivity.
- `EntryFilesAnalyser` for whole-project traversal with cycle detection
  (behind the default `fs` feature).
- `js-x-ray-wasm`: a `wasm-bindgen` wrapper building on
  `wasm32-unknown-unknown` for browser and edge use.

### Verified

- 691/691 etalon cases match the original library's output byte-for-byte;
  ~430 unit tests ported from upstream's spec files; panic-safety fuzzing
  over millions of mutated inputs.
