# 005 · CI 文档防线 — Spec

## 背景

v0.9.2 的多条验收标准依赖 CI 强制执行："`cargo doc` 无 warning"、"`cargo build --example basic_agent_run` 通过"、"`cargo build --examples` 全部通过"。但当前 `.github/workflows/ci.yml` 只跑 fmt / clippy / test / TS wire naming / lint-check，**没有 doc 检查，也没有 examples 编译**。

后果：issue 001 修好的 rustdoc warning 没有回归防线，下次再写裸 HTML tag 又会引入；issue 002 的示例编译也无人守护。本 issue 给 CI 加上这两道防线，让上述验收标准有可执行载体。

## 目标

`.github/workflows/ci.yml` 新增两步：rustdoc warning 硬失败、全 workspace examples 编译，并入既有 `check` job。

## 范围

在 `.github/workflows/ci.yml` 的 `check` job 中新增（置于 `cargo test` 之后）：

1. **rustdoc 防线**：
   ```yaml
   - name: cargo doc (deny warnings)
     run: RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
   ```
   用 `RUSTDOCFLAGS="-D warnings"` 让 warning 直接 fail，而非只打印（`grep` 方案在 CI 中不可靠）。

2. **examples 编译防线**：
   ```yaml
   - name: cargo build --examples
     run: cargo build --examples --workspace
   ```

## 不在范围内

- release workflow（→ v1.0）
- 运行 examples（需真实 provider key；只编译不运行）
- 缓存策略调整、job 拆分、matrix —— 保持现有 CI 结构，只加 step

## 依赖

- **issue 001**（CI doc 防线启用前，rustdoc 须已零 warning，否则 CI 立即红）
- **issue 002**（`basic_agent_run` 须已存在并注册，否则 `--examples` 失败）

## 验收标准

- [ ] `.github/workflows/ci.yml` 含 `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` step
- [ ] `.github/workflows/ci.yml` 含 `cargo build --examples --workspace` step
- [ ] 两步置于 `check` job，在 `cargo test` 之后
- [ ] 本地验证两条命令均通过（基线已确认：doc 零 warning、21 examples 全编译）
- [ ] PR 触发的 CI 全绿

## Notes

- 基线已实跑确认：修复 001 后 `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` 通过；`cargo build --examples --workspace` 当前 21 个示例全部通过（exit 0）。因此本 issue 在 001/002 合入后接入 CI 不会引入红线。
- `cargo doc` 已隐式编译全 workspace，与 `cargo test` 有部分重复编译；但 doc 产物和 warning 检查是 test 覆盖不到的，值得单列。若后续 CI 耗时敏感，可再评估合并，不在本 issue 处理。
