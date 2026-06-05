# v0.6 Architecture Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clean up eight accumulated architecture debts in the agent runtime: split run.rs, fix budget pricing, unify sub-agent paths, refactor AgentConfig, make MCP stdio concurrent, add tiktoken, share reqwest client, and improve Python SDK ergonomics.

**Architecture:** Pure refactor + targeted additions — no new user-facing features. Issues 001 and 002 must land first; 003–007 can proceed in parallel thereafter; 008 is last.

**Tech Stack:** Rust (tokio, serde, tiktoken-rs 0.6, reqwest), PyO3 0.28, napi-rs 2

---

## File Map

**Created:**
- `crates/agent-runtime-core/src/run/mod.rs`
- `crates/agent-runtime-core/src/run/config.rs`
- `crates/agent-runtime-core/src/run/handle.rs`
- `crates/agent-runtime-core/src/run/loop_.rs`
- `crates/agent-runtime-core/src/run/tool_exec.rs`
- `crates/agent-runtime-core/src/run/sub_agent.rs`
- `crates/agent-runtime-core/src/run/skills.rs`
- `crates/agent-runtime-core/src/run/webhook.rs`
- `crates/agent-runtime-core/src/run/compaction.rs`
- `crates/agent-runtime-core/src/run/helpers.rs`
- `crates/agent-runtime-core/src/tokenizer.rs`
- `crates/agent-runtime-providers/src/http.rs`
- `python/agent_runtime/exceptions.py`
- `python/tests/test_run_sync.py`

**Deleted:**
- `crates/agent-runtime-core/src/run.rs`

**Modified:**
- `crates/agent-runtime-core/src/lib.rs` (add `pub mod tokenizer`)
- `crates/agent-runtime-core/Cargo.toml` (add tiktoken-rs)
- `crates/agent-runtime-providers/src/types.rs` (ModelPricing, TokenUsage.cost_usd)
- `crates/agent-runtime-providers/src/anthropic.rs` (pricing, shared client)
- `crates/agent-runtime-providers/src/openai.rs` (shared client)
- `crates/agent-runtime-providers/src/deepseek.rs` (shared client)
- `crates/agent-runtime-providers/src/openrouter.rs` (shared client)
- `crates/agent-runtime-providers/src/lib.rs` (add http module)
- `crates/agent-runtime-core/src/budget.rs` (remove pricing constants)
- `crates/agent-runtime-core/src/events.rs` (add SubAgentEvent)
- `crates/agent-runtime-core/src/tool/mcp.rs` (concurrent McpStdioClient)
- `crates/agent-runtime-py/src/lib.rs` (structured error mapping)
- `python/agent_runtime/__init__.py` (run_sync, exception exports)

---

## Task 1: run.rs → run/ directory split (Issue 001)

**Files:**
- Delete: `crates/agent-runtime-core/src/run.rs`
- Create: `crates/agent-runtime-core/src/run/mod.rs`
- Create: `crates/agent-runtime-core/src/run/config.rs`
- Create: `crates/agent-runtime-core/src/run/handle.rs`
- Create: `crates/agent-runtime-core/src/run/loop_.rs`
- Create: `crates/agent-runtime-core/src/run/tool_exec.rs`
- Create: `crates/agent-runtime-core/src/run/sub_agent.rs`
- Create: `crates/agent-runtime-core/src/run/skills.rs`
- Create: `crates/agent-runtime-core/src/run/webhook.rs`
- Create: `crates/agent-runtime-core/src/run/compaction.rs`
- Create: `crates/agent-runtime-core/src/run/helpers.rs`
- Modify: `crates/agent-runtime-core/src/lib.rs`

- [ ] **Step 1: Verify baseline tests pass**

  ```bash
  cargo test --workspace 2>&1 | tail -5
  ```
  Expected: all tests pass, note the exact count for regression comparison.

- [ ] **Step 2: Create `run/mod.rs` with public re-exports**

  Create `crates/agent-runtime-core/src/run/mod.rs`:
  ```rust
  //! Agent run loop coordination — public types and entry point.

  mod config;
  mod handle;
  mod helpers;
  mod loop_;
  mod tool_exec;
  mod sub_agent;
  mod skills;
  mod webhook;
  mod compaction;

  pub use config::{AgentConfig, AgentRun, RunId, RunState, RunStatus, SubAgentRuntime};
  pub use handle::{ApprovalSlot, EventReceiver, RunHandle};
  ```

- [ ] **Step 3: Move types to `run/config.rs`**

  Move the following from `run.rs` into `crates/agent-runtime-core/src/run/config.rs`:
  - `RunId` (lines 32–51)
  - `AgentConfig` (lines 53–79)
  - `default_recent_messages()` (line 81)
  - `RunState` (lines 85–96)
  - `RunStatus` (lines 98–117)
  - `AgentRun` struct (line 186)
  - `AgentRun::start()` (lines 346–380)
  - `SubAgentRuntime` (lines 188–199)
  - `narrow_permission_list`, `min_option`, `min_option_f64` (lines 236–263)

  Add at the top:
  ```rust
  //! Agent configuration types and run entry point.
  use std::collections::HashMap;
  use std::sync::Arc;
  use serde::{Deserialize, Serialize};
  use tokio::sync::{mpsc, Mutex};
  use uuid;
  use crate::budget::{BudgetConfig, BudgetGuard};
  use crate::events::RuntimeEvent;
  use crate::model::{ModelAdapter, ModelSpec, RequestOptions};
  use crate::tool::mcp::McpServerConfig;
  use crate::tool::registry::ToolRegistry;
  use super::handle::{ApprovalSlot, EventReceiver, RunHandle};
  use super::loop_::run_loop;
  ```

