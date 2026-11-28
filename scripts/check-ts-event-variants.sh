#!/usr/bin/env bash
# Guards the TypeScript and Python RuntimeEvent unions against drift from
# the Rust enum's snake_case wire discriminant.
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

events="crates/orchest/src/events.rs"
declarations="js/types.d.ts"
python_declarations="python/orchest/__init__.pyi"

for file in "$events" "$declarations" "$python_declarations"; do
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

py_variants="$(
  python3 - "$python_declarations" <<'PYTHON'
import ast
import sys
from pathlib import Path

module = ast.parse(Path(sys.argv[1]).read_text())
classes = {node.name: node for node in module.body if isinstance(node, ast.ClassDef)}
union = next(
    node.value for node in module.body
    if isinstance(node, ast.AnnAssign) and node.target.id == "RuntimeEvent"
)
for name in sorted(node.id for node in ast.walk(union) if isinstance(node, ast.Name)):
    declaration = next(
        field.annotation for field in classes[name].body
        if isinstance(field, ast.AnnAssign) and field.target.id == "type"
    )
    print(ast.literal_eval(declaration.slice))
PYTHON
)"
py_variants="$(echo "$py_variants" | sort)"

if [[ -z "$core_variants" ]]; then
  echo "error: no RuntimeEvent variants parsed from $events" >&2
  exit 1
fi

check_declarations() {
  local path="$1"
  local variants="$2"
  local missing extra
  missing="$(comm -23 <(echo "$core_variants") <(echo "$variants"))"
  extra="$(comm -13 <(echo "$core_variants") <(echo "$variants"))"
  if [[ -n "$missing" || -n "$extra" ]]; then
    echo "error: RuntimeEvent declaration drift in $path" >&2
    if [[ -n "$missing" ]]; then echo "Missing: $missing" >&2; fi
    if [[ -n "$extra" ]]; then echo "Extra: $extra" >&2; fi
    return 1
  fi
}

check_declarations "$declarations" "$ts_variants"
check_declarations "$python_declarations" "$py_variants"
echo "RuntimeEvent variant check passed for both SDKs ($(echo "$core_variants" | wc -l | tr -d ' ') variants)."
