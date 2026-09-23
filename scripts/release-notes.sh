#!/usr/bin/env bash
# Print the CHANGELOG.md section body for VERSION (without its heading),
# with repository-relative links rewritten to absolute links at the tag so
# they resolve on the GitHub Release page.
#
#   scripts/release-notes.sh VERSION
set -euo pipefail

cd "$(dirname "$0")/.."

version=${1:?usage: release-notes.sh VERSION}
awk -v heading="## [$version]" '
  index($0, heading) == 1 { on = 1; next }
  on && /^## \[/ { exit }
  on && /^\[[^]]+\]: / { exit }
  on { print }
' CHANGELOG.md \
  | sed -e '/./,$!d' \
  | sed -E "s#\]\((docs/|CHANGELOG|README|LICENSE)#](https://github.com/jianantian/orchest/blob/v$version/\1#g"
