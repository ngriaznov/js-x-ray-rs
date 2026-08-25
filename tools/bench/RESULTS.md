# Performance: Rust port vs original @nodesecure/js-x-ray

Measured on this repo's etalon corpus and large single files. Reproduce with:

```bash
# Node upstream (requires Node >= 24, upstream cloned + npm-installed)
node --experimental-strip-types tools/bench/bench-upstream.mjs 10
# Rust native
cargo run --release --example bench -- 10
# Rust WASM under Node
cargo build -p js-x-ray-wasm --target wasm32-unknown-unknown --release
wasm-bindgen --target nodejs --out-dir /tmp/wasm-pkg target/wasm32-unknown-unknown/release/js_x_ray_wasm.wasm
node tools/bench/bench-wasm.mjs /tmp/wasm-pkg 10
# Per-phase breakdown on one file
cargo run --release --example bench_phases -- <file.js>
```

All runs analyse the identical 588-case corpus with identical per-case options
(fresh analyser + `dependency` collectable per case, warmup iterations first).
Both implementations produce identical warning kinds on every one of the 588
cases (verified per-case on Node 24).

## Etalon corpus (588 real-world snippets + fixtures), median of 10 runs

| Implementation | median | vs upstream |
| --- | --- | --- |
| Node.js upstream (Node 24, warmed JIT) | 130.5 ms | 1.00× |
| **Rust native (release)** | **120.1 ms** | **1.09× faster** |
| Rust WASM (Node host) | 195.9 ms | 0.67× (1.5× slower) |

## Single-file workloads (median)

| File | Node upstream | Rust native |
| --- | --- | --- |
| prop-types.min.js (2.8 KB, minified) | 1.7 ms | 3.7 ms |
| typescript.js (9.1 MB) | 2 389 ms | 6 049 ms |

## Why the profile looks like this

Per-phase timing for the 9.1 MB file (`bench_phases`):

| Phase | Time |
| --- | --- |
| oxc parse | 102 ms (~4× faster than meriyah's parse) |
| AST → ESTree JSON string (80 MB) | 129 ms |
| JSON string → `serde_json::Value` | ~1.1 s |
| `loc` injection (per-node allocation) | ~5.7 s |
| analysis walk + probes | ~1.4 s |

The port deliberately runs the analysis over an ESTree-shaped
`serde_json::Value` tree — the same data model the Node.js original uses —
which keeps every module a line-by-line mirror of upstream and is what makes
the 588/588 behavioral-parity guarantee tractable. The price is JSON tree
materialization: on typical npm-package files (the corpus) the port is
slightly faster than upstream; on multi-megabyte bundles the per-node
allocation dominates and upstream's JIT-compiled object graph wins.

Known optimization paths if large-file throughput ever matters, in order of
payoff: serialize oxc's AST directly into `serde_json::Value` (skipping the
80 MB string round-trip), compute `loc` lazily at the few read sites, and
intern the `loc`/`start`/`end` map keys.
