# Audio-Native Agent — 完整架构设计

**Status**: draft — 综合 ASR 供应商选型、voice agent 产品研究、genui 输出模型后的完整架构方案。
**覆盖范围**: 语音输入 → ASR 处理 → Agent 推理 → 多模态输出 → 语音合成的全链路。

---

## 一、总览：全链路架构

```
                              ┌─────────────────────┐
                              │   用户               │
                              │   说话 / 打字        │
                              └─────────┬───────────┘
                                        │
                    ┌───────────────────┴───────────────────┐
                    │                                       │
              ┌─────▼──────┐                          ┌─────▼──────┐
              │  IME 层     │                          │ Voice Input │
              │ (通用输入法) │                          │  Adapter    │
              │             │                          │             │
              │ 场景: 所有app│                          │ 场景: voice │
              │ 输出: 纯文本 │                          │  agent 产品  │
              └─────┬──────┘                          │ 输出: 富文本│
                    │                                  │ + metadata │
                    │ 文本                              └─────┬──────┘
                    │                                        │
                    └────────────┬───────────────────────────┘
                                 │ Message { text, voice_meta }
                                 │
                    ┌────────────▼───────────────────────────┐
                    │        Orchest Agent Runtime            │
                    │                                        │
                    │  Agent Loop → Reasoning → Tool Calling  │
                    │  Hook 框架注入 voice_meta 上下文        │
                    │  OutputComponent 选择 + 填充            │
                    │                                        │
                    │  产出: Vec<ContentBlock>                │
                    │  [Text, Spoken, Image, Component, ...]  │
                    └────────────┬───────────────────────────┘
                                 │
                    ┌────────────▼───────────────────────────┐
                    │           Showroom Engine               │
                    │                                        │
                    │  ComponentCatalog 映射                  │
                    │  ContentBlock variant → 渲染器路由      │
                    │  Text→Markdown, Spoken→TTS,             │
                    │  Image→Viewer, Component→React/Lit/...  │
                    └────────────┬───────────────────────────┘
                                 │
                    ┌────────────┴───────────┐
                    │                        │
              ┌─────▼──────┐          ┌─────▼──────┐
              │  屏幕       │          │  扬声器     │
              │  Text/Image │          │  TTS 语音   │
              │  /Component │          │             │
              └────────────┘          └─────────────┘
```

---

## 二、语音输入：两种场景，两种整合深度

### 场景 A：手机输入法 — IME 层独立

**结论：ASR 停留在 IME 层，Agent 只看到文本。**

IME 是通用基础设施——用户在任何 app（微信、邮件、Agent）里都用一个输入法。语音识别对所有 app 是共享能力。Agent 不关心输入来源是拼音、五笔还是语音——它只收到文本。

**这条线与 Orchest 无关。** IME 的 ASR 选型、热词纠错、联系人召回都是 IME 产品的事。

### 场景 B：Voice Agent 产品 — Agent 需要感知渠道

**结论：Voice Input Adapter 在 Agent 侧做轻量整合。Agent 知道用户用了语音，但不需要原始音频。**

之所以不是纯 IME 透明化：Agent 的回复策略取决于输入渠道。同样的提问，文字输入可以回复 2000 字报告 + 图表，语音输入应回复口语摘要 + 屏幕展示详细内容。Agent 需要知道渠道才能做这个路由决策——渲染层做不了，它不理解内容。

```rust
pub struct VoiceInputMeta {
    pub transcript: String,
    pub confidence: f32,
    pub language: Option<String>,
    pub emotion: Option<EmotionHint>,
    pub background: Option<NoiseLevel>,
    pub is_final: bool,
}
```

`VoiceInputMeta` 通过 Hook 框架（v0.7）注入 system prompt，让 Agent 自动化产出合适的输出策略，而不是靠 prompt engineering 提醒。

**Agent 不需要处理原始 PCM 音频。** 几百毫秒的音频数据对 LLM 没用。结构化 metadata 才是 Agent 需要的。

---

## 三、ASR Gateway：供应商抽象层

**位置**：在 Agent loop 之前。独立卫星 crate —— `agent-runtime-asr-providers`。

**对标**：v0.6.1 的 AIGC Gateway（`agent-runtime-aigc-providers`），同样的 provider abstraction 模式。

```rust
#[async_trait]
pub trait AsrProvider: Send + Sync {
    fn provider_name(&self) -> &str;

    async fn transcribe(
        &self,
        audio: AudioInput,
        options: &TranscribeOptions,
    ) -> Result<TranscribeResult, AsrError>;

    async fn stream_transcribe(
        &self,
        audio: AudioStream,
        options: &TranscribeOptions,
    ) -> Result<TranscribeStream, AsrError>;
}
```