- [ ] **Step 4: Move handle types to `run/handle.rs`**

  Move into `crates/agent-runtime-core/src/run/handle.rs`:
  - `ApprovalSlot` type alias (line 139)
  - `RunHandle` struct (lines 141–146)
  - `RunHandle::wait()` and `RunHandle::respond_approval()` (lines 148–173)
  - `take_and_send()` (lines 176–184)
  - `EventReceiver` type alias (line 119)

  ```rust
  //! RunHandle and approval routing.
  use std::collections::HashMap;
  use std::sync::Arc;
  use tokio::sync::{oneshot, Mutex};
  use super::config::RunId;
  use crate::events::RuntimeEvent;

  pub type ApprovalSlot = Arc<Mutex<Option<oneshot::Sender<bool>>>>;
  pub type EventReceiver = tokio::sync::mpsc::Receiver<RuntimeEvent>;
  ```

- [ ] **Step 5: Move webhook to `run/webhook.rs`**

  Move into `crates/agent-runtime-core/src/run/webhook.rs`:
  - `WebhookRuntime` struct (lines 121–131)
  - `WebhookRuntime::drop()` (lines 127–131)
  - `start_webhook_server()` (lines 1529–1647)
  - `write_http_response()` (lines 1648–1670)

  ```rust
  //! Webhook server for async tool callbacks.
  ```

- [ ] **Step 6: Move skill registration to `run/skills.rs`**

  Move into `crates/agent-runtime-core/src/run/skills.rs`:
  - `register_skills()` (lines 265–340)

  ```rust
  //! Skill discovery and tool registration.
  ```

- [ ] **Step 7: Move compaction to `run/compaction.rs`**

  Move into `crates/agent-runtime-core/src/run/compaction.rs`:
  - `maybe_compact_context()` (lines 1249–1359)

  ```rust
  //! Context window compaction.
  ```

- [ ] **Step 8: Move sub-agent functions to `run/sub_agent.rs`**

  Move into `crates/agent-runtime-core/src/run/sub_agent.rs`:
  - `execute_sub_agent_request()` (lines 918–1108)
  - `execute_agent_delegate()` (lines 1109–1233)
  - `parse_budget_config()` (lines 1234–1247)

  ```rust
  //! Sub-agent execution — delegating to child agent runs.
  ```

- [ ] **Step 9: Move tool execution to `run/tool_exec.rs`**

  The per-tool-call dispatch logic from within `run_loop` (lines 622–915 approximately, the inner `for tool_call in &tool_uses` loop body) should be extracted into a function in `run/tool_exec.rs`:

  ```rust
  //! Single tool call execution: approval gate, timeout, output handling.

  pub(super) async fn execute_tool_call(
      tool_call: &ToolCall,
      registry: &ToolRegistry,
      unfiltered_registry: &ToolRegistry,
      config: &AgentConfig,
      budget: &mut BudgetGuard,
      pending_approval: &ApprovalSlot,
      active_children: &Arc<Mutex<HashMap<RunId, ApprovalSlot>>>,
      tx: &mpsc::Sender<RuntimeEvent>,
      webhook_runtime: Option<&WebhookRuntime>,
      run_id: RunId,
      tool_defs: &mut Vec<ToolDef>,
  ) -> Option<ContentBlock> { ... }
  ```

  Move `poll_async_job()` (lines 1386–1528) and `connect_mcp_servers()` (lines 1671–end) here or to `helpers.rs`.

- [ ] **Step 10: Move helpers to `run/helpers.rs`**

  Move into `crates/agent-runtime-core/src/run/helpers.rs`:
  - `truncate_output()` (lines 201–218)
  - `truncate_str_utf8_safe()` (lines 223–234)
  - `append_searched_tool_defs()` (lines 1360–1385)
  - `connect_mcp_servers()` (~lines 1671–end)
  - `emit()` (line 383)

  ```rust
  //! Internal helpers: output truncation, MCP setup, tool search, event emit.
  ```

- [ ] **Step 11: Create `run/loop_.rs` with the remaining run_loop body**

  Move `run_loop()` function into `crates/agent-runtime-core/src/run/loop_.rs`. At this point `run_loop` should only contain the loop skeleton — tool dispatch calls `tool_exec::execute_tool_call`, compaction calls `compaction::maybe_compact_context`, etc.

  ```rust
  //! Main agent run loop.
  ```

- [ ] **Step 12: Update `lib.rs` if needed**

  `crates/agent-runtime-core/src/lib.rs` should already have `pub mod run;` — verify it still compiles after the file rename.

- [ ] **Step 13: Verify compile**

  ```bash
  cargo build -p agent-runtime-core 2>&1 | grep -E "^error"
  ```
  Expected: no output (no errors).

- [ ] **Step 14: Verify no regressions**

  ```bash
  cargo test --workspace 2>&1 | tail -10
  ```
  Expected: same test count as Step 1, all passing.

- [ ] **Step 15: Check file sizes**

  ```bash
  wc -l crates/agent-runtime-core/src/run/*.rs | sort -rn | head -15
  ```
  Expected: no file exceeds 700 lines.

- [ ] **Step 16: Lint**

  ```bash
  cargo clippy --workspace -- -D warnings 2>&1 | grep -E "^error"
  ```
  Expected: no output.

