# 002 · AgentConfig 分组重构 + Builder

## 背景

当前 `AgentConfig` 有 14 个扁平字段，混合了 model 配置、预算、skill 配置、运行时 feature flag 等不同关注点：

```rust
pub struct AgentConfig {
    pub system_prompt: String,
    pub model: ModelSpec,
    pub request_options: RequestOptions,
    pub budget: BudgetConfig,
    pub max_steps: u32,
    pub allowed_skills: Option<Vec<String>>,
    pub allowed_tools: Option<Vec<String>>,
    pub mcp_servers: Vec<McpServerConfig>,
    pub tool_search_enabled: bool,
    pub compaction_threshold: Option<f32>,
    pub compaction_recent_messages: usize,
    pub webhook_enabled: bool,
    pub code_execution_enabled: bool,
    pub skills_dir: Option<String>,
    pub run_depth: u32,
}
```

随着功能增长，字段会继续累积（例如 v0.7 计划的 tool middleware、provider 注册表等），扁平结构将失控。

## 目标

1. 把 `AgentConfig` 字段按职责分为 `ModelConfig`、`SkillsConfig`、`RuntimeConfig` 三个嵌套结构
2. 提供链式 builder API（`AgentConfig::builder(model)`）
3. 保持 JSON 序列化兼容性（旧字段路径在 JSON 层面发生变化，这是可接受的 breaking change，内部 SDK）

## 新类型定义

```rust
// run/config.rs

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
    /// 上下文使用率阈值（0.0–1.0），超过时触发 compaction
    pub threshold: f32,
    /// Compaction 时保留的最近消息数
    pub recent_messages: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self { threshold: 0.8, recent_messages: 10 }
    }
}
```

## Builder API

```rust
// run/config.rs

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

    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into(); self
    }
    pub fn max_cost_usd(mut self, usd: f64) -> Self {
        self.budget.max_cost_usd = Some(usd); self
    }
    pub fn max_tokens(mut self, n: u64) -> Self {
        self.budget.max_tokens = Some(n); self
    }
    pub fn max_tool_calls(mut self, n: u32) -> Self {
        self.budget.max_tool_calls = Some(n); self
    }
    pub fn skills_dir(mut self, dir: impl Into<String>) -> Self {
        self.skills.dir = Some(dir.into()); self
    }
    pub fn allowed_skills(mut self, skills: Vec<String>) -> Self {
        self.skills.allowed = Some(skills); self
    }
    pub fn max_steps(mut self, n: u32) -> Self {
        self.runtime.max_steps = n; self
    }
    pub fn mcp_server(mut self, config: McpServerConfig) -> Self {
        self.runtime.mcp_servers.push(config); self
    }
    pub fn enable_tool_search(mut self) -> Self {
        self.runtime.tool_search_enabled = true; self
    }
    pub fn enable_code_execution(mut self) -> Self {
        self.runtime.code_execution_enabled = true; self
    }
    pub fn enable_compaction(mut self, config: CompactionConfig) -> Self {
        self.runtime.compaction = Some(config); self
    }
    pub fn run_depth(mut self, depth: u32) -> Self {
        self.runtime.run_depth = depth; self
    }
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

## 受影响的调用点

以下文件需要更新字段访问路径：

| 原字段 | 新路径 |
|--------|--------|
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

主要影响文件（001 拆分后）：
- `run/loop_.rs`
- `run/sub_agent.rs`
- `run/skills.rs`
- `run/webhook.rs`
- `run/compaction.rs`
- `crates/agent-runtime-py/src/lib.rs`
- `crates/agent-runtime-node/src/lib.rs`
- `crates/agent-runtime-core/tests/e2e_validation.rs`
- `crates/agent-runtime-core/tests/v03_runtime.rs`

## 验收标准

### 类型定义

- [ ] `AgentConfig` 包含 `system_prompt`, `model: ModelConfig`, `budget: BudgetConfig`, `skills: SkillsConfig`, `runtime: RuntimeConfig` 五个字段
- [ ] `ModelConfig`, `SkillsConfig`, `RuntimeConfig`, `CompactionConfig` 四个嵌套类型定义存在
- [ ] `AgentConfigBuilder` 定义存在，`AgentConfig::builder(model)` 工厂方法可用
- [ ] Builder 支持 `system_prompt`, `max_cost_usd`, `max_tokens`, `max_tool_calls`, `skills_dir`, `allowed_skills`, `max_steps`, `mcp_server`, `enable_tool_search`, `enable_code_execution`, `enable_compaction`, `run_depth` 共 12 个方法链

### 内嵌 `#[cfg(test)]`

- [ ] `run/config.rs` 中有测试验证 builder 能正确构建 `AgentConfig`：
  ```rust
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
  ```
- [ ] `serde_json::to_string(&config)` 和 `serde_json::from_str::<AgentConfig>(...)` round-trip 测试通过

### 运行时状态

- [ ] `RunState.schema_version` 更新为 `"0.6"`

### 正确性

- [ ] `cargo build --workspace` 通过，无编译错误
- [ ] `cargo test --workspace` 全部通过，无 regression
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
