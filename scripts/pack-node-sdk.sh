#!/usr/bin/env bash
# Pack the orchest-sdk npm packages (ADR-0004 D4) into OUT_DIR:
# the main package, plus each platform package whose addon
# (orchest_node.<platform>.node, from `npm run build:native` or a CI
# artifact) is in the repository root.
#
#   scripts/pack-node-sdk.sh OUT_DIR
set -euo pipefail

cd "$(dirname "$0")/.."

out=${1:?usage: pack-node-sdk.sh OUT_DIR}
mkdir -p "$out"
out=$(cd "$out" && pwd)

packed=0
for dir in npm/*/; do
  platform=$(basename "$dir")
  addon="orchest_node.$platform.node"
  [[ -f "$addon" ]] || continue
  # The platform package redistributes the binary, so it carries the
  # license texts too.
  cp "$addon" LICENSE-MIT LICENSE-APACHE "$dir"
  (cd "$dir" && npm pack --pack-destination "$out" >/dev/null)
  echo "packed orchest-sdk-$platform"
  packed=$((packed + 1))
done
if [[ $packed -eq 0 ]]; then
  echo "error: no orchest_node.<platform>.node in the repository root; run npm run build:native" >&2
  exit 1
fi

npm pack --pack-destination "$out" >/dev/null
echo "packed orchest-sdk"
