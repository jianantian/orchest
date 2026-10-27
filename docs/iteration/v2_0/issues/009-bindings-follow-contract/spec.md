# 009 · 绑定跟上新契约（最小透传）

GitHub issue: —

## Background

`orchest-py` 与 `orchest-node` 是 workspace 成员，CI 跑 `cargo clippy --workspace`。001 改 `SessionStore` 与快照类型，004 把 `ApprovalBus` 从「按 run id 存 bool」改成带 request id 的请求通道，006 改取消语义。现有绑定的 `respond_approval(run_id, bool)`（`crates/orchest-py/src/lib.rs:917`、`crates/orchest-node/src/lib.rs:752`）与事件转换会直接编译失败。绑定里也没有任何取消入口。

## Goal

绑定在 v2.0 契约下编译通过、现有 SDK 测试全绿，并把取消与按 id 回复透传出去；不做其他 API 重设计。

## Acceptance Criteria

- [ ] `cargo clippy --workspace --all-targets -- -D warnings` 在 v2.0 分支上通过，绑定 crate 不加 `allow` 绕过。
- [ ] Python `Agent.abort()` 与 Node `agent.abort()`：取消当前 run，语义同 006。
- [ ] `respond_approval(run_id, bool)` 替换为 `respond(request_id, payload)`；事件里的请求带 `request_id`、`kind`、`payload`。
- [ ] 新增事件变体（007）进入 Python `.pyi` 与 Node `types.d.ts`，`scripts/check-ts-event-variants.sh` 通过。
- [ ] 现有 Python 与 Node SDK 测试全绿，新增 `abort` 与 `respond` 的用例。
- [ ] `CHANGELOG-SDK.md` 记录破坏性变更（随下一个 SDK 版本发布）。

## Blocked by

001、004、006。

## Notes

- 不改 `run` / `run_sync` / `run_stream` 的形状，不加 session / resume，不改事件为运行时类型：这些属 SDK 0.3。
