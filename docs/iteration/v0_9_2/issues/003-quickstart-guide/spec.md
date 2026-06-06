# 003 · 入门教程 quickstart — Spec

## 背景

新用户进入仓库后，README 提供了 Python/TS 的最短片段，但缺一篇"从零到一个可运行 Rust agent"的连贯教程。`docs/guide/quickstart.md` 填补这个空白：以 issue 002 的 `basic_agent_run.rs` 为代码锚点，逐步讲解每一步在做什么。

## 目标

新增 `docs/guide/quickstart.md`，覆盖从添加依赖到跑通一个 agent 的完整路径，代码与 `examples/rust/basic_agent_run.rs` 一致且可编译。

## 范围

新增 `docs/guide/quickstart.md`（中文为主，代码与标识符英文），结构：

1. **前置条件**：Rust toolchain、一个 provider API key（以 Anthropic 为例，`ANTHROPIC_API_KEY`）
2. **添加依赖**：在 `Cargo.toml` 加 `agent-runtime-core` 和 `agent-runtime-providers`
   - 因 v0.9.2 未发布 crates.io，给 git 依赖示例（`git = "https://github.com/jianantian/orchest"`）；附注"crates.io 版本待 v1.0 发布后更新为 `agent-runtime-core = \"x.y\"`"
3. **配置 provider**：`create_adapter_from_config` + `ProviderRuntimeConfig`，讲 `provider/model` 字符串约定和 API key 解析顺序（explicit → api_key_env → 默认 env var）
4. **注册 tool**：实现 `Tool` trait，讲 6 个方法各自作用 + `Approval` 三态
5. **启动 run + 监听事件**：`AgentRun::start` 返回 `(handle, rx)`；讲事件流模型，列出起步阶段会关心的几个 `RuntimeEvent` 变体
6. **等待完成**：`handle.wait().await`
7. **完整代码**：贴出与 `basic_agent_run.rs` 一致的完整文件，并指明 `cargo run --example basic_agent_run`
8. **下一步**：链接到进阶示例（hook / guardrail / session / SD）和 SDK 文档（issue 004）

并在 `README.md` 的 Documentation 表中新增一行链接到 `docs/guide/quickstart.md`，让新用户从 README 能发现入门教程。

## 约束

- 代码片段与 `examples/rust/basic_agent_run.rs`**逐字一致**（或直接引用其片段），避免文档漂移
- 不重复 SDK（Python/TS）内容——那是 issue 004
- 不讲发布 / crates.io 细节——那是 v1.0

## 验收标准

- [ ] `docs/guide/quickstart.md` 存在
- [ ] 覆盖范围中列出的 8 个小节
- [ ] 完整代码段与 `examples/rust/basic_agent_run.rs` 一致，可对照 `cargo build --example basic_agent_run` 验证
- [ ] 依赖小节给出 git 依赖示例，并注明 crates.io 版本待 v1.0
- [ ] 文末链接到进阶示例目录和 SDK 文档
- [ ] `README.md` Documentation 表新增 quickstart 链接
- [ ] 无失效内部链接（指向的 example 文件、sdk-*.md 路径存在）

## 依赖

- **issue 002**（`basic_agent_run.rs` 是本教程的代码锚点）

## Notes

- `RunHandle` 另有 `subscribe_events(capacity)` 可获得额外的事件订阅者（多订阅场景）；起步教程用 `start` 直接返回的 `rx` 即可，`subscribe_events` 作为"进阶"一句话带过。
- API key 解析顺序与其他 provider crate 一致：显式 `api_key` → `api_key_env` → provider 默认 env var。教程示例用 `api_key_env`。
