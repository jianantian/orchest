# Orchest Output Model — Generative UI 路线（修订）

**Status**: draft — 吸收 genui 业界实践后的修订版。放弃抽象 Document/Group 树，改用**组件声明式输出**。
**前置阅读**: [output-model-evolution.md](./output-model-evolution.md)（原方案，已被本文替代）

---

## 为什么 Vec<ContentBlock> 不够——一个最小的反例

```
[Text("## 分析"), Image("chart_1"), Text("Q1 强劲"), Image("chart_2"), Text("Q2 回落")]
```

渲染层收到这 5 个平铺块。它怎么知道 `chart_1` 的图注是 "Q1 强劲" 而不是 "## 分析"？怎么知道两张图是并列对比而不是先后排列？

靠 `Group` + `LayoutHint` 能解决——但那是在用布局原语模拟**语义**。Agent 想说的是「这是一张对比卡片，左边 Q1，右边 Q2」。`Group(Horizontal, [Group(Flow, [Image, Text]), Group(Flow, [Image, Text])])` 表达了结构，但丢失了语义——渲染层看到的是一棵嵌套盒子树，不知道这是一个 `ComparisonCard`。

**genui 的核心洞察**：输出不应该是抽象的内容块，而应该是**具名组件及其 props**。组件名携带语义，props 携带数据，渲染是确定性的。

---

## 业界共识：三种 genui 范式

CopilotKit 的 Tyler Slaton 在 2026 年提出了三种范式的分类，已经被业界广泛接受：

### 范式 1: Controlled（受控生成）

```
Agent 从开发者预定义的组件目录中选择
输出: { component: "ComparisonCard", props: { title: "...", items: [...] } }
渲染: <ComparisonCard title="..." items={[...]} />
```

**代表**: Vercel AI SDK `createStreamableUI`, CopilotKit `useComponent`, A2UI Protocol

特点：模型只选择组件名 + 填 props。组件本身是开发者写的，安全、可控、可测试。这是**当前 genui 的主流范式**。

### 范式 2: Declarative / Shared Control（声明式共享控制）

```
Agent 输出: A2UI JSON 描述 → 平台无关的 UI 描述
渲染: 任何 A2UI renderer（React/Lit/Angular/Flutter）渲染为原生组件
```

**代表**: Google A2UI (v0.9 draft), Open-JSON-UI

特点：跨平台、跨框架。一个 A2UI 描述可以在 Web、移动端、车载屏幕上渲染。组件目录是平台相关的，但描述格式统一。

### 范式 3: Autonomous（自主生成）

```
Agent 输出: 完整的 HTML/JSX/TSX 代码
渲染: 沙箱运行时执行
```

**代表**: Claude Artifacts, v0, Bolt, Lovable, Renderify

特点：最灵活，但最不安全。模型可以生成任意 UI 代码。适合原型和创意工具，不适合生产级产品。

---

## 对 Orchest 的影响：范式 1 的运行时抽象

Orchest 是底层运行时，不应该绑定到 React、A2UI、或任何特定渲染框架。但它需要提供**范式 1 的基础设施**：让开发者注册组件，让模型选择和填充组件。

### OutputComponent — 和 Tool 对称的新原语

```rust
/// 开发者定义的输出组件。
/// 对称于 `Tool` trait——Tool 是模型调用的能力，OutputComponent 是模型产出的内容。
#[async_trait]
pub trait OutputComponent: Send + Sync {
    /// 组件名称。模型在 system prompt 中看到这个名字。
    fn name(&self) -> &str;

    /// 组件描述。模型用它判断什么时候该用这个组件。
    fn description(&self) -> &str;

    /// Props schema (JSON Schema)。模型用它填充 props。
    fn props_schema(&self) -> JsonSchema;

    /// 渲染此组件。
    /// 返回的是给 showroom 的中间表示，不是 DOM。
    async fn render(
        &self,
        props: Value,
        ctx: &RenderContext,
    ) -> Result<RenderOutput, RenderError>;
}

pub struct RenderOutput {
    /// 组件的渲染产物：可以是子 ContentBlock 序列，
    /// 也可以是平台相关的渲染指令。
    pub kind: RenderOutputKind,
}

pub enum RenderOutputKind {
    /// 返回子树——组件展开为 ContentBlock 树。
    /// 适合简单组件（Card, Section 等），运行时可以直接渲染。
    Blocks(Vec<ContentBlock>),

    /// 不透明——传给外部渲染器处理。
    /// 适合需要框架特定渲染的组件（React component 等）。
    Opaque {
        component_name: String,
        props: Value,
    },
}
```

