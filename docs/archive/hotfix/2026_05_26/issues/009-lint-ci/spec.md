# 009 · Lint 配置 + CI 防线

## 背景

前面 8 个 issue 修复了存量问题。本 issue 落地 lint 和 CI 检查配置，确保这些问题不回归。

必须在 001–007 全部完成后执行，确认当前代码库已经满足所有检查条件后再锁定。

## clippy.toml

在项目根目录创建 `clippy.toml`：

```toml
too-many-lines-threshold = 200
too-many-arguments-threshold = 5
```

## Workspace lint 配置

在根 `Cargo.toml` 添加：

```toml
[workspace.lints.clippy]
too_many_lines = "warn"
too_many_arguments = "warn"
result_large_err = "warn"
```

各 crate 的 `Cargo.toml` 添加：

```toml
[lints]
workspace = true
```

## CI 脚本

创建 `scripts/lint-check.sh`：

```bash
#!/usr/bin/env bash
set -euo pipefail

EXIT_CODE=0

# 1. 文件超长检查（测试文件除外，阈值 700 行）
echo "=== File length check (max 700, excluding tests) ==="
LONG_FILES=$(find crates/ -name '*.rs' ! -name 'tests.rs' ! -name '*_test.rs' ! -path '*/target/*' \
  | xargs -r wc -l 2>/dev/null | awk '$1 > 700 {print}' | grep -v total || true)
if [ -n "$LONG_FILES" ]; then
    echo "FAIL: Files exceeding 700 lines:"
    echo "$LONG_FILES"
    EXIT_CODE=1
else
    echo "PASS"
fi

# 2. mod.rs 业务逻辑检查（mod.rs 不应超过 50 行）
echo ""
echo "=== mod.rs length check (max 50) ==="
LONG_MODS=$(find crates/ -name 'mod.rs' ! -path '*/target/*' \
  | xargs -r wc -l 2>/dev/null | awk '$1 > 50 {print}' | grep -v total || true)
if [ -n "$LONG_MODS" ]; then
    echo "FAIL: mod.rs files exceeding 50 lines:"
    echo "$LONG_MODS"
    EXIT_CODE=1
else
    echo "PASS"
fi

# 3. async 代码中的阻塞 I/O 检查
echo ""
echo "=== Blocking I/O in async code ==="
BLOCKING=$(grep -rn 'std::fs::' crates/ --include='*.rs' \
  | grep -v '/target/' | grep -v '/tests\.rs:' | grep -v '/_test\.rs:' | grep -v '/tests/' \
  | grep -v '// allow-blocking-io' || true)
if [ -n "$BLOCKING" ]; then
    echo "FAIL: std::fs usage in non-test code (use tokio::fs or spawn_blocking):"
    echo "$BLOCKING"
    EXIT_CODE=1
else
    echo "PASS"
fi

# 4. clippy allow 残留检查
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
```

注意：

- 文件长度阈值 700（而非 AGENTS.md 的 400），给 provider 实现留空间。007 完成 A4 拆分后，应评估是否可以收紧到 400 并同步更新 AGENTS.md。最终目标：CI 脚本阈值与 AGENTS.md 约定一致
- `// allow-blocking-io` 和 `// justified: <reason>` 是逃生舱——必须附带理由注释才能通过
- 脚本应加入 CI pipeline，与 `cargo clippy -- -D warnings` 并行执行

## GitHub Actions 集成

在现有 CI 配置中添加一个 step：

```yaml
- name: Lint checks
  run: bash scripts/lint-check.sh
```

## 验收标准

- [ ] `clippy.toml` 存在且配置正确
- [ ] `Cargo.toml` 含 `[workspace.lints.clippy]` 配置
- [ ] 各 crate 的 `Cargo.toml` 含 `[lints] workspace = true`
- [ ] `scripts/lint-check.sh` 存在且可执行
- [ ] `bash scripts/lint-check.sh` 在当前代码库上全部 PASS（零违规）
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
- [ ] CI pipeline 包含 lint-check step
- [ ] 文件长度阈值（CI 脚本）与 AGENTS.md 约定一致（若不一致需更新 AGENTS.md）
