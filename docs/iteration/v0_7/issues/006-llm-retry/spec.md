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
    Exponential { base: Duration, max: Duration, jitter: bool },
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy {
            max_retries: 3,
            backoff: BackoffStrategy::Exponential {
                base: Duration::from_secs(1),
                max: Duration::from_secs(30),
                jitter: true,
            },
        }
    }
}
```

**Jitter**：`jitter: true`（默认）时在计算出的退避时间上加 ±25% 随机偏移，防止多 agent 同时命中 429 后同步重试（thundering herd）。标准做法，AWS SDK / Stripe SDK / gRPC 均使用。

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

**每次重试是完整的 hook 周期**：before_model → model.complete() → after_model。Hook 看到每一次尝试（包括失败的），可以记录 metrics、修改 messages 等。这是最不令人意外的行为——重试对 hook 链完全透明。

- 如果 `before_model` hook 返回 `Abort` → 不执行 model call，不重试
- 如果 `after_model` hook 返回 `Abort` → 不重试，直接终止 run
- `after_model` hook 的副作用（如 metrics 记录）在每次重试时都会执行——这是预期行为

重试逻辑在 loop 层实现（不作为 Hook），因为重试需要重新调用 `model.complete()`，这不是 hook 能做的事。

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
- [ ] 429 错误自动重试，使用指数退避 + jitter
- [ ] 5xx 错误自动重试，使用固定间隔
- [ ] jitter 默认启用，退避时间有随机偏移
- [ ] 每次重试完整走 before_model → model → after_model hook 链
- [ ] 400/401/403 错误不重试
- [ ] `Retry-After` header 被尊重（如果 model error 携带）
- [ ] 超过 max_retries 后返回最后一个错误
- [ ] 每次重试发出 `RuntimeEvent::ModelRetry` 事件
- [ ] 测试：mock model 返回 429 → 重试后成功
- [ ] 测试：mock model 返回 429 × (max_retries+1) → 最终失败
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
