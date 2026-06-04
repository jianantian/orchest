# 007 · LlmWatcher

## 背景

`Watcher` trait 已存在（v0.8），需要一个 LLM-powered 实现——用另一个模型监控 worker 事件流，自主判断是否干预。这是 Supervised Delegation 的核心组件。

依赖 004（结构化事件让 LLM 输入更丰富）和 005（Steering API 让 watcher 能 steer 而不仅 inject/abort）。

## 契约

### 输入
- `Watcher` trait：`on_event(&self, event: &RuntimeEvent) -> WatcherAction`
- 结构化 `ToolCallStarted.metadata`、`ToolCallFailed.error: ToolError`（004）
- `WatcherAction::Steer` 变体（005）

### 输出
- `LlmWatcher` struct：实现 `Watcher` trait
- 事件累积 + 批量评估（`eval_interval`）
- LLM 输出通过 tool_use 结构化映射到 `WatcherAction`
- `LlmWatcherBuilder` 用于配置

## 影响范围

- 新建 `crates/agent-runtime-core/src/run/llm_watcher.rs`
- `crates/agent-runtime-core/src/run/mod.rs` — 导出
- `crates/agent-runtime-core/src/lib.rs` — 公共导出

## 设计要点

- watcher LLM 直接调用 `ModelAdapter::chat()`，不经过 run loop
- 事件累积到 buffer，每 `eval_interval` 个事件触发一次 LLM 评估
- LLM system prompt 描述监控职责；通过 tool_use 返回结构化 action（continue / inject / steer / abort + reason）
- 事件格式化为 LLM 可读的文本摘要（不是序列化 JSON）
- watcher 的 LLM 调用失败 → 降级为 `Continue`（不影响 worker）

## 验收标准

- [ ] `LlmWatcher` 实现 `Watcher` trait
- [ ] 事件累积 + `eval_interval` 批量评估
- [ ] LLM 输出通过 tool_use 结构化，映射为 `WatcherAction`
- [ ] watcher LLM 调用不经过 run loop
- [ ] LLM 调用失败降级为 `Continue`
- [ ] `LlmWatcherBuilder`：配置 model、system_prompt、eval_interval、intervention_criteria
- [ ] 单元测试：mock ModelAdapter → 验证事件累积和 action 映射
- [ ] 集成测试：LlmWatcher 成功 steer worker
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
