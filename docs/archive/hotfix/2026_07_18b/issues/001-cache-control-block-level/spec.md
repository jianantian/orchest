# 001 — cache_control 移到 content block 级

## 背景

`crates/orchest-provider-http/src/messages.rs:250-254` 在 `CachePolicy::Auto`/`Long` 下把 `cache_control` 写在请求 body **顶层**(`body["cache_control"] = …`)。按现行 vendor 文档(`docs/external/anthropic/api.md` Create a Message 参数表:顶层 `cache_control` "automatically applies a cache_control marker to the last cacheable block"),顶层写法在 Anthropic 侧是**合法**的——本 issue 的真实动机是:(a) 顶层写法把断点位置交给 API 决定,无法显式控制;(b) Anthropic-compatible 端点(如 Minimax,`docs/external/minimax/llm/activate_cache.md`)只文档化 block 级形态,顶层字段在这些端点行为未定义。(更正记录:本背景初稿称"Anthropic 只接受 block 级、缓存从未生效、严格端点 400",经 code review 对照 `api.md:1149-1151` 证伪。)

## 目标/范围

把 cache breakpoint 移到 block 级,单断点即可(不做多断点策略):

- system 非空时:断点挂在最后一个 system block(system 字段从 string 转为 block 数组);
- system 为空时:断点挂在最后一条消息的最后一个**可缓存** content block(跳过 thinking/redacted_thinking——API schema 中这两类块无 `cache_control` 字段);
- `CachePolicy::Off`:不带任何 `cache_control`;
- `CachePolicy::Long`:block 上带 `ttl: "1h"`。

非目标:多断点/缓存策略调优、tools 数组断点。

## 验收标准

- [x] 默认 Auto 且 system 非空:wire body 无顶层 `cache_control`,最后一个 system block 带 `cache_control: {"type":"ephemeral"}`
- [x] system 为空:断点落在最后一条消息的最后一个可缓存 content block
- [x] `Long`:block 上带 `ttl:"1h"`;`Off`:body 任何位置无 `cache_control`
- [x] 现有 cache policy 相关测试更新,新增断点位置断言;四件套全绿

## 实施要点(hotfix 内嵌 plan)

- 读: `crates/orchest-provider-http/src/messages.rs`(system/messages 序列化、cache policy 现状与测试)、`docs/external/anthropic/api.md` Create a Message 参数表(顶层与 block 级 `cache_control` 的形状;thinking 块无 `cache_control` 字段)
- 改: `messages.rs` 序列化逻辑;system 字段必要时从 string 构造改为 block 数组构造
- 测: 更新受影响测试,新增 wire 级断言(body 无顶层 cache_control、block 位置正确)

## 评审补充(2026-07-18,code review 后)

- 断点 fallback 循环跳过 `thinking`/`redacted_thinking` 块(避免把 `cache_control` 挂到 schema 不支持的块上);新增测试 `cache_policy_auto_skips_thinking_block_for_cacheable_one`
- 新增边界测试:空 messages + 无 system + Auto 时全 body 无断点(`cache_policy_auto_no_system_empty_messages_emits_no_breakpoint`)
- 相关:#215 的 budget 抬升改用 `saturating_add`,并补等式路径测试(`thinking_budget_equal_to_max_tokens_is_lifted`)
