#!/usr/bin/env bash
# Pre-publish guards for an SDK release (ADR-0004 D6).
#
#   scripts/sdk-release-check.sh [--skip-changelog] [TAG]
#
# Prints the SDK version on success. Fails when:
# - package.json, crates/orchest-py/Cargo.toml and
#   crates/orchest-node/Cargo.toml carry different versions;
# - a platform package under npm/ has another version, or package.json's
#   optionalDependencies do not pin exactly the npm/ packages at the SDK
#   version;
# - the version is below 0.2.0, the first SDK version (ADR-0004 D6);
#   published versions can never be replaced, so 0.1.x must not reach a
#   registry by accident;
# - TAG is given and differs from sdk-v<version>;
# - CHANGELOG-SDK.md has no `## [<version>]` section (unless
#   --skip-changelog, which dry runs from a branch use).
set -euo pipefail

cd "$(dirname "$0")/.."

check_changelog=1
if [[ "${1:-}" == "--skip-changelog" ]]; then
  check_changelog=0
  shift
fi
tag=${1:-}

crate_version() {
  awk '
    /^\[package\]/ { in_pkg = 1; next }
    /^\[/ { in_pkg = 0 }
    in_pkg && /^version *=/ { gsub(/.*= *"|".*/, ""); print; exit }
  ' "$1"
}

version=$(node -p 'require("./package.json").version')
status=0
for manifest in crates/orchest-py/Cargo.toml crates/orchest-node/Cargo.toml; do
  got=$(crate_version "$manifest")
  if [[ "$got" != "$version" ]]; then
    echo "error: $manifest has version \"$got\", package.json has \"$version\"" >&2
    status=1
  fi
done

expected_pins=""
for manifest in npm/*/package.json; do
  name=$(node -p "require('./$manifest').name")
  got=$(node -p "require('./$manifest').version")
  if [[ "$got" != "$version" ]]; then
    echo "error: $manifest has version \"$got\", expected \"$version\"" >&2
    status=1
  fi
  expected_pins+="$name=$version"$'\n'
done
actual_pins=$(node -p '
  const deps = require("./package.json").optionalDependencies || {};
  Object.entries(deps).map(([name, req]) => `${name}=${req}`).join("\n")
' | sort)
expected_pins=$(printf "%s" "$expected_pins" | sort)
if [[ "$actual_pins" != "$expected_pins" ]]; then
  echo "error: package.json optionalDependencies must pin exactly the npm/ packages at $version" >&2
  echo "  expected:" $expected_pins >&2
  echo "  found:   " $actual_pins >&2
  status=1
fi

IFS=. read -r major minor _ <<<"${version%%-*}"
if (( major == 0 && minor < 2 )); then
  echo "error: SDK version $version is below 0.2.0, the first SDK version (ADR-0004 D6)" >&2
  status=1
fi

if [[ -n "$tag" && "$tag" != "sdk-v$version" ]]; then
  echo "error: tag $tag does not match SDK version sdk-v$version" >&2
  status=1
fi

if [[ $check_changelog -eq 1 ]] && ! grep -qF "## [$version]" CHANGELOG-SDK.md; then
  echo "error: CHANGELOG-SDK.md has no \"## [$version]\" section" >&2
  status=1
fi

[[ $status -eq 0 ]] || exit 1
echo "$version"
