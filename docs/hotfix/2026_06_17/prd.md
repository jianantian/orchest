# Hotfix 2026-06-17 PRD:LLM Catalog 信息扩展

## 背景

`crates/agent-runtime-providers/src/catalog/mod.rs` 当前的 `LlmModelEntry` 只有 6 个字段:`model_id` / `provider` / `display_name` / `context_window` / `max_output_tokens` / `pricing`。

读者(SDK 用户、产品集成方)拿不到决策选模所需的关键身份信息:

1. 是否支持 thinking,以及 thinking 内容上限
2. 输入/输出模态(text / image / video / audio)
3. 模型擅长场景(general / coding / agent / chat / reasoning)
4. 自由文本特点描述(例如"擅长数学" / "Code & Agent SOTA")
5. 输入与输出 token 上限的细分(当前只有 `context_window` 与 `max_output_tokens`,没有 `max_input_tokens`,没有 `max_thinking_tokens`)

与 `agent-runtime-model::ModelCapabilities` 的关系:

- `ModelCapabilities` 是运行时校验契约,通过 `ModelAdapter::capabilities()` 暴露
- 当前**运行时无任何消费方**(grep `caps.streaming` / `caps.tool_use` 仅在测试和 adapter 自身实现里出现),adapter 老老实实填,但没有 `validate_options_against_capabilities()` 这种共享 helper 来读它
- catalog 与 capabilities 的 `context_window` / `max_output_tokens` / `pricing` **手写两份**,改一处不会同步另一处
- `ReasoningCapability.efforts: Vec<ThinkingLevel>` 已经在 capabilities 里描述了"通用 think level + adapter 各自映射"的能力矩阵,但**给人读的视角(上限值、是否支持)在 catalog 这层缺失**

本次 hotfix **不重构 capabilities → adapter → 运行时校验**那条路径,只在 catalog 这一层补齐"给人读"的字段,让 `list_providers()` / `list_models()` 成为 SDK 用户的单一选模入口。

## 目标

1. 让 `LlmModelEntry` 携带选模所需的全部静态身份信息
2. 19 个现有 model 条目按官方文档逐条补齐新字段,不留 `TODO`
3. 现有调用方零破坏(只新增字段,不修改/不删除字段)
4. catalog 与 capabilities 的字段重叠点(`context_window` / `max_output_tokens` / `pricing`)**仍然手写两份,本次不解决**——留作 v0.11+ 工作

## 成功指标

- 调用 `list_models()` 拿到的 `LlmModelEntry` 包含:`description`、`max_input_tokens`、`thinking`、`input_modalities`、`output_modalities`、`scenes`
- 现有 19 条模型(7 anthropic + 4 openai + 2 deepseek + 6 volcengine)全部填齐新字段,源信息有 `// Source:` 注释或文档锚点
- 测试钉住关键事实(M3 模态、Anthropic thinking 上限、coding scene 命中等),防回归
- `cargo test --workspace`、`cargo clippy --workspace -- -D warnings`、`cargo fmt --check` 全过

## 范围

### 1. 新类型

