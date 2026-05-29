# Orchest Output Model — 从 Markdown 到 Showroom 的演进设计

**Status**: draft — 输出模型的四级演进路线图，从纯文本到全模态可交互内容。
**原则**: 每级向后兼容，原语可组合，渲染与推理解耦。

---

## 总览

| 阶段 | 主题 | ContentBlock 原语 | 渲染层能力 |
|------|------|------------------|-----------|
| **1.0** | Markdown 文本 | `Text` | Markdown 渲染器 |
| **2.0** | 图文混排 + 语音 | `Text`, `Spoken`, `Image` | Markdown + 图片查看器 + TTS 播放器 |
| **3.0** | 可交互生成内容 | `Widget`, `Canvas` | 组件映射引擎 + 状态管理 |
| **4.0** | 空间与 3D | `Scene`, `Model3D`, `SpatialAnnotation` | 3D 引擎 + 空间交互 |

---

## 1.0 — Markdown 文本

### ContentBlock

```rust
pub enum ContentBlock {
    Text {
        text: String,
    },
    Thinking {
        text: Option<String>,
        signature: Option<String>,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,           // 工具返回的文本
    },
}
```

### 渲染契约

```
Agent → RuntimeEvent::TextDelta(delta)
Showroom → 拼成完整 Text block → Markdown 渲染
```

### 覆盖场景

- 对话式 agent
- 代码生成与审查
- 纯文本研究输出

### 在这个阶段奠定的基础

- `ContentBlock` 是 `Vec` 而非单个——一次响应可以多个 block
- `ToolResult.content: String` 已是文本——为下一阶段变成 `Vec<ContentBlock>` 做准备
- 渲染层已经按 variant 路由（虽然只有一个 variant）

---

## 2.0 — 图文混排 + 语音

### ContentBlock 扩展

```rust
pub enum ContentBlock {
    // --- 1.0 保留 ---
    Text {
        text: String,
    },
    Thinking { .. },
    ToolUse { .. },

    // --- 1.0 升级 ---
    // ToolResult 的 content 从 String 升级为 Vec<ContentBlock>
    ToolResult {
        tool_use_id: String,
        content: Vec<ContentBlock>,
    },

    // --- 2.0 新增 ---
    Spoken {
        text: String,
        voice: Option<String>,     // 可选：voice id
    },
    Image {
        asset_id: String,          // 来自 AIGC Gateway
        alt: String,
        caption: Option<String>,
        width: Option<u32>,
        height: Option<u32>,
    },
    Audio {
        asset_id: String,          // 来自 TTS Gateway 或音频生成
        kind: AudioKind,           // Speech | Music | SoundEffect
        caption: Option<String>,
        duration_ms: u64,
    },
}

pub enum AudioKind {
    Speech,
    Music,
    SoundEffect,
}
```

### 核心变化

**1. `ToolResult.content` 从 `String` → `Vec<ContentBlock>`**

这是最关键的一次升级。工具不再只能返回文本——搜索工具可以返回图片，数据分析工具可以返回图表（作为图片），代码执行工具可以同时返回文本输出和生成的图像。

```rust
// 之前（1.0）
ToolResult {
    tool_use_id: "call_1",
    content: "## Analysis\n\nData shows...",
}

// 之后（2.0）
ToolResult {
    tool_use_id: "call_1",
    content: vec![
        ContentBlock::Text { text: "## Analysis\n\nData shows...".into() },
        ContentBlock::Image {
            asset_id: "chart_2026_q1".into(),
            alt: "Q1 2026 revenue chart".into(),
            caption: Some("Figure 1: Revenue by quarter".into()),
            width: Some(800),
            height: Some(600),
        },
    ],
}
```

**2. `Spoken` 引入语音双轨**

```
深度研究 agent 的一次响应：
  [Text(5000字报告)]
  [Spoken("报告完成了，三个关键发现：第一……")]
```

渲染层：
- `Text` → 屏幕
- `Spoken` → TTS 引擎 → 扬声器

两个渠道并行，不互相阻塞。

**3. 流式输出模型**

