# 语音迭代计划

> 卫星迭代线，决策依据 [ADR-0006](../../adr/0006-voice-session-model.md)。
> 代码基线：`main` @ `59047f3`（2026-10-27）。
> 状态：草稿（待 owner 确认）；V1 的 SDK 改动须在 SDK 0.2 发布前完成（owner 决定，2026-10-27）。

## 结论

语音部分作为一条卫星迭代线，分五步走：先补契约（V1），再补能力目录和音频计量（V2），然后接聚合与网关 provider（V3），接通 TTS 双工并对齐到同一套模型（V4），最后补齐全双工的生命周期与第二个 provider（V5）。每一步都以 ADR-0006 冻结的六个概念（Session / Stable / Draft / Turn / Finalize / Extras）为准，不再改概念，只补实现。

顺序的理由：现有五个流式 ASR 方言已经能跑，但 Turn、Finalize、Append/Snapshot 的契约没写死，聚合 provider 一接进来就会把不一致放大。所以先把契约和契约测试钉住，再扩 provider。

这条线和 v2.0（Runtime 契约与可靠性）互不阻塞。和 SDK 0.2 的交叉点是 SDK：V1 会重设计 Python / Node 的 ASR 流式接口，已定在 SDK 0.2 发布之前完成，0.2 发出去的就是新接口，不保留旧的事件形状。

## 现状

六个概念里有四个在 `orchest-protocol` 里已有类型，缺的是 Finalize 和几条契约；SDK 层比 Rust 层更落后。

| 概念 | 现有代码 | 缺口 |
|---|---|---|
| Session | `Asr::start_stream` 返回 `RealtimeHandle { input, events }` | Python `start_asr_stream` 把 provider 和 model 合在一个 `provider` 参数里（`provider/model`），和设计里 key / provider / model 分开配置不一致 |
| Draft | `Transcript` + `Provisional` + `Snapshot` | 无 |
| Stable | `Committed`，`Append` / `Snapshot` 两种都已定义 | `SegmentRef` 注释规定 Append 只给原生增量流，和设计冲突 |
| Turn | `LifecycleEvent::EndOfSpeech { segment }` | ElevenLabs 从不发；Deepgram 的 `UtteranceEnd` 被丢弃 |
| Finalize | 无。`SessionInput` 只有 `Audio / Text / ToolResult / Interrupt` | Deepgram 只发 `CloseStream`，没有发原生 `Finalize`；Python 侧只有 `finish()`（关会话），没有「定稿但不关」 |
| Extras | `CapabilityEventExt::Asr(Value)`、`diagnostic_metadata` | 无 |
| 能力目录 | `AsrCatalogExt {}`、`TtsCatalogExt {}`（`crates/orchest-provider-core/src/catalog.rs`） | 空结构体，设计里的能力项都没有 |
| 计量 | `Done { usage: TokenUsage }` | 流事件不上报音频时长或字符数。`pricing.rs` 已有按时长计价（`Pricing::Duration`），但 ASR 目录行的 `pricing` 全是 `None`，也没有按字符计价 |

其他现状：

- `AsrStreamEvent` 在 Python 里是 `dict[str, Any]`，调用方看不到 Draft / Stable / Turn 的结构。
- TTS 与全双工都没有暴露到 Python 和 Node（绑定 crate 里只有 `asr.rs` / `asr_stream.rs`）。
- 三个 TTS 方言都实现了 `synthesize` 和 `stream_synthesize`（一次给全文本、音频流式返回），但 `start_duplex_stream`（边送文本边出音频）全部返回 `UnsupportedOperation`。
- omni 全双工只映射了音频、转写、文本和错误，没有任何输出侧生命周期事件（开始说话、被打断、本轮结束）。
- 直连方言：流式 ASR 有 aliyun、volcengine、deepgram、elevenlabs、soniox；AssemblyAI 只有批量；TTS 有 aliyun、volcengine、minimax。没有任何聚合类语音 provider。

