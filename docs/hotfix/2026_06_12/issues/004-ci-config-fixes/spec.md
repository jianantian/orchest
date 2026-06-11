# 004 · CI 和 Config 修复

## 背景

CI 脚本、.gitignore 和 Cargo 依赖配置中的 3 个 Medium 问题，全部是几行级修复。

## 4a. `.gitignore` 大小写错误

**文件**：`.gitignore:14`

```
.DS_STORE
```

macOS 生成的是 `.DS_Store`（混合大小写）。在 case-sensitive 文件系统（Linux CI runner）上，此规则不匹配实际文件。

**修复**：

```
.DS_Store
```

## 4b. `check-ts-event-wire-naming.sh` 扫描不存在的路径

**文件**：`scripts/check-ts-event-wire-naming.sh:10`

```bash
targets=(
  "js"
  "examples"
  "docs/iteration/v0_1/issues"   # 路径已移到 docs/archive/iteration/
)
```

`rg` 在不存在的路径上静默失败（`set -euo pipefail` 下 `if` 条件内不触发 abort），导致 `docs/archive/iteration/` 下的所有文件未被扫描。

**修复**：

1. 路径更正为 `docs/archive/iteration`
2. 加入启动时路径校验：

```bash
for t in "${targets[@]}"; do
  [[ -e "$t" ]] || { echo "error: scan target '$t' does not exist"; exit 1; }
done
```

## 4c. `agent-runtime-core` tokio features 过宽

**文件**：`crates/agent-runtime-core/Cargo.toml:7`

```toml
tokio = { version = "1", features = ["full"] }
```

`full` 启用所有 tokio 子系统。作为库 crate，应只声明实际使用的 features，避免强制下游引入不需要的子系统。对比 `agent-runtime-asr-providers` 正确使用了 `features = ["sync", "time", "rt", "macros"]`。

**修复**：替换为实际需要的 feature 列表。需要检查 core crate 中 tokio 的使用：

```bash
grep -rn "tokio::" crates/agent-runtime-core/src/
```

预期需要：`rt`（spawn/spawn_blocking）、`sync`（Mutex/mpsc/oneshot）、`time`（timeout/sleep）、`macros`（`#[tokio::main]` in tests）、`io-util`（AsyncBufReadExt/AsyncWriteExt in code_exec）、`process`（Command in code_exec/mcp）。

```toml
tokio = { version = "1", features = ["rt", "sync", "time", "macros", "io-util", "process"] }
```

实施时需验证 `cargo test -p agent-runtime-core` 全绿以确认无遗漏。

## 验收标准

- [ ] `.gitignore` 中为 `.DS_Store`（非 `.DS_STORE`）
- [ ] `check-ts-event-wire-naming.sh` 中所有 scan target 路径存在
- [ ] 脚本启动时校验路径存在性
- [ ] `agent-runtime-core/Cargo.toml` 的 tokio 不使用 `features = ["full"]`
- [ ] `cargo test --workspace` 全绿
