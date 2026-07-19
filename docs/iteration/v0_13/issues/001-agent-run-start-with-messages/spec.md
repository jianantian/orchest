# 001 — AgentRun 公开多轮启动入口

## 背景

`AgentRun::start(config, input, model, registry)` 公开签名只接受单个 user turn 的 `RunInput`(`crates/orchest/src/run/mod.rs:45-59`);真正带 `initial_messages` 的 `start_with_bus` 是 `pub(crate)`(`run/mod.rs:62-69`)。使用方要传多轮历史只能把所有消息拍平成一条 user 消息(music-gift `agent.rs:210-212` 的 flat_map),wire 上 `system: ""`、历史无角色边界——这是 SDK-A1,music-gift 歌词质量问题的最大单因。

`pre_start` 的现有组装(`run/actor.rs:277-290`):`[System(config.system_prompt)] + initial_messages + [User(input)]`——语义已经是对的,缺的只是公开入口。

## 目标/范围

公开一个多轮启动入口,让使用方能以 `system_prompt`(经 `AgentConfig`)+ 多轮历史(带完整角色结构)启动 run。推荐形态(实现时以代码现状定):

```rust
AgentRun::start_with_messages(config, initial_messages, input, model, registry)
```

语义与 `pre_start` 现有组装一致:`initial_messages` 为历史(User/Assistant 为主),`input` 为新一轮 user 输入;`initial_messages` 中允许出现 System 消息时按协议现状处理(Messages 协议抽出合并进顶层 system,Chat 协议原样置中),文档写明该行为。`start()` 行为不变(等价于 initial_messages 为空)。

Py/Node 绑定透传多轮启动(若绑定层改造成本明显超出,记录为绑定后续项并在 spec 注明)。

## 验收标准

- [x] 使用方能以 `.system_prompt(...)` + `[User, Assistant, …]` 历史启动 run;Anthropic Messages 协议 wire 上 `system` 字段非空、历史消息角色边界保留
- [x] `start()` 现有行为不变(现有测试不回归)
- [x] 历史中含 ToolUse/ToolResult blocks 时 wire 合法(不破坏 tool_use 配对)
- [x] rustdoc 写明 initial_messages 的角色约定与 System 消息处理
- [x] Py/Node 绑定透传,或显式记录为绑定后续项
- [x] 四件套 + cargo doc 全绿

## 备注

- 本 issue 是 demo 去拍平(optimization-plan D6)的前置;demo 侧改造不在本迭代。
- `resume_with_input` 走的是 SessionStore 快照路径,与本入口互补、不动。
- 绑定透传形态(2026-07):Py `Agent.run/run_sync/run_stream(input, messages=None)` 与 Node `runSync(input, messages?)`/`runStream(input, onEvent, messages?)`;`messages` 取核心 `Message` 的 serde JSON 形状(与 session 快照一致,如 `{"role": "user", "content": [{"Text": "..."}]}`),转换 helper 共享于 `orchest::bindings::messages_from_wire_values`。
