# 006 · LLM Retry

## 背景

当前 runtime 的模型调用（`run/loop_.rs` 中 `model.complete()`）失败时直接返回错误，无重试机制。生产环境中 rate limit（429）和 server error（5xx）是常见的瞬时错误，应该自动重试。

## 目标

提供可配置的 `RetryPolicy`，对可重试的模型调用错误自动重试。

## 范围

### RetryPolicy 类型

```rust
pub struct RetryPolicy {
    pub max_retries: u32,
    pub backoff: BackoffStrategy,
}

pub enum BackoffStrategy {
    Fixed(Duration),
    Exponential { base: Duration, max: Duration },
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy {
            max_retries: 3,
            backoff: BackoffStrategy::Exponential {
                base: Duration::from_secs(1),
                max: Duration::from_secs(30),
            },
        }
    }
}
```

### 错误分类

| 错误类型 | 行为 | 退避策略 |
|----------|------|---------|
| rate_limit（HTTP 429） | 重试 | 指数退避（优先使用 `Retry-After` header） |
| server_error（HTTP 5xx） | 重试 | 固定间隔 |
| timeout | 重试 | 指数退避 |
| auth_error（401/403） | 不重试 | — |
| invalid_request（400） | 不重试 | — |
| 其他 | 不重试 | — |

需要扩展 `RuntimeError`（或 model error 类型）以携带 HTTP status code 和 `Retry-After` 信息。

### AgentConfig 集成

```rust
pub struct AgentConfig {
    // ... 现有字段 ...
    pub retry_policy: Option<RetryPolicy>,  // 新增，None = 不重试
}
```

### 实现方式

在 `run/loop_.rs` 的 model call 位置包装重试逻辑：

```rust
let response = retry_with_policy(&config.retry_policy, || async {
    model.complete(&messages, &options).await
}).await?;
```

`retry_with_policy` 是一个通用的 async retry helper，不绑定 model 调用。

每次重试发出 `RuntimeEvent`：

```rust
RuntimeEvent::ModelRetry {
    attempt: u32,
    error: String,
    next_delay: Duration,
}
```

### 与 Hook 框架的关系

重试逻辑在 `before_model` 和 `after_model` hook 之间执行。如果 hook 返回 `Abort`，不重试。

重试也可以作为 Hook 实现（`after_model` 拦截错误并重试），但考虑到重试是 loop 层面的关注点（需要重新调用 `model.complete()`），直接在 loop 中实现更自然。

## 需要修改的文件

| 文件 | 变更 |
|------|------|
| `run/config.rs` | `AgentConfig` 新增 `retry_policy` 字段 |
| `run/loop_.rs` | model call 包装重试逻辑 |
| `events.rs` | 新增 `RuntimeEvent::ModelRetry` |
| 新增 `retry.rs` | `RetryPolicy` / `BackoffStrategy` / `retry_with_policy()` |

## 不在范围内

- Tool 调用重试（tool 自身的 retry 由 tool 实现负责）
- MCP server 重连重试（已有机制）
- 重试预算扣减（重试的 token 消耗正常计入 budget）

## 依赖

- 002（Hook Framework）：重试逻辑需要与 hook 调用链协调

## 验收标准

- [ ] `RetryPolicy` 和 `BackoffStrategy` 类型定义完整
- [ ] `AgentConfig` 支持 `retry_policy` 配置
- [ ] 429 错误自动重试，使用指数退避
- [ ] 5xx 错误自动重试，使用固定间隔
- [ ] 400/401/403 错误不重试
- [ ] `Retry-After` header 被尊重（如果 model error 携带）
- [ ] 超过 max_retries 后返回最后一个错误
- [ ] 每次重试发出 `RuntimeEvent::ModelRetry` 事件
- [ ] 测试：mock model 返回 429 → 重试后成功
- [ ] 测试：mock model 返回 429 × (max_retries+1) → 最终失败
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
