# ADR-0006: 语音会话模型（Voice Session Model）

- 状态：**Proposed**（六个概念与定位已由 owner 冻结，2026-10-27；其余条款待确认）
- 日期：2026-10-27
- 决策人：Orchest maintainer（emile）
- 相关：[语音迭代计划](../iteration/voice/plan.md)、[ADR-0001](./0001-provider-unification.md)、[ADR-0002](./0002-protocol-provider-decoupling.md)、[ADR-0004](./0004-sdk-packaging.md)

## 背景

Orchest 已经接入五个流式 ASR 方言（aliyun、volcengine、deepgram、elevenlabs、soniox）、一个批量 ASR 方言（assemblyai）、三个 TTS 方言（aliyun、volcengine、minimax）和一个全双工方言（volcengine omni）。`orchest-protocol` 里已有 `TranscriptStability`、`TranscriptUpdateKind`、`SegmentRef`、`LifecycleEvent::EndOfSpeech`、`SessionInput`、`RealtimeHandle` 等类型，但「什么时候算定稿」「什么时候算一轮说完」「客户端怎么要求尽快定稿」没有统一契约，各方言行为不一致。聚合类 provider（SpeechRouter、AllModels、LiveKit Inference、OpenRouter、Vercel AI Gateway）一接入，这些不一致会被放大。

本 ADR 固定一套对外的语音心智模型；各厂商协议和聚合服务都映射到它。

## 决策

### D1 — 定位：语音 SDK，不是中转站

- 目标：只配 `api_key`、`provider`、`model` 即可使用语音能力。
- Orchest 是 SDK / 引擎。基于它可以搭建中转站，但中转站不是 Orchest 本身；鉴权、预算、请求日志这类网关功能不进 Orchest。
- 对外只暴露一套心智模型。直连厂商与聚合服务都是 provider，映射到该模型；聚合服务在能力目录里标为 `source = gateway`。
- ASR 与 TTS 使用同一套概念结构。

### D2 — 心智模型六件套（已冻结）

| 概念 | 含义 | 协议落点 |
|---|---|---|
| Session | 与上游的一条连续流式连接，同一套 `api_key` / `provider` / `model`。一条 Session 内可有多轮说话，关闭连接才结束。 | `RealtimeHandle`；`ProviderConfig` |
| Stable | 已确认文稿，可交给业务或 Agent 使用。 | `Transcript` + `Committed`；`Append` 追加或 `Snapshot` 整段定稿 |
| Draft | 临时识别结果，下一包可能整段替换。 | `Transcript` + `Provisional` + `Snapshot` |
| Turn | 一轮说话结束，可以触发业务或 Agent。不等于「又确认了若干字」。 | `LifecycleEvent::EndOfSpeech` |
| Finalize | 客户端要求尽快定稿，并尽量随后给出 Turn。 | `SessionInput::Finalize`（新增） |
| Extras | 厂商特有数据，不进入上述主概念。 | `options` / `CapabilityEventExt` / `diagnostic_metadata` |

### D3 — Stable 的两种合法形态

- **Append**：向已确认文稿追加。
- **Snapshot**：该段已确认文稿以本次为准。

具体模型支持哪一种，由能力目录声明；应用 API 不因此拆成两套。`SegmentRef` 现有注释中「Append 只保留给原生增量流」的限制随本 ADR 取消。

### D4 — Session 与 VAD

端侧 VAD 与云端 VAD 都只产生 Turn，不新开 Session。

| 机制 | 作用 | 是否新开 Session |
|---|---|---|
| 端侧 VAD | 本地检测到停顿后发送 Finalize，等待定稿与 Turn | 否 |
| 云端 VAD / endpointing | 上游自动切句并产生 Turn | 否 |
| 新开 Session | 更换 provider/model、断线重连、主动关闭后再开、上游会话时长上限 | 是 |

### D5 — 时间线与应用侧默认处理

```text
打开 Session (key + provider + model)
  → 送音频 (SessionInput::Audio)
  → Draft（Provisional Snapshot）
  → Stable（Committed Append 或 Snapshot）
  → Finalize（可选，客户端收尾）
  → Turn（EndOfSpeech；与 Stable 不等价）
  → 应用 / Agent 消费 Stable
  → 下一 Turn（仍在同一 Session）
```

- Draft → 更新临时展示
- Stable → 写入已确认缓冲（Append 或按段 Snapshot）
- Turn → 触发业务或 Agent

