# v0.9.10 PRD: Minimax 多模态 Provider 接入

> 类型: 卫星(功能) | 状态: 规划
> 详细设计来源: [`docs/research/minimax-api-analysis.md`](../../research/minimax-api-analysis.md)
> 本 PRD 是迭代级契约;每个 issue 的字段表 / 文档行号锚点以设计文档为准,不在此重复。
>
> **文档路径注**:研究文档与各 issue 的 LLM 锚点形如 `llm.md:NNN`;实际供应商文档
> 已拆分为 `docs/external/minimax/llm/` 目录(`api.md` / `chat_model.md` / `desc.md` /
> `tool.md` / `activate_cache.md` / `prompt_cache.md`)。所有 `llm.md:NNN` 锚点均映射到
> `llm/api.md:NNN`(已统一重写);Her 与角色扮演角色补充语义位于 `llm/chat_model.md`,
> 模型名总表位于 `llm/desc.md`。

## 背景

Minimax 是一个**多模态厂商**:一家同时提供 LLM(Anthropic 兼容)、TTS/Voice、Music、Video
共 16 个 API。把它接入现有 Orchest provider crate 既能补齐多模态能力,又因为它横跨
`agent-runtime-providers` / `agent-runtime-tts-providers` / `agent-runtime-aigc-providers`
3 个 crate,会**集中暴露"单厂商在多个 crate 里重复 http+auth"的结构问题** —— 这正是后续
Provider 统一重构(见 [`docs/todo/provider-unification.md`](../../todo/provider-unification.md))需要的证据。

本迭代是"先堆熵"的一步:按现有 crate 布局具体落 Minimax,**不动 crate 拓扑**。

## 相对设计文档的 3 处决策变更

设计文档写于 v0.10 命名确定前,本迭代对其做 3 处调整(已在设计文档对应章节标注):

1. **版本号**:设计文档中所有 "v0.10" 指代本迭代,实际版本号为 **v0.9.10**(v0.10 已被
   Briefing Desk 占用)。
2. **Music 不新建 crate**:设计文档 §四 / §六 Phase 5 / §七 Q3 主张新建
   `agent-runtime-music-providers`。**本迭代改为放进 `agent-runtime-aigc-providers` 子模块**
   (music 语义即 AIGC,复用其 multipart + storage;单开 `MusicProvider` trait 文件不污染
   现有 `ImageProvider`/`VideoProvider`)。原因:Step 3 要合 crate,现在不加 crate。
3. **ContentBlock 一次加齐 Audio**:Phase 1 扩 `ContentBlock` 时,除设计文档要求的
   `Image` / `Video`,**一并加 `Audio { source }`**。这是 Step 2(omni 端到端语音)的前向占位 ——
   omni 已列入路线,不再是投机。

## 范围(5 块 → 6 个 issue)

| Issue | 标题 | crate | 依赖 | 设计文档 |
|---|---|---|---|---|
| 001 | 多模态 content model 地基 | model + providers | — | §二 2.3 / §七 Q1 |
| 002 | Minimax LLM adapter(Anthropic 兼容) | providers | 001 | §二 2.1-2.2 |
| 003 | Minimax Video provider | aigc | — | §五 |
| 004 | Minimax TTS 同步+异步 + 文件上传 | tts | — | §3.2-3.4 |
| 005 | Minimax Voice Management | tts | 004 | §3.5 |
| 006 | Minimax Music(aigc 子模块) | aigc | — | §四 |

硬依赖只有 **002→001**、**005→004**;003 / 004 / 006 互相独立。开发顺序 001→006 满足全部依赖。

## 非目标

- **不实现 Her(角色扮演 / 陪伴模型)**:`docs/external/minimax/` 无任何 Her schema,不凭印象设计字段
  (设计文档 §九)。确认其协议后另立 issue。