- [ ] **Step 17: Commit**

  ```bash
  git add crates/agent-runtime-core/src/run/ crates/agent-runtime-core/src/lib.rs
  git rm crates/agent-runtime-core/src/run.rs
  git commit -m "refactor(core): split run.rs into run/ module directory

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 2: AgentConfig grouped refactor + Builder (Issue 002)

**Files:**
- Modify: `crates/agent-runtime-core/src/run/config.rs`
- Modify: all files that access `AgentConfig` fields (run/loop_.rs, run/sub_agent.rs, run/skills.rs, run/webhook.rs, run/compaction.rs, agent-runtime-py/src/lib.rs, agent-runtime-node/src/lib.rs, tests/)

- [ ] **Step 1: Write failing test in `run/config.rs`**

  Add at the bottom of `crates/agent-runtime-core/src/run/config.rs`:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn builder_sets_fields_correctly() {
          let config = AgentConfig::builder("anthropic/claude-sonnet-4-6")
              .system_prompt("test")
              .max_cost_usd(1.0)
              .skills_dir("./skills")
              .max_steps(50)
              .enable_tool_search()
              .build();
          assert_eq!(config.system_prompt, "test");
          assert_eq!(config.budget.max_cost_usd, Some(1.0));
          assert_eq!(config.skills.dir.as_deref(), Some("./skills"));
          assert_eq!(config.runtime.max_steps, 50);
          assert!(config.runtime.tool_search_enabled);
      }

      #[test]
      fn config_roundtrips_json() {
          let config = AgentConfig::builder("anthropic/claude-sonnet-4-6")
              .system_prompt("hello")
              .max_steps(10)
              .build();
          let json = serde_json::to_string(&config).unwrap();
          let decoded: AgentConfig = serde_json::from_str(&json).unwrap();
          assert_eq!(decoded.system_prompt, "hello");
          assert_eq!(decoded.runtime.max_steps, 10);
      }
  }
  ```

- [ ] **Step 2: Run test to confirm it fails**

  ```bash
  cargo test -p agent-runtime-core builder_sets_fields_correctly 2>&1 | tail -5
  ```
  Expected: FAIL — `AgentConfig::builder` not found.

