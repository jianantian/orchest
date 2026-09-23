#!/usr/bin/env bash
# Publish the lockstep crates to crates.io in ADR-0003 D7 order.
#
#   scripts/release-publish.sh VERSION          # publish (needs CARGO_REGISTRY_TOKEN)
#   scripts/release-publish.sh VERSION --dry-run
#
# A crate whose VERSION is already on crates.io is skipped, so a failed
# release can be re-run. `--dry-run` packages and verifies every crate at
# once with `cargo publish --workspace --dry-run`, because later crates
# cannot resolve earlier ones from crates.io before they are uploaded.
set -euo pipefail

cd "$(dirname "$0")/.."

version=${1:?usage: release-publish.sh VERSION [--dry-run]}
mode=${2:-}

# ADR-0003 D7. Keep in sync with the publish set in ADR-0003 D2.
crates=(
  orchest-protocol
  orchest-storage
  orchest-provider-core
  orchest-provider-http
  orchest-provider-stream
  orchest-provider-visual
  orchest-provider
  orchest
)

if [[ "$mode" == "--dry-run" ]]; then
  cargo publish --workspace --dry-run --locked
  exit 0
fi

user_agent="orchest-release (https://github.com/jianantian/orchest)"
for crate in "${crates[@]}"; do
  code=$(curl -s -o /dev/null -w '%{http_code}' -A "$user_agent" \
    "https://crates.io/api/v1/crates/$crate/$version")
  if [[ "$code" == "200" ]]; then
    echo "skip: $crate $version is already on crates.io"
    continue
  fi
  echo "publish: $crate $version"
  # cargo waits until the upload is in the index before returning, so the
  # next crate can resolve it.
  cargo publish -p "$crate" --locked
done
