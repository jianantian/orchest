# Audio-Native Agent 产品横向研究

> **阅读前提**：本文分析的是业界 audio-native / voice agent 产品与平台，覆盖从模型级 API 到全栈平台。Orchest 是底层 agent runtime SDK，两者的关系是：Orchest 可以成为这些平台中 "LLM reasoning + tool calling" 那一层的引擎，但 Orchest 本身不做音频处理。
>
> 本文聚焦于架构模式、集成点、以及 Orchest 作为 runtime 在这些栈中的定位。

---

## 一、市场全景与分类

2026 年 voice AI agent 市场有 50+ 活跃厂商，按抽象层次分为 5 层：

| 层级 | 定位 | 代表产品 | 与 Orchest 的关系 |
|------|------|---------|------------------|
| **L1: 语音模型 API** | 提供 speech-to-speech 或 speech-to-text 基础能力 | OpenAI GPT-Realtime-2, Gemini Live, Hume EVI 3, Kyutai Moshi | Orchest 的 ModelAdapter 可对接其 reasoning 层 |
| **L2: 组件 API** | 提供 STT / TTS 单组件能力 | Deepgram (STT), ElevenLabs (TTS), Cartesia Sonic-3 (TTS), AssemblyAI (STT) | ASR Gateway 对接 STT；TTS Gateway 对接 TTS |
| **L3: 编排框架** | 把 STT + LLM + TTS 拼成 pipeline | LiveKit Agents, Pipecat, Vocode, TEN Framework | Orchest 作为 LLM 层的 runtime |
| **L4: 全栈平台** | 一站式 voice agent 构建与托管 | Vapi, Retell AI, Bland AI, ElevenLabs Conversational AI, Play.ai | Orchest 可作为 bring-your-own-runtime 选项 |
| **L5: 垂直行业方案** | 面向特定行业的 voice agent 产品 | Inworld AI (gaming), 3CLogic (contact center), Kore.ai (enterprise CX) | Orchest 不直接竞争 |

---

## 二、两种架构范式

### 2.1 Cascaded Pipeline（级联管道）

```
Audio → VAD → STT → LLM → TTS → Audio
         ↓       ↓      ↓      ↓
      端点检测  文本   推理   语音合成
```

**代表**: Vapi, Retell AI, LiveKit Agents, Pipecat, AssemblyAI Voice Agent API

**优势**:
- 每层独立选型、独立扩缩、独立调试
- 文本是天然的可审计中间层（合规、日志、内容审核）
- tool calling 在 LLM 层原生支持
- 组件级 fallback（STT A 挂了切 STT B）

**劣势**:
- 串行延迟叠加：STT 300ms + LLM 800ms + TTS 200ms ≈ 1.3s
- 文本中介丢失语音的情感信息（除非额外做 emotion extraction）
- 三轮序列化/反序列化增加工程复杂度

**2026 市场占比**: ~85%，仍是生产环境主流选择。

### 2.2 End-to-End Speech-to-Speech（端到端语音）

```
Audio → [Single Neural Model] → Audio
```

**代表**: OpenAI GPT-Realtime-2, Gemini Live (native audio), Hume EVI 3, Kyutai Moshi, Sesame CSM-1B

**优势**:
- 延迟极低：200-300ms vs cascaded 的 ~1.5s（最高 85% 延迟降低）
- 情感/韵律保留完整
- 架构简单，没有中间协议

**劣势**:
- 黑盒调试困难：不知道是"听错了"还是"说错了"
- 无文本审计 trail，合规场景受限
- tool calling 需要模型内部处理，不同模型能力差异大
- 缺乏成熟的 observability 和 evaluation 工具

**2026 市场占比**: <15%，增长中但受限于合规和调试需求。

### 2.3 混合架构（Hybrid）

部分平台开始探索中间路线：