- [ ] **Step 3: Replace `AgentConfig` with grouped struct + add sub-types**

  In `crates/agent-runtime-core/src/run/config.rs`, replace the flat `AgentConfig` with:

  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct AgentConfig {
      pub system_prompt: String,
      pub model: ModelConfig,
      pub budget: BudgetConfig,
      pub skills: SkillsConfig,
      pub runtime: RuntimeConfig,
  }

  #[derive(Debug, Clone, Serialize, Deserialize, Default)]
  pub struct ModelConfig {
      pub spec: ModelSpec,
      #[serde(default)]
      pub options: RequestOptions,
  }

  #[derive(Debug, Clone, Serialize, Deserialize, Default)]
  pub struct SkillsConfig {
      pub dir: Option<String>,
      pub allowed: Option<Vec<String>>,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct RuntimeConfig {
      pub max_steps: u32,
      pub allowed_tools: Option<Vec<String>>,
      #[serde(default)]
      pub mcp_servers: Vec<McpServerConfig>,
      #[serde(default)]
      pub tool_search_enabled: bool,
      #[serde(default)]
      pub compaction: Option<CompactionConfig>,
      #[serde(default)]
      pub webhook_enabled: bool,
      #[serde(default)]
      pub code_execution_enabled: bool,
      #[serde(default)]
      pub run_depth: u32,
  }

  impl Default for RuntimeConfig {
      fn default() -> Self {
          Self {
              max_steps: 100,
              allowed_tools: None,
              mcp_servers: vec![],
              tool_search_enabled: false,
              compaction: None,
              webhook_enabled: false,
              code_execution_enabled: false,
              run_depth: 0,
          }
      }
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct CompactionConfig {
      pub threshold: f32,
      pub recent_messages: usize,
  }

  impl Default for CompactionConfig {
      fn default() -> Self { Self { threshold: 0.8, recent_messages: 10 } }
  }

  impl AgentConfig {
      pub fn builder(model: impl Into<String>) -> AgentConfigBuilder {
          AgentConfigBuilder::new(model.into())
      }
  }

  pub struct AgentConfigBuilder {
      system_prompt: String,
      model: ModelConfig,
      budget: BudgetConfig,
      skills: SkillsConfig,
      runtime: RuntimeConfig,
  }

  impl AgentConfigBuilder {
      pub fn new(model: impl Into<String>) -> Self {
          Self {
              system_prompt: String::new(),
              model: ModelConfig { spec: ModelSpec(model.into()), options: RequestOptions::default() },
              budget: BudgetConfig::default(),
              skills: SkillsConfig::default(),
              runtime: RuntimeConfig::default(),
          }
      }
      pub fn system_prompt(mut self, p: impl Into<String>) -> Self { self.system_prompt = p.into(); self }
      pub fn max_cost_usd(mut self, v: f64) -> Self { self.budget.max_cost_usd = Some(v); self }
      pub fn max_tokens(mut self, v: u64) -> Self { self.budget.max_tokens = Some(v); self }
      pub fn max_tool_calls(mut self, v: u32) -> Self { self.budget.max_tool_calls = Some(v); self }
      pub fn skills_dir(mut self, d: impl Into<String>) -> Self { self.skills.dir = Some(d.into()); self }
      pub fn allowed_skills(mut self, s: Vec<String>) -> Self { self.skills.allowed = Some(s); self }
      pub fn max_steps(mut self, v: u32) -> Self { self.runtime.max_steps = v; self }
      pub fn mcp_server(mut self, c: McpServerConfig) -> Self { self.runtime.mcp_servers.push(c); self }
      pub fn enable_tool_search(mut self) -> Self { self.runtime.tool_search_enabled = true; self }
      pub fn enable_code_execution(mut self) -> Self { self.runtime.code_execution_enabled = true; self }
      pub fn enable_compaction(mut self, c: CompactionConfig) -> Self { self.runtime.compaction = Some(c); self }
      pub fn run_depth(mut self, v: u32) -> Self { self.runtime.run_depth = v; self }
      pub fn build(self) -> AgentConfig {
          AgentConfig {
              system_prompt: self.system_prompt,
              model: self.model,
              budget: self.budget,
              skills: self.skills,
              runtime: self.runtime,
          }
      }
  }
  ```

- [ ] **Step 4: Run builder test to confirm it passes**

  ```bash
  cargo test -p agent-runtime-core builder_sets_fields_correctly 2>&1 | tail -5
  ```
  Expected: PASS.

- [ ] **Step 5: Fix all field access sites**

  Use the mapping table from issue 002 spec to update every file that accesses `AgentConfig` fields. Run `cargo build -p agent-runtime-core` after each file to catch errors incrementally. Key substitutions:

  | Old | New |
  |-----|-----|
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

  Also update `AgentConfig` construction sites in FFI bindings (`agent-runtime-py/src/lib.rs`, `agent-runtime-node/src/lib.rs`) and integration tests.

- [ ] **Step 6: Full build and test**

  ```bash
  cargo build --workspace 2>&1 | grep "^error" && cargo test --workspace 2>&1 | tail -10
  ```
  Expected: no build errors, all tests pass.

- [ ] **Step 7: Lint**

  ```bash
  cargo clippy --workspace -- -D warnings 2>&1 | grep "^error"
  ```
  Expected: no output.

- [ ] **Step 8: Commit**

  ```bash
  git add -p
  git commit -m "refactor(core): group AgentConfig into ModelConfig/SkillsConfig/RuntimeConfig + add builder

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 3: BudgetGuard pricing decoupling (Issue 003)

**Files:**
- Modify: `crates/agent-runtime-providers/src/types.rs`
- Modify: `crates/agent-runtime-core/src/budget.rs`
- Modify: `crates/agent-runtime-providers/src/anthropic.rs`
- Modify: `crates/agent-runtime-providers/src/openai.rs`
- Modify: `crates/agent-runtime-providers/src/deepseek.rs`
- Modify: `crates/agent-runtime-providers/src/openrouter.rs`

- [ ] **Step 1: Write failing tests in `budget.rs`**

  Add to `crates/agent-runtime-core/src/budget.rs`:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use crate::model::TokenUsage;

      #[test]
      fn budget_skips_cost_when_adapter_reports_none() {
          let mut guard = BudgetGuard::new(BudgetConfig {
              max_cost_usd: Some(1.0), ..Default::default()
          });
          guard.record_model_call(&TokenUsage {
              input_tokens: 1000, output_tokens: 500, cost_usd: None, ..Default::default()
          });
          assert_eq!(guard.usage().cost_usd, 0.0);
      }

      #[test]
      fn budget_accumulates_reported_cost() {
          let mut guard = BudgetGuard::new(BudgetConfig::default());
          guard.record_model_call(&TokenUsage { cost_usd: Some(0.01), ..Default::default() });
          guard.record_model_call(&TokenUsage { cost_usd: Some(0.02), ..Default::default() });
          assert!((guard.usage().cost_usd - 0.03).abs() < 1e-10);
      }
  }
  ```

  Add to `crates/agent-runtime-providers/src/types.rs`:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn model_pricing_calculate_sonnet() {
          let pricing = ModelPricing {
              input_per_million_usd: 3.0,
              output_per_million_usd: 15.0,
              cache_read_per_million_usd: None,
              cache_write_per_million_usd: None,
          };
          let usage = TokenUsage { input_tokens: 1_000_000, output_tokens: 1_000_000, ..Default::default() };
          assert!((pricing.calculate(&usage) - 18.0).abs() < 1e-10);
      }
  }
  ```

- [ ] **Step 2: Run tests to confirm they fail**

  ```bash
  cargo test -p agent-runtime-core budget 2>&1 | tail -5
  cargo test -p agent-runtime-providers model_pricing 2>&1 | tail -5
  ```
  Expected: FAIL — `cost_usd` field and `ModelPricing` type don't exist yet.

- [ ] **Step 3: Add `cost_usd` to `TokenUsage` and add `ModelPricing` type**

  In `crates/agent-runtime-providers/src/types.rs`:

  ```rust
  // Add to TokenUsage:
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub cost_usd: Option<f64>,

  // Add new type:
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct ModelPricing {
      pub input_per_million_usd: f64,
      pub output_per_million_usd: f64,
      pub cache_read_per_million_usd: Option<f64>,
      pub cache_write_per_million_usd: Option<f64>,
  }

  impl ModelPricing {
      pub fn calculate(&self, usage: &TokenUsage) -> f64 {
          usage.input_tokens as f64 * self.input_per_million_usd / 1_000_000.0
              + usage.output_tokens as f64 * self.output_per_million_usd / 1_000_000.0
              + usage.cache_read_tokens.unwrap_or(0) as f64
                  * self.cache_read_per_million_usd.unwrap_or(0.0) / 1_000_000.0
              + usage.cache_write_tokens.unwrap_or(0) as f64
                  * self.cache_write_per_million_usd.unwrap_or(0.0) / 1_000_000.0
      }
  }

  // Add to ModelCapabilities:
  pub pricing: Option<ModelPricing>,
  ```

- [ ] **Step 4: Update `BudgetGuard::record_model_call` and remove pricing constants**

  In `crates/agent-runtime-core/src/budget.rs`:

  Remove:
  ```rust
  const INPUT_COST_PER_MILLION: f64 = 3.0;
  const OUTPUT_COST_PER_MILLION: f64 = 15.0;
  ```

  Replace `record_model_call`:
  ```rust
  pub fn record_model_call(&mut self, usage: &TokenUsage) {
      self.usage.tokens_used += usage.input_tokens + usage.output_tokens;
      if let Some(cost) = usage.cost_usd {
          self.usage.cost_usd += cost;
      }
  }
  ```

- [ ] **Step 5: Add `pricing()` and cost fill to each adapter**

  In `crates/agent-runtime-providers/src/anthropic.rs`, add:
  ```rust
  fn pricing(&self) -> ModelPricing {
      match self.model.as_str() {
          m if m.contains("claude-opus-4") => ModelPricing {
              input_per_million_usd: 15.0, output_per_million_usd: 75.0,
              cache_read_per_million_usd: Some(1.5), cache_write_per_million_usd: Some(18.75),
          },
          m if m.contains("claude-sonnet-4") => ModelPricing {
              input_per_million_usd: 3.0, output_per_million_usd: 15.0,
              cache_read_per_million_usd: Some(0.3), cache_write_per_million_usd: Some(3.75),
          },
          m if m.contains("claude-haiku-4") => ModelPricing {
              input_per_million_usd: 0.8, output_per_million_usd: 4.0,
              cache_read_per_million_usd: Some(0.08), cache_write_per_million_usd: Some(1.0),
          },
          _ => ModelPricing {
              input_per_million_usd: 3.0, output_per_million_usd: 15.0,
              cache_read_per_million_usd: None, cache_write_per_million_usd: None,
          },
      }
  }
  ```

  In `complete()`, when building `ModelResponse`, fill `cost_usd`:
  ```rust
  let cost_usd = Some(self.pricing().calculate(&usage));
  // usage.cost_usd = cost_usd;  (set before returning)
  ```

  Repeat for `openai.rs`, `deepseek.rs`, `openrouter.rs` with their respective pricing (use 0.0 defaults if pricing is not yet known, but set `cost_usd = Some(0.0)` rather than `None` to distinguish "known zero" from "unknown").

- [ ] **Step 6: Run all tests**

  ```bash
  cargo test --workspace 2>&1 | tail -10
  ```
  Expected: all pass, including the new budget tests.

- [ ] **Step 7: Verify constant removal**

  ```bash
  grep -r "INPUT_COST_PER_MILLION\|OUTPUT_COST_PER_MILLION" crates/
  ```
  Expected: no output.

- [ ] **Step 8: Lint and commit**

  ```bash
  cargo clippy --workspace -- -D warnings 2>&1 | grep "^error"
  git add crates/
  git commit -m "feat(budget): decouple pricing — adapters report cost_usd, remove hardcoded Anthropic rates

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 4: ApprovalBus + AgentDelegate unification (Issue 004)

**Files:**
- Modify: `crates/agent-runtime-core/src/run/handle.rs`
- Modify: `crates/agent-runtime-core/src/run/mod.rs`
- Modify: `crates/agent-runtime-core/src/run/sub_agent.rs`
- Modify: `crates/agent-runtime-core/src/run/loop_.rs`
- Modify: `crates/agent-runtime-core/src/events.rs`

- [ ] **Step 1: Write failing unit tests for ApprovalBus in `run/handle.rs`**

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[tokio::test]
      async fn approval_bus_round_trip() {
          let bus = ApprovalBus::default();
          let run_id = RunId::new();
          let rx = bus.request(run_id).await;
          bus.respond(run_id, true).await.unwrap();
          assert_eq!(rx.await.unwrap(), true);
      }

      #[tokio::test]
      async fn approval_bus_unknown_run_id_returns_err() {
          let bus = ApprovalBus::default();
          let result = bus.respond(RunId::new(), true).await;
          assert!(result.is_err());
      }

      #[tokio::test]
      async fn approval_bus_cancel_clears_slot() {
          let bus = ApprovalBus::default();
          let run_id = RunId::new();
          let _rx = bus.request(run_id).await;
          bus.cancel(run_id).await;
          let result = bus.respond(run_id, true).await;
          assert!(result.is_err());
      }
  }
  ```

- [ ] **Step 2: Run tests to confirm they fail**

  ```bash
  cargo test -p agent-runtime-core approval_bus 2>&1 | tail -5
  ```
  Expected: FAIL — `ApprovalBus` type doesn't exist.

- [ ] **Step 3: Implement `ApprovalBus` in `run/handle.rs`**

  Replace `ApprovalSlot` type alias and `RunHandle::active_children` with:

  ```rust
  use std::collections::HashMap;
  use std::sync::Arc;
  use tokio::sync::{oneshot, Mutex};
  use super::config::RunId;

  #[derive(Clone, Default)]
  pub struct ApprovalBus {
      pending: Arc<Mutex<HashMap<RunId, oneshot::Sender<bool>>>>,
  }

  impl ApprovalBus {
      pub async fn request(&self, run_id: RunId) -> oneshot::Receiver<bool> {
          let (tx, rx) = oneshot::channel();
          self.pending.lock().await.insert(run_id, tx);
          rx
      }

      pub async fn respond(&self, run_id: RunId, approved: bool) -> Result<(), String> {
          match self.pending.lock().await.remove(&run_id) {
              Some(tx) => tx.send(approved)
                  .map_err(|_| format!("run {run_id} is no longer waiting for approval")),
              None => Err(format!("no pending approval for run {run_id}")),
          }
      }

      pub async fn cancel(&self, run_id: RunId) {
          self.pending.lock().await.remove(&run_id);
      }
  }

  pub struct RunHandle {
      pub run_id: RunId,
      task: tokio::task::JoinHandle<()>,
      pub(crate) approval_bus: ApprovalBus,
  }

  impl RunHandle {
      pub async fn respond_approval(&self, run_id: RunId, approved: bool) -> Result<(), String> {
          self.approval_bus.respond(run_id, approved).await
      }

      pub async fn wait(self) {
          let _ = self.task.await;
      }
  }
  ```

- [ ] **Step 4: Run ApprovalBus tests**

  ```bash
  cargo test -p agent-runtime-core approval_bus 2>&1 | tail -5
  ```
  Expected: all 3 tests PASS.

- [ ] **Step 5: Add `SubAgentEvent` to `events.rs`**

  In `crates/agent-runtime-core/src/events.rs`, add variant:
  ```rust
  SubAgentEvent {
      parent_run_id: RunId,
      child_run_id: RunId,
      event: Box<RuntimeEvent>,
  },
  ```

- [ ] **Step 6: Add `AgentRun::start_with_bus` and update `start`**

  In `run/mod.rs` / `run/config.rs`:
  ```rust
  impl AgentRun {
      pub fn start(
          input: String,
          config: AgentConfig,
          model: Arc<dyn ModelAdapter>,
          registry: ToolRegistry,
      ) -> (RunHandle, EventReceiver) {
          Self::start_with_bus(input, config, model, registry, ApprovalBus::default())
      }

      pub(crate) fn start_with_bus(
          input: String,
          config: AgentConfig,
          model: Arc<dyn ModelAdapter>,
          registry: ToolRegistry,
          bus: ApprovalBus,
      ) -> (RunHandle, EventReceiver) {
          let run_id = RunId::new();
          let (event_tx, event_rx) = mpsc::channel(256);
          let bus_clone = bus.clone();
          let task = tokio::spawn(async move {
              run_loop(run_id, config, input, model, registry, event_tx, bus_clone).await;
          });
          (RunHandle { run_id, task, approval_bus: bus }, event_rx)
      }
  }
  ```

- [ ] **Step 7: Update `run_loop` signature to accept `ApprovalBus`**

  Change `run_loop` in `run/loop_.rs` to accept `bus: ApprovalBus` instead of `pending_approval: ApprovalSlot` and `active_children: Arc<Mutex<HashMap<...>>>`.

  In the approval gate inside the tool execution loop:
  ```rust
  // Old: write to pending_approval slot
  // New:
  let rx = bus.request(run_id).await;
  emit(&tx, RuntimeEvent::ApprovalRequested { tool_call: tool_call.clone() }).await;
  let approved = rx.await.unwrap_or(false);
  bus.cancel(run_id).await;  // cleanup in case approval arrived but run is ending
  ```

- [ ] **Step 8: Update `sub_agent.rs` to share the bus**

  In `execute_agent_delegate()`, pass `bus.clone()` to `AgentRun::start_with_bus`. Forward sub-agent events as `RuntimeEvent::SubAgentEvent`.

- [ ] **Step 9: Remove `execute_sub_agent_request` and `__sub_agent_request` branch**

  In `run/loop_.rs` (was `run.rs:778`), remove:
  ```rust
  if value.get("__sub_agent_request").and_then(Value::as_bool) == Some(true) {
      value = execute_sub_agent_request(...).await;
  }
  ```

  Delete `execute_sub_agent_request` function from `run/sub_agent.rs`.

- [ ] **Step 10: Verify cleanup**

  ```bash
  grep -r "__sub_agent_request" crates/
  grep -r "execute_sub_agent_request" crates/
  grep -r "active_children" crates/
  ```
  Expected: no output for any of the three.

- [ ] **Step 11: Full test suite**

  ```bash
  cargo test --workspace 2>&1 | tail -10
  cargo clippy --workspace -- -D warnings 2>&1 | grep "^error"
  ```
  Expected: all pass, no errors.

- [ ] **Step 12: Commit**

  ```bash
  git add crates/
  git commit -m "feat(core): replace active_children with ApprovalBus, unify sub-agent via AgentDelegate

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 5: McpStdioClient concurrent dispatch (Issue 005)

**Files:**
- Modify: `crates/agent-runtime-core/src/tool/mcp.rs`

- [ ] **Step 1: Write failing test for concurrent ID routing**

  Add to `crates/agent-runtime-core/src/tool/mcp.rs`:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use std::collections::HashMap;
      use tokio::sync::{oneshot, Mutex};
      use std::sync::Arc;

      #[tokio::test]
      async fn pending_map_routes_responses_by_id() {
          let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<serde_json::Value>>>> =
              Default::default();
          let (tx1, rx1) = oneshot::channel::<serde_json::Value>();
          let (tx2, rx2) = oneshot::channel::<serde_json::Value>();
          pending.lock().await.insert(1, tx1);
          pending.lock().await.insert(2, tx2);

          // Deliver in reverse order
          let resp2 = serde_json::json!({"jsonrpc":"2.0","id":2,"result":"b"});
          let resp1 = serde_json::json!({"jsonrpc":"2.0","id":1,"result":"a"});
          for resp in [resp2, resp1] {
              if let Some(id) = resp["id"].as_u64() {
                  if let Some(tx) = pending.lock().await.remove(&id) {
                      let _ = tx.send(resp);
                  }
              }
          }

          assert_eq!(rx1.await.unwrap()["result"], "a");
          assert_eq!(rx2.await.unwrap()["result"], "b");
      }
  }
  ```

- [ ] **Step 2: Run test to confirm it fails**

  ```bash
  cargo test -p agent-runtime-core pending_map_routes 2>&1 | tail -5
  ```
  Expected: FAIL (test doesn't compile yet due to changed struct).

- [ ] **Step 3: Replace `McpStdioClient` implementation**

  In `crates/agent-runtime-core/src/tool/mcp.rs`, replace the entire struct and impl with the concurrent design from issue 005 spec. Key changes:
  - Remove `Mutex<McpStdioInner>` (and `McpStdioInner` struct)
  - Add `stdin: Arc<Mutex<ChildStdin>>`
  - Add `pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>`
  - Add `next_id: Arc<AtomicU64>`
  - Add `reader_abort: tokio::task::AbortHandle`
  - Add `child: Arc<Mutex<Child>>`
  - Spawn reader task in `connect()`
  - Implement `send_request()` with register-then-send pattern
  - Update `Drop` to abort reader and kill child

- [ ] **Step 4: Run the new test**

  ```bash
  cargo test -p agent-runtime-core pending_map_routes 2>&1 | tail -5
  ```
  Expected: PASS.

- [ ] **Step 5: Verify old struct is gone**

  ```bash
  grep -n "McpStdioInner" crates/agent-runtime-core/src/tool/mcp.rs
  ```
  Expected: no output.

- [ ] **Step 6: Full suite**

  ```bash
  cargo test --workspace 2>&1 | tail -10
  cargo clippy --workspace -- -D warnings 2>&1 | grep "^error"
  ```

- [ ] **Step 7: Commit**

  ```bash
  git add crates/agent-runtime-core/src/tool/mcp.rs
  git commit -m "perf(mcp): concurrent McpStdioClient via reader task + ID dispatch

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 6: tiktoken-rs integration (Issue 006)

**Files:**
- Modify: `crates/agent-runtime-core/Cargo.toml`
- Create: `crates/agent-runtime-core/src/tokenizer.rs`
- Modify: `crates/agent-runtime-core/src/lib.rs`
- Modify: `crates/agent-runtime-core/src/run/helpers.rs`

- [ ] **Step 1: Add dependency**

  In `crates/agent-runtime-core/Cargo.toml`, under `[dependencies]`:
  ```toml
  tiktoken-rs = "0.6"
  ```

- [ ] **Step 2: Write failing tests**

  Create `crates/agent-runtime-core/src/tokenizer.rs` with just the test module:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use serde_json::json;

      #[test]
      fn short_ascii_not_truncated() {
          let v = serde_json::Value::String("hello world".into());
          let result = truncate_to_tokens(v.clone(), 100);
          assert_eq!(result, v);
      }

      #[test]
      fn cjk_truncated_at_token_boundary() {
          let text = "你好".repeat(100);
          let v = serde_json::Value::String(text);
          let result = truncate_to_tokens(v, 10);
          let s = result.as_str().unwrap();
          assert!(s.contains("[output truncated]"));
          assert!(count_tokens(s) <= 10);
      }

      #[test]
      fn json_object_truncated_when_too_long() {
          let big = json!({"key": "a".repeat(10_000)});
          let result = truncate_to_tokens(big, 50);
          assert!(result.is_string());
          assert!(result.as_str().unwrap().contains("[output truncated]"));
      }
  }
  ```

- [ ] **Step 3: Run tests to confirm they fail**

  ```bash
  cargo test -p agent-runtime-core tokenizer 2>&1 | tail -5
  ```
  Expected: FAIL — functions not defined.

- [ ] **Step 4: Implement `tokenizer.rs`**

  ```rust
  //! Token counting using tiktoken cl100k_base BPE.

  use std::sync::OnceLock;
  use serde_json::Value;
  use tiktoken_rs::{cl100k_base, CoreBPE};

  static TOKENIZER: OnceLock<CoreBPE> = OnceLock::new();

  fn tokenizer() -> &'static CoreBPE {
      TOKENIZER.get_or_init(|| cl100k_base().expect("tiktoken init failed"))
  }

  pub fn count_tokens(text: &str) -> usize {
      tokenizer().encode_ordinary(text).len()
  }

  pub fn truncate_to_tokens(value: Value, max_tokens: usize) -> Value {
      const SUFFIX: &str = "\n[output truncated]";
      let limit = max_tokens.saturating_sub(count_tokens(SUFFIX));

      match value {
          Value::String(s) => {
              let tokens = tokenizer().encode_ordinary(&s);
              if tokens.len() <= max_tokens { return Value::String(s); }
              let truncated = tokenizer().decode(tokens[..limit].to_vec()).unwrap_or_default();
              Value::String(format!("{truncated}{SUFFIX}"))
          }
          other => {
              let serialized = serde_json::to_string(&other).unwrap_or_default();
              let tokens = tokenizer().encode_ordinary(&serialized);
              if tokens.len() <= max_tokens { return other; }
              let truncated = tokenizer().decode(tokens[..limit].to_vec()).unwrap_or_default();
              Value::String(format!("{truncated}{SUFFIX}"))
          }
      }
  }
  ```

- [ ] **Step 5: Register module and update `truncate_output`**

  Add to `crates/agent-runtime-core/src/lib.rs`:
  ```rust
  pub mod tokenizer;
  ```

  In `crates/agent-runtime-core/src/run/helpers.rs`, replace `truncate_output`:
  ```rust
  use crate::tokenizer::truncate_to_tokens;

  pub(crate) fn truncate_output(value: serde_json::Value, max_tokens: u64) -> serde_json::Value {
      truncate_to_tokens(value, max_tokens as usize)
  }
  ```

  Delete `truncate_str_utf8_safe` function (no remaining callers).

- [ ] **Step 6: Run tests**

  ```bash
  cargo test -p agent-runtime-core tokenizer 2>&1 | tail -5
  ```
  Expected: all 3 PASS.

- [ ] **Step 7: Verify byte estimation is gone**

  ```bash
  grep -n "max_bytes\|* 4" crates/agent-runtime-core/src/run/helpers.rs
  ```
  Expected: no output.

- [ ] **Step 8: Full suite and commit**

  ```bash
  cargo test --workspace 2>&1 | tail -5
  cargo clippy --workspace -- -D warnings 2>&1 | grep "^error"
  git add crates/agent-runtime-core/
  git commit -m "feat(core): replace byte-based truncation with tiktoken token counting

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 7: Shared reqwest::Client (Issue 007)

**Files:**
- Create: `crates/agent-runtime-providers/src/http.rs`
- Modify: `crates/agent-runtime-providers/src/lib.rs`
- Modify: `crates/agent-runtime-providers/src/anthropic.rs`
- Modify: `crates/agent-runtime-providers/src/openai.rs`
- Modify: `crates/agent-runtime-providers/src/deepseek.rs`
- Modify: `crates/agent-runtime-providers/src/openrouter.rs`

- [ ] **Step 1: Write failing test**

  Create `crates/agent-runtime-providers/src/http.rs`:
  ```rust
  //! Global shared reqwest::Client for all provider adapters.

  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn shared_client_is_singleton() {
          let a = shared_client() as *const _;
          let b = shared_client() as *const _;
          assert_eq!(a, b);
      }
  }
  ```

- [ ] **Step 2: Run test to confirm it fails**

  ```bash
  cargo test -p agent-runtime-providers shared_client 2>&1 | tail -5
  ```
  Expected: FAIL.

- [ ] **Step 3: Implement `http.rs`**

  ```rust
  //! Global shared reqwest::Client for all provider adapters.

  use std::sync::OnceLock;
  use std::time::Duration;

  static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

  pub fn shared_client() -> &'static reqwest::Client {
      CLIENT.get_or_init(|| {
          reqwest::Client::builder()
              .pool_max_idle_per_host(20)
              .timeout(Duration::from_secs(300))
              .build()
              .expect("failed to build shared reqwest::Client")
      })
  }
  ```

- [ ] **Step 4: Register module**

  In `crates/agent-runtime-providers/src/lib.rs`:
  ```rust
  pub(crate) mod http;
  ```

- [ ] **Step 5: Update all adapters**

  For each of `anthropic.rs`, `openai.rs`, `deepseek.rs`, `openrouter.rs`:
  1. Remove `client: reqwest::Client` field from the struct
  2. Remove `client: reqwest::Client::new()` from `from_config()`
  3. Change `self.client.post(...)` to `crate::http::shared_client().post(...)`

- [ ] **Step 6: Run test and verify**

  ```bash
  cargo test -p agent-runtime-providers shared_client 2>&1 | tail -5
  ```
  Expected: PASS.

  ```bash
  grep -r "self\.client\." crates/agent-runtime-providers/src/
  ```
  Expected: no output.

- [ ] **Step 7: Full suite and commit**

  ```bash
  cargo test --workspace 2>&1 | tail -5
  cargo clippy --workspace -- -D warnings 2>&1 | grep "^error"
  git add crates/agent-runtime-providers/
  git commit -m "perf(providers): share single reqwest::Client across all adapter instances

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 8: Python SDK ergonomics (Issue 008)