### D6 — 映射规则

上游字段名不进入对外 API，provider 负责翻译。

| 上游现象 | 落入模型 |
|---|---|
| interim / partial / intermediate / non-final | Draft |
| 稳定增量（final tokens、delta 等） | Stable（Append） |
| 整段定稿（committed、completed、end_of_turn 等） | Stable（Snapshot），必要时同时给 Turn |
| speech_final / utterance end / finished / turn end | Turn |
| commit / finalize / ForceEndpoint | Finalize |
| words / speaker / confidence / 原始字段 | Extras；由能力目录声明是否具备 |

上游没有原生 Turn 信号的方言，由映射层在定稿后合成 Turn。

### D7 — 补充约定

1. **Turn 指向它结束的 Stable**：`EndOfSpeech.segment` 应携带它所结束那段 Stable 的 `segment_id`，应用据此决定把哪段文本交给 Agent。
2. **Finalize 不被支持时报错**：能力目录声明 `client_finalize = false` 的模型收到 Finalize，返回 `UnsupportedOption`，不静默降级、不本地合成 Turn。
3. **failover 不新开 Session**：聚合 provider 在会话中切换上游不算新 Session，但必须发出 `LifecycleEvent::RouteSelected`。
4. **协议按目录行声明**：同一厂商、同一模型族可能有多套协议（例如 aliyun 的 run-task 与 realtime 两套 WebSocket；OpenAI `gpt-realtime` 与 `gpt-live`），用哪个映射器由能力目录行决定，不从模型名推断。
5. **TTS 对照**：Session = 连接；Stable = 已合成的音频分片及其文本区间；Finalize = flush；Turn = 一个合成上下文结束；Draft 不适用；Extras = 词级时间戳。

### D8 — 能力目录

主模型保持稳定，各 provider 的差异用能力目录表达。不支持的便携选项返回 `UnsupportedOption`，禁止静默忽略。

| 能力项 | 含义 |
|---|---|
| `committed_append` | 是否支持说话过程中 Stable Append |
| `server_endpointing` | 是否由上游自动切句 |
| `client_finalize` | 是否支持 Finalize |
| `words` / `speaker` / `confidence` | 词级时间戳、说话人、置信度 |
| `speech_started` | 是否提供开始说话信号 |
| `failover` | 是否可能在会话中切换上游 |
| `source` | `direct` 或 `gateway` |
| `protocol` | 映射器使用的方言标识 |

`source` 是新增字段，与 `CapabilityDescriptor.source`（能力信息来源：`Static` / `ProviderMetadata` / `Assumed`）不是一回事。

## 后果

- `SessionInput` 新增 `Finalize`；`SegmentRef` 契约改写；ElevenLabs、Deepgram 方言需补 Turn。
- `AsrCatalogExt` / `TtsCatalogExt` 从空结构体扩为上述能力项。
- Python / Node 的 ASR 流式接口按本 ADR 重设计，并在 SDK 0.2 发布前完成（owner 决定，2026-10-27）；不保留旧接口形状。
- 具体拆分与验收见 [语音迭代计划](../iteration/voice/plan.md)。

## 参考

- SpeechRouter — https://speechrouter.ai/
- AllModels — https://allmodels.io/
- router.audio — https://router.audio/
- Vapi — https://docs.vapi.ai/quickstart/introduction
- LiveKit Inference — https://docs.livekit.io/agents/models/inference/
- Vercel AI Gateway — https://vercel.com/docs/ai-gateway
- Azure MAI Transcribe 2 Streaming — https://learn.microsoft.com/en-us/azure/ai-services/speech-service/mai-transcribe-2-streaming-realtime
- OpenAI Realtime Transcription — https://developers.openai.com/api/docs/guides/realtime-transcription
- ElevenLabs Realtime STT — https://elevenlabs.io/docs/eleven-api/guides/how-to/speech-to-text/realtime/event-reference
- Deepgram endpointing / interim — https://developers.deepgram.com/docs/understand-endpointing-interim-results
- Deepgram UtteranceEnd — https://developers.deepgram.com/docs/utterance-end
- AssemblyAI streaming — https://www.assemblyai.com/docs/streaming/message-sequence
- Soniox WebSocket STT — https://soniox.com/docs/api-reference/stt/websocket-api
- Soniox endpoint detection — https://soniox.com/docs/stt/rt/endpoint-detection
- Soniox manual finalization — https://soniox.com/docs/stt/rt/manual-finalization