- **ElevenLabs Conversational AI**: STT 独立（可用 Deepgram 或自家 Scribe），LLM + TTS 紧耦合（自家的对话引擎 + 语音合成），情感从音频提取后作为 metadata 注入 LLM prompt
- **AssemblyAI Voice Agent API**: 专用 STT pipeline（Universal-3 Pro Streaming）→ 独立 LLM → 专用 TTS，但在 STT 层做 speaker diarization + emotion detection，把语音信息以结构化标签注入 LLM

这是 Orchest 最值得关注的模式：**ASR Gateway 提供富文本（text + emotion/speaker metadata），Orchest runtime 做 LLM reasoning + tool calling，TTS Gateway 生成语音**。

---

## 三、平台逐一分析

### 3.1 L1: 语音模型 API

#### OpenAI GPT-Realtime-2（2026-05 发布）

**架构**: 原生 speech-to-speech，WebSocket/WebRTC/SIP 接入。家族包括 `gpt-realtime-2`（旗舰）、`gpt-realtime-translate`（70+ 语言实时翻译）、`gpt-realtime-whisper`（超低延迟流式转写）。

**定价**: GPT-4o $0.65/hr，GPT-4o-mini $0.16/hr。

**值得注意的设计**:
- **VAD 内置**: 模型原生处理语音活动检测，不需要外部 VAD
- **Function calling 原生**: 在语音对话中支持 tool calling，这是端到端模型中少有的
- **多模态扩展**: 同一 WebSocket 连接可同时发送音频和图片（如拍照+说话）
- **SIP 支持**: 直接接电话线，不需要中间网关

**对 Orchest 的启示**: GPT-Realtime-2 是目前最完整的端到端方案，但它不解决"多个 ASR 供应商路由"的问题。Orchest 的 ASR Gateway 和它不冲突——当用户选择 OpenAI 作为语音接口时，Gateway 不介入；当用户需要 Soniox + Claude 的组合时，Gateway 做 STT→text 转换。

#### Google Gemini Live（2026 I/O 发布）

**架构**: 原生 speech-to-speech API，基于 Gemini 2.5 系列。通过 Gemini API 和 Google ADK 暴露，可与 Pipecat/Twilio/WebRTC 集成。

**值得注意的设计**:
- **Project Astra 的工程化落地**: Google 把 Astra 的研究能力（多模态、记忆、实时交互）工程化为 Live API
- **Android/iOS 原生集成**: 在 Gemini app 里可用，不是纯 API 产品
- **与 ADK 深度耦合**: Google 的 Agent Development Kit 直接集成 Live API，voice agent 和 tool calling 一起用

**对 Orchest 的启示**: Gemini Live 的 native audio 模式同样不经过文本——如果 Orchest 未来对接 Gemini，`ModelAdapter` 需要支持 `ContentBlock::InputAudio`，跳过 ASR Gateway。

#### Hume AI EVI 3（2025 发布）

**架构**: 端到端 speech-to-speech，核心卖点是情感智能——模型在推理时同时分析语音中的情感信号（语调、停顿、音高），生成情感匹配的语音响应。

**技术栈**: expression measurement models（面部/声音/语言情感分析）→ EVI speech-language model → Octave TTS 引擎。

**值得注意的设计**:
- **情感不是后处理，是模型推理的一部分**: 模型在看到音频 token 时同步生成情感标签，不是先 STT→文本→情感分析
- **语音人格可指令化**: EVI 3 支持用 prompt 定义 voice personality，不仅是音色，还包括说话风格、情绪倾向
- **API 三层**: Expression Measurement API（纯情感分析）、EVI API（speech-to-speech + 情感）、Octave API（纯 TTS）

**对 Orchest 的启示**: Hume 的情感提取值得在 ASR Gateway 中作为可选 metadata 补充——即使 Orchest 用 cascaded pipeline，也可以在 STT 层并行跑情感分析，把结果作为 system prompt 的一部分注入 LLM。

#### Kyutai Moshi（开源，Apache 2.0）

**架构**: 开源 speech-to-speech 模型，24kHz 音频 → Mimi acoustic tokenizer（压缩到 ~1.1kbps）→ Transformer → 同时输出文本和语音。端到端延迟 ~80ms。

