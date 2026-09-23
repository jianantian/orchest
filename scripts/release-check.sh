#!/usr/bin/env bash
# Pre-publish guards for a lockstep release (ADR-0003 D3/D7).
#
#   scripts/release-check.sh [TAG]
#
# Prints the workspace version on success. Fails when:
# - TAG is given and differs from v<workspace version>;
# - an internal [workspace.dependencies] requirement does not match the
#   workspace version (caret for Supported crates, `=` for Internal ones);
# - CHANGELOG.md has no `## [<version>]` section.
set -euo pipefail

cd "$(dirname "$0")/.."

version=$(awk '
  /^\[workspace\.package\]/ { in_pkg = 1; next }
  /^\[/ { in_pkg = 0 }
  in_pkg && /^version *=/ { gsub(/.*= *"|".*/, ""); print; exit }
' Cargo.toml)
if [[ -z "$version" ]]; then
  echo "error: no version in [workspace.package]" >&2
  exit 1
fi

tag=${1:-}
if [[ -n "$tag" && "$tag" != "v$version" ]]; then
  echo "error: tag $tag does not match workspace version v$version" >&2
  exit 1
fi

status=0
while IFS= read -r line; do
  name=${line%% =*}
  req=$(sed -E 's/.*version = "([^"]+)".*/\1/' <<<"$line")
  case "$name" in
    orchest-provider-core|orchest-provider-http|orchest-provider-stream|orchest-provider-visual)
      want="=$version" ;;
    *)
      want="$version" ;;
  esac
  if [[ "$req" != "$want" ]]; then
    echo "error: [workspace.dependencies] $name requires \"$req\", expected \"$want\"" >&2
    status=1
  fi
done < <(awk '
  /^\[workspace\.dependencies\]/ { in_deps = 1; next }
  /^\[/ { in_deps = 0 }
  in_deps && /^orchest/ { print }
' Cargo.toml)
[[ $status -eq 0 ]] || exit 1

if ! grep -qF "## [$version]" CHANGELOG.md; then
  echo "error: CHANGELOG.md has no \"## [$version]\" section" >&2
  exit 1
fi

echo "$version"