**多供应商路由**（与 [ASR Vendor Landscape](./asr-vendor-landscape.md) 联动）：

```
中文/国内网络 → 阿里云/火山引擎/腾讯云
英语 + 新兴市场口音 → Deepgram / ElevenLabs / Soniox / Speechmatics
Code-switching → Soniox / Deepgram / Speechmatics
高实时语音入口 → Deepgram Flux / ElevenLabs Scribe v2
成本兜底 → Soniox / Speechmatics Standard
```

路由决策基于语言、网络区域、成本上限、延迟要求——不硬编码到 Agent loop。

**为什么 ASR 不是 Tool**：Tool 是模型在 loop 中决定调用的。ASR 发生在模型看到文本之前——它是输入预处理。

**为什么 ASR 不是 Skill**：Skill 的过程性知识。ASR 是能力。

---

## 四、Agent Runtime：输出模型 — 组件声明式

核心设计原则经过两次修正：

1. ~~`Vec<ContentBlock>` 平铺~~ → 图文混排时丢失结构语义
2. ~~`Document` + `Group` + `LayoutHint` 抽象树~~ → 布局原语模拟不了语义
3. **`OutputComponent` 组件声明式** → 组件名 = 语义，props = 数据，和 Tool 对称

### OutputComponent trait（与 Tool 对称）

```rust
// Tool: 模型调用的能力（输入侧）
pub trait Tool {
    fn metadata(&self) -> ToolMetadata;        // name, description, input_schema
    async fn execute(&self, input: Value) -> ToolOutput;
}

// OutputComponent: 模型产出的内容（输出侧）
pub trait OutputComponent {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn props_schema(&self) -> JsonSchema;
    async fn render(&self, props: Value) -> RenderOutput;
}
```

同一个注册/发现/系统提示注入模式，方向相反。

### ContentBlock：叶子节点

```rust
pub enum ContentBlock {
    Text { text: String },
    Spoken { text: String, voice: Option<String> },
    Image { asset_id: String, alt: String, ... },
    Audio { asset_id: String, kind: AudioKind, duration_ms: u64 },

    // 组件引用——genui 的入口
    Component {
        component_name: String,      // "ComparisonCard", "DataTable"
        props: Value,
        children: Vec<ContentBlock>,
    },

    Thinking { .. },
    ToolUse { .. },
    ToolResult { tool_use_id: String, content: Vec<ContentBlock> },
}
```

`Component` variant 是 genui 的全部能力——组件名携带语义，props 携带数据，children 携带子内容。Showroom 按 `component_name` 查找渲染器。

### 流式协议

```
StreamEvent::ComponentOpen { name: "ComparisonCard", props: { title: "Q1 vs Q2" } }
StreamEvent::ComponentChild { index: 0, block: Text("Q1 强劲") }
StreamEvent::ComponentChild { index: 1, block: Image("chart_1.png") }
StreamEvent::ComponentClose
```

Showroom 收到 `ComponentOpen` → 实例化组件 → 子块逐个填充 → 实时渲染。

### 为什么 TTS 不是 Tool

TTS 是输出渲染，不是模型决策。模型不需要「决定调用 speak」——它生成 `Spoken` block，showroom 看到这个 variant 就自动路由到 TTS 引擎。这和 `println!` 一样——文本已经产出了，怎么展示是渲染层的事。

**例外**：TTS 应是 Tool，仅当模型需要生成「作为产物的音频」——比如「把这段 podcast 脚本录制成 MP3 文件」。此时 TTS 是内容生成能力，和 AIGC 的图片生成同类。

### 语音输出的渠道路由

Agent 根据输入渠道自动调整输出策略：

```
文字输入 → Agent 产出:
  [Text(2000字), Image("chart_1")]

语音输入 → Agent 产出:
  [Spoken("三个关键发现。第一..."),
   Text(2000字), Image("chart_1")]
```

`voice_meta` 通过 Hook 注入 system prompt，Agent 不需要被 prompt engineering 提醒——这是系统级行为。

---

## 五、与业界标准的关系

2026 年 voice agent 市场形成了一套共识协议栈：

| 协议 | 层 | 角色 | Orchest 的关系 |
|------|---|------|---------------|
| **MCP** | 工具协议 | 连接外部 tool 提供方 | 已有——Tool trait 统一对接 MCP + in-process |
| **A2A** | Agent-to-Agent | 多 agent 协调 | 未来——sub-agent handoff (v0.7) |
| **AG-UI** (CopilotKit v1.2) | Agent ↔ UI 事件流 | 17 种事件类型，双向实时流 | Orchest `StreamEvent` 可映射到 AG-UI 事件 |
| **A2UI** (Google v0.9) | UI 描述格式 | 平台无关的声明式 UI JSON | `OutputComponent.render()` 的 `Opaque` 输出可以是 A2UI JSON |