**值得注意的设计**:
- **极低延迟**: 80ms 是目前公开数据中最快的，因为 tokenizer 把音频压缩到极低比特率
- **文本和语音并行输出**: 模型同时生成 transcript 和 audio，这意味着可审计性比纯端到端好
- **仅 7B 参数**: 比 GPT-Realtime-2 小得多，但推理质量也相应下降

**对 Orchest 的启示**: Moshi 的并行文本+语音输出是一个聪明的折中——既保留端到端的低延迟，又有可审计的文本。如果未来 Orchest 要支持 speech-to-speech，这个模式比纯端到端更合适。

### 3.2 L2: 组件 API

这一层与 [ASR Vendor Landscape](./asr-vendor-landscape.md) 高度重叠，此处只补充 voice agent 特定视角。

#### Deepgram（STT 专精）

**在 voice agent 栈中的定位**: STT + 端点检测（endpointing）+ VAD。不做 LLM，不做 TTS，专注"语音→文本"。

**Voice agent 关键能力**:
- **Smart Endpointing**: 判断用户是否说完（不是简单静音检测，而是语义+韵律综合判断）
- **Interim Results**: 流式 partial transcript，支持 agent 在用户说完之前就开始推理（barge-in 的前置条件）
- **Utterance End 事件**: 告诉 agent "用户说完了，现在可以回复了"

**对 Orchest 的启示**: ASR Gateway 的 `TranscribeStreamItem` 需要区分 `Partial`（用户还在说）和 `Final`（用户说完了），这是 voice agent turn-taking 的基础信号。

#### ElevenLabs（TTS + Conversational AI 平台）

**在 voice agent 栈中的定位**: L2（TTS API）+ L4（Conversational AI 全栈平台）。

**TTS 能力**: Scribe v2（STT，$0.39/hr）+ 语音合成（多语言、情感控制、voice cloning）。

**Conversational AI 平台**: 完整的 voice agent builder，包括自定义 prompt、tool calling、RAG、guardrails、telephony integration。

**定价**: TTS ~$0.10/min；Conversational AI 平台按用量。

**对 Orchest 的启示**: ElevenLabs 从 TTS 起家，向上扩展到全栈 voice agent。它的 Conversational AI 平台的 LLM 推理 + tool calling 部分，理论上是 Orchest 可以替代的——如果 ElevenLabs 开放 bring-your-own-runtime。

#### Cartesia Sonic-3（TTS 专精）

**架构**: State-space model（SSM）驱动的流式 TTS，不是传统的 transformer + vocoder。第一个字符到第一个音频样本仅 90ms。

**Voice agent 关键能力**:
- **流式优先**: WebSocket/SSE 接口，不等待完整文本就能开始合成
- **情感控制**: 笑声、叹息、语气词——不是后处理，是模型原生能力
- **40+ 语言**: 包括新兴市场语言

**对 Orchest 的启示**: Cartesia 的 90ms 首字节延迟意味着 TTS 不再是 cascaded pipeline 的瓶颈。整个栈的延迟天花板在 LLM reasoning。这对 Orchest 的意义：**如果 LLM 推理足够快，cascaded pipeline 的整体延迟可以接近端到端**。

### 3.3 L3: 编排框架

#### LiveKit Agents

**架构**: WebRTC-first 的 voice agent 框架。Agent 作为 LiveKit room 的参与者，通过 WebRTC 收发音频。内置 STT/LLM/TTS 插件，但也支持外部 provider。

**核心设计**:
- **Agent 是 room participant**: 不是服务器端回调，而是 room 内的"虚拟人"
- **Sequential pipeline**: STT → LLM → TTS 顺序执行，每一层可换 provider
- **LiveKit Inference**: 可选的在 LiveKit Cloud 上直接跑模型（减少网络往返）
- **SIP bridge**: 电话线和 WebRTC room 之间互转
- **低代码 Agent Builder**: 可视化配置 voice agent 行为（非开发者入口）

**延迟**: 典型 750-900ms（含网络）。

**定价**: 开源框架免费；LiveKit Cloud 按 room 时长 + 带宽。

