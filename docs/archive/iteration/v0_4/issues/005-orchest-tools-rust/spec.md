# 005 · `orchest-tools` Rust crate（含 WebFetch / WebSearch）

## 背景

定下"统一一个基础扩展包 + 一对三语言 binding"的方向后，需要先建立这个统一 crate 的 Rust 实现。本 issue 只做 Rust 层，binding 留给 issue 006。

第一批入驻 `orchest-tools` 的能力是 web 类，因为：
- 语义清晰、状态轻
- 跨语言通用
- 没有 v0.4 内争议（shell 设计上还有讨论空间，留 v0.5）

## 目标

新建 `crates/orchest-tools/` Rust crate，提供 `WebFetchTool` 与 `WebSearchTool`，且具备良好的内部模块边界，便于后续追加更多基础扩展 tool。

## 验收标准

### crate 结构

- [ ] `crates/orchest-tools/Cargo.toml`：workspace member，依赖 `agent-runtime-core`（path）、`reqwest`（rustls-tls）、`serde`、`async-trait`
- [ ] 模块结构：
  ```
  src/
    lib.rs           # 公开 register 函数、tool 类型
    web/
      mod.rs
      fetch.rs       # WebFetchTool
      search.rs      # WebSearchTool + SearchBackend trait
      duckduckgo.rs  # 默认 SearchBackend 实现
  ```
- [ ] `lib.rs` 提供 `register_all(registry: &mut ToolRegistry)` 一键注册函数，以及单 tool 构造器（`WebFetchTool::new()` / `WebSearchTool::with_backend(...)`）
- [ ] 预留模块位置 `src/shell/`（空 mod 文件 + `TODO: v0.5` 注释），表明后续扩展位置

### WebFetchTool

- [ ] Input schema：`{ url: string, method?: "GET"|"POST"|..., headers?: object, body?: string, timeout_ms?: number }`
- [ ] 默认超时 30s，上限 5min（超过上限的 `timeout_ms` 输入返回输入校验错误）
- [ ] 返回结构化 JSON：`{ status: number, headers: object, body: string }`
- [ ] 大 body 截断（默认 1MB，构造时可配置），截断时响应中加入 `truncated: true`
- [ ] 网络错误 / DNS 失败 / 超时返回 `ToolOutput::Error { message }`，message 含可识别字符串（`network`/`dns`/`timeout`），不 panic
- [ ] tool metadata 的 `requires_approval: true`、`side_effect: true`（network egress）
- [ ] **以上各条均有对应单元测试**（见下文测试小节）

### WebSearchTool

- [ ] Input schema：`{ query: string, max_results?: number }`，默认 max_results=5，上限 20（超出返回输入校验错误）
- [ ] 内部使用 `SearchBackend` trait：`async fn search(&self, query: &str, max_results: usize) -> Result<Vec<SearchHit>, SearchError>`
- [ ] 默认实现 `DuckDuckGoBackend`：scrape `https://duckduckgo.com/html/?q=...`，解析为 `SearchHit { title, url, snippet }`
- [ ] backend 可由用户通过 `WebSearchTool::with_backend(Box<dyn SearchBackend>)` 替换
- [ ] HTML 解析失败 / 网络失败均返回结构化错误（不 panic），错误 message 包含 `search_backend` / `network` 字段以便区分
- [ ] **以上各条均有对应单元测试**

### 测试

- [ ] 单元测试：mock `SearchBackend` 验证 `WebSearchTool` 的输入校验与错误传播
- [ ] 单元测试：使用 `wiremock` 或 in-process HTTP server 验证 `WebFetchTool` 的超时、截断、错误路径
- [ ] **不**在 CI 里跑真实 DuckDuckGo 请求（避免依赖外网 + 反爬）；DuckDuckGo backend 的实际抓取测试用 `#[ignore]` 标记，需要本地手动跑
- [ ] `cargo test -p orchest-tools` 全绿
- [ ] `cargo clippy -p orchest-tools -- -D warnings` 全绿

### 文档

- [ ] crate 根 `README.md`：crate 定位（基础扩展统一包）、当前包含的 tool、未来追加规则
- [ ] 每个 tool 在 doc comment 中说明 input schema、错误码、approval 行为

## 注意

- 这个 crate 是 binding 投入的"入口"——所有发布到 PyPI / npm 的扩展能力都会从这里走。设计 API 时要假设 binding 作者会反复使用，命名要稳定
- 不要在 `orchest-tools` 内引入异步运行时（如 `tokio::main`）；这是库，调度交给调用方
- HTTP client 强制 TLS 验证；不提供"关闭证书验证"的选项（必要时由 SerpAPI backend 类似的扩展实现自己处理）
- 注意区分 tool 的 `requires_approval`：网络 egress 默认 true。用户可在注册时调整。文档要说明。
