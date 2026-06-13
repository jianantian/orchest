# 006 · Event Backpressure 与 Context Window 防御

## 背景

外部架构评审识别的两个 P0 级问题：event subscriber 阻塞 run loop，以及 model call 前无 context window 检查。

## 6a. Primary subscriber 阻塞 run loop（Critical）

**文件**：`crates/agent-runtime-core/src/run/actor.rs:1182-1198`

```rust
async fn emit(subs: &[mpsc::Sender<RuntimeEvent>], event: RuntimeEvent) {
    if let Some(primary) = subs.first() {
        let _ = primary.send(event.clone()).await;  // 阻塞等待！
    }
    // secondary: try_send (静默丢弃)
}
```

`primary.send().await` 在 channel 满时阻塞。慢消费者（如慢速日志写入、网络延迟的 dashboard）会直接卡死整个 agent run loop。

**修复**：primary subscriber 也改为有超时的 send：

```rust
async fn emit(subs: &[mpsc::Sender<RuntimeEvent>], event: RuntimeEvent) {
    if let Some(primary) = subs.first() {
        match tokio::time::timeout(
            Duration::from_millis(500),
            primary.send(event.clone()),
        ).await {
            Ok(Ok(())) => {},
            Ok(Err(_)) => {},  // channel closed
            Err(_) => {
                // 超时：primary 消费太慢，丢弃原事件并 best-effort 通知
                if primary.try_send(RuntimeEvent::EventsDropped {
                    subscriber_id: 0,
                    count: 1,
                }).is_err() {
                    tracing::warn!("primary event subscriber timed out and EventsDropped notification channel is full");
                }
            }
        }
    }
    // secondary subscribers 保持 try_send 不变
    // ...
}
```

`EventsDropped` 当前不带 `run_depth` 字段；如果后续运行时事件模型统一添加 run depth，应先更新 Rust enum 和 type stubs，再调整这里。

**设计决策**：超时时长 500ms 是折中——足够让正常消费者消化，不至于让 run loop 停顿太久。可提取为常量 `EVENT_SEND_TIMEOUT`。

## 6b. `EventsDropped` 通知本身可被丢弃（Critical）

**文件**：同 `actor.rs:1182-1198`

Secondary subscriber 的 `EventsDropped` 通知使用 `try_send`，如果 channel 已满则通知本身也被丢弃。消费者完全无感知。

**修复**：6a 的方案中，`EventsDropped` 通知 best-effort 发送到 primary channel（而非发给丢事件的那个 subscriber）。如果 primary channel 已满，记录 `tracing::warn!`，确保不是完全静默丢弃。

如果 primary 也满了（即 6a 的超时也触发了），`EventsDropped` 仍然可能无法入队。这是可接受的——此时整个系统处于严重 backpressure，后续迭代可考虑迁移到 `tokio::sync::broadcast` 或 dropped-count accumulator。hotfix 阶段的目标是防止 run loop 无限阻塞，并提供 best-effort 可观测信号，不是保证零丢失。

## 6c. Model call 前无 context window 检查（Important）

**文件**：`crates/agent-runtime-core/src/run/actor.rs` 约 line 480

Compaction 在 model response 后触发（line 620）。如果对话已积累到超出 context window，model call 会失败，provider 返回不可预期的错误（HTTP 400/500、truncation、或 vendor-specific error code）。

**修复**：在 model call 前估算 token count 并检查：

```rust
// run_one_step 中，model call 之前
let estimated_tokens = state.tokenizer.estimate_messages(&messages)
    + state.tokenizer.estimate_tool_defs(&state.tool_defs);
if let Some(context_window_size) = state.config.model.spec.context_window_size {
    if estimated_tokens as u64 > context_window_size {
        emit(&subs, RuntimeEvent::RunFailed {
            error: format!(
                "context window exceeded: estimated {estimated_tokens} tokens, limit {context_window_size}"
            ),
        }).await;
        return false;
    }
}
```

`RunFailed` 的 Rust enum 变体不带 `run_depth`；语言绑定层会按现有转换逻辑补充公开 wire metadata。

**前提条件与降级策略**：

1. 复用现有 `AgentConfig.model.spec.context_window_size: Option<u64>`。默认 `None`（不检查），由调用方根据 provider 的 context window 设置
2. Token 估算：`tokenizer.rs` 中已有 `estimate_tokens()` 用于 compaction 决策。复用同一接口估算 messages。tool_defs 的 token count 用 JSON 序列化后字符数 / 4 粗估（tiktoken 对英文的近似比例）
3. 如果 `context_window_size` 为 `None`，跳过检查——行为与当前一致，不引入 breaking change

**不做 proactive compaction**——只检查并报错。Proactive compaction 是功能迭代，不是 hotfix。

## 验收标准

- [ ] `emit` 函数的 primary subscriber send 有超时（不再无限阻塞）
- [ ] 超时常量 `EVENT_SEND_TIMEOUT` 已提取
- [ ] Primary 超时后 best-effort 发送 `EventsDropped`；通知 channel 满时记录 `tracing::warn!`
- [ ] Model call 前有 token count 估算，超限时返回 `RunFailed` 并包含 token count 信息
- [ ] 检查逻辑复用 `AgentConfig.model.spec.context_window_size`，不新增重复的 context window 配置字段
- [ ] `cargo test -p agent-runtime-core` 全绿
