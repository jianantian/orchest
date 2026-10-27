# v1.2 PRD：语音契约补齐与新 provider

> 状态：已确认（owner，2026-10-27）。GitHub issue 已建（#342–#348），编号已回填各 spec。
> 决策依据：[ADR-0006](../../adr/0006-voice-session-model.md)（语音会话模型）、[语音迭代线计划](../voice/plan.md) 的 V1。
> 版本：只做增量变更，按 ADR-0003 发 Rust `1.2.0`。

## Background

两件互不依赖、但都要赶在 SDK 0.2 之前落地的事：

1. **语音契约没写死。** ADR-0006 冻结了六个概念（Session / Stable / Draft / Turn / Finalize / Extras），代码里缺 Finalize，Turn 在 ElevenLabs 上从不出现，Deepgram 的 `UtteranceEnd` 被丢弃，`SegmentRef` 的注释还规定 Append 只给原生增量流。Python / Node 的 `start_asr_stream` 把 provider 和 model 合在一个参数里，事件是 `dict[str, Any]`。owner 已定：SDK 的 ASR 流式接口在 0.2 发布前按 ADR-0006 重设计，不保留旧形状。
2. **缺两个常用的模型入口。** owner 要求接入 [Vercel AI Gateway](https://vercel.com/docs/ai-gateway) 和 [Fireworks](https://app.fireworks.ai/models)。两家都是 OpenAI 兼容协议、模型列表会变，和已接入的 OpenRouter 同属动态网关。

## Goal

- 五个流式 ASR 方言在 Draft / Stable / Turn / Finalize 上行为一致，并有方言无关的契约测试护住。
- SDK 0.2 发出去的 ASR 流式接口就是 ADR-0006 的形状。
- `vercel/<vendor>/<model>`、`fireworks/<model>` 可直接用于 chat 与 decision。
- 发布 Rust `1.2.0`，让 SDK 0.2 的 SDK↔crate 版本矩阵对应一个已发布版本（ADR-0005 D6）。

## Decisions

| 主题 | 决定 | 来源 |
|---|---|---|
| Finalize | `SessionInput::Finalize`；不支持的方言返回 `UnsupportedOption`，不静默降级、不本地合成 Turn | ADR-0006 D2、D7.2 |
| Stable | Append 与 Snapshot 都合法，由能力目录声明；删除 `SegmentRef` 注释里的 Append 限制 | ADR-0006 D3 |
| Turn | `EndOfSpeech.segment` 携带它结束的 Stable 的 `segment_id`；没有原生 Turn 的方言由映射层在定稿后合成 | ADR-0006 D6、D7.1 |
| SDK 接口 | 硬切换：`model` 独立参数、`finalize()`、带 `kind` 的事件类型；不留兼容层 | owner 2026-10-27 |
| 新 provider 接法 | 沿用 OpenRouter 的动态网关路径（`create_adapter` + profile），不写静态模型目录 | 现有 OpenRouter 先例 |
| 新 provider 范围 | 只接 chat 与 decision；Vercel 的 transcribe / speech 与 Fireworks 的 ASR 留到语音 V3 | 语音线计划 V3 |
| 版本 | 全部增量变更（`SessionInput` 是 `#[non_exhaustive]`），发 `1.2.0` | ADR-0003 D4 |

## Scope

- `orchest-protocol`：`SessionInput::Finalize`；`SegmentRef` 与 `EndOfSpeech` 的契约注释。
- `orchest-provider-stream`：ElevenLabs 合成 Turn；Deepgram `UtteranceEnd` 映射为 Turn 并与 `speech_final` 去重；各方言接原生 finalize（Deepgram `Finalize`、Soniox manual finalization、ElevenLabs commit），不支持的返回 `UnsupportedOption`。
- 方言无关的 ASR 契约测试，用录制的 WebSocket fixture 对五个方言各跑一遍。
- `orchest-py` / `orchest-node`：`start_asr_stream` 重设计与类型化事件。
- `orchest-provider-http`：Vercel AI Gateway 与 Fireworks 的 chat 与 decision。
- `1.2.0` 发布。

## Out of Scope

- 语音 V2–V5（能力目录与计量、网关语音、TTS 双工、全双工）。
- `AsrCatalogExt` 的能力项（V2）；本迭代用代码常量判断方言是否支持 Finalize。
- Vercel / Fireworks 的语音、图像、embedding 能力。
- 任何 v2.0 的 runtime 契约改动。

## Issues

依赖顺序执行；同组内可并行。

| Doc | Issue | Work | Type | Blocked by |
|---|---|---|---|---|
| 001 | [#342](https://github.com/jianantian/orchest/issues/342) | `SessionInput::Finalize` 与 Stable / Turn 契约 | AFK | — |
| 002 | [#343](https://github.com/jianantian/orchest/issues/343) | 方言补 Turn 与原生 finalize | AFK | 001 |
| 003 | [#344](https://github.com/jianantian/orchest/issues/344) | 方言无关的 ASR 契约测试 | AFK | 002 |
| 004 | [#345](https://github.com/jianantian/orchest/issues/345) | SDK ASR 流式接口重设计 | AFK | 001 |
| 005 | [#346](https://github.com/jianantian/orchest/issues/346) | Vercel AI Gateway：chat 与 decision | AFK | — |
| 006 | [#347](https://github.com/jianantian/orchest/issues/347) | Fireworks：chat 与 decision | AFK | — |
| 007 | [#348](https://github.com/jianantian/orchest/issues/348) | 发布 `1.2.0` | HITL | 001–006 |

1. [`SessionInput::Finalize` 与 Stable / Turn 契约](issues/001-session-finalize-contract/spec.md)
2. [方言补 Turn 与原生 finalize](issues/002-dialect-turn-and-finalize/spec.md)
3. [方言无关的 ASR 契约测试](issues/003-asr-contract-tests/spec.md)
4. [SDK ASR 流式接口重设计](issues/004-sdk-asr-stream-redesign/spec.md)
5. [Vercel AI Gateway](issues/005-vercel-ai-gateway/spec.md)
6. [Fireworks](issues/006-fireworks/spec.md)
7. [发布 1.2.0](issues/007-release-1-2-0/spec.md)

主要风险在 003：录 fixture 需要五个方言的真实 key，缺 key 的方言会卡住验收。

## Acceptance Criteria

- [ ] 同一段录音在五个流式 ASR 方言上都产出 Draft → Stable → Turn，且 Turn 携带它结束的 `segment_id`。
- [ ] 端侧 VAD 场景下调 `finalize()` 不断开连接；不支持的方言返回 `UnsupportedOption`。
- [ ] Python 与 Node 的 `start_asr_stream` 接受独立的 `provider` 与 `model`，事件带 `kind`（draft / stable / turn），类型声明覆盖每种事件。
- [ ] `vercel/openai/gpt-5.4` 一类的 id 与 `fireworks/<model>` 能跑通 `complete` 与 `decide`，并有不依赖网络的单元测试。
- [ ] `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --check` 全绿；SDK 测试全绿。
- [ ] `1.2.0` 发布到 crates.io，`CHANGELOG.md` 记录新增项。