**Files:**
- Create: `python/agent_runtime/exceptions.py`
- Modify: `python/agent_runtime/__init__.py`
- Create: `python/tests/test_run_sync.py`
- Modify: `crates/agent-runtime-py/src/lib.rs`

- [ ] **Step 1: Write failing tests**

  Create `python/tests/test_run_sync.py`:
  ```python
  import pytest
  from agent_runtime import AgentError, BudgetExceededError
  from agent_runtime.exceptions import from_code


  def test_from_code_budget():
      exc = from_code("budget exceeded", "budget_exceeded")
      assert isinstance(exc, BudgetExceededError)
      assert exc.code == "budget_exceeded"


  def test_from_code_unknown():
      exc = from_code("something went wrong", None)
      assert isinstance(exc, AgentError)
      assert exc.code is None


  def test_agent_error_repr():
      exc = AgentError("test message", code="test_code")
      assert "test_code" in repr(exc)
      assert "test message" in repr(exc)


  def test_agent_error_is_exception():
      exc = AgentError("msg")
      assert isinstance(exc, Exception)
  ```

- [ ] **Step 2: Run tests to confirm they fail**

  ```bash
  cd /Users/emile/Develop/orchest && python -m pytest python/tests/test_run_sync.py -v 2>&1 | tail -10
  ```
  Expected: FAIL — import errors.

