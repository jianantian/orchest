# Issue 001:新类型与 LlmModelEntry 扩展

## 背景

`LlmModelEntry` 当前 6 字段不足以承载选模信息。本 issue 落地新类型与字段扩展,**不动模型条目数据**——后者拆到 002-005。

## 目标

1. 新增 `Modality` / `ModelScene` / `ThinkingSpec` 三个类型
2. `LlmModelEntry` 加 6 个字段
3. 砍掉 `usd_model` / `cny_model` 便利函数(参数过多已反生产力)
4. 现有 19 个模型条目暂时填**最小默认值**(`description: ""`、`scenes: &[General]`、`thinking: None`、`modalities: &[Text] -> &[Text]`),保证 build pass 与现有测试通过
5. 19 条数据的真实填充交给 issue 002-005

## 范围

### 新类型

`crates/agent-runtime-providers/src/catalog/mod.rs`:

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
    General,
    Coding,
    Agent,
    Chat,
    Reasoning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThinkingSpec {
    /// thinking 内容 token 上限。`None` = 供应商未公开具体上限
    pub max_thinking_tokens: Option<u32>,
}
```

需要 `use serde::{Deserialize, Serialize}` 引入。

### 扩展 `LlmModelEntry`

```rust
#[derive(Debug, Clone)]
pub struct LlmModelEntry {
    pub model_id: &'static str,
    pub provider: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,                     // 新增
    pub context_window: u64,
    pub max_input_tokens: Option<u64>,                 // 新增
    pub max_output_tokens: Option<u32>,
    pub thinking: Option<ThinkingSpec>,                // 新增
    pub input_modalities: &'static [Modality],         // 新增
    pub output_modalities: &'static [Modality],        // 新增
    pub scenes: &'static [ModelScene],                 // 新增
    pub pricing: Option<ModelPricing>,
}
```

### 砍掉便利函数

删除 `usd_model(...)` 与 `cny_model(...)` 函数体。
所有调用方在 `anthropic_models()` / `openai_models()` / `deepseek_models()` / `volcengine_models()` 中改为 struct literal。

### 19 条数据的占位填法(本 issue 临时占位,真值由 002-005 填)

```rust
// 占位示例(本 issue 仅保证编译通过):
LlmModelEntry {
    model_id: "anthropic/claude-opus-4-8",
    provider: "anthropic",
    display_name: "Claude Opus 4.8",
    description: "",                                   // ← 002 填
    context_window: 1_000_000,
    max_input_tokens: None,                            // ← 002 填
    max_output_tokens: Some(128_000),
    thinking: None,                                    // ← 002 填
    input_modalities: &[Modality::Text],               // ← 002 填(M3 需要 Image)
    output_modalities: &[Modality::Text],              // ← 002 填
    scenes: &[ModelScene::General],                    // ← 002 填
    pricing: Some(ModelPricing { ... }),
}
```

**约束**: issue 001 落地后,所有现有测试(10 个)必须继续通过。`description` 暂为空字符串、`scenes` 暂为 `&[General]`、`modalities` 暂为 `&[Text]->&[Text]` 不会破坏现有断言。

### 公开 API

`pub use catalog::{LlmModelEntry, LlmModelList, LlmProviderInfo, Modality, ModelScene, ThinkingSpec};`

`crates/agent-runtime-providers/src/lib.rs` 已 `pub mod catalog`,导出三个新类型即可。

## 不在本 issue 范围

- ❌ 给 19 条数据填真实的 description / scenes / modalities / thinking — 见 002-005
- ❌ 加新测试 — 见 006
- ❌ 修改 `ModelCapabilities` — 永远不在本 hotfix 范围

## 验收标准

- [ ] `Modality` / `ModelScene` / `ThinkingSpec` 三个类型在 `catalog/mod.rs` 内定义
- [ ] 三个类型 `pub` 导出,`#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]` 齐全
- [ ] `LlmModelEntry` 含 6 个新字段,字段顺序按 PRD 第 2 节
- [ ] `usd_model` / `cny_model` 函数从 `catalog/mod.rs` 删除
- [ ] 19 条 model 条目全部改为 struct literal,新字段填占位值
- [ ] `cargo test -p agent-runtime-providers --lib catalog` 10 个测试继续通过
- [ ] `cargo clippy -p agent-runtime-providers --lib -- -D warnings` 通过
- [ ] `cargo fmt --check` 在改动文件上无 diff

## 注意事项

- 砍 helper 后文件会变长(每条多 4-5 行)。这是预期的,后续 issue 002-005 还会让每条更长。catalog 数据 declarative,长不是问题
- struct literal 的 `pricing` 字段也用 `Some(ModelPricing { ... })` 形式,不要再走 helper
- `&'static [Modality]` 字面量写法:`&[Modality::Text]`、`&[Modality::Text, Modality::Image]`
