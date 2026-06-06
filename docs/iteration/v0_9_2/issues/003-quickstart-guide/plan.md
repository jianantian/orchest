# 003 · 入门教程 quickstart — 实现计划

## 前置

issue 002 完成（`examples/rust/basic_agent_run.rs` 已存在且 `cargo build --example basic_agent_run` 通过）。

## 要读的现有代码

- `examples/rust/basic_agent_run.rs` — 代码锚点，逐字引用
- `README.md:27-83` — Quick start / Provider format 既有措辞，保持一致
- `crates/agent-runtime-providers/src/lib.rs` — API key 解析顺序、`provider/model` 约定
- `crates/agent-runtime-core/src/events.rs` — 挑选起步阶段要介绍的 `RuntimeEvent` 变体

## 步骤

### 1. 建目录 + 文件

`docs/guide/quickstart.md`（`docs/guide/` 目录此 issue 首次创建）。

### 2. 按 8 小节填充

- 前置条件 / 添加依赖 / 配置 provider / 注册 tool / 启动 run + 事件 / 等待完成 / 完整代码 / 下一步
- 代码块直接从 `basic_agent_run.rs` 复制，分段穿插讲解
- 依赖小节：

  ```toml
  [dependencies]
  agent-runtime-core = { git = "https://github.com/jianantian/orchest" }
  agent-runtime-providers = { git = "https://github.com/jianantian/orchest" }
  tokio = { version = "1", features = ["full"] }
  ```

  附注："crates.io 发布后改为 `agent-runtime-core = \"x.y\"`（计划于 v1.0）"

### 3. 事件小节列出起步关心的变体

`RunStarted` / `ModelStreamChunk` / `ToolCallStarted` / `ToolCallCompleted` / `ToolCallFailed` / `RunCompleted` / `RunFailed`，各一行说明。完整变体列表指向 rustdoc。

### 4. 下一步链接

- 进阶示例：`examples/rust/`（点名 hook / guardrail / session_persist_resume / supervised_delegation）
- SDK：`docs/guide/sdk-python.md`、`docs/guide/sdk-typescript.md`（issue 004 产出；若 004 未完成，先留链接）

### 5. 验证

```bash
cargo build --example basic_agent_run   # 确认锚点代码可编译
# 人工核对 quickstart 代码块与 basic_agent_run.rs 一致
```

## 关键决策

- **代码锚点单一来源**：quickstart 不维护独立代码，全部引用 `basic_agent_run.rs`，杜绝文档与示例漂移。若两者有出入，以 example（可编译者）为准。
- **依赖用 git 而非 path**：教程面向外部用户，path 依赖只适合仓库内开发；git 依赖是 crates.io 发布前的过渡形态。
