# js-x-ray-rs

A Rust port of [`@nodesecure/js-x-ray`](https://github.com/NodeSecure/js-x-ray) —
JavaScript/TypeScript AST X-Ray analysis for detecting suspicious and malicious
patterns in packages — powered by [oxc](https://oxc.rs) instead of meriyah.

The port is a **behavioral clone** of the upstream Node.js library (v16): same
probes, same warning kinds, values and locations, same flags and scores. It is
verified two ways:

- **691/691 etalon cases** — reference outputs produced by running the
  *original* library over a corpus extracted from upstream's entire test suite
  match the Rust port exactly (warnings incl. locations, flags, `idsLengthAvg`,
  `stringScore`, dependencies).
- **~430 ported unit tests** — upstream's spec files for internal APIs
  (walker, estree helpers, VariableTracer, Deobfuscator, probes, utils,
  EntryFilesAnalyser, …) transcribed to Rust and passing.

## What it detects

Same catalogue as upstream (`@nodesecure/js-x-ray` v16):

| Warning | Description |
| --- | --- |
| `parsing-error` | The source could not be parsed |
| `unsafe-import` | Unable to follow an import (require/import) statement/expression |
| `unsafe-regex` | Regex vulnerable to ReDoS (exponential backtracking) |
| `unsafe-stmt` | Use of dangerous statements like `eval()` or `Function("")` |
| `encoded-literal` | Encoded literals (hex, base64, unicode sequences) |
| `short-identifiers` | Average identifier length below 1.5 chars |
| `suspicious-literal` | Suspiciously scored string literals |
| `suspicious-file` | More than ten encoded literals in one file |
| `obfuscated-code` | Obfuscator detection (jsfuck, jjencode, obfuscator.io, morse, trojan-source…) |
| `shady-link` | Links to suspicious domains / raw IPs |
| `unsafe-command` | Suspicious `spawn()`/`exec()` commands |
| `serialize-environment` | `JSON.stringify(process.env)` |
| `data-exfiltration` | Reads of `os.userInfo()`, `dns.getServers()`, … |
| `sql-injection` | SQL built by string concatenation/templating |
| `monkey-patch` | Redefinition of built-in prototypes/methods |
| `prototype-pollution` | `__proto__` / `constructor.prototype` assignment patterns |
| `unsafe-vm-context` | Dangerous `vm` context usage |
| `crypto.weak-algorithm` | md5/sha1/ripemd160 usage |
| optional: `synchronous-io`, `log-usage`, `insecure-random`, `crypto.weak-scrypt`, `crypto.unsafe-prehash`, `crypto.weak-bcrypt`, `crypto.password-shucking` |

## Usage (Rust)

```rust
use js_x_ray::{AstAnalyser, AstAnalyserOptions, RuntimeOptions};

let analyser = AstAnalyser::default();
let report = analyser
    .analyse(
        r#"const stream = require("node:stream"); eval("console.log('hello')");"#,
        RuntimeOptions::default(),
    )
    .expect("source parses");

for warning in &report.warnings {
    println!("{}: {:?}", warning.kind, warning.value);
}
println!("dependencies: {:?}", report.dependencies.keys().collect::<Vec<_>>());
```

Options mirror upstream:

```rust
use js_x_ray::{AstAnalyser, AstAnalyserOptions, OptionalWarnings, Sensitivity};

let analyser = AstAnalyser::new(AstAnalyserOptions {
    optional_warnings: OptionalWarnings::All, // or ::Names(vec!["log-usage".into()])
    sensitivity: Sensitivity::Aggressive,     // default: Conservative
    ..Default::default()
});
```

File analysis (`analyse_file`, with `.min`/minified detection and TypeScript
support via oxc) is behind the default `fs` feature; disable it for WASM.

## WASM

`crates/js-x-ray-wasm` exposes `analyse(source, optionsJson)` through
`wasm-bindgen`:

```bash
rustup target add wasm32-unknown-unknown
cargo build -p js-x-ray-wasm --target wasm32-unknown-unknown --release
# or, with wasm-pack for a ready-to-use npm package:
wasm-pack build crates/js-x-ray-wasm --target web
```

```js
import init, { analyse } from "js-x-ray-wasm";
await init();
const report = JSON.parse(analyse("eval('2 + 2')", JSON.stringify({ optionalWarnings: true })));
```

## How the port works

- **Parser**: oxc parses JS/TS/JSX and serializes to the same ESTree JSON shape
  meriyah produces (`Literal` nodes with `value`/`raw`/`regex`, `loc` objects
  with 1-based lines / 0-based columns). The whole analysis layer then works on
  `serde_json::Value` trees, exactly like the Node.js implementation works on
  ESTree objects — which keeps every module a line-by-line mirror of upstream.
- **1:1 file mapping**: every Rust module starts with an `//! Upstream:` header;
  `tools/sync/mapping.tsv` maps all upstream files to their Rust counterparts.
- **Known divergences**: parsing-error *messages* come from oxc, not meriyah
  (kinds/locations match); a handful of parse edge cases accepted by one parser
  and not the other may differ.
- **Deliberately not ported**: upstream's `src/i18n/*.js` locale bundles
  (Arabic/English/French/Korean/Turkish translations of warning descriptions)
  and its `i18nLocation()` export — these are presentation-layer strings for
  consumers, not analysis behavior, and are out of scope for this crate. Same
  for `AstAnalyser`/`VariableTracer` being an upstream `EventEmitter`: the
  port surfaces the same events through return values instead (`Report` /
  `ReportOnFile`, and `VariableTracer::drain_events`) rather than a Rust
  event-emitter API.

## Staying in sync with upstream

Upstream is pinned in [`UPSTREAM.lock`](UPSTREAM.lock). When NodeSecure ships a
new version:

```bash
tools/sync/pull-upstream.sh   # lists upstream changes → affected Rust files
# port the diffs, then regenerate reference snapshots:
node --experimental-strip-types tools/etalon/generate.mjs
cargo test --workspace        # etalon + unit tests must pass
# finally bump commit= in UPSTREAM.lock
```

## Testing

- `cargo test --workspace` — unit tests (ported from upstream's spec files) plus
  the etalon suite.
- The etalon suite (`crates/js-x-ray/tests/etalon.rs`) replays
  `tests/etalon/corpus/**` and compares against `tests/etalon/snapshots/**`
  generated from the original library (`tools/etalon/README.md`).
  Use `ETALON_FILTER=isRequire cargo test -p js-x-ray --test etalon` to focus,
  and `ETALON_VERBOSE=1` for full diffs.

## License

MIT — same as upstream. Portions of the test corpus and fixtures are derived
from [NodeSecure/js-x-ray](https://github.com/NodeSecure/js-x-ray) (MIT,
© GENTILHOMME Thomas).
