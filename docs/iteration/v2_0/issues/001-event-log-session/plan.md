# 001 implementation plan

## Files to read

- `crates/orchest/src/session/{store.rs,snapshot.rs,sqlite.rs,persistence_hook.rs,mod.rs}`
- `crates/orchest/src/run/{actor.rs,config.rs,mod.rs,handle.rs}`（`AgentRunState` 里的 messages / step / budget 与快照字段的对应关系）
- `crates/orchest/src/events.rs`（`RuntimeEvent` 与新日志事件的边界）
- `crates/orchest/src/model/mod.rs` 与 `crates/orchest-protocol/src/{types.rs,response.rs}`（`Message` / `ContentBlock` 形状）
- `crates/orchest/src/budget.rs`（`BudgetUsage`）

## Files to change

- 新增 `crates/orchest/src/session/log.rs`：`SessionEvent`、`SessionEventKind`、序列校验、投影 `SessionProjection`
- `crates/orchest/src/session/store.rs`：`SessionStore` 新 trait、内存实现
- `crates/orchest/src/session/sqlite.rs`：表结构改为 `(session_id, seq)` 事件表 + 元数据表
- `crates/orchest/src/session/mod.rs`：导出新类型，移除 `snapshot` / `persistence_hook`
- `crates/orchest/src/session/snapshot.rs`、`persistence_hook.rs`：删除
- `crates/orchest/src/run/actor.rs`：消息/step/预算的写入点改为追加日志事件
- `crates/orchest/src/run/config.rs`：去掉 `session_store` 之外的快照相关字段与 `#[serde(skip)]` 假设；`AgentConfig` 不再需要序列化整体
- `crates/orchest/src/run/mod.rs`：`resume` / `resume_with_input` 改为从日志恢复
- `crates/orchest/src/bindings.rs`、`crates/orchest-py/src/lib.rs`、`crates/orchest-node/src/lib.rs`：同步公开面
- `examples/`、`python/`、`js/`：跟随公开面变更

## Steps

1. 定义 `SessionEvent` / `SessionEventKind` 与 seq 连续性校验；写单元测试（乱序、跳号、重复 seq 被拒）。
2. 实现投影：日志 → 消息历史 / step / 预算；用既有 role 约定测试工具调用与结果的配对。
3. 改造 `SessionStore` trait 与两个后端；`append` 只保证顺序接受，`flush` 才是耐久屏障（语义写进 trait 文档）。
4. 把 actor 的状态写入点改成追加事件：run 起点、每个 step 的起止、消息与工具调用/结果、压缩、handoff、终止。
5. 状态读取改为"从日志投影"：确认 `AgentRunState` 不再持有与日志重复的消息副本（若需要缓存，只能是从日志派生的只读缓存，不能成为写入源）。
6. `resume` / `resume_with_input` 改为读日志 → 投影 → 重建 run；配置由宿主重新提供（hooks / retry / handoffs / store），运行时不从日志恢复这些。
7. 删除快照与终止时自动保存钩子；确认 `ConfigError::SessionStoreMissing` 一类错误按新模型重写。
8. 同步 bindings 与示例；跑 `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --check`、`scripts/check-ts-event-variants.sh`。
9. 在 `CHANGELOG.md` 记录破坏性变更。
