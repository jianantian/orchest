# 001 — cache_control 移到 content block 级

## 背景

`crates/orchest-provider-http/src/messages.rs:250-254` 在 `CachePolicy::Auto`/`Long` 下把 `cache_control` 写在请求 body **顶层**(`body["cache_control"] = …`)。Anthropic Messages API 只接受 content block 级的 `cache_control`(见 `docs/external/anthropic/api.md` prompt caching 章节;`system` 字段接受 block 数组)。后果:默认配置(`CachePolicy::Auto`)下每个 Anthropic 请求都携带非法顶层字段——最好情况被静默忽略(**prompt caching 从未生效**),严格端点返回 400。

## 目标/范围

把 cache breakpoint 移到 block 级,单断点即可(不做多断点策略):

- system 非空时:断点挂在最后一个 system block(system 字段从 string 转为 block 数组);
- system 为空时:断点挂在最后一条消息的最后一个 content block;
- `CachePolicy::Off`:不带任何 `cache_control`;
- `CachePolicy::Long`:block 上带 `ttl: "1h"`。

非目标:多断点/缓存策略调优、tools 数组断点。

## 验收标准

- [x] 默认 Auto 且 system 非空:wire body 无顶层 `cache_control`,最后一个 system block 带 `cache_control: {"type":"ephemeral"}`
- [x] system 为空:断点落在最后一条消息的最后一个 content block
- [x] `Long`:block 上带 `ttl:"1h"`;`Off`:body 任何位置无 `cache_control`
- [x] 现有 cache policy 相关测试更新,新增断点位置断言;四件套全绿

## 实施要点(hotfix 内嵌 plan)

- 读: `crates/orchest-provider-http/src/messages.rs`(system/messages 序列化、cache policy 现状与测试)、`docs/external/anthropic/api.md` 的 prompt caching 章节(block 级 `cache_control` 的确切形状)
- 改: `messages.rs` 序列化逻辑;system 字段必要时从 string 构造改为 block 数组构造
- 测: 更新受影响测试,新增 wire 级断言(body 无顶层 cache_control、block 位置正确)
