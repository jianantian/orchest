# Issue 006:Minimax Messages profile — `map_role` + `path_overrides`

Parent: [ADR-0002](../../../../adr/0002-protocol-provider-decoupling.md) Phase 1 · AFK · 依赖 002 + 005

## 现状

Minimax 是 Anthropic Messages 兼容,但在 Messages 侧 fork 最多,是本模型的**压力测试**:
- **role 下调**:有 Minimax 独有 role,`role_compat.rs` 的 `downgrade_minimax_role` 把
  canonical role 映射到 provider 接受的 role;
- **thinking 方言**:用 `thinking: {type, display}`(adaptive vs disabled,display
  summarized/omitted)而非 Anthropic 的 `budget_tokens` + signature block;
- **非标准 endpoint path**:`POST /anthropic/v1/messages`(不是 `/v1/messages`);
- image content:与 Anthropic 同 schema(adapter 内注释引 Minimax API 文档确认),**不构成**
  另一个协议,留在 profile 内即可。

`MinimaxFactory` 已在 141fcfb 修复注册。本 slice 把它迁到 Messages protocol factory。

## 方向(本 slice 建什么)

把 Minimax 迁到 `MessagesProtocolFactory` + `MinimaxProfile`。本 slice **诞生 `map_role`
hook,并接通 Messages 侧的 profile 分发 + `path_overrides`**:

- 接通 Messages 侧 profile 分发:005 建的 `MessagesProtocolFactory` 此前无 profile 调用点,
  本 slice 让它复用 002 诞生的 `ProviderProfile` trait 与"查表 → 调 hook"结构(同一 trait,
  不复制第二套)。
- `map_role(&self, &ResolvedModel, &Role) -> WireRole` 诞生:承载 `role_compat.rs` 的
  Minimax role 下调。
- `lower_options` 覆盖成 Minimax 的 `thinking: {type, display}` 方言——这会是本仓最大的
  `lower_options` 覆盖,是 ADR "dialect-fork threshold" 的观察对象:它只改 option lowering /
  role,**没有**改 content-block 编码或 stream-event 形状,故仍是 profile 而非新协议。
- `path_overrides` 落地并被 `MessagesProtocolFactory` 使用:Minimax entry 声明
  `(Messages, "/anthropic/v1/messages")`,factory 有 override 时用它、否则用 canonical path。
- `service_tier` 等 gateway 级 meta-option 走 `ProviderConfig::options`,由 profile 的
  `lower_options` 应用,不进 canonical `RequestOptions`(ADR Resolved Q10)。

## 落地与测试

- Minimax model string 行为与迁移前一致:role 下调、`thinking: {type, display}`、
  `/anthropic/v1/messages` 落点、image content、usage(含 `cache_read_input_tokens`)、
  `service_tier` 透传(对比现有 `minimax/request.rs` / `minimax/response.rs` 断言)。
- 新增单测:`map_role` 对 Minimax 独有/标准 role 的映射;`path_overrides` 生效;Messages 侧
  profile 分发在无 profile(Anthropic)时仍走 canonical。

## 验收标准

- [ ] `map_role` hook 诞生;Messages 侧 profile 分发复用同一 `ProviderProfile` trait(不复制)
- [ ] `path_overrides` 落地,Minimax 命中 `/anthropic/v1/messages`
- [ ] Minimax 走 `MessagesProtocolFactory` + `MinimaxProfile`,现存断言全过;Anthropic(无
      profile)行为不变
- [ ] image content 仍作为 Messages profile 处理(未被误判为新协议)
- [ ] `cargo test --workspace` / clippy `-D warnings` / fmt / `scripts/lint-check.sh` 全过

## 非目标

- 不为 image content 或 thinking 方言另立 `Protocol` 变体(未越过 dialect-fork threshold)。