- [ ] **Step 3: Create `exceptions.py`**

  Create `python/agent_runtime/exceptions.py`:
  ```python
  class AgentError(Exception):
      def __init__(self, message: str, code: str | None = None) -> None:
          super().__init__(message)
          self.code = code

      def __repr__(self) -> str:
          return f"{type(self).__name__}(message={str(self)!r}, code={self.code!r})"


  class BudgetExceededError(AgentError):
      pass


  class ApprovalDeniedError(AgentError):
      pass


  class ModelError(AgentError):
      pass


  class ToolError(AgentError):
      pass


  class SkillError(AgentError):
      pass


  def from_code(message: str, code: str | None) -> AgentError:
      mapping = {
          "budget_exceeded": BudgetExceededError,
          "max_steps_reached": BudgetExceededError,
          "approval_denied": ApprovalDeniedError,
          "model_error": ModelError,
          "tool_error": ToolError,
          "skill_error": SkillError,
      }
      cls = mapping.get(code or "", AgentError)
      return cls(message, code)
  ```

- [ ] **Step 4: Update `__init__.py` with exports and `run_sync`**

  Add to `python/agent_runtime/__init__.py`:
  ```python
  import asyncio
  from .exceptions import (
      AgentError,
      BudgetExceededError,
      ApprovalDeniedError,
      ModelError as AgentModelError,
      ToolError as AgentToolError,
      SkillError,
  )

  # Add to Agent class:
  def run_sync(self, prompt: str) -> list:
      """Synchronous convenience — runs agent and collects all events."""
      return asyncio.run(self._collect_events(prompt))

  async def _collect_events(self, prompt: str) -> list:
      return [event async for event in self.run(prompt)]
  ```

  Add to `__all__`:
  ```python
  __all__ = [
      "Agent",
      "AgentError",
      "BudgetExceededError",
      "ApprovalDeniedError",
      "SkillError",
  ]
  ```

