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
- [ ] 使用 mock model provider（不依赖 ANTHROPIC_API_KEY）：本 issue 内在 `playground/src/mock/` 实现一个最小 `ModelAdapter`，按预设脚本返回 tool call 与文本
- [ ] 运行 agent，让模型依次：调用 `read_file` → 调用 `get_weather` → 输出最终文本
- [ ] 所有 `RuntimeEvent` 实时打印到 stdout（结构化文本，不是 JSON dump）
- [ ] 触发以下三条路径并验证：
  - 正常 tool 调用：`ToolCallStarted` → `ToolCallCompleted` 事件成对出现
  - Approval gate：scenario 内将某 tool 标记为 `requires_approval`，stdout 模拟用户输入 `y`/`n`，验证拒绝路径输出 `ToolCallDenied`
  - Budget 上限：scenario 配置极小 `BudgetConfig`，验证触发 `BudgetExceeded` 后 run 终止

### 验收可重放

- [ ] `cargo run -p playground -- scenario v0_1_basic_loop` 在 clean checkout（无任何 API key）下成功
- [ ] CI 配置中新增一个 step 运行该 scenario（exit code 0 即视为通过）
- [ ] scenario 跑完输出一行 `Scenario v0_1_basic_loop: OK`（失败时输出 `FAIL: <reason>`）

## 注意

- mock provider 用最小代码实现，**不要**引入 mockito / wiremock 这类 HTTP mock 框架——直接实现 `ModelAdapter` trait 返回预设 `ModelStreamChunk`
- scenario 文件结构要清晰：每个 scenario 一个文件，统一 trait（如 `trait Scenario { async fn run(&self) -> Result<()> }`），方便 issue 003 加新 scenario
- 不要把 scenario 中的事件断言写得太严格——目标是"跑通且事件序列符合预期形状"，不是单元测试粒度的精确比对
- REPL 实现留到 issue 003，本 issue 只占位
