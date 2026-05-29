#!/usr/bin/env bash
# Supplementary lint checks that complement cargo clippy.
# Run in CI alongside `cargo clippy --workspace -- -D warnings`.
set -euo pipefail

EXIT_CODE=0

# 1. File length check (excluding test files, threshold 700 lines)
# Target: 400 after remaining file splits (see #77 A4).
echo "=== File length check (max 700, excluding tests) ==="
LONG_FILES=$(find crates/ -name '*.rs' \
  ! -name 'tests.rs' ! -name '*_test.rs' \
  ! -path '*/target/*' ! -path '*/tests/*' \
  -exec wc -l {} + 2>/dev/null | awk '$1 > 700 {print}' | grep -v total || true)
if [ -n "$LONG_FILES" ]; then
    echo "WARN: Files exceeding 700 lines (split candidate):"
    echo "$LONG_FILES"
    # Not failing yet — provider files exceed threshold pending A4 splits.
    # EXIT_CODE=1
else
    echo "PASS"
fi

# 2. mod.rs business logic check (mod.rs should stay concise)
# Threshold 150 allows trait definitions in mod.rs; target 50 for pure re-export mods.
echo ""
echo "=== mod.rs length check (max 150) ==="
LONG_MODS=$(find crates/ -name 'mod.rs' \
  ! -path '*/target/*' ! -path '*aigc*' \
  -exec wc -l {} + 2>/dev/null | awk '$1 > 150 {print}' | grep -v total || true)
if [ -n "$LONG_MODS" ]; then
    echo "FAIL: mod.rs files exceeding 150 lines:"
    echo "$LONG_MODS"
    EXIT_CODE=1
else
    echo "PASS"
fi

# 3. Blocking I/O in async code (std::fs usage outside tests)
# The allow-blocking-io marker may be on the same line or within ±2 lines
# (cargo fmt sometimes moves trailing comments to the next line).
echo ""
echo "=== Blocking I/O in async code ==="
BLOCKING=""
while IFS=: read -r file lineno _rest; do
    # Skip test files and target dir
    case "$file" in
        */target/*|*/tests.rs|*_test.rs|*/tests/*) continue ;;
    esac
    # Check ±2 lines for the escape-hatch marker
    start=$((lineno > 2 ? lineno - 2 : 1))
    end=$((lineno + 2))
    if ! sed -n "${start},${end}p" "$file" | grep -q 'allow-blocking-io'; then
        # Also skip if the line is inside a #[cfg(test)] block
        if ! sed -n "${start},${end}p" "$file" | grep -q '#\[cfg(test)\]'; then
            BLOCKING="${BLOCKING}${file}:${lineno}: ${_rest}"$'\n'
        fi
    fi
done < <(grep -rn 'std::fs::' crates/ --include='*.rs' | grep -v '/target/' || true)
BLOCKING="${BLOCKING%$'\n'}"
if [ -n "$BLOCKING" ]; then
    echo "FAIL: std::fs usage in non-test code (use tokio::fs or spawn_blocking):"
    echo "$BLOCKING"
    EXIT_CODE=1
else
    echo "PASS"
fi

# 4. Clippy allow residual check
echo ""
echo "=== Clippy allow residuals ==="
ALLOWS=$(grep -rn '#\[allow(clippy::' crates/ --include='*.rs' \
  | grep -v '/target/' | grep -v '// justified:' || true)
if [ -n "$ALLOWS" ]; then
    echo "FAIL: Unjustified clippy allows (add '// justified: <reason>' if necessary):"
    echo "$ALLOWS"
    EXIT_CODE=1
else
    echo "PASS"
fi

exit $EXIT_CODE
