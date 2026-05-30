# 006 · LLM Retry — 实施计划

## 前置条件

- 002（Hook Framework）已合入，`AgentConfig.hooks` 字段已存在
- `cargo test --workspace` 全绿

---

## 步骤

### 步骤 1：扩展 `UpstreamErrorDetail` — 新增 `retry_after_secs`

文件 `crates/agent-runtime-model/src/error.rs`，`UpstreamErrorDetail` struct 新增字段：

```rust
pub struct UpstreamErrorDetail {
    pub code: Option<String>,
    pub message: Option<String>,
    pub body: Option<Value>,
    pub retry_after_secs: Option<u64>,  // 新增：来自 Retry-After header
}
```

此字段由各 provider adapter 在解析 429 响应头时填充（`Retry-After: <seconds>`）。

---

### 步骤 2：新建 `src/retry.rs` — RetryPolicy 和核心 retry 逻辑

新建文件 `crates/agent-runtime-core/src/retry.rs`：

```rust
use std::time::Duration;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RetryPolicy {
    pub max_retries: u32,
    /// 429 rate-limit 使用指数退避（默认）
    pub rate_limit_backoff: BackoffStrategy,
    /// 5xx server error 使用固定间隔（默认 2 秒）
    pub server_error_backoff: BackoffStrategy,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum BackoffStrategy {
    Fixed(Duration),
    Exponential {
        base: Duration,
        max: Duration,
        jitter: bool,
    },
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy {
            max_retries: 3,
            rate_limit_backoff: BackoffStrategy::Exponential {
                base: Duration::from_secs(1),
                max: Duration::from_secs(30),
                jitter: true,
            },
            server_error_backoff: BackoffStrategy::Fixed(Duration::from_secs(2)),
        }
    }
}

/// 错误分类——决定使用哪种退避策略
pub enum RetryClass {
    RateLimit,   // 429
    ServerError, // 5xx
}

/// 返回 None 表示不可重试
pub fn classify_status(status: Option<u16>) -> Option<RetryClass> {
    match status {
        Some(429) => Some(RetryClass::RateLimit),
        Some(500) | Some(502) | Some(503) | Some(504) => Some(RetryClass::ServerError),
        _ => None,
    }
}

impl RetryPolicy {
    /// 计算第 attempt 次（0-indexed）的退避时间
    pub fn delay(&self, attempt: u32, class: &RetryClass, retry_after_secs: Option<u64>) -> Duration {
        // Retry-After header 优先
        if let Some(secs) = retry_after_secs {
            return Duration::from_secs(secs);
        }
        let backoff = match class {
            RetryClass::RateLimit => &self.rate_limit_backoff,
            RetryClass::ServerError => &self.server_error_backoff,
        };
        compute_backoff(backoff, attempt)
    }
}

fn compute_backoff(strategy: &BackoffStrategy, attempt: u32) -> Duration {
    match strategy {
        BackoffStrategy::Fixed(d) => *d,
        BackoffStrategy::Exponential { base, max, jitter } => {
            let factor = 2u64.saturating_pow(attempt);
            let delay = base.saturating_mul(factor as u32).min(*max);
            if *jitter {
                // ±25% 随机偏移：使用 attempt + thread_id 作为熵源
                // （比 Instant::now() 更稳定——同一纳秒内多次调用仍有差异）
                let range = delay.as_millis() / 4;
                if range > 0 {
                    use std::hash::{Hash, Hasher};
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    (attempt, std::thread::current().id()).hash(&mut h);
                    let offset = (h.finish() % (range * 2)) as i64 - range as i64;
                    let millis = delay.as_millis() as i64 + offset;
                    Duration::from_millis(millis.max(0) as u64)
                } else {
                    delay
                }
            } else {
                delay
            }
        }
    }
}
```

在 `src/lib.rs` 中注册：`pub mod retry;`

---

### 步骤 3：在 `events.rs` 中添加 `ModelRetry` variant

文件 `crates/agent-runtime-core/src/events.rs`，在 `HookPanicked` 之前（或 `RunAborted` 之前）追加：

```rust
ModelRetry {
    attempt: u32,
    max_retries: u32,
    error: String,
    next_delay_ms: u64,
},
```

---

### 步骤 4：修改 `run/config.rs` — 添加 retry_policy 字段

文件 `crates/agent-runtime-core/src/run/config.rs`，`AgentConfig` struct（L38–46）新增字段：

```rust
#[serde(default)]
pub retry_policy: Option<crate::retry::RetryPolicy>,
```

在 `impl AgentConfig` block 新增 builder 方法：

```rust
pub fn with_retry_policy(mut self, policy: crate::retry::RetryPolicy) -> Self {
    self.retry_policy = Some(policy);
    self
}
```

---

### 步骤 5：修改 `run/loop_.rs` — 包装 model.complete() 调用

文件 `crates/agent-runtime-core/src/run/loop_.rs`：

**4.1** 将现有的 model call（L203–235）提取为 retry loop。当前代码：