```
StreamEvent::TextDelta("## 分")
StreamEvent::TextDelta("析\n\n")

StreamEvent::BlockStart { kind: "image", id: 1 }
StreamEvent::AssetProgress { id: 1, status: "generating" }
StreamEvent::AssetReady { id: 1, asset_id: "chart_1", mime: "image/png" }
StreamEvent::BlockEnd { id: 1 }

StreamEvent::BlockStart { kind: "spoken", id: 2 }
StreamEvent::SpokenDelta("报告")
StreamEvent::SpokenDelta("完成了")
StreamEvent::BlockEnd { id: 2 }
```

关键设计：**资产（Image/Audio/Video）在产出时可能还没生成完**。`AssetProgress` 和 `AssetReady` 事件让渲染层可以显示占位符 → loading → 最终内容，而不是卡住等生成。

### 渲染契约

```
Showroom Engine:

  ContentBlock::Text    → Markdown 渲染器
  ContentBlock::Spoken  → TTS 引擎（流式播放）
  ContentBlock::Image   → 图片查看器（progressive + lightbox）
  ContentBlock::Audio   → 音频播放器（波形 + 暂停/拖动）
```

### 覆盖场景

- AI 搜索（文字结果 + 相关图片）
- 数据分析 report（Markdown 分析 + 图表图片）
- Voice agent（Spoken 回复 + 文字备选）
- 深度研究（长文 + 口语摘要）
- 图片生成 agent（prompt → 生成 → 展示 + 描述）

### 向后兼容

渲染层不认识 `Image`/`Audio` 的旧客户端：
- 跳过不认识的 variant
- 至少能看到 `Text` 块

这是 `ContentBlock` 作为 `enum` 的结构优势——加 variant 不破坏现有消费者。

---

## 3.0 — 可交互生成内容

### 核心概念

2.0 的内容是**静态的**——图片、音频一旦生成就不变。3.0 的内容是**可交互的**——图表可以缩放、表单可以填写、游戏可以玩。

交互内容分两类：

| 类别 | 描述 | 例子 |
|------|------|------|
| **Widget** | 有状态、可交互的 UI 组件 | 表格（排序/筛选）、图表（缩放/悬浮）、表单（填写/提交）、日历（选择日期）、地图（拖动/标记） |
| **Canvas** | agent 控制渲染的像素/矢量画布 | 白板（画图）、游戏画面（实时更新）、思维导图（节点拖动）、数据可视化动画 |

### ContentBlock 扩展

```rust
pub enum ContentBlock {
    // --- 2.0 保留 ---
    Text { .. },
    Spoken { .. },
    Image { .. },
    Audio { .. },
    Thinking { .. },
    ToolUse { .. },
    ToolResult { .. },

    // --- 3.0 新增 ---
    Widget {
        widget_id: String,         // 唯一标识，用于后续更新
        kind: WidgetKind,
        data: Value,               // 初始数据
        state: Value,              // 初始状态（如排序、筛选）
    },
    Canvas {
        canvas_id: String,
        width: u32,
        height: u32,
        commands: Vec<DrawCommand>, // 初始绘制指令
    },
}

pub enum WidgetKind {
    Table,                         // 可排序/筛选的表格
    Chart(ChartType),              // 可交互的图表
    Form,                          // 可填写的表单
    Map,                           // 可拖动的地图
    Timeline,                      // 时间线
    Kanban,                        // 看板
    Custom(String),                // 自定义 widget type
}

pub enum ChartType {
    Bar, Line, Pie, Scatter, Area, Heatmap, Radar,
}

pub enum DrawCommand {
    Clear,
    Rect { x: f32, y: f32, w: f32, h: f32, fill: Option<String>, stroke: Option<String> },
    Circle { cx: f32, cy: f32, r: f32, fill: Option<String> },
    Text { x: f32, y: f32, text: String, font_size: f32 },
    Line { points: Vec<(f32, f32)>, stroke: String, width: f32 },
    Image { asset_id: String, x: f32, y: f32, w: f32, h: f32 },
    // ...
}
```

### 交互模型：StreamEvent 的双向扩展

2.0 的 StreamEvent 是单向的：agent → 渲染层。3.0 需要渲染层 → agent 的反馈通道，因为 Widget 和 Canvas 可以产生用户交互事件。