## 外部资料带来的约束

1. **协议按目录行声明，不从模型名推断。** Vercel AI Gateway 文档明确说 `openai/gpt-live-1` 要用另一套 endpoint 和事件，改 slug 不会切换协议。Aliyun 也是同一厂商两套 WebSocket 协议（run-task 与 `/api-ws/v1/realtime`）。
2. **Session 要支持「先换 token 再连」。** Vercel Realtime 先用 `getToken({ model })` 拿 `{ token, url }`，客户端再直连 WebSocket，音频不经过网关。浏览器端和不少聚合服务都是这个形态。
3. **Turn 信号各家不一致，必须在映射层统一。** Deepgram 有 `speech_final` 和 `UtteranceEnd` 两种，Soniox 用 endpoint token，AssemblyAI streaming 用 `end_of_turn`，ElevenLabs 只有 committed。没有原生 Turn 的方言由映射层合成。
4. **网关是 provider，不是 Orchest 要变成的东西。** Vercel AI Gateway、SpeechRouter、OpenRouter 提供统一 key、日志、预算、failover；Orchest 把它们接成 `source = gateway` 的 provider，并把「实际服务的上游 + 用量」透传出来。

Vercel 的 Speech 接口（`generateSpeech` / `transcribe`）是一次请求一次结果，形状和 `Tts::synthesize`、`Asr::transcribe` 几乎一致，是接网关最便宜的切入点。Realtime 的事件按对话（response）划分，没有 Draft / Stable / Turn，映射时需要按模型单独处理。

## 业界参考：流式 TTS 与全双工

业界做法已经收敛：流式 TTS 用「上下文 + flush + 取消」三件套，全双工用「轮次检测可配 + 打断时截断到已播放位置 + 输出侧生命周期事件」。

### 流式 TTS（输入流式）

