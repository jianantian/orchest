# 005 · CI 文档防线 — 实现计划

## 前置

issue 001（rustdoc 零 warning）和 issue 002（`basic_agent_run` 已注册）均已合入。否则接入 CI 会立即红线。

## 要读的现有代码

- `.github/workflows/ci.yml` — 既有 `check` job 结构，确认 step 顺序和 toolchain/cache 配置

## 步骤

### 1. 在 ci.yml 的 check job 追加两步

定位 `- name: cargo test` 之后，插入：

```yaml
      - name: cargo doc (deny warnings)
        run: RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

      - name: cargo build --examples
        run: cargo build --examples --workspace
```

保持与现有 step 相同的缩进和风格。放在 test 之后、TS wire naming check 之前或之后均可（无依赖），建议紧跟 test。

### 2. 本地验证

```bash
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo build --examples --workspace
```

两条都应 exit 0。

### 3. 推送触发 CI

合入后观察 PR / push 的 CI 是否全绿。

## 关键决策

- **`RUSTDOCFLAGS="-D warnings"` 而非 grep**：让 rustdoc warning 直接以非零退出码失败，是 CI 中可靠的硬门；`2>&1 | grep warning` 在 pipeline 退出码和误报上都不稳。
- **只加 step、不重构 CI**：保持现有单 job 结构，降低本 docs 迭代对 CI 的扰动面。matrix / job 拆分 / 缓存优化都不在范围。
