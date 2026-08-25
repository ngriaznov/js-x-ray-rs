#!/usr/bin/env node
// Generates reference-output snapshots for the etalon test corpus by running
// the upstream @nodesecure/js-x-ray library over every corpus case.
//
// Usage (from repo root):
//   node --experimental-strip-types --disable-warning=ExperimentalWarning tools/etalon/generate.mjs
//
// See ./README.md for details on the snapshot format and import strategy.

import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, "..", "..");
const ETALON_ROOT = path.join(REPO_ROOT, "tests", "etalon");
const CORPUS_ROOT = path.join(ETALON_ROOT, "corpus");
const SNAPSHOTS_ROOT = path.join(ETALON_ROOT, "snapshots");

const UPSTREAM_SRC_INDEX = "/home/user/nodesecure/js-x-ray/workspaces/js-x-ray/src/index.ts";
const UPSTREAM_DIST_INDEX = "/home/user/nodesecure/js-x-ray/workspaces/js-x-ray/dist/index.js";

async function loadUpstream() {
  try {
    return await import(UPSTREAM_SRC_INDEX);
  }
  catch (srcError) {
    console.error(`[generate] importing upstream src/index.ts via type-stripping failed: ${srcError.message}`);
    console.error("[generate] falling back to dist/index.js (requires `npx tsc -b` in the upstream repo)");

    return import(UPSTREAM_DIST_INDEX);
  }
}

async function* walkJsonFiles(dir) {
  for (const entry of await fs.readdir(dir, { withFileTypes: true })) {
    const entryPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      yield* walkJsonFiles(entryPath);
    }
    else if (entry.name.endsWith(".json")) {
      yield entryPath;
    }
  }
}

function round4(value) {
  return Math.round((value + Number.EPSILON) * 10000) / 10000;
}

// Deep-clones through JSON to strip undefined fields and Symbols, matching
// what a Rust `serde_json::Value` snapshot comparison would see; a
// parsing-error warning's `value` carries the underlying parser's message,
// which is not stable across engines/versions, so it is dropped entirely.
function normalizeWarning(warning) {
  const cloned = JSON.parse(JSON.stringify(warning));

  return cloned.kind === "parsing-error" ? { kind: "parsing-error" } : cloned;
}

function sortWarnings(warnings) {
  return warnings
    .map(normalizeWarning)
    .map((warning) => [JSON.stringify(warning), warning])
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([, warning]) => warning);
}

function extractDependencies(collectableSet) {
  const dependencies = {};
  for (const { value, locations } of collectableSet) {
    const metadata = locations[0]?.metadata ?? {};
    dependencies[value] = {
      unsafe: metadata.unsafe ?? false,
      inTry: metadata.inTry ?? false
    };
  }

  return dependencies;
}

function failureSnapshot(warnings) {
  return { ok: false, warnings: sortWarnings(warnings) };
}

async function runCase(testCase, { AstAnalyser, DefaultCollectableSet }) {
  const collectableSet = new DefaultCollectableSet("dependency");
  const analyser = new AstAnalyser({
    ...(testCase.analyserOptions ?? {}),
    collectables: [collectableSet]
  });
  const runtimeOptions = testCase.runtimeOptions ?? {};

  if (typeof testCase.code === "string") {
    let report;
    try {
      report = analyser.analyse(testCase.code, runtimeOptions);
    }
    catch {
      return failureSnapshot([{ kind: "parsing-error" }]);
    }

    return {
      ok: true,
      warnings: sortWarnings(report.warnings),
      flags: [...report.flags].sort(),
      idsLengthAvg: round4(report.idsLengthAvg),
      stringScore: round4(report.stringScore),
      dependencies: extractDependencies(collectableSet)
    };
  }

  const absPath = path.join(ETALON_ROOT, testCase.file);
  const report = typeof analyser.analyseFileSync === "function" ?
    analyser.analyseFileSync(absPath, runtimeOptions) :
    await analyser.analyseFile(absPath, runtimeOptions);

  if (!report.ok) {
    return failureSnapshot(report.warnings);
  }

  return {
    ok: true,
    warnings: sortWarnings(report.warnings),
    flags: [...report.flags].sort(),
    dependencies: extractDependencies(collectableSet)
  };
}

async function main() {
  const upstream = await loadUpstream();
  const { AstAnalyser, DefaultCollectableSet } = upstream;
  if (!AstAnalyser || !DefaultCollectableSet) {
    throw new Error("upstream module did not export AstAnalyser / DefaultCollectableSet");
  }

  const corpusFiles = [];
  for await (const file of walkJsonFiles(CORPUS_ROOT)) {
    corpusFiles.push(file);
  }
  corpusFiles.sort();

  let okCount = 0;
  let failedCount = 0;
  const warningKindCounts = new Map();
  const malformed = [];

  for (const corpusFile of corpusFiles) {
    const relPath = path.relative(CORPUS_ROOT, corpusFile);
    const raw = await fs.readFile(corpusFile, "utf-8");

    let testCase;
    try {
      testCase = JSON.parse(raw);
    }
    catch (error) {
      malformed.push(`${relPath}: invalid JSON (${error.message})`);
      continue;
    }

    if (typeof testCase.code !== "string" && typeof testCase.file !== "string") {
      malformed.push(`${relPath}: missing both "code" and "file"`);
      continue;
    }

    let snapshot;
    try {
      snapshot = await runCase(testCase, { AstAnalyser, DefaultCollectableSet });
    }
    catch (error) {
      throw new Error(`harness crashed while processing ${relPath}: ${error.stack ?? error}`);
    }

    if (snapshot.ok) {
      okCount++;
    }
    else {
      failedCount++;
    }
    for (const warning of snapshot.warnings) {
      warningKindCounts.set(warning.kind, (warningKindCounts.get(warning.kind) ?? 0) + 1);
    }

    const outPath = path.join(SNAPSHOTS_ROOT, relPath);
    await fs.mkdir(path.dirname(outPath), { recursive: true });
    await fs.writeFile(outPath, `${JSON.stringify(snapshot, null, 2)}\n`);
  }

  console.log("=== etalon snapshot generation summary ===");
  console.log(`cases processed: ${corpusFiles.length}`);
  console.log(`  ok:     ${okCount}`);
  console.log(`  failed: ${failedCount}`);
  console.log("warning kind distribution:");
  for (const [kind, count] of [...warningKindCounts].sort((a, b) => b[1] - a[1])) {
    console.log(`  ${kind}: ${count}`);
  }
  if (malformed.length > 0) {
    console.log("malformed corpus entries (skipped):");
    malformed.forEach((entry) => console.log(`  ${entry}`));
  }
  else {
    console.log("malformed corpus entries: none");
  }
}

await main();
