# 002 实现路线

## 步骤

1. **在 `run/config.rs` 中替换 `AgentConfig`**
   - 先在文件底部写新的类型定义（`ModelConfig`, `SkillsConfig`, `RuntimeConfig`, `CompactionConfig`, 新版 `AgentConfig`, `AgentConfigBuilder`），不删旧的
   - 让 `AgentConfig` 新旧同名——Rust 会报重复定义，此时删除旧的
   - 运行 `cargo build -p agent-runtime-core` 看哪些调用点报错，按错误列表逐个修

2. **按字段映射表修改所有调用点**

   字段访问替换规则（见 spec）：

   | 旧 | 新 |
   |----|-----|
   | `config.model` | `config.model.spec` |
   | `config.request_options` | `config.model.options` |
   | `config.max_steps` | `config.runtime.max_steps` |
   | `config.allowed_skills` | `config.skills.allowed` |
   | `config.allowed_tools` | `config.runtime.allowed_tools` |
   | `config.mcp_servers` | `config.runtime.mcp_servers` |
   | `config.tool_search_enabled` | `config.runtime.tool_search_enabled` |
   | `config.compaction_threshold` | `config.runtime.compaction.as_ref().map(\|c\| c.threshold)` |
   | `config.compaction_recent_messages` | `config.runtime.compaction.as_ref().map(\|c\| c.recent_messages).unwrap_or(10)` |
   | `config.webhook_enabled` | `config.runtime.webhook_enabled` |
   | `config.code_execution_enabled` | `config.runtime.code_execution_enabled` |
   | `config.skills_dir` | `config.skills.dir` |
   | `config.run_depth` | `config.runtime.run_depth` |

   主要改动文件：
   - `run/loop_.rs` — 最多改动，config 字段访问集中在这里
   - `run/sub_agent.rs` — `SubAgentRuntime::cap_budget` 和 `execute_agent_delegate`
   - `run/skills.rs` — `register_skills` 函数签名
   - `run/webhook.rs` — `webhook_enabled` 字段
   - `run/compaction.rs` — `compaction_threshold` / `compaction_recent_messages`
   - `crates/agent-runtime-py/src/lib.rs` — FFI 构建 `AgentConfig` 处
   - `crates/agent-runtime-node/src/lib.rs` — 同上
   - `crates/agent-runtime-core/tests/e2e_validation.rs` — 测试中的 `AgentConfig { ... }` 字面量
   - `crates/agent-runtime-core/tests/v03_runtime.rs` — 同上

3. **修改测试中的 `AgentConfig` 构建方式**
   - 用 builder API 替换所有 `AgentConfig { system_prompt: "...", model: ModelSpec("..."), ... }` 字面量
   - 在集成测试里用 `AgentConfig::builder("provider/model").system_prompt("...").build()`

4. **添加 builder 测试和 serde round-trip 测试**
   - 在 `run/config.rs` 底部的 `#[cfg(test)]` 块添加 spec 中要求的两个测试
   - 运行 `cargo test -p agent-runtime-core config` 确认通过

5. **验收**
   - `cargo test --workspace` 全绿
   - `cargo clippy --workspace -- -D warnings` 全绿

## 要读的现有代码

- `crates/agent-runtime-core/src/run/config.rs`（001 拆分后）— 现有 `AgentConfig` 的完整字段
- `crates/agent-runtime-core/tests/e2e_validation.rs` — 了解测试中如何构建 `AgentConfig`，需要全部改为 builder
- `crates/agent-runtime-py/src/lib.rs` — FFI 层如何序列化/反序列化 `AgentConfig`

## 关键决策

- **`run_depth` 放在 `RuntimeConfig` 还是单独字段**：放 `RuntimeConfig`，它是运行时内部状态，不是用户配置——builder 上提供 `run_depth()` 方法但文档标注为 internal use
- **`CompactionConfig` 是否提供 `Default`**：提供，默认 `threshold=0.8, recent_messages=10`，与现有 `default_recent_messages()` 的值一致
- **serde 兼容性**：这是 breaking change（JSON 结构变了）。内部 SDK 可以接受，但如果有持久化的 `RunState`（含 `AgentConfig`），需要迁移脚本或版本字段——`RunState.schema_version` 已存在，可升级为 `"0.6"`
