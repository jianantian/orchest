#!/usr/bin/env bash
# Prune the Cargo.lock inside an orchest-py sdist to the trimmed workspace
# maturin writes into it, so `cargo build --locked` works on the extracted
# sdist. maturin copies the repository lockfile verbatim, which still lists
# the workspace members it leaves out (orchest-storage, orchest-node, the
# demos). Only entries are removed: the script fails if any package would be
# added or change version.
#
#   scripts/prune-sdist-lock.sh SDIST.tar.gz
set -euo pipefail

sdist=$(cd "$(dirname "${1:?usage: prune-sdist-lock.sh SDIST.tar.gz}")" && pwd)/$(basename "$1")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

tar -xzf "$sdist" -C "$work"
root=$(find "$work" -mindepth 1 -maxdepth 1 -type d)
[[ $(wc -l <<<"$root") -eq 1 ]] || { echo "error: expected one top-level directory in $sdist" >&2; exit 1; }

packages() {
  awk '/^name = / { name = $3 } /^version = / { print name " " $3 }' "$1" | sort
}

cp "$root/Cargo.lock" "$work/Cargo.lock.orig"
(cd "$root" && cargo metadata --format-version 1 >/dev/null)
added=$(comm -13 <(packages "$work/Cargo.lock.orig") <(packages "$root/Cargo.lock"))
if [[ -n "$added" ]]; then
  echo "error: pruning would add or change lockfile entries:" >&2
  echo "$added" >&2
  exit 1
fi
(cd "$root" && cargo metadata --locked --format-version 1 >/dev/null)

removed=$(comm -23 <(packages "$work/Cargo.lock.orig") <(packages "$root/Cargo.lock") | wc -l)
# COPYFILE_DISABLE keeps macOS tar from adding AppleDouble (._*) members,
# which pip unpacks as a second source tree and then fails on.
COPYFILE_DISABLE=1 tar -czf "$sdist" -C "$work" "$(basename "$root")"
if tar -tzf "$sdist" | grep -Eq '(^|/)\._'; then
  echo "error: $sdist contains AppleDouble (._*) members" >&2
  exit 1
fi
echo "pruned $removed lockfile entries in $(basename "$sdist")"