**对 Orchest 的启示**: LiveKit Agents 的 "agent as room participant" 模型和 Orchest 的 "agent run lifecycle" 模型是互补的——LiveKit 管音频会话，Orchest 管推理+工具调用。可以在 LiveKit agent 的 LLM 层嵌入 Orchest，让 voice agent 获得 Orchest 的 tool dispatch、approval gate、budget guard。

#### Pipecat

**架构**: 开源 Python 框架，做实时语音 + 多模态 agent 的编排。Transport 抽象（WebRTC、Twilio、WebSocket）→ Pipeline（VAD → STT → LLM → TTS）→ 输出。

**核心设计**:
- **Transport 可插拔**: WebRTC、Twilio Media Streams、Daily、自定义 socket
- **"Skills" 抽象**: 可复用的 agent 行为模块（类似 Orchest 的 Skill，但 Pipecat skill 是 Python 函数而非文件系统包）
- **本地 + 云端部署**: `pip install pipecat` 开发 → Pipecat Cloud CLI 部署
- **与 Claude Code 集成**: 用 Claude Code scaffold 新的 Pipecat skill

**对 Orchest 的启示**: Pipecat 的 Transport 抽象和 Orchest 的 `ModelAdapter` 是同一层次的不同关注点——Pipecat 抽象音频传输，Orchest 抽象模型推理。两者可以叠加：Pipecat 处理音频进出，Orchest 处理 LLM reasoning。Pipecat 用 Python 的灵活性 vs Orchest 用 Rust 的性能+类型安全——这是设计哲学的差异，不是功能重叠。

#### TEN Framework

**架构**: 开源 voice agent 框架，主打低延迟和扩展性。C++ 核心 + Python/Go/JS 扩展。

**对 Orchest 的启示**: 与 LiveKit/Pipecat 同类，属于编排层。Orchest 不与它竞争——它管音频 IO，Orchest 管推理 loop。

### 3.4 L4: 全栈平台

#### Vapi

**定位**: "Voice agent 的 Vercel"——开发者通过 API 定义 agent 行为，Vapi 处理底层所有组件（telephony、STT、LLM、TTS、failover）。

**架构**: 组件可换（自带 STT/LLM/TTS key），Vapi 做 orchestration + telephony + monitoring。

**定价**: 按分钟计费，包含 telephony。典型 $0.10-0.30/min 全包。

**值得注意的设计**:
- **Bring-your-own-key (BYOK)**: 用你自己的 Deepgram/ElevenLabs/OpenAI key，Vapi 只收平台费
- **Server-side tool calling**: 在 Vapi 后台定义 tool schema，Vapi 执行 HTTP 回调
- **实时监控面板**: latency breakdown（STT 耗时、LLM 耗时、TTS 耗时）

**对 Orchest 的启示**: Vapi 的 tool calling 是最简单的 HTTP 回调模式——没有 agent loop，没有 budget guard，没有 approval gate。这是 Orchest 可以填补的空白：Vapi 用户如果想用更复杂的 agent runtime（多步推理、skill 系统、approval gate），可以用 Orchest 替代 Vapi 的内置 LLM 调度。

#### Retell AI

**定位**: 全栈 voice agent 平台，内置 SIP 电话、IVR 路由、可视化 agent builder。

**架构**: 级联 pipeline（默认 STT → LLM → TTS），但内置了所有组件，不暴露组件选择。也支持 BYOK。

**延迟**: ~780ms real-world。

**定价**: 按分钟，比 Vapi 略高。

**值得注意的设计**:
- **Visual Agent Builder**: 拖拽式定义对话流程（类似 IVR 设计工具），非开发者友好
- **SIP native**: 自带电话基础设施，不需要 Twilio
- **Human handoff**: AI → 人工的无缝转接

**对 Orchest 的启示**: Retell 的视觉化 builder 是产品层，不是 SDK 层。Orchest 不需要这种 UI，但 Retell 的 human handoff 模式对 Orchest 的 sub-agent handoff 设计有参考价值——agent 可以 `handoff_to_human(summary)` 而不是只是 `handoff_to_agent`。

