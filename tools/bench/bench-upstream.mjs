// Benchmark the ORIGINAL @nodesecure/js-x-ray over the etalon corpus.
// Usage: node --experimental-strip-types tools/bench/bench-upstream.mjs [iterations]
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "../..");
const corpusDir = path.join(repoRoot, "tests/etalon/corpus");
const upstream = "/home/user/nodesecure/js-x-ray/workspaces/js-x-ray/src/index.ts";

const { AstAnalyser, DefaultCollectableSet } = await import(upstream);

function loadCases(dir, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    const p = path.join(dir, entry.name);
    if (entry.isDirectory()) loadCases(p, out);
    else if (entry.name.endsWith(".json")) out.push(JSON.parse(fs.readFileSync(p, "utf8")));
  }
  return out;
}

const cases = loadCases(corpusDir)
  .map((c) => ({
    ...c,
    code: c.code ?? fs.readFileSync(path.join(repoRoot, "tests/etalon", c.file), "utf8"),
  }));

function runAll() {
  let warnings = 0;
  for (const c of cases) {
    const analyser = new AstAnalyser({
      ...(c.analyserOptions ?? {}),
      collectables: [new DefaultCollectableSet("dependency")],
    });
    try {
      const report = analyser.analyse(c.code, c.runtimeOptions ?? {});
      warnings += report.warnings.length;
    } catch {
      warnings += 1;
    }
  }
  return warnings;
}

const iterations = Number(process.argv[2] ?? 10);
runAll(); // warmup (JIT)
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
const stats = {
  impl: "node-upstream",
  cases: cases.length,
  iterations,
  best_ms: times[0],
  median_ms: times[Math.floor(times.length / 2)],
  mean_ms: times.reduce((a, b) => a + b, 0) / times.length,
};
console.log(JSON.stringify(stats));
