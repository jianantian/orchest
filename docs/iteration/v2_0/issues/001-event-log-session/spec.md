# 001 · 会话事件日志与派生投影

GitHub issue: #331

## Background

Orchest 的执行态持久化是全量快照形态：`SessionSnapshot`（`crates/orchest/src/session/snapshot.rs`）持有 messages / step / budget / active_config，`SessionStore` 只有 `save` / `load` / `delete` / `list`（`crates/orchest/src/session/store.rs:22-29`），自动保存钩子只在 run 正常结束或出错时落盘（`crates/orchest/src/session/persistence_hook.rs`）。

三个后果：崩溃即丢执行态；恢复后 `AgentConfig` 的 hooks / retry / handoffs / store 因 `#[serde(skip)]` 全部为空（`crates/orchest/src/run/config.rs:134-141`），行为与恢复前不一致；快照无法表达"某个工具已派发但结果未知"。

业界一致采用 append-only 事件日志 + 派生投影（调研 §1）：dsh 的 `deriveMessages()`、Codex 的 rollout decoder、LangGraph 的 `channel_versions`、openhuman 的 stream/collection 分工、jcode 的 journal 重放。

## Goal

把执行态改成**唯一真相是事件日志**：运行过程中只追加事件，消息历史、step、预算、待决请求全部由日志投影得出；`SessionStore` 改为日志语义（append / read / flush / delete / list）。这是 002–008 的地基。

## Acceptance Criteria

- [ ] 新增 `SessionEvent`：单调 `seq`、事件种类、时间戳、载荷；`seq` 连续且不回退。
- [ ] 事件种类覆盖运行语义：`RunStarted`、`StepStarted`、`StepEnded`、`UserMessage`、`AssistantMessage`、`ToolCall`、`ToolResult`、`Compacted`、`Handoff`、`RunEnded`、`RunFailed`、`RunAborted`。003/004/007 需要的账本与请求事件在对应 issue 里追加。
- [ ] 提供投影函数：日志 → `{ messages, step, budget_used, pending }`；同一日志任何时刻投影结果一致，且日志是唯一状态源（删除 `AgentRunState` 之外的重复消息副本）。
- [ ] `SessionStore` 改为：`append(session_id, events)`（接受并按 seq 排序）、`read(session_id, from_seq)`、`flush(session_id)`（耐久屏障）、`delete`、`list`。
- [ ] `InMemorySessionStore` 与 `SqliteSessionStore` 都实现新 trait；SQLite 后端能按 `session_id + seq` 增量读取。
- [ ] 会话元数据（schema 版本、session id、创建时间、agent 名）与事件日志分开存放，读取时不与事件混流。
- [ ] `SessionSnapshot` 与 `SessionPersistenceHook` 被移除（硬切换，无兼容层）；`AgentRun::resume` / `resume_with_input` 改为从日志恢复。
- [ ] 投影出的消息历史遵循既有的 role 约定：`ToolUse` 在 assistant 消息、`ToolResult` 在紧随的 user 消息，且 id 成对。
- [ ] 绑定层（`orchest-py` / `orchest-node`）的公开面随 trait 变更同步更新，`scripts/check-ts-event-variants.sh` 仍通过。
- [ ] `CHANGELOG.md` 记录破坏性变更（ADR-0003：主版本号）。

## Blocked by

None — 第一个 issue。

## Notes

- 事件载荷用 `serde_json::Value` 还是强类型：倾向强类型枚举 + `#[serde(tag = "type")]` 标签化，避免再出现一次"事件面靠字符串约定"的漂移；代价是新增变体属公开面变更（`#[non_exhaustive]` 可控）。
- 日志与 `RuntimeEvent` 的关系要写清：事件日志是**执行态**，运行时事件流是**观察流**；两者不是同一份东西，也不要合成一份。002 的 checkpoint 写日志，007 补观察流事件。
