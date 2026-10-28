#!/usr/bin/env bash
# Print a changelog section body for VERSION (without its heading), with
# repository-relative links rewritten to absolute links at the tag so they
# resolve on the GitHub Release page.
#
#   scripts/release-notes.sh VERSION [CHANGELOG [TAG]]
#
# CHANGELOG defaults to CHANGELOG.md and TAG to v<VERSION>; SDK releases
# pass CHANGELOG-SDK.md and sdk-v<VERSION>.
set -euo pipefail

cd "$(dirname "$0")/.."

version=${1:?usage: release-notes.sh VERSION [CHANGELOG [TAG]]}
changelog=${2:-CHANGELOG.md}
tag=${3:-v$version}
awk -v heading="## [$version]" '
  index($0, heading) == 1 { on = 1; next }
  on && /^## \[/ { exit }
  on && /^\[[^]]+\]: / { exit }
  on { print }
' "$changelog" \
  | sed -e '/./,$!d' \
  | sed -E "s#\]\((docs/|CHANGELOG|README|LICENSE)#](https://github.com/jianantian/orchest/blob/$tag/\1#g"