Orchest 不重新发明 AG-UI 或 A2UI——它做的是 **runtime 内的组件发现与模型选择**。这和 Tool registry 在 runtime 内的角色完全对齐。

### 架构范式：Cascaded Pipeline 主导

2026 年市场 ~85% 是 cascaded（STT → LLM → TTS），端到端 S2S 不到 15%。原因是：
- 文本审计 trail（合规）
- Tool calling 在文本层原生可用
- 每层独立扩缩、替换、调试
- 成熟的可观测性工具

Orchest 的自然位置是 **cascaded pipeline 的 LLM runtime 层**——替代 Vapi/Retell/Bland 内置的简单 LLM 调度。最自然的合作伙伴是 AssemblyAI（只做 STT+TTS）+ LiveKit/Pipecat（做编排）。

---

## 六、演进路线

| 阶段 | 主题 | 输出模型 | 核心新增 |
|------|------|---------|---------|
| **1.0** | Markdown 文本 | `Vec<Text>` | `ToolResult.content: String` |
| **2.0** | 图文混排 + 语音 | 引入 `OutputComponent` trait + `Component` block | `Spoken`, `Image`, `Audio`, 内置 5 个组件（Article, Comparison, Gallery, Card, Section）。`ToolResult.content` → `Vec<ContentBlock>`。 |
| **3.0** | 可交互生成内容 | 组件获得交互能力 | `Widget`, `Canvas`, `ComponentAction` 事件（showroom → agent）。Showroom 维持组件状态。 |
| **4.0** | 3D + 空间 | 空间原语 + 空间组件 | `Scene`, `Model3D`, `SpatialAnnotation`。`OutputComponent` 组装为 `ProductShowcase`, `ArchitecturalReview` 等空间组件。 |

唯一 breaking change：`ToolResult.content` 从 `String` → `Vec<ContentBlock>`，必须在 v1.0 之前。

### 与 Orchest 主线的时序

| Orchest 版本 | 输出模型阶段 | 关键依赖 |
|-------------|------------|---------|
| v0.9 | 引入 `ContentBlock` 新 variants + `OutputComponent` trait | 不依赖 Hook/Session |
| v1.0 | 2.0 输出模型完整发布 | 含 `ToolResult.content` 迁移、breaking change 窗口关闭 |
| v1.5 | 3.0 交互输出 | 依赖 Hook 框架（v0.7）、Session（v0.8）维持组件状态 |
| v2.0 | 4.0 空间输出 | 3D 引擎成熟度 |

### 卫星 crates（独立于主线）

- `agent-runtime-asr-providers` — ASR Gateway，对标 AIGC Gateway 模式
- `agent-runtime-tts-providers` — TTS Gateway（未来）

---

## 七、明确不做的事

| 不做 | 原因 | 谁做 |
|------|------|------|
| End-to-end S2S 模型 | 文本审计 + tool calling 是 cascaded 的核心优势 | OpenAI/Gemini 自己做 |
| 音频编排（turn-taking, barge-in） | 编排层的事，不是 runtime 的事 | LiveKit Agents / Pipecat |
| Telephony（SIP/Twilio/WebRTC） | 基础设施 | Twilio / LiveKit SIP bridge |
| 渲染引擎（Markdown/React/3D） | 展示层的事 | Showroom engine |
| Voice Activity Detection | ASR 供应商自带，Gateway 只封装差异 | Deepgram / Soniox / 等 |
| 自主代码生成（Claude Artifacts 模式） | 不安全、不可审计、与极简 core 冲突 | v0 / Bolt / Claude |

---

## 八、已产出的研究文档

| 文档 | 内容 |
|------|------|
| [ASR Vendor Landscape](./asr-vendor-landscape.md) | 14 家 ASR 供应商的价格、支付、适用性 |
| [ASR Integration Architecture](./audio-native-agent-asr-integration.md) | ASR Gateway 的 crate 设计、集成点 |
| [Voice Agent Product Landscape](./audio-native-agent-products-landscape.md) | 50+ 厂商的市场全景、架构范式、跨平台模式 |
| [Output Model — GenUI Revision](./output-model-genui.md) | `OutputComponent` 组件声明式输出设计 |
| **本文** | 全链路架构综合设计 |