### 两层架构

```
┌─────────────────────────────────────────┐
│  Orchest Runtime (Rust)                 │
│                                         │
│  OutputComponent trait                  │
│  → render() → RenderOutput             │
│     → Blocks(Vec<ContentBlock>)        │
│     → Opaque { component_name, props } │
│                                         │
│  ComponentRegistry                      │
│  → 和 ToolRegistry 对称                 │
│  → 管理已注册的 OutputComponent         │
└──────────────┬──────────────────────────┘
               │ RenderOutput (其中 Opaque 透传)
┌──────────────▼──────────────────────────┐
│  Showroom Engine (JS/Swift/Kotlin)      │
│                                         │
│  ComponentCatalog                        │
│  → "ComparisonCard" → <ComparisonCard/> │
│  → "DataTable"      → <DataTable/>      │
│  → Opaque props → 框架组件              │
│                                         │
│  ContentBlock renderers                 │
│  → Text → Markdown                      │
│  → Image → <img>                        │
│  → Spoken → TTS                         │
└─────────────────────────────────────────┘
```

关键分离：
- **Orchest 管注册 + 模型选择 + 初始渲染**：`OutputComponent` trait 让 Rust 侧知道有哪些组件、它们的 schema、以及如何把它们渲染为基础块或透传给 showroom
- **Showroom 管最终渲染**：收到 `Opaque` 后，按 `component_name` 映射到实际 UI 组件

### 这和 Tool 有多像？

```rust
// Tool: 模型调用 → 执行 → 返回结果
pub trait Tool {
    fn metadata(&self) -> ToolMetadata;      // name, description, input_schema
    async fn execute(&self, input: Value) -> ToolOutput;
}

// OutputComponent: 模型产出 → 渲染 → 返回块
pub trait OutputComponent {
    fn name(&self) -> &str;                 // 组件名
    fn description(&self) -> &str;          // 给模型看的描述
    fn props_schema(&self) -> JsonSchema;   // props schema
    async fn render(&self, props: Value) -> RenderOutput;
}
```

对称但方向相反：Tool 是模型调用的**输入**（action），OutputComponent 是模型产出的**输出**（presentation）。它们共享同一个注册/发现模式。

---

## 修订后的 ContentBlock

有了 OutputComponent 后，`ContentBlock` 就变简单了——它是**叶子节点**的集合：

```rust
pub enum ContentBlock {
    // 基础块——和之前一样
    Text { text: String },
    Spoken { text: String, voice: Option<String> },
    Image { asset_id: String, alt: String, width: Option<u32>, height: Option<u32> },
    Audio { asset_id: String, kind: AudioKind, duration_ms: u64 },

    // 结构块——但语义由 OutputComponent 提供
    // ContentBlock 本身不表达语义，只表达原子内容

    // 组件引用——指向 OutputComponent 的渲染产物
    Component {
        component_name: String,        // "ComparisonCard", "DataTable"
        props: Value,                  // 传给组件的 props
        children: Vec<ContentBlock>,   // 子内容块（如果有）
    },

    // 保留
    Thinking { .. },
    ToolUse { .. },
    ToolResult { tool_use_id: String, content: Vec<ContentBlock> },
}
```

`Component` variant 是 genui 的入口——模型产出一个带组件名 + props + children 的块，渲染层按组件名查找对应实现。

### 流式协议

```
StreamEvent::ComponentOpen { name: "ComparisonCard", props: { title: "Q1 vs Q2" } }
StreamEvent::ComponentChild { index: 0, block: Text("### Q1\n\n强劲") }
StreamEvent::ComponentChild { index: 1, block: Image("chart_1.png") }
StreamEvent::ComponentChild { index: 2, block: Text("### Q2\n\n回落") }
StreamEvent::ComponentChild { index: 3, block: Image("chart_2.png") }
StreamEvent::ComponentClose
```

Showroom 收到 `ComponentOpen` 时，查找 `ComparisonCard` 的渲染器，创建实例，然后子块逐个填充进卡片槽位。

---

## 演进路线（修订）

