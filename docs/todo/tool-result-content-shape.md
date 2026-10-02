# Bug: Anthropic 协议下 tool_result 携带 JSON 对象被上游 400 拒绝

> 状态: **主问题已修复** —— `tool_result` 对象归一化由 `6db4c79`(2026-08-21)落地,见下文
> 「处理结果」;附带发现(长流被掐)的重试区分已具备,「是否容忍缺失 `message_stop`」仍是
> **未决的设计问题** | 记录于 2026-08-20,来自 motif lyrics-lab 实测;状态更新于 2026-10-02

## 现象

任何返回 `ToolOutput::Immediate(json!({...}))`(JSON 对象)的工具——包括 SDK
自带的 `load_skill`——在 Anthropic messages 协议下会让下一步请求被上游拒绝:

```
API returned 400: ...content.0.tool_result.content: Found an object, but
`tool_result` content must either be a string or a list of content blocks.
```

## 根因链路

1. `orchest-protocol/src/types.rs:60-63` — `ContentBlock::ToolResult.content`
   是裸 `serde_json::Value`, 没有形状约束。
2. `orchest/src/run/actor.rs:1467` — 工具输出 `Value` 原样塞入 `ToolResult`。
3. `orchest-provider-http/src/messages.rs:134-139` — Anthropic 序列化器把
   `content` 原样透传到 wire。

Anthropic 规范: `tool_result.content` 只接受 string 或 content block 数组,
不接受裸 object。SDK 自己的 `load_skill` 就返回对象
(`{name, skill_md, bundled_files}`, `skill/disclosure.rs:execute_skill_md`),
所以任何用到 skills 的 Anthropic 协议会话都是一颗定时炸弹——是否爆炸取决于
上游网关校验的严格程度(elss 时严时松, 实测同一会话形态三次两种结果)。

## 建议修法

在 `messages.rs` 的 `ToolResult` 序列化处做归一化(最小侵入):

- `Value::String` → 原样
- `Value::Array` → 原样(假定已是 content blocks)
- 其他(object/number/bool/null) → `Value::String(value.to_string())`

也可以选择在 actor 写入 `ToolResult` 处归一化, 但协议层修能同时保护其他
写入方。归一化处建议加一条针对 object 输出的协议测试。

## 处理结果

已按上面的建议修法在协议层归一化(`6db4c79`):

- `crates/orchest-provider-http/src/messages.rs` 的 `normalize_tool_result_content`:
  `String` / `Array` 原样,其余(object / number / bool / null)序列化为 JSON 文本。
- 回归测试: `providers::anthropic::tests::tool_result_object_content_is_flattened_to_json_text`。

## 附带发现: 长流被掐时的硬失败

同一批实测中两次出现 `SSE stream ended without message_stop`
(`messages/response.rs:307`, code `stream_interrupted`), 发生在 47s/90s 的
长生成流上, 断流前内容大概率已完整。当前实现丢弃全部已收内容并报错。
可以考虑: 当 stream 干净结束(EOF)且已收到完整 text block 时, 容忍缺失的
`message_stop` 并返回已收内容(可带 degraded 标记), 或至少让调用方能区分
"可重试的传输中断"与"真正的协议错误"。是否放宽属于设计决策, 在此仅记录现象。

**现状(2026-10-02):**

- 「让调用方区分可重试的传输中断与协议错误」已具备: 该错误带 `code = "stream_interrupted"`,
  `orchest::run::retry` 归类为 `RetryClass::StreamInterrupted`,`RetryPolicy::recommended()`
  会重试(v0.13,[#222](https://github.com/jianantian/orchest/issues/222))。
- 「stream 干净结束且已有完整 text block 时,容忍缺失的 `message_stop` 并返回已收内容」**未做,
  也未决定**: `messages/response.rs` 在 `!got_message_stop` 时仍丢弃已收内容并返回
  `stream_interrupted`。是否放宽(以及是否带 degraded 标记)是设计决策,待 owner 决定。
