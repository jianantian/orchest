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
- 测试:带 `session_id` 的快照不重挂 store 直接 resume → `SessionStoreMissing`;重挂后成功;无 `session_id` 的快照(从未开启持久化)resume 不受影响
- `with_session_store` rustdoc 补"resume 前必须重挂"说明
- 重跑 `cargo test -p briefing-desk-demo`,输出贴回 #198 或关闭它的 PR

## 验收标准(对齐 GitHub #198)

- [ ] resume 路径对"曾持久化但 store 缺失"响亮报错,`ConfigError` 新变体落地
- [ ] demo `resume()` 适配并重跑,输出贴回 issue/PR
- [ ] `with_session_store`/`resume` 文档补充
- [ ] `docs/review/v0_10_demo_validation.md` 更新(Triage #4 行)