| 阶段 | 核心 | 组件模型 |
|------|------|---------|
| **1.0** | 纯 Markdown 文本 | `ContentBlock::Text`。无组件概念。`ToolResult` 返回 `String`。 |
| **2.0** | 图文混排 + 语音 | 引入 `OutputComponent` trait + `ContentBlock::Component`。内置组件：`Article`, `Card`, `Comparison`, `Gallery`。`ToolResult.content` 从 `String` → `Vec<ContentBlock>`。 |
| **3.0** | 可交互生成内容 | 组件可获得交互能力。`ComponentAction` 事件：showroom → agent 反馈用户的组件交互。组件可用 `Canvas` 和 `Widget` 做更复杂的交互。 |
| **4.0** | 3D + 空间 | 新增 `Scene`, `Model3D`, `SpatialAnnotation` 作为叶子块。`OutputComponent` 可用这些块构建空间组件（`ProductShowcase`, `ArchitecturalReview`）。 |

---

## 具体：2.0 的内置组件

2.0 引入 `OutputComponent` trait。同时提供几个内置组件，覆盖最常见的图文混排场景：

```rust
// 文章（长文档）
struct Article;
// props: { title: String, sections: [{ heading: String, content: Vec<ContentBlock> }] }

// 对比卡片（并排）
struct Comparison;
// props: { title: String, items: [{ label: String, content: Vec<ContentBlock> }] }

// 图库（轮播）
struct Gallery;
// props: { images: [{ asset_id: String, caption: String }] }

// 通用卡片
struct Card;
// props: { title: String, subtitle: Option<String>, content: Vec<ContentBlock>, image: Option<String> }
```

模型看到 system prompt：

```
可用的输出组件：
- Article: 长文档，有章节结构
- Comparison: 并排对比视图，2-4 个条目
- Gallery: 图片轮播
- Card: 通用信息卡片
- (none): 纯 Markdown 文本，默认输出模式
```

2.0 的 showroom 只需要实现 5 个组件渲染器（加上纯 Markdown fallback）。

---

## 和 AG-UI / A2UI 的关系

Orchest 的 `OutputComponent` 和 AG-UI / A2UI 是互补的，不是竞争的：

| 层 | 协议/标准 | Orchest 的位置 |
|----|---------|---------------|
| Agent ↔ Showroom 事件流 | **AG-UI** (event types, streaming) | Orchest 的 `StreamEvent` 可以映射到 AG-UI 事件 |
| UI 描述格式 | **A2UI** (declarative JSON) | `OutputComponent.render()` 的 `Opaque` 输出可以是 A2UI JSON |
| 组件注册与发现 | **Orchest** `OutputComponent` trait | 提供运行时级别的组件 schema 和注册 |
| 渲染器 | **React/Lit/Angular/Flutter** | Showroom engine，不在 Orchest 内 |

Orchest 不重新发明 AG-UI 或 A2UI——它提供的是：在 agent runtime 内，如何让模型知道有哪些组件、如何选择、如何填充。这和 Tool 在 runtime 内的角色完全对称。

---

## 为什么不做范式 3（Autonomous）

Claude Artifacts 和 v0 的自主生成模式不适合 Orchest：

1. **安全**：任意 HTML/JS 需要沙箱（Renderify）——这不是 runtime 层的事
2. **可观测性**：代码生成让 agent 行为不可审计——你不知道它生成了什么 UI
3. **一致性**：每次生成不同的 UI 结构和样式——产品体验不可控
4. **与 Orchest 的极简 core 冲突**：范式 1/2 只需要组件目录，范式 3 需要完整的代码执行+沙箱环境

范式 3 适合「让 AI 帮你写个 demo」，不适合「AI 驱动一个产品化的 showroom」。Orchest 做的是后者。

---

## 总结：从平铺 Vec 到组件声明式

```
1.0:  Vec<Text>                           —— 纯文本，平铺
2.0:  Vec<Text | Image | Spoken | Component>  —— 叶子块 + 组件引用
3.0:  Vec<... | Widget | Canvas | Component(interactive)> —— 交互组件
4.0:  Vec<... | Scene | Model3D | Component(spatial)> —— 空间组件
```

核心变化不是树结构（`Group`/`Document`），而是**组件声明**（`Component { name, props, children }`）。组件名承载语义，props 承载数据，children 承载内容。这和 Web 开发从 `<div class="card">` 进化到 `<Card>` 的路径完全一致——语义不在 class 名里，在标签名里。