```rust
// --- Agent → Showroom（已有）---
pub enum OutputEvent {
    TextDelta(String),
    SpokenDelta(String),
    BlockStart { kind: String, id: u32 },
    BlockEnd { id: u32 },
    AssetProgress { id: u32, status: String },
    AssetReady { id: u32, asset_id: String, mime: String },

    // --- 3.0 新增 ---
    WidgetUpdate {
        widget_id: String,
        data: Value,               // 新数据
        state: Option<Value>,      // 新状态
    },
    CanvasDraw {
        canvas_id: String,
        commands: Vec<DrawCommand>,
    },
    CanvasClear {
        canvas_id: String,
    },
}

// --- Showroom → Agent（新增）---
pub enum InputEvent {
    // 用户交互事件，注入到 agent run 的上下文中
    WidgetAction {
        widget_id: String,
        action: String,            // "sort", "filter", "click", "submit"
        payload: Value,
    },
    CanvasInteraction {
        canvas_id: String,
        kind: String,              // "click", "drag", "hover"
        x: f32,
        y: f32,
        payload: Value,
    },
    VoiceInterrupt {               // barge-in
        partial_text: String,
    },
}
```

### Widget 更新循环

```
Agent 产出 Widget {
    widget_id: "inventory_table",
    kind: Table,
    data: [1000 rows of inventory],
    state: { sort: "name", page: 0, page_size: 50 }
}

用户在表格上点击 "price" 列排序
    ↓
Showroom → Agent: WidgetAction {
    widget_id: "inventory_table",
    action: "sort",
    payload: { column: "price", direction: "desc" }
}
    ↓
Agent 处理（不一定是新的 LLM call——可以是前端逻辑）
    ↓
Agent → Showroom: WidgetUpdate {
    widget_id: "inventory_table",
    data: [same 1000 rows, sorted by price desc],
    state: { sort: "price", page: 0, page_size: 50 }
}
```

**关键设计选择**：Widget 更新不一定触发 Agent Loop。客户端可以做纯排序/筛选（不调 LLM），只在需要语义理解时才回到 Agent Loop（比如用户说 "去掉价格低于 ¥100 的"）。

这意味着 3.0 需要一个新的**轻量交互通道**，不经过完整的 `AgentRun` 生命周期。

### Canvas 与 Agent Loop 的实时交互

Canvas 是最灵活的交互形式——agent 像游戏引擎一样控制画面。

```
用例：agent 画了一个思维导图

Agent: CanvasDraw {
    canvas_id: "mindmap",
    commands: [
        Circle(cx:400, cy:300, r:60, fill:"blue"),
        Text(x:400, y:300, text:"核心概念", font_size:14),
        Line(points:[(460,300), (600,200)], stroke:"gray", width:2),
        // ...
    ]
}

用户点击 "核心概念" 节点
    ↓
Showroom → Agent: CanvasInteraction {
    canvas_id: "mindmap",
    kind: "click", x: 400, y: 300,
    payload: { hit_target: "核心概念" }
}
    ↓
Agent: CanvasDraw { canvas_id: "mindmap", commands: [
    // 高亮被点击的节点
    Circle(cx:400, cy:300, r:60, fill:"yellow", stroke:"orange"),
    Text(x:400, y:300, text:"核心概念", font_size:14),
    // 展开子节点
    Text(x:600, y:200, text:"子概念A", font_size:12),
    Text(x:600, y:400, text:"子概念B", font_size:12),
]}
```

### 渲染契约（3.0 新增）

```
Widget  → 组件映射引擎
          Table    → DataTable 组件（排序/筛选/分页）
          Chart    → ECharts/Recharts（缩放/悬浮 tooltip/数据下钻）
          Form     → 表单组件（字段校验/提交/重置）
          Map      → Leaflet/Mapbox（拖动/缩放/标记点击）
          Timeline → 时间线组件（缩放/事件详情）
          Kanban   → 看板组件（拖拽排序/列移动）
          Custom   → 用户注册的自定义组件

Canvas  → Canvas/WebGL 渲染引擎
          维护场景图
          处理 DrawCommand 序列
          捕获点击/拖动事件 → InputEvent
```

### 覆盖场景

- 数据分析 agent：产出可交互的表格和图表，用户可以在界面上钻取和排序
- 项目管理 agent：产出看板，用户可以拖拽卡片
- 教学 agent：产出可交互的思维导图和白板
- 游戏 agent：产出 Canvas，用户可以交互
- 旅行规划 agent：产出带标记的地图，用户可以拖动和点击

### 向后兼容