```rust
emit(&tx, RuntimeEvent::ModelCallStarted { step }).await;
let (stream_tx, mut stream_rx) = mpsc::channel::<ModelStreamChunk>(64);
...
let response = model.complete(&messages, &tool_defs, &config.model.options, Some(stream_tx)).await;
let _ = forward_task.await;
let response: ModelResponse = match response {
    Ok(r) => r,
    Err(e) => {
        emit(&tx, RuntimeEvent::RunFailed { error: e.to_string() }).await;
        return;
    }
};
```

**4.2** 替换为 retry loop（保留 hook 调用点——每次重试走完整 before_model → model → after_model）：

```rust
emit(&tx, RuntimeEvent::ModelCallStarted { step }).await;

let response: ModelResponse = {
    let max_attempts = config.retry_policy.as_ref()
        .map(|p| p.max_retries + 1)
        .unwrap_or(1);
    let mut last_err = None;
    let mut response_opt = None;

    'retry: for attempt in 0..max_attempts {
        // --- before_model hook（每次重试都走） ---
        // （002 插入的 before_model hook 调用在此处保留）

        let (stream_tx, mut stream_rx) = mpsc::channel::<ModelStreamChunk>(64);
        let event_tx_clone = tx.clone();
        let forward_task = tokio::spawn(async move {
            while let Some(chunk) = stream_rx.recv().await {
                let _ = event_tx_clone.send(RuntimeEvent::ModelStreamChunk { delta: chunk }).await;
            }
        });

        let result = model.complete(&messages, &tool_defs, &config.model.options, Some(stream_tx)).await;
        let _ = forward_task.await;

        match result {
            Ok(r) => {
                response_opt = Some(r);
                break 'retry;
            }
            Err(ref e) if attempt < max_attempts - 1 => {
                let retry_class = match crate::retry::classify_status(e.status) {
                    Some(class) => class,
                    None => {
                        emit(&tx, RuntimeEvent::RunFailed { error: e.to_string() }).await;
                        return;
                    }
                };
                let policy = config.retry_policy.as_ref().unwrap();
                let retry_after = e.upstream.as_ref()
                    .and_then(|u| u.retry_after_secs);
                let delay = policy.delay(attempt, &retry_class, retry_after);
                emit(&tx, RuntimeEvent::ModelRetry {
                    attempt: attempt + 1,
                    max_retries: policy.max_retries,
                    error: e.to_string(),
                    next_delay_ms: delay.as_millis() as u64,
                }).await;
                tokio::time::sleep(delay).await;

                // --- after_model hook 不在失败路径调用（无 response 可传）---
            }
            Err(e) => {
                last_err = Some(e);
                break 'retry;
            }
        }
    }

    match response_opt {
        Some(r) => r,
        None => {
            let err = last_err.map(|e| e.to_string()).unwrap_or_default();
            emit(&tx, RuntimeEvent::RunFailed { error: err }).await;
            return;
        }
    }
};
// （002 插入的 after_model hook 调用在此处保留）
```

---

### 步骤 6：编写测试

在 `crates/agent-runtime-core/src/run/tests.rs` 末尾新增：

**测试 A**：mock model 第一次返回 429（status=429），第二次成功 → 总共一次 retry，发出 `ModelRetry` 事件

**测试 B**：mock model 连续返回 429 超过 max_retries → `RunFailed` 事件，包含最后一次错误信息

**测试 C**：mock model 返回 400（invalid_request）→ 不重试，直接 `RunFailed`

**测试 D**：`retry_policy: None`（默认）→ 失败时不重试，直接 `RunFailed`

---

## 验证

```bash
cargo test -p agent-runtime-core
cargo clippy -p agent-runtime-core -- -D warnings

# 确认 ModelRetry 事件存在
grep -rn "ModelRetry" crates/ --include="*.rs"

# 确认 retry_policy 字段在 AgentConfig
grep -n "retry_policy" crates/agent-runtime-core/src/run/config.rs

# 确认 retry.rs 模块存在
ls crates/agent-runtime-core/src/retry.rs
```

---

## 关键决策

- **每次重试走完整 before_model → model → after_model hook 链**：重试对 hook 完全透明，符合最小惊讶原则；hook metrics 记录在每次尝试时触发
- **`after_model` hook 不在 model 失败时调用**：失败时没有 response，无法构建 `ModelHookContext`；只在成功时触发
- **429 vs 5xx 退避策略独立配置**：`RetryPolicy` 拆为 `rate_limit_backoff`（默认指数退避）和 `server_error_backoff`（默认固定 2 秒），与 spec 要求一致；用户可单独调整
- **`Retry-After` header 优先于 backoff 计算**：步骤 1 在 `UpstreamErrorDetail` 中新增 `retry_after_secs: Option<u64>`，由 provider adapter 在解析 429 响应头时填充，`policy.delay()` 优先使用该值
- **jitter 使用 `(attempt, thread_id)` 作为熵源**：比 `Instant::now()` 可靠——同一纳秒内多次调用仍能产生差异；不依赖 `rand` crate，精度足够防 thundering herd
