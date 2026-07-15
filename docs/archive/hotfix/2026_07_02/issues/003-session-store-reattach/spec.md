# Issue 003:反序列化 `AgentConfig` 丢 session store 时响亮失败

GitHub: [#198](https://github.com/jianantian/orchest/issues/198) · release-blocker · 依赖 issue 002(共用 resume 面,顺序改动避免冲突)

## 现状

`AgentConfig.session_store` 是 `#[serde(skip)]`(`crates/orchest/src/run/config.rs:65-66`,`Arc<dyn SessionStore>` 本就不可序列化)。`SessionStore::load()` 反序列化出的 snapshot 里 `active_config.session_store` 恒为 `None`——不重新 `.with_session_store(store, id)` 就 resume,持久化静默停止,再次 resume 时更新已丢,无任何报错。

关键事实:**`session_id: Option<String>` 没有 skip,会随快照序列化存活**(`config.rs:67`)。这给了一个廉价且准确的"曾配置过持久化"标记。

## 方向(提案)

采用 issue 内三选项中的"响亮报错"路线,机制:

```rust
// resume / resume_with_input 入口处(002 落地后两处共用同一检查):
// snapshot.active_config.session_id 有值(说明持久化曾开启)
// 且 session_store 为 None(store 没被重新挂上)
// → 返回错误,不静默继续
```

配套签名变化:`resume`/`resume_with_input` 返回值改为
`Result<(RunHandle, EventReceiver), ConfigError>`,新增
`ConfigError::SessionStoreMissing { session_id: String }`,错误消息直接告诉调用方补 `.with_session_store(store, id)`。pre-1.0 破坏可接受,且这正是 v1.0 冻结前该做的收口;`AgentConfigBuilder::build()` 已确立 `Result<_, ConfigError>` 先例。

不采用"resume 显式收 store 参数"选项的理由:非持久化 run 的 resume(纯内存中断恢复)会被迫传一个无意义参数或 `Option`,把少数场景的成本摊给所有调用方;标记检查只对"确实配置过持久化"的快照收严。

## 落地与测试

- `examples/demo/briefing-desk/src/app.rs` 的 `resume()` 适配 `Result` 返回(其重挂 store 的既有做法就是正确用法,保留)
- **`examples/rust/resilience/session_persist_resume.rs:130` 也直接调
  `AgentRun::resume(...)`,同样需要适配 `Result` 返回**——`docs/guide/quickstart.md:187`
  按文件名点名引用这个 example,断了会导致文档指向一份编译不过的代码
- 测试:带 `session_id` 的快照不重挂 store 直接 resume → `SessionStoreMissing`;重挂后成功;无 `session_id` 的快照(从未开启持久化)resume 不受影响
- 文档:`with_session_store` rustdoc 补"resume 前必须重挂"说明;`SessionSnapshot`
  自身的 doc comment(`crates/orchest/src/session/snapshot.rs:13-15`)已经写了
  "`hooks`/`retry_policy`/`handoffs` 反序列化后为空,调用前需重新注册"——**漏了
  `session_store`**(它也是 `#[serde(skip)]`,正是本 issue 要修的对象),本次一并补上,
  这是调用方读快照类型时第一眼看到的地方,比 `with_session_store` 侧的说明更醒目
- 重跑 `cargo test -p briefing-desk-demo`,输出贴回 #198 或关闭它的 PR

## 验收标准(对齐 GitHub #198)

- [x] resume 路径对"曾持久化但 store 缺失"响亮报错,`ConfigError` 新变体落地
- [x] demo `resume()` 与 `examples/rust/resilience/session_persist_resume.rs` 均适配 `Result` 返回,`cargo test --workspace`(含 examples 编译)过
- [x] demo 测试重跑,输出贴回 issue/PR
- [x] `with_session_store` rustdoc 与 `SessionSnapshot` 自身 doc comment 都补充 `session_store` 重挂说明
- [x] `docs/review/v0_10_demo_validation.md` 更新(Triage #4 行)

## 实现记录

- `ConfigError::SessionStoreMissing { session_id: String }` 新增(`crates/orchest/src/run/config.rs`),错误消息直接给出待补的 `.with_session_store(store, "<id>")` 调用
- `resume`/`resume_with_input` 签名改为 `Result<(RunHandle, EventReceiver), ConfigError>`;共用私有 `check_session_store_attached(&AgentConfig)`(`crates/orchest/src/run/mod.rs`):`session_id: Some` 且 `session_store: None` 时返回 `Err`,否则(含 `session_id: None` 的从未持久化快照)放行
- 调用方适配:`examples/demo/briefing-desk/src/app.rs` 的 `resume()`(`?` 传播进 `DemoError`)、`examples/rust/resilience/session_persist_resume.rs:130`(`?`)、`crates/orchest/tests/v08_integration.rs` 三处、`crates/orchest/src/run/tests.rs` 既有两处(均补 `.expect(..)`,场景本身不触发新检查)
- 新测试(`crates/orchest/src/run/tests.rs`):`resume_fails_loudly_when_persisted_session_store_not_reattached`、`resume_with_input_fails_loudly_when_persisted_session_store_not_reattached`(均断言 `ConfigError::SessionStoreMissing`)、`resume_without_session_id_is_unaffected_by_session_store_check`(从未持久化的快照不受影响)
- `with_session_store` rustdoc 与 `SessionSnapshot::active_config` 字段 doc comment 均补充 `session_store` 重挂 + 响亮报错的说明
- `cargo test --workspace --features orchest/sqlite-session`:全部通过(orchest lib 251 个测试,较 002 前新增 3 个)
- `cargo clippy --workspace --all-targets -- -D warnings`:无新增 finding(与 001/002 记录的两处既有基线 finding 一致)
- `cargo fmt --check`:通过
- `bash scripts/lint-check.sh`:通过(exit 0)
- `cargo test -p briefing-desk-demo`:20 个测试全绿
