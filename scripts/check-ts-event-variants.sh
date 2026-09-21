#!/usr/bin/env bash
# Guards the JS RuntimeEvent union against drift from the core enum.
#
# js/types.d.ts declares one union variant per `RuntimeEvent` variant in
# crates/orchest/src/events.rs, in the wire shape the bindings emit
# (`type` = snake_case discriminant, plus injected run_depth/child_run_id).
# This check fails when the two sets differ in either direction — the union
# had drifted twice before (missing variants, missing injected fields).
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

events="crates/orchest/src/events.rs"
declarations="js/types.d.ts"

for file in "$events" "$declarations"; do
  if [[ ! -e "$file" ]]; then
    echo "error: expected file '$file' does not exist" >&2
    exit 1
  fi
done

core_variants="$(
  awk '/^pub enum RuntimeEvent \{/{inside=1;next} inside && /^\}/{exit} inside' "$events" |
    sed -n 's/^    \([A-Z][A-Za-z0-9]*\) *{.*$/\1/p' |
    sed -e 's/\([a-z0-9]\)\([A-Z]\)/\1_\2/g' |
    tr '[:upper:]' '[:lower:]' |
    sort
)"

ts_variants="$(
  sed -n '/^export type RuntimeEvent =/,/;$/p' "$declarations" |
    sed -n 's/.*type: "\([a-z0-9_]*\)".*/\1/p' |
    sort
)"

if [[ -z "$core_variants" ]]; then
  echo "error: no RuntimeEvent variants parsed from $events" >&2
  exit 1
fi
if [[ -z "$ts_variants" ]]; then
  echo "error: no RuntimeEvent variants parsed from $declarations" >&2
  exit 1
fi

missing="$(comm -23 <(echo "$core_variants") <(echo "$ts_variants"))"
extra="$(comm -13 <(echo "$core_variants") <(echo "$ts_variants"))"

status=0
if [[ -n "$missing" ]]; then
  echo "error: RuntimeEvent variants missing from $declarations:" >&2
  echo "$missing" | sed 's/^/  - /' >&2
  status=1
fi
if [[ -n "$extra" ]]; then
  echo "error: $declarations declares variants absent from $events:" >&2
  echo "$extra" | sed 's/^/  - /' >&2
  status=1
fi

if [[ "$status" -ne 0 ]]; then
  exit 1
fi

echo "RuntimeEvent variant check passed ($(echo "$core_variants" | wc -l | tr -d ' ') variants)."