#### Bland AI

**定位**: 全栈 voice agent，强调对话智能——内置数据库查询、动态报价、上下文保留的人工转接。

**架构**: 紧耦合 pipeline，内置 STT/LLM/TTS。

**定价**: 按分钟，在三个全栈平台中最便宜。

**值得注意的设计**:
- **Pathways**: 对话流程图，比 Retell 的 builder 更偏向开发者（代码+流程图混合）
- **Live context injection**: 对话中实时从外部数据库拉数据（如 CRM、库存），作为 prompt context 注入
- **Voice cloning**: 内置，不需要 ElevenLabs

**对 Orchest 的启示**: Bland 的 live context injection 是一个值得关注的模式——agent loop 中不仅是 tool calling 的结果可以注入，外部 context source（数据库、API）的实时数据也应该能注入 system prompt。Orchest 可以通过 hook 框架（v0.7）实现类似的 "context augmentation" 机制。

#### AssemblyAI Voice Agent API（2026-04 发布）

**定位**: 从 STT 起家，向上扩展到 voice agent API。不做 LLM 推理——用户自带 LLM。

**架构**: 专用 STT pipeline（Universal-3 Pro Streaming + diarization + emotion detection）→ 用户自行处理 LLM → AssemblyAI TTS。

**定价**: $4.50/hr 全包（STT + TTS），不含 LLM 成本。

**值得注意的设计**:
- **不做 LLM**: 区别于 Vapi/Retell/Bland，AssemblyAI 明确 stay in their lane——只做 speech 处理
- **富文本输出**: STT 不只是 text，还带 speaker labels、emotion scores、confidence
- **与任何 LLM 配合**: 用户自己选 LLM（OpenAI、Anthropic、开源模型），AssemblyAI 只处理进出

**对 Orchest 的启示**: AssemblyAI Voice Agent API 是 Orchest 最自然的合作伙伴——AssemblyAI 做语音处理，Orchest 做 LLM reasoning + tool calling。两者没有重叠，拼在一起就是一个完整的 voice agent stack。

---

## 四、Voice Agent 基础设施栈

```
┌─────────────────────────────────────────────┐
│              TELEPHONY / TRANSPORT           │
│  Twilio · SignalWire · Plivo · SIP · WebRTC │
└──────────────────┬──────────────────────────┘
                   │
┌──────────────────▼──────────────────────────┐
│              ORCHESTRATION (L3)              │
│  LiveKit Agents · Pipecat · Vocode · TEN     │
│  → audio session · turn-taking · barge-in   │
└──────────────────┬──────────────────────────┘
                   │
       ┌───────────┴───────────┐
       │                       │
┌──────▼──────┐          ┌─────▼──────┐
│  STT (L2)   │          │ TTS (L2)   │
│  Deepgram   │          │ ElevenLabs │
│  Soniox     │          │ Cartesia   │
│  Speechmatics│         │ Play.ht    │
│  AssemblyAI │          │ OpenAI TTS │
└──────┬──────┘          └─────▲──────┘
       │                       │
       │    ┌──────────────┐   │
       └───►│  LLM RUNTIME │───┘
            │  (Orchest)   │
            │              │
            │ Agent Loop   │
            │ Tool Calling │
            │ Skill System │
            │ Budget Guard │
            │ Approval Gate│
            └──────────────┘
```

Orchest 在这个栈中的位置是 **LLM Runtime**——它是 cascaded pipeline 中 "推理 + 工具调用" 的那一层。它不替代编排框架，也不替代 STT/TTS 组件，而是替代 Vapi/Retell/Bland 内置的 LLM 调度逻辑。

---

## 五、开源 vs 商业全景