- 不认识的 Widget → 降级为 `Structured` 文本展示（显示原始数据）
- 不认识的 Canvas DrawCommand → 跳过
- 不支持 `InputEvent` 的客户端 → Widget 只能看不能交互（和 2.0 的图片一样）

---

## 4.0 — 空间与 3D

### 核心概念

3.0 的内容在 2D 平面上可交互。4.0 的内容在 3D 空间中存在——场景、模型、空间标注、动画。

### ContentBlock 扩展

```rust
pub enum ContentBlock {
    // --- 3.0 保留 ---
    Text { .. }, Spoken { .. }, Image { .. }, Audio { .. },
    Widget { .. }, Canvas { .. },
    Thinking { .. }, ToolUse { .. }, ToolResult { .. },

    // --- 4.0 新增 ---
    Scene {
        scene_id: String,
        camera: CameraState,               // 初始视角
        lights: Vec<LightDef>,
        objects: Vec<SceneObject>,
    },
    Model3D {
        asset_id: String,                  // glTF / USDZ
        transform: Option<Transform>,      // 位置/旋转/缩放
        animations: Vec<String>,           // 播放哪些动画
        caption: Option<String>,
    },
    SpatialAnnotation {
        target: SpatialTarget,             // 标注附着在哪个物体上
        text: String,
        style: AnnotationStyle,
    },
}

pub struct CameraState {
    pub position: [f32; 3],
    pub target: [f32; 3],
    pub fov: f32,
}

pub struct SceneObject {
    pub id: String,
    pub kind: SceneObjectKind,
    pub transform: Transform,
    pub material: Option<MaterialDef>,
}

pub enum SceneObjectKind {
    Model { asset_id: String },
    Primitive(PrimitiveKind),
    Light(LightDef),
    PointCloud { asset_id: String, point_size: f32 },
}

pub enum PrimitiveKind {
    Cube, Sphere, Plane, Cylinder, Cone,
}

pub struct Transform {
    pub position: [f32; 3],
    pub rotation: [f32; 4],   // quaternion
    pub scale: [f32; 3],
}

pub struct SpatialTarget {
    pub scene_id: String,
    pub object_id: String,
    pub offset: Option<[f32; 3]>,
}

pub enum AnnotationStyle {
    Label,           // 浮空文字标签
    Arrow,           // 箭头指向
    BoundingBox,     // 包围盒
    Highlight,       // 高亮闪烁
}
```

### 空间交互

```
Agent 产出:
  Scene { scene_id: "product_view", camera, lights, objects: [
    Model3D { asset_id: "chair_model", transform: ... },
    SpatialAnnotation { target: "chair_model", text: "这是人体工学椅背", style: Arrow },
  ]}

用户在场景中旋转/缩放/点击物体
    ↓
Showroom → Agent: SceneInteraction {
    scene_id: "product_view",
    kind: "object_click",
    object_id: "chair_model",
    camera_state: { position, target, fov },  // 当前视角
}

Agent 响应:
  SceneUpdate {
    scene_id: "product_view",
    camera: { target: [close-up of chair arm] },
    add_annotations: [
      SpatialAnnotation { target: "chair_armrest", text: "可调节扶手" },
    ],
  }
```

### 场景命令（SceneCommand）

和 Canvas 的 `DrawCommand` 类似，Scene 有增量更新命令：

```rust
pub enum SceneCommand {
    // 物体操作
    SpawnObject(SceneObject),
    RemoveObject { object_id: String },
    MoveObject { object_id: String, transform: Transform },
    Animate { object_id: String, animation: String, loop: bool },

    // 材质操作
    SetMaterial { object_id: String, material: MaterialDef },

    // 相机操作
    MoveCamera { camera: CameraState, duration_ms: u64 },
    LookAt { object_id: String, duration_ms: u64 },

    // 标注操作
    AddAnnotation(SpatialAnnotation),
    RemoveAnnotation { annotation_id: String },

    // 环境操作
    SetEnvironment { skybox_asset_id: String },
    SetLighting { lights: Vec<LightDef> },
}
```

Agent 可以像导演一样控制场景——推进镜头、高亮物体、添加标注、切换材质。

### 渲染契约（4.0 新增）