- [ ] **Step 5: Run Python tests**

  ```bash
  python -m pytest python/tests/test_run_sync.py -v 2>&1 | tail -10
  ```
  Expected: all 4 tests PASS.

- [ ] **Step 6: Smoke test `run_sync` import**

  ```bash
  python -c "from agent_runtime import Agent; print(hasattr(Agent, 'run_sync'))"
  ```
  Expected: `True`.

- [ ] **Step 7: Commit**

  ```bash
  git add python/
  git commit -m "feat(python): add run_sync(), structured exception types with code field

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 9: Final verification

- [ ] **Step 1: Full workspace test**

  ```bash
  cargo test --workspace 2>&1 | tail -15
  ```
  Expected: all tests pass.

- [ ] **Step 2: Clippy clean**

  ```bash
  cargo clippy --workspace -- -D warnings
  ```
  Expected: no warnings or errors.

- [ ] **Step 3: File size check**

  ```bash
  wc -l crates/agent-runtime-core/src/run/*.rs | sort -rn | head -12
  ```
  Expected: max file ≤ 700 lines.

- [ ] **Step 4: Dead code check**

  ```bash
  grep -rn "__sub_agent_request\|McpStdioInner\|active_children\|INPUT_COST_PER_MILLION" crates/
  ```
  Expected: no output.

- [ ] **Step 5: Python smoke**

  ```bash
  python -c "from agent_runtime import Agent, BudgetExceededError, AgentError; print('ok')"
  ```
  Expected: `ok`.

- [ ] **Step 6: Final commit if anything was missed**

  ```bash
  git status
  # If clean: done. If dirty: stage and commit remaining changes.
  ```
