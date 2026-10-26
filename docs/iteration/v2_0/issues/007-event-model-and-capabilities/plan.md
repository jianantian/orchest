# 007 implementation plan

## Files to read

- `crates/orchest/src/events.rs`（37 个变体的现状与 `deliver_to_subscribers`）
- `crates/orchest/src/bindings.rs`（事件扁平化规则：变体名 → snake_case `type`）
- `crates/orchest/src/run/{actor.rs,handle.rs}`（事件发出点）
- `crates/orchest/src/tool/mod.rs`（账本事件与请求事件的发出点）
- `crates/orchest/src/run/config.rs`（哪些能力由配置决定：并行工具、委派、批准作用域）
- `crates/orchest/src/skill/executor.rs` 与 005 的执行后端接口（隔离强度事实）
- `crates/orchest-py/src/lib.rs`、`crates/orchest-node/src/lib.rs`、`js/types.d.ts`（事件类型在三端的镜像）

## Files to change

- `crates/orchest/src/events.rs`：新增消息级、请求级、账本级事件变体
- 新增 `crates/orchest/src/capabilities.rs`：`Capabilities` 结构与查询方法
- `crates/orchest/src/run/{actor.rs,handle.rs,config.rs}`：发出新事件；暴露能力
- `crates/orchest/src/tool/mod.rs`：账本与请求事件发出
- `crates/orchest/src/bindings.rs`、`orchest-py`、`orchest-node`、`js/types.d.ts`：事件类型同步
- `docs/guide/`：事件参考页

## Steps

1. 定义并加入事件变体：消息开始/更新/结束（含文本与思考增量区分）、请求发出/解决、派发/结算、终止原因；保持 `#[non_exhaustive]`。
2. 在对应发出点接入：模型流处发消息增量；004 的请求通道发请求事件；003 的账本发派发/结算；006 发终止原因。
3. 定义 `Capabilities`：恢复、抢占取消、批准作用域、请求 kind 支持、并行工具、委派、效果分类集合、隔离等级、事件零丢失保证；提供 `RunHandle::capabilities()` 与配置级查询。
4. 能力必须反映运行时事实：隔离等级取自 005 注入的后端，不是编译期常量；"声明支持"的能力要有对应测试。
5. 不支持的能力返回显式的不支持错误（不用 panic、不静默降级）。
6. 同步三端事件类型；跑 `scripts/check-ts-event-variants.sh`。
7. 写事件参考页：逐变体语义 + "执行态日志 vs 观察流事件"的区分。
8. 跑 `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --check`。
