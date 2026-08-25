// Benchmark the WASM build over the etalon corpus.
// Usage: node tools/bench/bench-wasm.mjs <wasm-pkg-dir> [iterations]
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "../..");
const corpusDir = path.join(repoRoot, "tests/etalon/corpus");

const pkgDir = process.argv[2];
const require = createRequire(import.meta.url);
const { analyse } = require(path.join(pkgDir, "js_x_ray_wasm.js"));

function loadCases(dir, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    const p = path.join(dir, entry.name);
    if (entry.isDirectory()) loadCases(p, out);
    else if (entry.name.endsWith(".json")) out.push(JSON.parse(fs.readFileSync(p, "utf8")));
  }
  return out;
}

const cases = loadCases(corpusDir).map((c) => ({
  code: c.code ?? fs.readFileSync(path.join(repoRoot, "tests/etalon", c.file), "utf8"),
  options: JSON.stringify({ ...(c.analyserOptions ?? {}), ...(c.runtimeOptions ?? {}) }),
}));

function runAll() {
  let warnings = 0;
  for (const c of cases) {
    const report = JSON.parse(analyse(c.code, c.options));
    warnings += report.warnings.length;
  }
  return warnings;
}

const iterations = Number(process.argv[3] ?? 10);
runAll(); // warmup
runAll();

const times = [];
for (let i = 0; i < iterations; i++) {
  const t0 = process.hrtime.bigint();
  const w = runAll();
  const t1 = process.hrtime.bigint();
  times.push(Number(t1 - t0) / 1e6);
  if (i === 0) console.error(`sanity: ${w} warnings over ${cases.length} cases`);
}
times.sort((a, b) => a - b);
console.log(JSON.stringify({
  impl: "rust-wasm(node)",
  cases: cases.length,
  iterations,
  best_ms: times[0],
  median_ms: times[Math.floor(times.length / 2)],
  mean_ms: times.reduce((a, b) => a + b, 0) / times.length,
}));