| 层级 | 开源方案 | 商业方案 |
|------|---------|---------|
| **S2S 模型** | Kyutai Moshi (Apache 2.0), Sesame CSM-1B | OpenAI GPT-Realtime-2, Gemini Live, Hume EVI 3 |
| **STT** | Whisper (MIT), NVIDIA Riva | Deepgram, Soniox, Speechmatics, AssemblyAI |
| **TTS** | Coqui TTS, Piper, XTTS v2 | ElevenLabs, Cartesia Sonic-3, OpenAI TTS, Play.ht |
| **编排框架** | LiveKit Agents (Apache 2.0), Pipecat (BSD), TEN (Apache 2.0), Vocode (MIT) | Vapi, Retell AI, Bland AI |
| **Agent Runtime** | **Orchest** (planned), LangGraph, CrewAI | OpenAI Agents SDK, Google ADK |

> 开源 S2S 模型（Moshi、CSM-1B）质量仍落后商业方案一代以上，但适合本地部署、隐私敏感场景、以及需要 fine-tune 的特定领域。

---

## 六、关键指标对比

| 平台 | 架构 | 典型延迟 | 定价模式 | Tool Calling | Telephony | Open Source |
|------|------|---------|---------|-------------|-----------|------------|
| **OpenAI GPT-Realtime-2** | E2E S2S | ~250ms | $0.16-0.65/hr | 原生 | SIP/WebRTC | No |
| **Gemini Live** | E2E S2S | ~300ms | GCP 计费 | ADK 集成 | Pipecat/WebRTC | No |
| **Hume EVI 3** | E2E S2S + Emo | ~300ms | 按分钟 | Limited | No | No |
| **Moshi** | E2E S2S | ~80ms | Free (Apache 2.0) | No | No | Yes |
| **Deepgram** | STT only | ~300ms STT | $0.46-0.55/hr | N/A | Via partner | No |
| **Cartesia Sonic-3** | TTS only | ~90ms TTFB | 按字符 | N/A | Via partner | No |
| **ElevenLabs Conv AI** | Cascaded | ~1.2s | ~$0.10/min | 内置 | 内置 | No |
| **AssemblyAI Voice Agent** | Cascaded | ~1s | $4.50/hr (STT+TTS) | 用户自带 | Via partner | No |
| **Vapi** | Cascaded | ~1s | $0.10-0.30/min | HTTP 回调 | 内置 | No |
| **Retell AI** | Cascaded | ~780ms | 按分钟 | 内置 | 内置 SIP | No |
| **Bland AI** | Cascaded | ~900ms | 按分钟（最低） | 内置 | 内置 | No |
| **LiveKit Agents** | Cascaded | 750-900ms | 开源 + Cloud 按量 | 内置 | SIP bridge | Yes (Apache 2.0) |
| **Pipecat** | Cascaded | ~1s | 开源 + Cloud 按量 | 内置 | Via transport | Yes (BSD) |

---

## 七、跨平台共同模式

### 7.1 Turn-taking 是所有人的痛点

每个平台都在解决 "用户什么时候说完了" 的问题：
- **Deepgram** 的 Smart Endpointing 是纯 STT 层解决
- **OpenAI Realtime** 的 VAD 内置在模型里
- **Vapi/Retell** 在编排层做 turn-taking 状态机
- **Pipecat** 通过 pipeline 的 `EndOfSpeech` 事件触发 LLM 推理

**对 Orchest 的启示**: Turn-taking 不应该是 Orchest 的问题。Orchest 只需要知道 "现在有一句完整的话，开始推理"。谁来决定 "话是否完整"——ASR Gateway 或编排层——与 Orchest 无关。

### 7.2 Tool Calling 是分水岭

**Cascaded 平台**的工具调用在 LLM 层自然支持（因为 LLM 已经是 text-in/text-out）。这是 cascaded 架构最大的护城河——你可以用最强大的 reasoning model（Claude, GPT-5）配合工具系统。

**端到端平台**的工具调用取决于模型本身。GPT-Realtime-2 支持 function calling，但 Moshi 不支持，Gemini Live 通过 ADK 间接支持。

**对 Orchest 的启示**: Orchest 的 tool system（trait Tool, ToolRegistry, approval gate, budget guard）在 cascaded pipeline 中有直接价值。如果使用端到端模型（GPT-Realtime-2），则 Orchest 的 tool 系统不适用——模型自己调度工具。

