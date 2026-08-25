# Etalon snapshot generator

`generate.mjs` runs the upstream `@nodesecure/js-x-ray` library over every
case in `tests/etalon/corpus/` and writes its output as a reference snapshot
under `tests/etalon/snapshots/`, mirroring the corpus's directory structure.
The Rust port is expected to reproduce these snapshots exactly.

## Running

Requires **Node.js >= 24** (upstream's own `engines` constraint — it uses
`RegExp.escape`, added in V8/Node 24). On Node 22 the harness still runs
without crashing, but `RegExp.escape is not a function` gets thrown from
inside `Deobfuscator.assertObfuscation()` for any case whose deobfuscation
step actually runs, and the harness records that throw as a (spurious)
`parsing-error` — inflating the failure count roughly 20x. If only Node 22
is available, install 24 via `nvm install 24 && nvm use 24` first.

```sh
node --experimental-strip-types --disable-warning=ExperimentalWarning \
  tools/etalon/generate.mjs
```

## Importing upstream

The harness imports upstream directly from TypeScript source —
`/home/user/nodesecure/js-x-ray/workspaces/js-x-ray/src/index.ts` — via
Node's `--experimental-strip-types` flag. This worked without modification;
no `tsc -b` build step was needed. If a future upstream file uses a
TypeScript feature type-stripping can't handle (e.g. enums, decorators),
the harness falls back to importing `dist/index.js` instead (build it first
with `npx tsc -b` in the upstream repo) — see `loadUpstream()`.

## Snapshot format

```jsonc
{
  "ok": true,
  "warnings": [ /* sorted by JSON.stringify(warning), deterministic order */ ],
  "flags": [ /* sorted SourceFlags strings */ ],
  "idsLengthAvg": 3.5,   // only present for "code" cases (.analyse())
  "stringScore": 0,      // ditto — ReportOnFile (file cases) has neither
  "dependencies": {
    "http": { "unsafe": false, "inTry": false }
  }
}
```

- A fresh `AstAnalyser` and a fresh `DefaultCollectableSet("dependency")` are
  built per case, per `analyserOptions`.
- `"code"` cases call `.analyse(code, runtimeOptions)`; a thrown parse error
  is caught by the harness itself (`.analyse()` does not catch internally).
- `"file"` cases resolve the path relative to `tests/etalon/` and call
  `.analyseFileSync(path, runtimeOptions)` (falls back to `await
  .analyseFile(...)` if `analyseFileSync` is absent).
- Any parse failure — whether a thrown error from `.analyse()`, or a
  `{ ok: false }` result from `.analyseFile(Sync)` — collapses to exactly
  `{ "ok": false, "warnings": [{ "kind": "parsing-error" }] }`. The
  underlying parser's error message is dropped: it is parser-version
  specific and not something the Rust port needs to match character-for-
  character.
- Every other warning keeps `kind`/`value`/`location`/`severity`/`source`/
  `i18n`/`experimental`/`file`, deep-cloned through `JSON.parse(JSON.stringify(...))`
  to drop `undefined` fields, then sorted by its own JSON string so
  comparisons are order-insensitive.
- `dependencies` is derived from the `"dependency"` collectable set, not
  from the `Report`/`ReportOnFile` types (neither exposes dependencies
  directly). For each collected value, `unsafe`/`inTry` are read off its
  *first* recorded location's metadata.
- `executionTime` is omitted everywhere (nondeterministic).

## Verification

A full run over all 588 corpus cases completes without a harness crash:

```
cases processed: 588
  ok:     583
  failed: 5
```

The 5 `ok:false` snapshots are all genuine parser failures baked into the
corpus on purpose (unremoved shebang not at start of file, unremoved HTML
comments, an explicit `searchRuntimeDependencies/parsingError.js` fixture,
etc.) — verified individually by re-running upstream on each case's source
directly and confirming a `ParseError` is thrown for the right reason.
