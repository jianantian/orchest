# 002 · basic_agent_run 示例 — Spec

## 背景

`examples/rust/` 已有 20 个示例，覆盖 Hook / Guardrail / Session / Handoff / Steering / SD / 多 Provider / Agent-as-Tool 等进阶场景，唯独缺一个**最简起步**示例。新用户第一眼需要的不是 supervised delegation，而是"如何用最少的代码跑起一个 agent"。

本示例是 quickstart 教程（issue 003）的代码锚点：quickstart 的分步代码须与本示例一致且可编译。

## 目标

新增 `examples/rust/basic_agent_run.rs`，展示最小可用 agent：provider → tool → config → run → 事件 → 完成，面向初学者，带分步注释。

## 范围

新增 `examples/rust/basic_agent_run.rs`，并在 `crates/agent-runtime-core/Cargo.toml` 注册 `[[example]]`。

示例须依次展示以下 5 步（与 quickstart 一一对应）：

1. **配置 provider**：用 `create_adapter_from_config(ProviderRuntimeConfig { model: "anthropic/claude-...", api_key_env: Some("ANTHROPIC_API_KEY"), .. })` 得到 `Box<dyn ModelAdapter>`，`Arc::from` 成 `Arc<dyn ModelAdapter>`
2. **定义并注册一个最简 tool**：实现 `Tool` trait（或用 `InProcessTool`），一个无副作用的 `get_current_time`，`approval: Approval::Never`；`ToolRegistry::new()` + `register(Arc::new(...))`
3. **构造 config**：`AgentConfig::builder("anthropic/claude-...").system_prompt(...).max_steps(n).build()?`
4. **启动 run**：`AgentRun::start(config, input, model, registry)` → `(handle, rx)`
5. **消费事件 + 等待完成**：`while let Some(ev) = rx.recv().await` 匹配关键事件（`ModelStreamChunk` / `ToolCallStarted` / `ToolCallCompleted` / `RunCompleted` / `RunFailed`），最后 `handle.wait().await`

## 约束

- **可编译可链接**，但**不强制可运行**（运行需真实 `ANTHROPIC_API_KEY`）。CI 只 `cargo build --example basic_agent_run`
- 只用 core + providers 的公共 API，不碰 Hook / Guardrail / Session / Watcher / Handoff
- tool 实现保持最短，逻辑用注释说明（example 面向初学者，是 AGENTS.md "不写注释"原则的明确例外）
- 用真实 provider（Anthropic），不用 MockModel —— 起步示例要展示真实接法；运行依赖 env var 在注释和顶部 `//!` 中说明

## 验收标准

- [ ] `examples/rust/basic_agent_run.rs` 存在
- [ ] 顶部有 `//!` 文档：一句话说明 + `Run with: ANTHROPIC_API_KEY=... cargo run --example basic_agent_run`
- [ ] 在 `crates/agent-runtime-core/Cargo.toml` 注册了 `[[example]] name = "basic_agent_run"`
- [ ] `cargo build --example basic_agent_run` 通过
- [ ] 示例覆盖上述 5 步，每步有注释
- [ ] 不依赖任何进阶模块（hook / guardrail / session / watcher / handoff）
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
- [ ] `cargo fmt --check` 通过

## Notes

- `create_adapter_from_config` 返回 `Result<Box<dyn ModelAdapter>, ModelError>`；`AgentRun::start` 要 `Arc<dyn ModelAdapter>`，用 `Arc::from(boxed)` 转换。
- `ProviderRuntimeConfig` 字段：`model` / `api_key` / `api_key_env` / `api_url` / `max_tokens`。起步示例用 `api_key_env: Some("ANTHROPIC_API_KEY".into())`，其余 `None`。
- tool 的 `input_schema()` 返回 `&JsonSchema`（即 `&serde_json::Value`）。无参 tool 可返回一个 `{"type":"object","properties":{}}` 的静态 schema（用 `OnceLock` 或 `Box::leak` 持有引用，参考 `tool/builtin.rs` 的做法）。
- 参考现有最简示例 `examples/rust/hook_logging.rs`（结构）和 `approval_mode_side_effect.rs`（自定义 Tool impl 写法），但去掉 hook / approval 部分。
