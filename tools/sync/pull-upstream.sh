#!/usr/bin/env bash
# Diff the pinned upstream commit against upstream HEAD and report which
# ported Rust files are affected, using the 1:1 file mapping in
# tools/sync/mapping.tsv. Run this when a new @nodesecure/js-x-ray version
# ships to get a work-list for updating the port.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LOCK="$ROOT/UPSTREAM.lock"
REPO=$(grep '^repository=' "$LOCK" | cut -d= -f2)
WORKSPACE=$(grep '^workspace=' "$LOCK" | cut -d= -f2)
PINNED=$(grep '^commit=' "$LOCK" | cut -d= -f2)

CLONE_DIR="${UPSTREAM_CLONE:-$ROOT/.upstream/js-x-ray}"
if [ ! -d "$CLONE_DIR/.git" ]; then
  mkdir -p "$(dirname "$CLONE_DIR")"
  git clone --filter=blob:none "$REPO" "$CLONE_DIR"
fi
git -C "$CLONE_DIR" fetch origin
HEAD=$(git -C "$CLONE_DIR" rev-parse origin/HEAD)

echo "Pinned:   $PINNED"
echo "Upstream: $HEAD"
if [ "$PINNED" = "$HEAD" ]; then
  echo "Port is up to date with upstream."
  exit 0
fi

echo
echo "== Upstream changes ($WORKSPACE) since pin =="
git -C "$CLONE_DIR" --no-pager log --oneline "$PINNED..$HEAD" -- "$WORKSPACE/src" "$WORKSPACE/test" | cat

echo
echo "== Changed upstream files → Rust files to update =="
git -C "$CLONE_DIR" --no-pager diff --name-status "$PINNED..$HEAD" -- "$WORKSPACE/src" |
while IFS=$'\t' read -r status file rest; do
  rel="${file#"$WORKSPACE"/src/}"
  mapped=$(awk -F'\t' -v f="$rel" '$1 == f { print $2 }' "$ROOT/tools/sync/mapping.tsv")
  printf '%s\t%s\t→ %s\n' "$status" "$rel" "${mapped:-'(unmapped — check mapping.tsv)'}"
done

echo
echo "Review each mapped Rust file against the upstream diff:"
echo "  git -C $CLONE_DIR diff $PINNED..$HEAD -- $WORKSPACE/src/<file>"
echo "Then regenerate etalon snapshots (tools/etalon/README.md), run"
echo "  cargo test --workspace"
echo "and update UPSTREAM.lock (commit=$HEAD)."
