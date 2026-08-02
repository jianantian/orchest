#!/bin/bash
# show-gen.sh [gift_id] — print the exact GenRequest JSON sent to the provider.
#
# The debugging handle for "what did Suno actually receive": the submit path
# stores the wire payload in the gifts.gen_request column. With no argument,
# shows the most recently created gift.
#
# Examples:
#   ./scripts/show-gen.sh                 # latest gift, full payload (jq if available)
#   ./scripts/show-gen.sh 8b03b586a965    # a specific gift
#   ./scripts/show-gen.sh | jq -r .music.style    # just the composed style string
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DB="$SCRIPT_DIR/../data/gifts.db"

if [ ! -f "$DB" ]; then
  echo "no database at $DB" >&2
  exit 1
fi

if [ "${1:-}" = "" ]; then
  SQL="SELECT id, gen_request FROM gifts ORDER BY created_at DESC LIMIT 1;"
else
  # Gift ids are hex; reject anything else instead of interpolating blindly.
  case "$1" in
    *[!a-z0-9]*) echo "invalid gift id: $1" >&2; exit 1 ;;
  esac
  SQL="SELECT id, gen_request FROM gifts WHERE id = '$1';"
fi

row="$(sqlite3 -separator $'\t' "$DB" "$SQL")"
if [ -z "$row" ]; then
  echo "gift not found: ${1:-<empty database>}" >&2
  exit 1
fi

id="${row%%$'\t'*}"
req="${row#*$'\t'}"

if [ -z "$req" ]; then
  echo "gift $id: no gen_request stored (not generated yet, or generated before this column existed)"
  exit 0
fi

echo "# gift $id — wire payload sent to provider:"
if command -v jq >/dev/null 2>&1; then
  echo "$req" | jq .
else
  echo "$req"
fi