- **不实现 video callback_url webhook**:v0.9.10 只做轮询,已覆盖 99% 用例(设计文档 §5.5 / §七 Q4)。
- **不动 crate 拓扑**:不抽 provider-core、不合并 crate、不做 umbrella。那是 Step 3 的重构迭代。
- **不实现 omni / 全双工统一 session 抽象**:Step 2 接入真实 omni 后才设计(`docs/todo`)。
- **不补缺失的供应商文档**:`voice_list.md`、`pricing.md` 等缺口(设计文档 §八)单独处理,
  本迭代用既有文档能落的部分。

## 关键设计决策(锁定,details 见各 issue)

- **LLM**:`ModelAdapter` trait 不动,以 `AnthropicAdapter` 为模板 fork `MinimaxAdapter`
  (Minimax 自称 Anthropic Messages 兼容)。
- **ContentBlock 破坏性扩展(§七 Q1-A)**:加 `Image`/`Video`/`Audio`/`MidConvSystem` variant 后,
  现有 LLM provider(anthropic/openai/deepseek/openrouter/volcengine)的穷尽 match 必须处理新
  variant —— 走 `OptionAdjustment` + 丢弃该 block(对齐现有 `thinking_budget_tokens` unsupported
  路径)。**例外**:Anthropic 原生支持 image,顺手实现真序列化;OpenAI vision 留作后续 follow-up。
- **Voice Management(§七 Q2-A)**:独立 `VoiceManager` trait,与 `TtsProvider` 同文件,只 Minimax 实现,
  不污染其他 TTS provider。
- **Video**:`VideoProvider` / `VideoContentItem` / `VideoImageRole` 现有类型已覆盖 4 个变体,
  types 层零改动;minimax 专属字段(`prompt_optimizer` / `fast_pretreatment`)进 `provider_options` JSON。
- **Music**:放 aigc,单开 `MusicProvider` trait;hex/url 输出复用 aigc storage/asset 持久化。
- **文件下载去重(§3.3 决策 A)**:`/v1/files/retrieve` 在 tts crate 内复制一份薄 helper,
  v0.11+ 出现第三方调用方再抽。

## 验收标准(迭代级)

- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 无 warning
- [ ] `cargo fmt --check` 通过
- [ ] `bash scripts/lint-check.sh` 通过
- [ ] `agent-runtime-model::ContentBlock` 含 `Image`/`Video`/`Audio`/`MidConvSystem`,
      所有现有 LLM adapter 穷尽 match 编译通过且不 panic
- [ ] `MinimaxAdapter` 注册进 `ProviderRegistry`,catalog 含 8 条非 Her 模型条目(M3 / M2.7 / M2.7-highspeed / M2.5 / M2.5-highspeed / M2.1 / M2.1-highspeed / M2)
- [ ] Minimax video 5 变体经 `VideoProvider` 提交,轮询状态映射正确,asset 在 download_url 过期前落库
- [ ] Minimax TTS 同步 WSS + 异步路径可用,`TtsOperation::Async` 存在
- [ ] `VoiceManager` trait 落地,Minimax 实现 clone/design/delete
- [ ] aigc 内 `MusicProvider` 落地,generation/lyrics/cover 三路可用
- [ ] Her 未被纳入实现;callback webhook 未实现(均记为非目标)

> 凡需 live provider key 的端到端验证(真实 Minimax 调用)均为 env-var gated 手动验证,
> 在各 issue 验收用 fake/单元测试覆盖确定性部分;真实调用记录命令、模型、日期、结果。

## 依赖

- v0.6.1 Image AIGC Gateway(`agent-runtime-aigc-providers` 的 gateway/storage/asset 基建)
- v0.9.3 TTS Provider Gateway(`agent-runtime-tts-providers` 的 trait/catalog/streaming)
- hotfix 06-17 LLM Catalog 扩展(`LlmModelEntry` 字段,catalog 条目据此填)

## 验证

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
bash scripts/lint-check.sh
```

Live Minimax 调用为手动、env-var gated,不进 CI。
