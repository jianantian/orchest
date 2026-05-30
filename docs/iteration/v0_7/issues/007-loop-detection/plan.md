# 007 · Loop Detection — 实施计划

## 前置条件

- 002（Hook Framework）已合入：`src/hook/mod.rs` 存在，`Hook` trait 定义完整，`ToolHookContext.tool_input` 可访问，`ModelHookContext.messages` 可修改
- `cargo test --workspace` 全绿

---

## 步骤

### 步骤 1：新建 `src/hook/loop_detection.rs`

新建文件 `crates/agent-runtime-core/src/hook/loop_detection.rs`：

#### 1.1 配置类型

```rust
#[derive(Debug, Clone)]
pub struct LoopDetectionConfig {
    pub window_size: usize,     // 默认 10
    pub warn_threshold: usize,  // 默认 3
    pub stop_threshold: usize,  // 默认 5
}

impl Default for LoopDetectionConfig {
    fn default() -> Self {
        LoopDetectionConfig { window_size: 10, warn_threshold: 3, stop_threshold: 5 }
    }
}
```

#### 1.2 内部状态（Mutex 保护）

```rust
use std::collections::VecDeque;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq)]
struct ToolCallPattern {
    tool_name: String,
    input_hash: u64,
    input_value: serde_json::Value,  // 用于哈希碰撞时的全量比较
}

struct LoopState {
    window: VecDeque<ToolCallPattern>,
    pending_warning: Option<String>,
}
```

#### 1.3 稳定哈希（不受 JSON key 顺序影响）

```rust
fn stable_hash(value: &serde_json::Value) -> u64 {
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;

    fn normalize(v: &serde_json::Value) -> String {
        match v {
            serde_json::Value::Object(map) => {
                let mut pairs: Vec<_> = map.iter().collect();
                pairs.sort_by_key(|(k, _)| k.as_str());
                let inner: Vec<String> = pairs.iter()
                    .map(|(k, v)| format!("{}:{}", k, normalize(v)))
                    .collect();
                format!("{{{}}}", inner.join(","))
            }
            other => other.to_string(),
        }
    }

    let normalized = normalize(value);
    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    hasher.finish()
}
```

#### 1.4 LoopDetectionHook struct

```rust
pub struct LoopDetectionHook {
    config: LoopDetectionConfig,
    state: Mutex<LoopState>,
}

impl LoopDetectionHook {
    pub fn new(config: LoopDetectionConfig) -> Self {
        LoopDetectionHook {
            config,
            state: Mutex::new(LoopState {
                window: VecDeque::new(),
                pending_warning: None,
            }),
        }
    }
}

impl Default for LoopDetectionHook {
    fn default() -> Self { Self::new(LoopDetectionConfig::default()) }
}

impl From<LoopDetectionConfig> for LoopDetectionHook {
    fn from(config: LoopDetectionConfig) -> Self { Self::new(config) }
}
```

#### 1.5 Hook trait impl（两阶段设计）

```rust
use async_trait::async_trait;
use crate::hook::{Hook, HookAction, ModelHookAction, ToolHookContext, ModelHookContext};
use crate::model::{ContentBlock, Message, Role};

#[async_trait]
impl Hook for LoopDetectionHook {
    async fn after_tool(
        &self, ctx: &mut ToolHookContext, _output: &crate::tool::ToolOutput
    ) -> HookAction {
        let hash = stable_hash(&ctx.tool_input);
        let pattern = ToolCallPattern {
            tool_name: ctx.tool_name.clone(),
            input_hash: hash,
            input_value: ctx.tool_input.clone(),
        };

        let mut state = self.state.lock().unwrap();

        // 维护滑动窗口
        state.window.push_back(pattern.clone());
        if state.window.len() > self.config.window_size {
            state.window.pop_front();
        }

        // 统计窗口内相同 pattern 出现次数（hash 匹配后全量比较）
        let count = state.window.iter().filter(|p| {
            p.tool_name == pattern.tool_name
                && p.input_hash == pattern.input_hash
                && p.input_value == pattern.input_value  // 消除哈希碰撞
        }).count();

        if count >= self.config.stop_threshold {
            return HookAction::Abort(format!(
                "loop detected: '{}' called {} times with same input in last {} calls",
                pattern.tool_name, count, self.config.window_size
            ));
        }

        if count >= self.config.warn_threshold {
            state.pending_warning = Some(format!(
                "[Warning] You have called the tool '{}' with similar arguments {} times \
                in the last {} calls. This suggests a loop. Please try a different approach \
                or use a different tool.",
                pattern.tool_name, count, self.config.window_size
            ));
        }

        HookAction::Continue
    }

    async fn before_model(&self, ctx: &mut ModelHookContext) -> ModelHookAction {
        let warning = {
            let mut state = self.state.lock().unwrap();
            state.pending_warning.take()
        };

        if let Some(msg) = warning {
            // 当前 Role 枚举只有 System/User/Assistant/Tool（无 Developer）
            // 使用 Role::User 注入警告——位置在 messages 末尾，模型最容易注意到。
            // 已知限制：User 角色的警告消息与真实用户消息在对话结构上无法区分。
            // 后续可在 agent-runtime-model 添加 Role::Developer 并修改此处。
            ctx.messages.push(Message {
                role: Role::User,
                content: vec![ContentBlock::Text(msg)],
            });
        }

        ModelHookAction::Continue
    }
}
```

