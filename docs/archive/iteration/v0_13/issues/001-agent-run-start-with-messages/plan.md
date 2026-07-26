# 001 — 实施计划

## 要读的文件

- `crates/orchest/src/run/mod.rs`(`AgentRun::start` / `start_with_bus` / `resume_with_input` 的签名与分工)
- `crates/orchest/src/run/actor.rs`(`pre_start` 的消息组装,:277-290 附近)
- `crates/orchest/src/run/handle.rs`(RunHandle/EventReceiver 接线)
- `crates/orchest-protocol/src/types.rs`(Message/Role/ContentBlock 形状)
- `crates/orchest-py/src/lib.rs`、`crates/orchest-node/src/lib.rs`(绑定层 start 的现有透传方式)
- 现有 run 测试(`crates/orchest/src/run/tests.rs` 的 model stub/test harness 用法)

## 要改的文件

- `crates/orchest/src/run/mod.rs`(公开多轮启动入口)
- `crates/orchest/src/run/actor.rs`(仅在组装语义需要调整时)
- `crates/orchest/src/run/tests.rs`(新测试)
- `crates/orchest-py/src/lib.rs`、`crates/orchest-node/src/lib.rs`(绑定透传,或记录后续项)

## 步骤

1. 读 `start_with_bus` 的现状签名与调用方(supervisor/sub-agent 路径),确认 initial_messages 的内部语义。
2. 设计公开签名(推荐 `start_with_messages(config, initial_messages, input, model, registry)`),保持 `start()` 为便捷包装;内部复用同一条组装路径,不复制造诣。
3. rustdoc:角色约定(历史应以 user/assistant 为主)、System 消息处理、与 `resume_with_input` 的分工。
4. 测试:① 多轮启动后第一次模型调用的消息序列(system 非空、角色边界、顺序);② 含 ToolUse/ToolResult 历史的 wire 合法性(可经 Messages 适配器 request_body 断言);③ `start()` 不回归。
5. 绑定透传:Py/Node 各加一个多轮启动入口(输入消息数组);若绑定层改造成本明显超出,在 spec 备注记录为绑定后续项。
6. 四件套 + `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`。
