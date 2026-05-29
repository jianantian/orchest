# 009 · Lint 配置 + CI 防线 — 实施计划

## 依赖

在 001–007 全部完成后执行。先确认代码库满足所有检查条件，再锁定。

## 前置检查

在动手之前先验证当前状态：

```bash
# 1. clippy 干净度
cargo clippy --workspace -- -D warnings

# 2. 文件长度
find crates/ -name '*.rs' ! -name 'tests.rs' ! -name '*_test.rs' ! -path '*/target/*' \
  | xargs wc -l | awk '$1 > 700' | sort -rn

# 3. mod.rs 长度
find crates/ -name 'mod.rs' ! -path '*/target/*' \
  | xargs wc -l | awk '$1 > 50' | sort -rn

# 4. std::fs 残留
grep -rn 'std::fs::' crates/ --include='*.rs' | grep -v target | grep -v test

# 5. clippy allow 残留
grep -rn '#\[allow(clippy::' crates/ --include='*.rs' | grep -v target
```

如果有违规，退回对应 issue 修复后再来。

## 步骤

### Step 1: clippy.toml

**文件**：项目根目录 `clippy.toml`（新建）

```toml
too-many-lines-threshold = 200
too-many-arguments-threshold = 5
```

### Step 2: Workspace lint 配置

**文件**：根 `Cargo.toml`

在 `[workspace]` 下方添加：
```toml
[workspace.lints.clippy]
too_many_lines = "warn"
too_many_arguments = "warn"
result_large_err = "warn"
```

### Step 3: 各 crate 继承 workspace lint

**文件**：每个 crate 的 `Cargo.toml`（4 个文件）

添加：
```toml
[lints]
workspace = true
```

### Step 4: 验证 clippy 配置生效

```bash
cargo clippy --workspace -- -D warnings
```

如果新的 lint 规则触发新的警告，说明 001–007 有遗漏，退回修复。

### Step 5: CI 脚本

**文件**：`scripts/lint-check.sh`（新建）

按 spec 中的脚本内容创建，确保：
1. `xargs` 使用 `-r` flag（macOS 无此 flag，需要兼容处理或用 `xargs` 无 `-r`）
2. 阻塞 I/O 检查用文件路径排除测试文件（而非 `grep -v '#[cfg(test)]'`）
3. `// allow-blocking-io` 和 `// justified: <reason>` 作为逃生舱

macOS 兼容性注意：macOS 的 `xargs` 不支持 `-r`。替代方案：
```bash
# 替代 xargs -r
find ... -print0 | xargs -0 wc -l 2>/dev/null | ...
# 或用 if 检查是否有输出
```

### Step 6: 设置脚本可执行

```bash
chmod +x scripts/lint-check.sh
```

### Step 7: 运行脚本验证全 PASS

```bash
bash scripts/lint-check.sh
```

所有 4 项检查应该 PASS。如果不是，退回修复。

### Step 8: GitHub Actions 集成

**文件**：`.github/workflows/ci.yml`（或现有 CI 配置文件）

在现有 CI 步骤后添加：
```yaml
- name: Lint checks
  run: bash scripts/lint-check.sh
```

### Step 9: 阈值对齐检查

确认 CI 脚本的文件长度阈值（700）与 AGENTS.md 约定（400）的关系。如果 007 完成后所有文件都在 400 以内，收紧阈值。否则更新 AGENTS.md 约定为 700 并说明原因（provider 实现文件需要更多空间）。

## 文件影响范围

```
clippy.toml                          — 新建
Cargo.toml                           — workspace lints
crates/agent-runtime-core/Cargo.toml — [lints] workspace = true
crates/agent-runtime-providers/Cargo.toml — 同上
crates/agent-runtime-py/Cargo.toml   — 同上
crates/agent-runtime-node/Cargo.toml — 同上
scripts/lint-check.sh                — 新建
.github/workflows/ci.yml             — 新增 step
```

## 验证

```bash
cargo clippy --workspace -- -D warnings  # 全绿
bash scripts/lint-check.sh               # 全 PASS
cargo test --workspace                   # 不破坏已有测试
```
