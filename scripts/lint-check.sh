#!/usr/bin/env bash
# Supplementary lint checks that complement cargo clippy.
# Run in CI alongside `cargo clippy --workspace -- -D warnings`.
set -euo pipefail

EXIT_CODE=0

# 1. File length check (excluding test files, threshold 500 lines)
echo "=== File length check (max 500, excluding tests) ==="
LONG_FILES=$(find crates/ -name '*.rs' \
  ! -name 'tests.rs' ! -name '*_test.rs' \
  ! -path '*/target/*' ! -path '*/tests/*' \
  -exec wc -l {} + 2>/dev/null | awk '$1 > 500 {print}' | grep -v total || true)
if [ -n "$LONG_FILES" ]; then
    echo "WARN: Files exceeding 500 lines (split candidate):"
    echo "$LONG_FILES"
    # Not failing yet — provider files exceed threshold pending A4 splits.
    # EXIT_CODE=1
else
    echo "PASS"
fi

# 2. mod.rs business logic check (mod.rs should stay concise)
# Threshold 150 allows trait definitions in mod.rs; target 50 for pure re-export mods.
# Skips any mod.rs that has a sibling tests.rs — those are implementation modules
# whose tests were extracted to a subfile, not pure re-export coordinators.
echo ""
echo "=== mod.rs length check (max 150) ==="
LONG_MODS=""
while IFS= read -r modfile; do
    dir=$(dirname "$modfile")
    if [ -f "$dir/tests.rs" ]; then
        continue
    fi
    lines=$(wc -l < "$modfile")
    if [ "$lines" -gt 150 ]; then
        LONG_MODS="${LONG_MODS}     ${lines} ${modfile}"$'\n'
    fi
done < <(find crates/ -name 'mod.rs' \
  ! -path '*/target/*' ! -path '*aigc*' ! -path '*asr*' ! -path '*tts*' \
  2>/dev/null)
if [ -n "$LONG_MODS" ]; then
    echo "FAIL: mod.rs files exceeding 150 lines:"
    printf '%s' "$LONG_MODS"
    EXIT_CODE=1
else
    echo "PASS"
fi

# 3. Blocking I/O in async code (std::fs usage outside tests)
# Uses awk to track #[cfg(test)] block depth so inline test modules are
# correctly excluded regardless of how many lines separate the annotation
# from the std::fs usage.
echo ""
echo "=== Blocking I/O in async code ==="
BLOCKING=""
while IFS=: read -r file lineno _rest; do
    # Skip test files and target dir
    case "$file" in
        */target/*|*/tests.rs|*_test.rs|*/tests/*) continue ;;
    esac
    # Check ±2 lines for the explicit escape-hatch marker
    start=$((lineno > 2 ? lineno - 2 : 1))
    end=$((lineno + 2))
    if sed -n "${start},${end}p" "$file" | grep -q 'allow-blocking-io'; then
        continue
    fi
    # Use awk to determine whether this line is inside a #[cfg(test)] block.
    # Tracks brace depth from the cfg(test) annotation to the target line.
    in_test=$(awk -v target="$lineno" '
        /^[[:space:]]*#\[cfg\(test\)\]/ { pending=1; next }
        pending && /\{/ { test_depth=depth+1; pending=0 }
        pending { pending=0 }
        /\{/ { depth++ }
        /\}/ { depth--; if (test_depth > 0 && depth < test_depth) test_depth=0 }
        NR==target { print (test_depth > 0 ? "yes" : "no"); exit }
    ' "$file")
    if [ "$in_test" = "yes" ]; then
        continue
    fi
    BLOCKING="${BLOCKING}${file}:${lineno}: ${_rest}"$'\n'
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
