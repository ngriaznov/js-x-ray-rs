# Changelog

All notable changes to `js-x-ray-rs` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow
[SemVer](https://semver.org/).

## [0.1.0] — Unreleased

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