### 7.3 富上下文注入是差异化竞争力

所有成熟平台都在做 "不只是 transcription"：
- **AssemblyAI**: STT 输出带 speaker labels + emotion scores
- **Hume**: 情感信号是模型推理的一部分
- **Bland**: live context injection（CRM、库存）→ prompt 增强
- **ElevenLabs**: 语音 metadata（语速、音量）影响 TTS 表达

**对 Orchest 的启示**: Orchest 的 hook 框架（v0.7）应该是这个能力的注入点——hook 可以在每次 LLM call 前从外部数据源拉上下文，增强 system prompt。

### 7.4 可观测性是生产瓶颈

所有平台的用户反馈都指向同一个问题：**voice agent 出错了，但不知道是 STT 听错了、LLM 理解错了、还是 TTS 说错了**。

**Cascaded pipeline 的优势在这里**: 每层有独立日志。AssemblyAI 的 monitoring dashboard 显示 latency breakdown by stage。Vapi 也类似。

**端到端 pipeline 的劣势**: 黑盒——不知道问题在哪一层，只能换模型或调 prompt。

**对 Orchest 的启示**: Orchest 的 `RuntimeEvent` 事件流已经有良好的可观测性基础。在 voice agent 集成中，需要把 ASR 的 telemetry（WER, latency, confidence）和 Orchest 的 telemetry（token usage, tool call trace）在同一个 timeline 上对齐——这需要一个统一的 `trace_id` 贯穿 ASR Gateway → Orchest → TTS Gateway。

---

## 八、对 Orchest 的具体影响

### 8.1 立即可做（不阻塞主线）

1. **ASR Gateway 卫星 crate** (`agent-runtime-asr-providers`) — 独立于 v0.7/v0.8/v0.9 主线，对标 AIGC Gateway 模式
2. **`RuntimeEvent` 加 ASR 相关事件** — `AsrPartialTranscript`, `AsrFinalTranscript`, `AsrError`，让 voice agent 构建者能消费统一的 event stream

### 8.2 中期规划（v0.8-v0.9）

3. **Hook 框架支持 context augmentation** — 允许 hook 在 LLM call 前注入外部数据（ASR emotion metadata、CRM context、用户历史等）
4. **统一 trace_id** — 横跨 ASR Gateway → Orchest → TTS Gateway 的请求追踪

### 8.3 远期规划（v1.0+）

5. **`ContentBlock::InputAudio`** — 支持多模态模型的 audio input
6. **Voice Agent 参考实现** — 展示 Pipecat/LiveKit + Orchest 的完整集成

### 8.4 明确不做

- **不做编排层**: audio session management、turn-taking、barge-in 是 LiveKit/Pipecat 的事
- **不做 STT/TTS**: 通过 Gateway trait 对接，不内置
- **不做 Telephony**: SIP/Twilio/WebRTC 传输是基础设施层
- **不做 Voice Activity Detection**: ASR 供应商自带，Gateway 只封装差异

---

## 九、总结

Voice agent 市场在 2026 年处于 cascaded pipeline 主导、端到端 S2S 快速增长的技术过渡期。关键发现：

1. **Cascaded pipeline 不会很快消失**——文本审计、tool calling、组件独立性在企业和合规场景是刚需
2. **Orchest 的自然位置是 cascaded pipeline 的 LLM runtime 层**——替代 Vapi/Retell/Bland 内置的简单 LLM 调度，提供 agent loop + tool system + skill + budget + approval 的完整 runtime
3. **最自然的合作伙伴是 AssemblyAI**（不做 LLM）+ **LiveKit/Pipecat**（做编排，不做推理）——Orchest 填补它们空缺的 agent runtime 层
4. **端到端模型（GPT-Realtime-2, Gemini Live）是 Orchest 的非目标场景**——它们自己处理推理和工具调用，Orchest 的价值在 cascaded pipeline
5. **统一可观测性**（trace_id 贯穿 ASR → LLM → TTS）是生产部署的最大差异化，Orchest 的 event stream 是天然优势
