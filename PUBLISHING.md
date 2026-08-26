# Publishing `js-x-ray-rs` to crates.io

Only the `js-x-ray-rs` library crate is published. `js-x-ray-wasm` is
`publish = false` — it ships as a WASM/npm artifact via `wasm-pack`, not to
crates.io.

## Automated releases (recommended)

CI publishes automatically. The `publish` job in
[`.github/workflows/ci.yml`](.github/workflows/ci.yml) runs on every push to
`main`, after the test and wasm jobs pass, and:

1. reads the crate version from `Cargo.toml`,
2. queries crates.io, and
3. runs `cargo publish` **only if that version isn't already published**,
   then tags the commit `v<version>`.

So cutting a release is just: bump `version` under `[workspace.package]` in
the root `Cargo.toml`, add a `CHANGELOG.md` entry, and merge to `main`.
Re-runs on an already-published version are a no-op.

**One-time setup:** add a crates.io API token (from
<https://crates.io/settings/tokens>, scope publish-new + publish-update) as
the repository secret **`CARGO_REGISTRY_TOKEN`**
(Settings → Secrets and variables → Actions). Until it is set, the publish
job skips cleanly and CI stays green.

## Manual publishing

For a local release (or the very first publish before the secret exists):

1. A crates.io account (log in with GitHub at <https://crates.io>).
2. `cargo login <token>` with a token from
   <https://crates.io/settings/tokens> (scope: publish-new + publish-update).

## Each release

```bash
# 1. Green on the pinned toolchain CI uses.
rustup run 1.98.0 cargo fmt --all --check
rustup run 1.98.0 cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                       # 691/691 etalon + unit + fuzz

# 2. Bump the version in the workspace Cargo.toml ([workspace.package] version)
#    and add a CHANGELOG.md entry. Commit.

# 3. Dry run: packages and verify-builds without uploading.
cargo publish -p js-x-ray-rs --dry-run

# 4. Publish.
cargo publish -p js-x-ray-rs

# 5. Tag the release.
git tag -a v0.1.0 -m "js-x-ray-rs 0.1.0" && git push origin v0.1.0
```

## What ships

`cargo package -p js-x-ray-rs --list` is the source of truth. The tarball
contains the crate `src/`, `examples/`, crate-local `tests/` (unit specs and
their small fixtures), `README.md`, and `LICENSE` — about 150 KiB
compressed. The 6.2 MB etalon corpus at the workspace root is intentionally
excluded; the etalon and panic-safety tests detect its absence and skip, so
`cargo test` on the published tarball is green (unit tests + doctest run).

## Docs

`docs.rs` builds automatically after publish, with `all-features` per
`[package.metadata.docs.rs]`.
