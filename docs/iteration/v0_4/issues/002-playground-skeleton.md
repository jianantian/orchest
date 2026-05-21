# 002 · Playground crate 骨架与 v0.1 scenario

## 背景

需要一个"真实代码路径上的端到端验收工具"，让 runtime 各能力被实际跑过、event 流被实际打印过。这个工具同时是 SDK 文档示例的来源。

形态已定：Rust CLI binary，不做 Web UI。

## 目标

建立 `playground/` crate 骨架，跑通第一个 scenario（v0.1 能力验收），并定下 scenario 框架，为后续 v0.2 / v0.3 scenario 留好扩展位。

## 验收标准

### Crate 骨架

- [ ] 新建 `playground/` 目录，作为 workspace member 加入根 `Cargo.toml`
- [ ] `playground/Cargo.toml` 声明 binary `orchest-playground`，依赖 `agent-runtime-core`、`clap`、`tokio`
- [ ] `playground/src/main.rs` 提供两个子命令：
  - `orchest-playground scenario <name>`：跑指定 scenario
  - `orchest-playground repl`：进入交互模式（本 issue 可仅留 stub，issue 003 完善）
- [ ] `playground/scenarios/mod.rs` + `playground/scenarios/v0_1_basic_loop.rs` 文件就位
- [ ] `cargo build --workspace` 通过

### v0.1 scenario：`v0_1_basic_loop`

- [ ] 注册一个 in-process tool（如 `get_weather`），用 Rust 实现
- [ ] 注册一个 builtin `read_file` tool，准备一个 fixture 文件
- [ ] 使用 mock model provider（不依赖 ANTHROPIC_API_KEY）：本 issue 内在 `playground/src/mock/` 实现一个 `ScriptedModelAdapter`，按预设脚本返回 tool call、文本、`stop_reason`。Mock provider **契约**（issue 003 / 007 复用）：
  - 支持按"轮"返回不同响应（构造时传入 `Vec<MockTurn>`）
  - 每个 `MockTurn` 可以是：文本输出（含 stream chunk 序列）、单个 tool call、`stop`
  - 支持 token usage 上报，以便 budget 路径可触发
  - 公共构造器在 `playground/src/mock/mod.rs` 暴露，issue 003 直接复用
- [ ] 运行 agent，让模型依次：调用 `read_file` → 调用 `get_weather` → 输出最终文本
- [ ] 所有 `RuntimeEvent` 实时打印到 stdout（结构化文本，不是 JSON dump）
- [ ] 触发以下三条路径并验证（事件名以 `crates/agent-runtime-core/src/events.rs` 为准）：
  - 正常 tool 调用：`ToolCallStarted` → `ToolCallCompleted` 事件成对出现
  - Approval gate：scenario 内将某 tool 标记为 `requires_approval`，事件流中观察到 `ApprovalRequested`；scenario 模拟拒绝，观察 `ApprovalDenied` 事件，且**该 tool 不产生 `ToolCallStarted`**
  - Budget 上限：scenario 配置极小 `BudgetConfig`，观察 `BudgetWarning` 事件出现，并在超额时 run 终止（事件流以 `RunFailed { error }` 收尾，error 字符串包含 "budget"）

### 验收可重放

- [ ] `cargo run -p playground -- scenario v0_1_basic_loop` 在 clean checkout（无任何 API key）下成功，进程 exit code 0
- [ ] scenario 跑完输出一行 `Scenario v0_1_basic_loop: OK`（失败时输出 `FAIL: <reason>` 并 exit code 非 0）
- [ ] 本 issue 同时创建 `.github/workflows/v0_4-playground.yml`（如 `.github/workflows/` 不存在则一并创建），包含运行 `cargo run -p playground -- scenario v0_1_basic_loop` 的 step；该 workflow 是后续 issue 003 / 006 追加 CI step 的统一入口

## 注意

- mock provider 用最小代码实现，**不要**引入 mockito / wiremock 这类 HTTP mock 框架——直接实现 `ModelAdapter` trait 返回预设 `ModelStreamChunk`
- scenario 文件结构要清晰：每个 scenario 一个文件，统一 trait（如 `trait Scenario { async fn run(&self) -> Result<()> }`），方便 issue 003 加新 scenario
- 不要把 scenario 中的事件断言写得太严格——目标是"跑通且事件序列符合预期形状"，不是单元测试粒度的精确比对
- REPL 实现留到 issue 003，本 issue 只占位