| 厂商 | 上下文 | 切分 / flush | 打断 | 对齐 |
|---|---|---|---|---|
| [Cartesia](https://docs.cartesia.ai/use-the-api/tts-websocket/contexts) | `context_id`，`continue: true` 表示后面还有，空文本 + `continue: false` 结束 | [`flush: true`](https://docs.cartesia.ai/use-the-api/tts-websocket/context-flushing-and-flush-i-ds) 递增 `flush_id`，音频分片带上它；`max_buffer_delay_ms` 控制缓冲 | `cancel: true` | 词级 / 音素级时间戳 |
| [ElevenLabs 多上下文 WS](https://elevenlabs.io/docs/eleven-api/guides/how-to/websockets/multi-context-web-socket) | 一条连接最多 5 个 context，空闲 20 秒超时 | 句末发 `flush: true`；[`chunk_length_schedule`](https://elevenlabs.io/docs/developers/websockets) 控制缓冲 | `close_context` 关旧的、开新的 | alignment 字符时间戳 |
| aliyun / minimax / volcengine（已接） | 一个 task 即一个上下文 | `continue-task` / `task_continue` 追加 | 无专门取消 | 无 |

启示：TTS 的双工输入应当是「追加文本 / Flush / 取消上下文 / 关闭」四种，输出的音频分片带 `context_id` 和 `flush_id`，有对齐时把词级时间戳放进 Extras。

### 全双工

| 方案 | 轮次检测 | 被打断后 | 输出侧生命周期 |
|---|---|---|---|
| [OpenAI Realtime](https://developers.openai.com/api/docs/guides/realtime-vad) | `server_vad` / `semantic_vad`（`eagerness` 可调），可关掉 `create_response` / `interrupt_response` 改为手动 | 服务端取消响应；WebSocket 模式下客户端发 [`conversation.item.truncate`](https://developers.openai.com/api/docs/guides/realtime-conversations)（`audio_end_ms`）截断到已播放位置 | `speech_started`、`response.done`、`response.cancelled` |
| [Gemini Live](https://ai.google.dev/api/live) | 自动检测，或手动发 `activityStart` / `activityEnd` | 服务端发 `interrupted` | `generationComplete` 或 `interrupted`，之后一定有 `turnComplete` |
| [Pipecat](https://docs.pipecat.ai/pipecat/fundamentals/interruptions) | 可插拔的轮次策略 | 中断信号传遍整条管线：LLM 停、TTS 清缓冲、丢弃未播音频；上下文只保留真正播出的字 | `on_assistant_turn_stopped`（含是否被打断） |
| [LiveKit Agents](https://docs.livekit.io/agents/logic/turns/adaptive-interruption-handling) | VAD + 轮次模型 | 自适应打断：过滤「嗯、对」这类附和，只对真打断让轮 | 框架事件 |

启示：

1. **输出侧要有生命周期**：至少「用户开始说话」「模型本轮生成完」「被打断」「本轮结束」四个信号，且规定顺序（参考 Gemini：生成完或被打断，之后必有本轮结束）。
2. **打断要带已播放位置**：`SessionInput::Interrupt` 增加可选的 `played_ms`，由 provider 映射成 truncate，保证对话历史里的助手文本只到用户听到的地方。
3. **轮次检测做成便携选项**：服务端 VAD / 语义 VAD / 手动（用 Finalize 表达 activityEnd），外加「自动回复」开关，由能力目录声明。
4. **附和词过滤、管线编排不进 SDK**：这是 Pipecat / LiveKit 这一层的事，Orchest 只需把信号给全。

## 迭代拆分

五步按依赖顺序排，V1 是后面所有步骤的前提；V2、V4、V5 可以并行，V3 依赖 V2 的能力目录。

### V0 设计落库

- ADR-0006 落库（本次提交），冻结六个概念、映射表、能力项清单。
- 在 `roadmap.md` 登记这条卫星线。

### V1 契约补齐（P0，SDK 0.2 前置）

落地迭代：[v1.2](../v1_2/prd.md)（issue 001–004）。

目标：现有五个流式 ASR 方言在 Draft / Stable / Turn / Finalize 上行为一致，并有契约测试护住。

- `orchest-protocol`：加 `SessionInput::Finalize`；改 `SegmentRef` 注释，Append 与 Snapshot 都是合法 Stable；写明 Turn 必须携带它结束的 `segment_id`。
- 方言：ElevenLabs 在 committed 后合成 Turn；Deepgram 把 `UtteranceEnd` 映射成 Turn（与 `speech_final` 去重）；各方言接上原生 finalize（Deepgram `Finalize`、Soniox manual finalization、ElevenLabs commit），不支持的返回 `UnsupportedOption`。
- 契约测试：一套方言无关的断言（同段 Draft 全是 Snapshot、Stable 之后不再有同段 Draft、Finalize 后必有 Turn 或明确报错），用录制的 WS fixture 对五个方言各跑一遍。
- SDK（API 重设计，必须在 SDK 0.2 发布前合入）：Python / Node 的 `start_asr_stream` 把 `model` 从 `provider` 字符串里拆成独立参数，加 `finalize()` 方法；`AsrStreamEvent` 改成带 `kind`（draft / stable / turn）的 TypedDict。
- 验收：同一段录音在五个方言上都产出 Draft → Stable → Turn；在端侧 VAD 场景下调 `finalize()` 不断开连接。

### V2 能力目录与音频计量（P1）

计划落地迭代：v2.1（与 V4 同一迭代，见 [roadmap](../roadmap.md)）。

- `AsrCatalogExt` 填上 ADR-0006 D8 的能力项；五个方言的目录行逐一填写，并在 V1 的契约测试里校验「声明与行为一致」。
- 便携选项校验：调用方请求目录未声明的能力（如 speaker）时，在开 Session 前返回 `UnsupportedOption`。
- 音频计量：流事件结束时上报音频秒数（ASR）和字符数（TTS），并带上实际服务的 provider / model；复用 `pricing.rs` 已有的 `Pricing::Duration` 给 ASR 目录行填上价格（现在全是 `None`），TTS 补按字符计价。
- 验收：`list_models` 能按能力项筛出模型；一次流式转写结束后能拿到秒数和估算成本。

### V3 聚合与网关 provider（P1）

未排期：先回答下方两个开放问题。Vercel AI Gateway 与 Fireworks 的 chat 已在 [v1.2](../v1_2/prd.md) 接入，V3 只补它们的语音接口。

- 先接一个批量型网关：Vercel AI Gateway 的 transcribe / speech（形状和现有一次性接口一致，风险最小），验证 `source = gateway`、实际上游透传、计量。
- 再接一个流式聚合：SpeechRouter 或 LiveKit Inference 二选一，验证会话内 failover 和 `RouteSelected`。
- Session 握手支持「先换 token 再连」，为浏览器端和 Vercel Realtime 一类 provider 铺路。
- 验收：切换到网关 provider 只需改 `provider` / `model` / `api_key`，应用代码不动，契约测试同样通过。

### V4 TTS 双工与对齐（可与 V2 并行）

计划落地迭代：v2.1。

- 接通三个方言的 `start_duplex_stream`（目前全部返回 `UnsupportedOperation`）。输入按业界形态定为「追加文本 / Flush / 取消 / 关闭」，输出音频分片带 `context_id` 与 `flush_id`。
- 按 ADR-0006 D7.5 的 TTS 对照落实事件。
- Python / Node 暴露 `synthesize()`、流式合成与双工合成，沿用 V1 的事件结构和 V2 的字符计量。
- 验收：用 LLM 逐字输出驱动双工合成，三个方言都能边输入边出音频；中途取消后不再收到旧上下文的音频。

### V5 全双工补齐（在 V1 之后）

计划落地迭代：v2.2。

- omni 补输出侧生命周期：用户开始说话、模型本轮生成完、被打断、本轮结束，并写进契约顺序（生成完或被打断，之后必有本轮结束）。
- `SessionInput::Interrupt` 增加可选 `played_ms`，截断到已播放位置。
- 轮次检测便携选项：服务端 VAD / 语义 VAD / 手动，外加自动回复开关；能力目录声明。
- 接第二个全双工 provider（OpenAI Realtime 或 Gemini Live），验证映射层不是只为火山定制的。
- Python / Node 暴露 `RealtimeSession`。
- 验收：同一段应用代码切换两个 provider 都能完成「说话 → 回复 → 打断 → 历史只含已播部分」。

## 不做的事与开放问题

不做：

- Orchest 自己不做中转站（鉴权、预算、请求日志这些网关功能留给下游或第三方网关）。
- 不做端侧 VAD 实现；Orchest 只提供 `finalize()`，VAD 由应用自己接。
- 不做附和词过滤、自适应打断和管线编排；这些留给 Pipecat / LiveKit 一类框架，Orchest 只提供完整信号。

开放问题：

- V3 的流式聚合先选 SpeechRouter 还是 LiveKit Inference？取决于哪个在下游（如 Murmur）里更可能用到。
- Vercel AI Gateway 的流式转写协议能否给出 Draft / Stable / Turn，还没有核实。
- V5 的第二个全双工 provider 选 OpenAI Realtime 还是 Gemini Live。

## 来源

- 设计：[ADR-0006](../../adr/0006-voice-session-model.md)（来自 owner 的语音栈设计稿）
- [Vercel AI Gateway](https://vercel.com/docs/ai-gateway)、[Realtime Quickstart](https://vercel.com/docs/ai-gateway/getting-started/realtime)、[Speech Quickstart](https://vercel.com/docs/ai-gateway/getting-started/speech)、[Modalities](https://vercel.com/docs/ai-gateway/modalities)
- 代码：`crates/orchest-protocol/src/stream.rs`、`capability.rs`、`crates/orchest-provider-core/src/catalog.rs`、`pricing.rs`、`crates/orchest-provider-stream/src/{asr,tts}/`、`omni.rs`、`python/orchest/__init__.pyi`
