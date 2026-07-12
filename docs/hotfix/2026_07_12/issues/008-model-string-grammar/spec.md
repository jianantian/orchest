# Issue 008:model-string 语法 — `provider/[protocol/]model` 解析

Parent: [ADR-0002](../../../../adr/0002-protocol-provider-decoupling.md) Phase 2 · AFK · 依赖 001 + 005

## 现状

001 已让 `normalize_provider_model` 保持旧行为(`split_once('/')`,provider + 其余全是
model 名)。ADR Phase 2 要引入 `provider/protocol/model` 显式协议形,但 model id 本身可能
含 `/`(OpenRouter 是 `openrouter/<upstream-provider>/<model-name>`),按段数计数会歧义。
本 slice 落地 ADR "Parsing rule" 的 vocabulary-based 解析——通用机器,不含任何 Elss 专属
逻辑(Elss 溶解是 009)。

## 方向(本 slice 建什么)

- **解析规则**(ADR "Parsing rule"):
  1. 按 `/` split 出 provider(第一段,行为不变)。
  2. 若余下含 `/`,第一段仅当它匹配 canonical 协议名(`messages`/`chat`/`responses`)**或**
     该 provider 声明的 `protocol_aliases` 时才当协议,否则整段是 model 名。
- `NormalizedProviderModel` 加 `protocol: Option<Protocol>` 字段。
- **auto-detection 优先级**(ADR "Auto-detection and precedence"):显式段 > 按 provider
  `protocols` 过滤的 model-prefix 表(`claude-*`→Messages,其余→Chat)> provider `protocols`
  首选。显式协议若不在 provider 支持列表内 → 报错,不静默 fallback。`Responses` **从不**
  auto-detect,仅显式可选。
- **URL 解析规则**(ADR "URL resolution")统一收口:configured URL(entry `default_base_url`
  与用户 `api_url`)一律当 base URL,protocol factory 追加 canonical path 或 `path_overrides`;
  若 `api_url` 已以该 path 结尾则原样用(幂等 append)。这把 001/005 里各自的临时 append
  收敛成一条确定规则(Elss 的 `resolve_api_url` 后缀嗅探留到 009 删)。
- **保留字**:canonical 协议名成为 model-string 第二段的保留字;文档记录该限制(ADR
  已裁定无已知 vendor 冲突)。

本 slice 需要 001 的 `Protocol`/`ProviderEntry`/`ResolvedModel`,以及 005 的
`MessagesProtocolFactory`,这样显式协议(如 `openai/chat/...` 无操作等价、未来 `.../messages/...`)
能真正路由到两个协议目标之一。故依赖 001 + 005。

## 落地与测试

- 现存两段形全部回归:`anthropic/claude-sonnet-5`、`openai/gpt-4.1`、`deepseek/...` 行为不变
  (auto-detect)。
- **关键反例**:`openrouter/anthropic/claude-opus-4-8` 第二段仍属 model 名(`anthropic` 非
  canonical 协议名、此刻无 provider 声明它为别名)。
- 显式形:`openai/chat/gpt-4.1` 解析出 `protocol=Some(Chat)`;不支持的协议报错。
- 保留字:`<provider>/messages/...` 第二段被当协议。
- URL 幂等:传入完整 endpoint 不重复追加 path。
- node/py 经 `normalize_provider_model` 的调用点:新增 `protocol` 字段向后兼容,现有调用不变
  (回归 `orchest-node`/`orchest-py` 构建与测试)。

## 验收标准

- [ ] `normalize_provider_model` 落地 vocabulary-based 解析;`NormalizedProviderModel.protocol`
      字段就位
- [ ] auto-detection 三级优先级 + 显式协议不支持时报错 + `Responses` 从不 auto-detect
- [ ] URL 解析统一为 base URL + 幂等 append
- [ ] `openrouter/<vendor>/<model>` 多段形不回归(第二段仍属 model 名)
- [ ] node/py 构建与测试通过(字段新增向后兼容)
- [ ] `cargo test --workspace` / clippy `-D warnings` / fmt / `scripts/lint-check.sh` 全过

## 非目标

- 不删 `ElssFactory`/`resolve_api_url`(009)。不做任何 provider 的 `protocol_aliases` 声明
  (009 给 Elss 声明)。不为 `Protocol::Responses` 建 factory。