```
Scene     → 3D 引擎（Three.js / Bevy / Unity WebGL）
             场景图管理
             相机控制（轨道/飞行/第一人称）
             物理基础交互（拾取、拖拽）
             SceneCommand → 场景图更新

Model3D   → 3D 模型加载器
             glTF/USDZ 解析
             PBR 材质
             骨骼动画播放
             LOD 自动切换

SpatialAnnotation → 标注渲染器
                      世界空间 → 屏幕空间投影
                      遮挡检测（被遮挡时半透明）
```

### 覆盖场景

- 产品展示 agent：3D 模型 + 交互式讲解（"点击任意部位了解详情"）
- 建筑设计 agent：在 3D 场景中标注和修改设计方案
- 教育 agent：解剖模型 + 空间标注 + 动画
- 游戏 agent：场景生成 + 物体放置 + 交互叙事
- 数据分析 agent：3D 散点图/表面图/网络拓扑

### 向后兼容

- 不支持 3D 的客户端 → Scene 降级为静态截图（pre-rendered Image）+ 文字描述
- 不支持的空间交互 → 忽略 SceneInteraction 事件

---

## 演进路线中的不变式

### 1. ContentBlock 是 Vec，不是 Item

从 1.0 第一天就是这样。一次响应永远是多 block 的序列。这个不变式让之后加 Spoken、Image、Widget 都不需要改基础结构——只是在序列中加更多元素。

### 2. 渲染层按 variant 路由，不看内容

从 1.0 到 4.0，Showroom engine 始终：

```
match content_block {
    Text(..)   → render_markdown(..)
    Spoken(..) → play_tts(..)
    Image(..)  → show_image(..)
    // 新 variant → 新分支
    _           → skip or degrade
}
```

不看 `Text` 里面是什么内容——那是 Markdown 渲染器的事。不看 `Image` 里是什么文件——那是图片解码器的事。**内容语义属于渲染器，块类型属于协议。**

### 3. 资产的生命周期独立于 ContentBlock

从 2.0 的 Image 到 4.0 的 Model3D，所有资产都通过 `asset_id` 引用，不内联数据。资产有自己的生命周期（生成 → 就绪 → 缓存 → 过期），独立于 agent 响应。ContentBlock 只是指向资产的指针。

这复用了 AIGC Gateway 已有的 job/asset/storage 基础设施。

### 4. 增量更新优先于全量替换

从 3.0 的 WidgetUpdate 到 4.0 的 SceneCommand，所有交互都是增量式的：

```
正确: SceneCommand::MoveObject { object_id: "chair", transform: new_pos }
错误: Scene { objects: [重新发送所有物体] }
```

增量更新让了渲染层可以做平滑过渡（动画），也避免了网络传输整棵树。

---

## Orchest Core 需要的具体变更

| 版本 | 变更 | 破坏性 |
|------|------|--------|
| v0.9 | `ContentBlock` 加 `Spoken`, `Image`, `Audio`, `Custom` | 否（新增 variant） |
| v0.9 | `ToolResult.content: Value` → `Vec<ContentBlock>` | **是**（类型变更） |
| v0.9 | `StreamEvent` 加 `BlockStart`, `BlockEnd`, `AssetProgress`, `AssetReady`, `SpokenDelta` | 否（新增 variant） |
| v1.0 | `ContentBlock` 加 `Widget`, `Canvas` | 否 |
| v1.0 | `StreamEvent` 加 `WidgetUpdate`, `CanvasDraw`, `CanvasClear` | 否 |
| v1.0 | 新增 `InputEvent` enum（Showroom → Agent 的反向通道） | 否（新类型） |
| v2.0 | `ContentBlock` 加 `Scene`, `Model3D`, `SpatialAnnotation` | 否 |
| v2.0 | `StreamEvent` 加 `SceneCommand` | 否 |

唯一一个 breaking change 是 `ToolResult.content` 的类型变更。这必须在 v1.0 之前做，因为 v1.0 承诺 API 稳定。

### 建议的版本锚点

- **Orchest v1.0**: 2.0 输出模型（Text + Spoken + Image + Audio + ToolResult 富内容）
- **Orchest v1.5**: 3.0 输出模型（Widget + Canvas + InputEvent）
- **Orchest v2.0**: 4.0 输出模型（Scene + Model3D + SceneCommand）

这些和 Orchest 主线的 v0.7/v0.8/v0.9 迭代节奏不冲突——输出模型是类型系统的扩展，不依赖 Hook 框架或 Session 持久化。
