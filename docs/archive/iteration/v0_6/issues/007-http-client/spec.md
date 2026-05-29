# 007 · reqwest::Client 全局共享

## 背景

每个 provider adapter 在 `from_config()` 时独立创建 `reqwest::Client::new()`：

```rust
// anthropic.rs（当前）
Ok(Self {
    api_key,
    api_url,
    model: config.model,
    max_tokens: config.max_tokens,
    client: reqwest::Client::new(),   // ← 每个 adapter 实例独立的 client
})
```

问题：
- 每个 `reqwest::Client` 实例持有自己的连接池，并发 agent 场景下连接池碎片化
- 多个 agent 并发运行时（常见于批量处理），连接无法被复用，TCP 连接数膨胀
- `reqwest::Client::new()` 本身有一定初始化成本

`reqwest::Client` 是线程安全的（`Clone` 是浅拷贝，内部 Arc），设计上就是给所有 request 共享一个实例用的。

## 变更

### 新模块：共享 client

文件：`crates/agent-runtime-providers/src/http.rs`

```rust
//! Global shared reqwest::Client for all provider adapters.

use std::sync::OnceLock;
use std::time::Duration;

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// Returns the process-wide shared HTTP client.
/// Initialized once on first call with sensible defaults.
pub fn shared_client() -> &'static reqwest::Client {
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .pool_max_idle_per_host(20)
            .timeout(Duration::from_secs(300))   // 5 分钟：覆盖长流式响应
            .build()
            .expect("failed to build shared reqwest::Client")
    })
}
```

### 各 adapter 改用共享 client

文件：`crates/agent-runtime-providers/src/anthropic.rs`（以及 `openai.rs`, `deepseek.rs`, `openrouter.rs`）

1. 删除 `struct AnthropicAdapter` 中的 `client: reqwest::Client` 字段
2. `from_config()` 中不再调用 `reqwest::Client::new()`
3. 发送请求时改为 `crate::http::shared_client().post(...)`

```rust
// 修改前
self.client.post(&self.api_url)

// 修改后
crate::http::shared_client().post(&self.api_url)
```

### lib.rs 注册模块

文件：`crates/agent-runtime-providers/src/lib.rs`

```rust
pub(crate) mod http;
```

## 验收标准

### 结构

- [ ] `crates/agent-runtime-providers/src/http.rs` 文件存在，包含 `shared_client()` 函数
- [ ] `AnthropicAdapter`、`OpenAiAdapter`、`DeepSeekAdapter`、`OpenRouterAdapter` 的 struct 定义中均不含 `client: reqwest::Client` 字段
- [ ] 四个 adapter 的 `from_config()` 均不调用 `reqwest::Client::new()`

### 功能

- [ ] 所有 adapter 的 HTTP 请求通过 `shared_client()` 发出（grep `self.client.` 应无匹配）
- [ ] 现有 adapter 测试（mock 或 real API）继续通过

### 单元测试（`http.rs` 内嵌）

- [ ] 多次调用 `shared_client()` 返回同一实例（指针相等）：
  ```rust
  #[test]
  fn shared_client_is_singleton() {
      let a = shared_client() as *const _;
      let b = shared_client() as *const _;
      assert_eq!(a, b);
  }
  ```

### 正确性

- [ ] `cargo build --workspace` 通过
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
