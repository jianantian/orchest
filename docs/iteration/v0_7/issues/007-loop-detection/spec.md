# 007 · Loop Detection

## 背景

Agent 有时陷入循环——反复调用同一个工具、用相同参数、得到相同结果，无限消耗 budget 但不产生进展。当前 runtime 没有检测机制，只能靠 budget 上限或 step 上限被动终止。

## 目标

检测 agent 循环调用同一工具模式，两级防御：先警告（注入提示消息），后强制终止。

## 范围

### 实现为 Hook

Loop detection 作为 `Hook` trait 的具体实现（002 提供的框架），不修改 core loop 逻辑：

```rust
pub struct LoopDetectionHook {
    window_size: usize,     // 滑动窗口大小（默认 10）
    warn_threshold: usize,  // 重复次数达到此值时注入警告（默认 3）
    stop_threshold: usize,  // 重复次数达到此值时终止（默认 5）
}
```

### 检测逻辑

在 `after_tool` hook 中记录工具调用模式：

```rust
struct ToolCallPattern {
    tool_name: String,
    input_hash: u64,  // 对 tool input Value 做稳定哈希
}
```

使用滑动窗口（最近 N 次 tool call）检测重复，采用**两阶段设计**（`after_tool` 记录 + `before_model` 注入）：

1. 维护一个 `VecDeque<ToolCallPattern>`，容量为 `window_size`
2. 每次 `after_tool` 时 push 新 pattern，统计窗口内相同 pattern 出现次数——**hash 匹配时做 `Value` 全量比较**（消除 u64 哈希碰撞导致的误终止）
3. 达到 `warn_threshold` → 设置内部 `pending_warning: Option<String>` flag（不在此处直接注入——`ToolHookContext` 不持有 `messages`）
4. 达到 `stop_threshold` → 返回 `HookAction::Abort("loop detected")`
5. `before_model` 时检查 `pending_warning`：若有待注入警告，通过 `ModelHookContext.messages` append developer 角色消息，然后清除 flag

`LoopDetectionHook` 使用 `Mutex<LoopState>` 保证内部状态线程安全。

### 警告消息注入

当 `before_model` 检测到 `pending_warning` 时，在 `ModelHookContext.messages` 末尾 append：

警告作为 **developer 角色消息**（不是 system message——system message 在对话开头，模型对位置敏感；末尾的 developer message 是最近的上下文，模型更容易注意到）：

```
[Warning] You have called the tool '{tool_name}' with similar arguments {count} times 
in the last {window_size} calls. This suggests a loop. Please try a different approach 
or use a different tool.
```

### 配置

```rust
impl AgentConfig {
    pub fn with_loop_detection(self) -> Self {
        self.with_hook(Arc::new(LoopDetectionHook::default()))
    }

    pub fn with_loop_detection_config(self, config: LoopDetectionConfig) -> Self {
        self.with_hook(Arc::new(LoopDetectionHook::from(config)))
    }
}
```

### 稳定哈希

Tool input 的 `Value` 需要稳定哈希（不受 JSON key 顺序影响）：

- 对 `Value` 做规范化排序（key 按字典序）
- 然后计算 hash

可以用 `serde_json::to_string()` 加排序，或对 `Value` 做递归规范化。不需要密码学级别哈希，`std::hash::DefaultHasher` 足够。

## 需要修改的文件

| 文件 | 变更 |
|------|------|
| 新增 `hook/loop_detection.rs` | `LoopDetectionHook` 实现 |
| `run/config.rs` | `with_loop_detection()` builder 方法 |

## 不在范围内

- 跨 run 的循环检测（每次 run 独立）
- 基于输出内容的循环检测（只看 tool name + input）
- 基于 LLM 的循环检测（那是 Guardrail 的事）

## 依赖

- 002（Hook Framework）：作为 `Hook` trait 实现

## 验收标准

- [ ] `LoopDetectionHook` 实现 `Hook` trait
- [ ] 滑动窗口正确维护最近 N 次 tool call pattern
- [ ] 相同 tool + input 达到 warn_threshold 时注入警告消息
- [ ] 相同 tool + input 达到 stop_threshold 时返回 `HookAction::Abort`
- [ ] Tool input 哈希不受 JSON key 顺序影响，hash 匹配时做 Value 全量比较消除碰撞
- [ ] 警告消息作为 developer 角色 append 到 messages 末尾
- [ ] `AgentConfig::with_loop_detection()` builder 可用
- [ ] 测试：模拟循环调用 → 先警告后终止
- [ ] 测试：不同 input 的同一工具不触发检测
- [ ] 测试：窗口滑动正确（旧 pattern 被移出后不再计数）
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
