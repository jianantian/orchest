# 007 实现路线

## 步骤

1. **创建 `http.rs` 模块**
   - 新建 `crates/agent-runtime-providers/src/http.rs`
   - 实现 `shared_client() -> &'static reqwest::Client`（见 spec）
   - 用 `OnceLock` + `reqwest::Client::builder()` 配置：`pool_max_idle_per_host(20)`、`timeout(Duration::from_secs(300))`
   - 在文件底部加 `#[cfg(test)]` singleton 测试

2. **注册模块**
   - 在 `crates/agent-runtime-providers/src/lib.rs` 添加 `pub(crate) mod http;`
   - 运行 `cargo build -p agent-runtime-providers` 确认编译通过

3. **更新 Anthropic adapter**
   - 在 `anthropic.rs` 中：
     1. 删除 `AnthropicAdapter` struct 的 `client: reqwest::Client` 字段
     2. 删除 `from_config()` 中的 `client: reqwest::Client::new()`
     3. 把所有 `self.client.post(...)` 改为 `crate::http::shared_client().post(...)`
   - 运行 `cargo build -p agent-runtime-providers` 确认编译通过

4. **对其余三个 adapter 做相同改动**
   - `openai.rs`、`deepseek.rs`、`openrouter.rs` 各做一遍步骤 3 的三个改动
   - 每改完一个就运行 `cargo build` 确认

5. **验收**
   - `grep -rn "self\.client\." crates/agent-runtime-providers/src/` — 无输出
   - `grep -rn "reqwest::Client::new()" crates/agent-runtime-providers/src/` — 无输出
   - `cargo test -p agent-runtime-providers shared_client` — PASS
   - `cargo test --workspace` 全绿
   - `cargo clippy --workspace -- -D warnings` 全绿

## 要读的现有代码

- `crates/agent-runtime-providers/src/anthropic.rs` — `AnthropicAdapter` struct 定义和 `from_config()` 中的 client 构建，以及所有调用 `self.client` 的地方
- `crates/agent-runtime-providers/src/openai.rs`、`deepseek.rs`、`openrouter.rs` — 同上，确认四个 adapter 都有 `client` 字段

## 关键决策

- **timeout 设置 300 秒**：LLM 流式响应可能持续很长时间（大量 token 的长任务），300 秒能覆盖绝大多数场景。如果有特殊需求可以通过 `AgentConfig` 的 timeout 在工具层面控制，不需要 HTTP client 层面的精细调整
- **`pool_max_idle_per_host(20)`**：合理的连接池大小，覆盖并发 20 个 agent 同时请求同一 provider 的场景。连接池数量不影响并发上限（超出池大小会新建连接），只影响复用效率
- **不需要处理 `OnceLock` 初始化失败**：`reqwest::Client::builder().build()` 在正常环境下不会失败，`expect()` 是合理的 panic 点。如果真的失败（系统 TLS 配置损坏等），程序启动就应该 panic
- **测试隔离**：singleton 测试只验证同一进程内多次调用返回同一指针，不会影响其他测试（client 是无状态的，不同测试共享同一 client 没有副作用）
