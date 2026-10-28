#!/usr/bin/env bash
# Fail when a built Linux wheel or Node addon links the host's OpenSSL
# dynamically (ADR-0004 D5).
#
#   scripts/check-static-openssl.sh FILE...
#
# FILE is a .whl (each extension module inside is checked) or a .node/.so.
# Wheels are unpacked with $PYTHON (default python3).
set -euo pipefail

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

libraries=()
for file in "$@"; do
  case "$file" in
    *.whl)
      dir="$work/$(basename "$file")"
      mkdir -p "$dir"
      "${PYTHON:-python3}" -m zipfile -e "$file" "$dir"
      while IFS= read -r so; do libraries+=("$so"); done < <(find "$dir" -name '*.so')
      ;;
    *)
      libraries+=("$file")
      ;;
  esac
done
if [[ ${#libraries[@]} -eq 0 ]]; then
  echo "error: no shared library to check in: $*" >&2
  exit 1
fi

status=0
for library in "${libraries[@]}"; do
  deps=$(ldd "$library")
  if grep -Eq 'libssl|libcrypto' <<<"$deps"; then
    echo "error: $library links OpenSSL dynamically:" >&2
    grep -E 'libssl|libcrypto' <<<"$deps" >&2
    status=1
  else
    echo "ok: $(basename "$library") has no libssl/libcrypto dependency"
  fi
done
exit $status