`crates/agent-runtime-providers/src/catalog/mod.rs` 新增:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Modality {
    Text,
    Image,
    Video,
    Audio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelScene {
    /// 通用任务(默认)
    General,
    /// 代码场景 SOTA
    Coding,
    /// Tool use / 长程任务
    Agent,
    /// 角色扮演 / 对话陪伴
    Chat,
    /// 数学/逻辑/推理 SOTA
    Reasoning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThinkingSpec {
    /// 思考内容 token 上限。`None` = 供应商未公开具体上限
    pub max_thinking_tokens: Option<u32>,
}
```

**说明**:

- `Modality` 不含 `Vision`——多模态视觉能力通过 `input_modalities: &[Text, Image]` 表达,避免概念冗余
- `ModelScene` 一个模型可命中多个 scene(M3 同时是 Coding + Agent SOTA),所以 `scenes: &'static [ModelScene]` 是切片
- `ThinkingSpec` 当前只含 `max_thinking_tokens: Option<u32>`。`Option<ThinkingSpec>` 表"是否支持";`max_thinking_tokens: None` 表"支持但上限未公开"。两层 Option 的语义清晰

### 2. 扩展 `LlmModelEntry`

```rust
#[derive(Debug, Clone)]
pub struct LlmModelEntry {
    // === 身份 ===
    pub model_id: &'static str,
    pub provider: &'static str,
    pub display_name: &'static str,

    /// 一句话特点描述,catalog 给人读的核心字段。
    /// 例如:"Anthropic 旗舰,擅长复杂推理与代码" / "MiniMax 角色扮演专属"
    pub description: &'static str,

    // === 上下文 / 输出 / 思考(全显式,不靠减法推) ===
    pub context_window: u64,
    pub max_input_tokens: Option<u64>,
    pub max_output_tokens: Option<u32>,

    /// `None` = 不支持 thinking;`Some` = 支持,带上限值
    pub thinking: Option<ThinkingSpec>,

    // === 模态 ===
    pub input_modalities: &'static [Modality],
    pub output_modalities: &'static [Modality],

    // === 场景标签 ===
    pub scenes: &'static [ModelScene],

    // === 成本 ===
    pub pricing: Option<ModelPricing>,
}
```

### 3. 砍掉 `usd_model` / `cny_model` helper

新签名会有 12+ 参数,helper 已反生产力。改用 struct literal,字段具名,IDE 字段提示可用。

### 4. 19 个现有模型条目逐条更新

详见 issue 002-005。

### 5. 测试钉住关键事实

详见 issue 006。

## 非目标(本次不做)

- **不重构 `ModelCapabilities` ↔ catalog 的数据来源**:`context_window` / `max_output_tokens` / `pricing` 仍然在 adapter 与 catalog 各写一份。这条解耦留给 v0.11+
- **不实现 `validate_options_against_capabilities()` 共享 helper**:每个 adapter 仍各自处理 thinking 级别的降级/拒绝
- **不动 `ModelCapabilities` 字段**:不加 modalities 到 capabilities,不加 description 到 capabilities
- **不引入新 provider**:Minimax 接入是另一个独立工作流,与本 hotfix 解耦
- **不动 ASR / TTS / AIGC 的 catalog**:那些 crate 各有独立 catalog 形态,本次只动 LLM catalog
- **`Modality::Vision` / `MultiModalContentBlock` 等运行时类型不动**:本次只描述"模型支持哪些模态",不动消费这些字段的代码路径

## Issue 拆分

| Issue | 标题 | 范围 |
|-------|------|------|
| 001 | 新类型与 LlmModelEntry 扩展 | 加 `Modality` / `ModelScene` / `ThinkingSpec` 三个类型,扩 `LlmModelEntry` 6 个字段,砍 `usd_model` / `cny_model` helper |
| 002 | Anthropic 模型条目更新 | 7 个 claude 模型逐条填新字段,源信息引用 `docs/external/anthropic/models.md` |
| 003 | OpenAI 模型条目更新 | 4 个 GPT 模型逐条填,源信息引用 OpenAI 官网 |
| 004 | DeepSeek 模型条目更新 | 2 个 V4 模型逐条填,源引用 DeepSeek pricing 页 |
| 005 | Volcengine 模型条目更新 | 6 个 doubao 模型逐条填,源引用 `docs/external/volceengine/llm/` |
| 006 | catalog 测试钉住关键事实 | 加 5-8 个回归测试:模态命中、scene 命中、thinking 上限、description 非空 |

## 验收标准

- [ ] `LlmModelEntry` 含 `description` / `max_input_tokens` / `thinking` / `input_modalities` / `output_modalities` / `scenes` 6 个新字段
- [ ] `Modality` / `ModelScene` / `ThinkingSpec` 三个类型 `pub` 暴露,`Serialize + Deserialize`
- [ ] 19 个现有 model 条目全部填齐新字段,无 `TODO` 占位
- [ ] 每个 `*_models()` 函数顶部有 `// Source:` 注释指向供应商文档
- [ ] `usd_model` / `cny_model` 便利函数已删除,所有条目改为 struct literal
- [ ] 新增至少 5 条回归测试:模态命中、scene 命中、thinking 上限、description 非空
- [ ] 现有 10 条 catalog 测试不破坏(全部继续通过)
- [ ] `cargo test --workspace` / `cargo clippy --workspace -- -D warnings` / `cargo fmt --check` 全过

## 依赖

- 无外部依赖。本 hotfix 完全在 `agent-runtime-providers/catalog` 模块内
- 不阻塞 v0.9.4 / v0.9.5 / v0.10 任何工作

## 后续工作(本次不做)

- v0.11+:解耦 catalog 与 capabilities 的字段双写。`AnthropicAdapter::capabilities()` 直接从 catalog 查 `context_window` / `pricing`,不再硬编码
- v0.11+:实现 `validate_options_against_capabilities()` 共享 helper,把当前每个 adapter 各自处理"thinking 级别不支持"的逻辑统一
- v0.11+:`ModelCapabilities` 加运行时消费(请求前预校验、自动 OptionAdjustment 生成),让接口落地