---

### 步骤 2：在 `src/hook/mod.rs` 中注册子模块

文件 `crates/agent-runtime-core/src/hook/mod.rs` 末尾追加：

```rust
pub mod loop_detection;
```

---

### 步骤 3：修改 `run/config.rs` — 添加 `with_loop_detection()` builder

文件 `crates/agent-runtime-core/src/run/config.rs`，在 `impl AgentConfig` block 中新增：

```rust
pub fn with_loop_detection(self) -> Self {
    self.with_hook(std::sync::Arc::new(
        crate::hook::loop_detection::LoopDetectionHook::default()
    ))
}

pub fn with_loop_detection_config(
    self, config: crate::hook::loop_detection::LoopDetectionConfig
) -> Self {
    self.with_hook(std::sync::Arc::new(
        crate::hook::loop_detection::LoopDetectionHook::from(config)
    ))
}
```

---

### 步骤 4：编写测试

在 `crates/agent-runtime-core/src/run/tests.rs` 末尾新增：

**测试 A**：相同 tool + 相同 input 调用 `warn_threshold` 次 → `before_model` 前 messages 末尾有警告消息，run 继续

**测试 B**：相同 tool + 相同 input 调用 `stop_threshold` 次 → `after_tool` 返回 `HookAction::Abort`，run 以 `RunFailed` 终止

**测试 C**：相同 tool + 不同 input（只是 key 顺序不同）→ stable_hash 相同，全量 Value 比较不同 → 不触发检测

**测试 D**：不同 tool → 不触发检测

**测试 E**：窗口滑动——调用 `window_size` 次后旧 pattern 滑出，计数重置

---

## 验证

```bash
cargo test -p agent-runtime-core
cargo clippy -p agent-runtime-core -- -D warnings

# 确认 LoopDetectionHook 文件存在
ls crates/agent-runtime-core/src/hook/loop_detection.rs

# 确认 with_loop_detection builder
grep -n "with_loop_detection" crates/agent-runtime-core/src/run/config.rs

# 确认 stable_hash 不受 key 顺序影响
# （由测试 C 验证）
```

---

## 关键决策

- **两阶段设计（`after_tool` 记录 + `before_model` 注入）**：`ToolHookContext` 不含 `messages`，无法在 `after_tool` 直接注入消息。`pending_warning` flag 在 `before_model` 时才注入到 `ModelHookContext.messages`。
- **`Mutex<LoopState>` 线程安全**：`Hook` 要求 `Send + Sync`，内部可变状态必须用 `Mutex`；每次调用只在临界区内 push/count，锁持有时间极短
- **hash + 全量比较双重验证**：u64 哈希碰撞概率低但非零；hash 匹配后做 `serde_json::Value` 全量比较消除误报，避免误终止用户 run
- **警告用 `Role::User` 而非 `Role::Developer`**：`agent-runtime-model` 的 `Role` 枚举当前无 `Developer` variant（只有 System/User/Assistant/Tool）。使用 `Role::User` 是当前唯一可行选项；已知限制是警告消息与真实用户消息无法区分。后续扩展路径：在 model crate 添加 `Role::Developer` variant，更新此处注入逻辑，无破坏性变更
