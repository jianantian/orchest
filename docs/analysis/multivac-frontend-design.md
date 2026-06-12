# Multivac 前端设计参考

> 2026-06-01 | 参考源：Claude macOS app (Epitaxy 设计体系), Craft Agents OSS (组件架构), Codex macOS app (插件系统)
>
> 定位与优先级以 [ADR-001](./adr-001-product-positioning.md) 为准：工作台 = **Surface 区（主舞台）+ Chat Lane**；deixis 是第一交互原语；人读审指，agent 写。

---

## 一、设计原则

1. **安静** — 界面不抢注意力。Agent 在工作时用户应该感觉到「事情在被处理」，而不是被 UI 噪音打断。
2. **渐进式披露** — 默认折叠，按需展开。参考 Craft Agents 的 TurnCard：tool calls 折叠为一行，点开才看细节。
3. **空间即状态** — panel stack 的推入/弹出反映用户的工作深度。不是 tab，不是抽屉——是「我进入了这个 session，现在在这里」。
4. **动效表达物理关系** — 参考 Craft Agents 的 Island 菜单：从文本选择点发出，速度决定动画距离。不是花哨，是指引注意力。
5. **每 session 独立** — 状态管理用 Jotai atom family，不是全局 store。不同 session 的状态互相不可见。
6. **关键动作永远可见** — approval、cancel/interrupt、permission mode 切换、artifact accept/reject 不藏在 hover、context menu 或组合键里。Craft Agents 的反面教训：视觉精致但可发现性差，「UI 好看但 UX 难用」的主要来源就是过多隐式交互。
7. **固定信息架构先于灵活 panel** — 先让用户形成空间记忆，再逐步引入折叠与推入。v0 布局做死（§四）。
8. **指代是第一交互原语** — 选中任何 surface 上的任何东西（文本 / file:line / diff hunk / 终端行 / DOM 元素）→ 引用 chip → 指挥 agent。让「指」代替「说」是整个产品的介质论（ADR-001 D3），值得最大的设计投入。
9. **上下文对人可见** — agent 将看到什么，在发送前可见、可勾选（context tray）。信任来自对称：人看得见 agent 的所见。
10. **Agent 在面上现身** — agent 改文件、跑命令、操作 browser 时，对应 surface 显示 agent attribution（色标 + 操作标记），而不是只在聊天里报告。介质感来自看见对方在同一张桌上动手。

---

## 二、Claude macOS 设计体系（Epitaxy）

### 2.1 字体

| 用途 | 字体 | 说明 |
|------|------|------|
| UI 文本 | AnthropicSans | 可变字重 300-800，-0.01em letter-spacing |
| 正文内容 | AnthropicSerif | 可变字重 300-800，proportional-nums |
| 代码 | 系统等宽 (SF Mono) | — |

### 2.2 色彩系统

Epitaxy 使用 CSS 自定义属性构建完整色彩体系：

```
--color-primary / --color-primary-rgb
--color-secondary / --color-secondary-rgb
--color-info / --color-info-rgb
--color-success / --color-success-rgb
--color-warning / --color-warning-rgb
--color-error / --color-error-rgb

--background           # 主背景
--foreground           # 主文本
--surface              # 卡片/面板背景
--surface-secondary    # 嵌套面板
--border               # 分割线
--border-secondary     # 轻分割线

color-mix(in srgb, var(--info) X%, transparent)  # 最常用模式
```

**关键模式**：不是预定义 50 个固定色阶（slate-50, slate-100...），而是用 `color-mix()` 在语义色基础上动态生成。语义色变化时整个 UI 自动跟随。

### 2.3 间距

| Token | 用途 |
|-------|------|
| `--spacing-xs` ~ 4px | icon 与 label 之间 |
| `--spacing-sm` ~ 8px | 紧密元素间 |
| `--spacing-md` ~ 16px | 卡片 padding |
| `--spacing-lg` ~ 24px | section 间距 |
| `--spacing-xl` ~ 32px | 大段间距 |

### 2.4 圆角

统一 `border-radius: var(--radius-md)` — 不混用 sharp/smooth，全界面一致。

### 2.5 布局模式

Claude macOS app 的主布局：
```
┌──────────────┬──────────────────────────┐
│   Sidebar    │      Chat Area            │
│  ┌────────┐  │  ┌────────────────────┐  │
│  │ Chats  │  │  │  Messages          │  │
│  │ Projects│  │  │                    │  │
│  │ Files   │  │  │                    │  │
│  │ Settings│  │  └────────────────────┘  │
│  └────────┘  │  ┌────────────────────┐  │
│              │  │  Input Area        │  │
│              │  └────────────────────┘  │
└──────────────┴──────────────────────────┘
```

Sidebar 内是简单的列表（Chats, Projects, Files, Settings），每个 item 是 icon + label。当前选中项有背景色高亮。Sidebar 底部是用户头像 + 设置入口。

Chat Area 顶部是当前 session 标题 + 操作按钮（分享、导出、设置）。中间是消息列表。底部是输入框 + 附件按钮 + 发送按钮。

---

## 三、Craft Agents 组件架构（核心参考）

### 3.1 TurnPhase 状态机

```
pending → tool_active → awaiting → streaming → complete
```

`awaiting` 是过渡态：模型已返回 tool_call，等待用户批准或工具执行完成。UI 在这个阶段显示处理中指示器（不是 spinners，是骨架态）。

### 3.2 TurnCard 结构（email-like 模型）

TurnCard 是 Multivac 执行透明度的用户界面——让用户看到 agent 的真实执行过程：每一步调了什么、结果如何、中间失败了几次、为什么需要确认。这是 Multivac 区别于「黑盒 agent 聊天」的根本 UX 差异。

```
┌─────────────────────────────────────────┐
│  [Plan]  Accept Plan  [collapsed]       │  ← 可选 plan header
├─────────────────────────────────────────┤
│  [Tool: search_codebase]  ✓ 2 files    │  ← ActivityRow (可折叠)
│    ├─ src/auth.ts                       │
│    └─ src/session.ts                    │
├─────────────────────────────────────────┤
│  [Tool: read_file]  ✓ 145 lines        │
│  ┌────────────────────────────────┐    │
│  │  // auth.ts content...         │    │  ← 展开的 tool result
│  └────────────────────────────────┘    │
├─────────────────────────────────────────┤
│  I've analyzed the auth module.         │  ← ResponseCard
│  Here's what I found:                   │     (流式缓冲)
│                                         │
│  The TokenManager handles JWT...        │
├─────────────────────────────────────────┤
│  [Tool: Task]  ○ creating subtask...   │  ← ActivityGroupRow
│    └─ [SubTask: fix-auth]               │     (子 agent tree)
│       ├─ [Tool: edit] ✓ auth.ts:42     │
│       └─ ✓ Completed                   │
├─────────────────────────────────────────┤
│  I've created a subtask to fix the      │
│  auth token refresh issue. You can      │
│  track it here: [link]                  │
└─────────────────────────────────────────┘
```

**关键设计**：
- Tool calls 默认折叠为一行（icon + name + status + brief result）
- 用户点击展开后看到完整 input/output
- 子 agent 的 tool calls 缩进，形成树状；每个 ActivityRow 标注执行者 agent 的 icon + color
- Response 文本有缓冲：不是每个 token 都刷新渲染，而是 `MIN_WORDS=4` 或 `MIN_BUFFER_MS=100` 间隔
- **Completion Gate 状态**进入 TurnCard：`验收中`（judge model 评估）/ `未通过`（展示 gate 理由 + agent 继续返工）/ `通过`。后台 sub-agent 的 gate 结果进 Inbox。用户看到的不是「子 agent 说自己做完了」，而是验收后的真实状态——执行透明度的一部分

### 3.3 Annotation 系统（Multivac 必须实现）

这是 Craft Agents 最精致的 UX。

```
用户选中一段文字 → Island 菜单浮现
┌─────────┐
│ 高亮     │
│ 追问 AI  │  ← 点击后展开 textarea
│ 编辑     │
└─────────┘

追问后 → 文字被标注
┌──┐
│①│ The TokenManager handles JWT refresh via a sliding window...
└──┘
  ↑ 彩色背景 + 数字标记
  ↑ 鼠标悬浮显示追问内容
```

**实现关键**：

1. **文本选择检测**：
```typescript
// 监听 mouseup → 检查 window.getSelection()
// 如果选择非空且在当前 TurnCard 内 → 打开 Island
```

2. **Island 动画**：
```typescript
// 保存 pointerdown 位置 (x, y, ts)
// mouseup 时计算速度向量
// velocity → entryDistancePx (clamp 20-132px)
// entryAngleDeg = direction of movement
// Island 从 entryDistancePx 位置以 entryStartScale=0.25 动画进入
```

3. **Overlay 层**：
```typescript
// 绝对定位覆盖在文本上
// TreeWalker 遍历 DOM 收集 TextNode → 计算 offset → getClientRects
// 每行一个彩色 rect + 行末一个数字 chip
```

**Multivac 的实现策略**：
- 不做 Craft Agents 的全套 `AnnotationOverlayLayer`——太重
- 做简化版：选中文本 → Island 菜单 → 追问。标注用 `<mark>` 元素 + CSS `::after` 伪元素显示序号
- 追问存为 session metadata，后端处理

### 3.4 Session 管理

Jotai atom family 模式：
```typescript
// 每 session 独立 atom
const sessionAtomFamily = atomFamily((sessionId: string) => atom<SessionState>())

// 轻量列表
const sessionMetaMapAtom = atom<Map<string, SessionMeta>>()
const sessionIdsAtom = atom<string[]>()

// 操作
const addSessionAtom = atom(null, (get, set, session: Session) => { ... })
const updateSessionAtom = atom(null, (get, set, update: SessionUpdate) => { ... })
const removeSessionAtom = atom(null, (get, set, sessionId: string) => { ... })
```

**关键**：`sessionAtomFamily` 是懒加载——SessionViewer mount 时才从 atom family 取，不 mount 的 session 不占用内存。

### 3.5 Panel Stack

```typescript
// 单 lane panel 模型
const panelStackAtom = atom<Panel[]>([])

// 操作
pushPanel(panel)
closePanel(panelId)
reconcilePanels(newPanels)  // 从 URL 同步

// session → panel 路由
// /session/:id → pushPanel({ type: 'session', sessionId })
```

不是 tab bar。是「我进入了这个 session，之前的 session 在背后」——物理栈的感受。

### 3.6 Overlay 预览系统

```typescript
// 代码预览
<CodePreviewOverlay content={code} language="typescript" />

// Diff 预览
<MultiDiffPreviewOverlay diffs={diffs} />

// JSON 预览
<JSONPreviewOverlay data={jsonData} />

// 终端输出预览
<TerminalPreview content={stdout} />

// 图片预览
<ImagePreviewOverlay src={imageUrl} />

// PDF 预览
<PDFPreviewOverlay src={pdfUrl} />
```

每个 overlay 是全屏/近全屏的模态框，有：
- 顶部 header（标题 + 关闭 + 复制按钮）
- 滚动内容区
- 底部 footer（行号、语言标识等）

---

## 四、信息架构：工作台 = Surface 区 + Chat Lane

定位（ADR-001 D1/D2）：根对象是 Workspace，chat 只是其中一个面。布局不是「聊天应用 + 周边面板」，而是**人和 agent 共享的一组工作面（surface）**——这与传统 agent UI「聊天为中心、工具为弹窗」相反：**创作介质占最大面积，对话退为右侧常驻的指挥信道**。

```
┌──────────────┬────────────────────────────────┬──────────────────────┐
│  Left Rail   │  Surface 区（主舞台）            │  Chat Lane           │
│              │                                │                      │
│  Workspaces  │  Files / Reader（多媒体预览）    │  TurnCard timeline   │
│  Runs        │  Git（status / log / diff）     │  ApprovalStrip       │
│  Inbox       │  Terminal（共享 PTY）           │  ContextTray         │
│              │  Browser（实时预览 + agent 感知）│  SessionInput        │
│              │                                │   + 引用 chips        │
└──────────────┴────────────────────────────────┴──────────────────────┘
```

- **v0 五个面**：对话、文件阅读（代码/markdown/图片/PDF/视频，只读为主——人读审指，agent 写）、git、终端、browser。每个 surface 实现**三件套合同**：渲染器 + 指代适配器 + 感知适配器（ADR-001 D2）。
- **内容驱动激活**：diff 到达 → git 面前置；dev server 起 → browser 面打开；不做角色档案，不做 GenUI 布局引擎（v2 用 layout preset）。「按角色自动布局」由「按内容激活面」涌现。
- **Chat Lane 常驻可调宽**：Stop/Interrupt、permission mode 在 lane 头部常驻；pending approval 在 ApprovalStrip 常驻可见，PermissionDialog 只是模态强化，不是唯一入口（设计原则 6）。
- **Left Rail**：Workspaces（切换工作区）、Runs（运行列表 + 状态点）、Inbox（后台 run 完成/阻塞的通知，badge；daemon 模式下 agent 在窗口关闭后仍在工作，Inbox 是用户回来时「发生了什么」的入口）。
- 布局 v0 做死：Surface 区 tab / 二分屏，Chat Lane 固定右侧；不提供自由拖拽分栏。

### 4.1 Deixis：第一交互原语

Craft Agents 的 annotation（选中文本 → Island → 追问）是这个原语在 chat 面上的特例。Multivac 把它泛化到全工作区：

```
任何 surface 上选中任何对象
  文本 span / file:line / diff hunk / 终端行区间 / browser DOM 元素或截图区域
→ Island 菜单（追问 / 指令 / 加入上下文）
→ 生成引用 chip 进入 SessionInput
→ 发送时 chip 解析为 Locator URI + 精确内容注入
→ 对象写入 objects 表（provenance + scope），未来可被检索复用（ADR-001 D4）
```

实现要点：
- 每个 surface 的指代适配器只做两件事：**选区 → Locator URI**；**URI → 高亮回显**（agent 回复中引用同一 URI 时反向定位到 surface 上）
- chip 在输入框中可删除、可点击预览——和 context tray 一起构成「人看得见 agent 将看到什么」
- v0 范围：文本 / 代码 / diff / 终端四种选区 + **session/turn 引用**（把一个会话作为 chip 注入另一个会话——跨 agent 共享上下文的 v0 形态，ADR-001 创世卡点 2）；browser 元素指代在阶段 2；时间码 / 波形区域是 v2 surface 的事

### 4.2 Context Tray

SessionInput 上方常驻一行，展示本条消息将携带的 WorkbenchContext：

```
[✓ 当前文件 auth.ts] [✓ 选区 L42-58] [✓ git diff (3 files)] [□ 终端最后 50 行] [□ browser console]
```

- 廉价环境摘要默认勾选（几百 token）；重对象默认不勾选——agent 可通过 `read_terminal` / `inspect_browser` 等工具按需查询（注意力策略三层模型，ADR-001 D3）
- 点击任一 chip 预览 agent 将看到的确切内容
- 这是权限与信任的主要 UX 载体：控制 agent 的所见，从这里开始，而不是从 RBAC 配置页开始

---

## 五、Multivac 前端架构

### 5.1 技术栈

| 层 | 选择 | 理由 |
|----|------|------|
| **框架** | React 18 + TypeScript | 生态、类型安全 |
| **构建** | Vite | 快 |
| **富文本** | Tiptap (编辑器) + react-markdown (渲染) | 轻量，可扩展 |
| **状态** | Jotai (atom family) | 对齐 Craft Agents 模式 |
| **样式** | Tailwind CSS + CSS 自定义属性 | Tailwind 做布局，CSS 属性做语义色 |
| **动画** | framer-motion | Island 动画、panel 转场 |
| **代码高亮** | Shiki | 与 Craft Agents 同选 |
| **图表** | Mermaid (客户端渲染) | tool call 流程图 |
| **网络** | fetch (REST) + WebSocket (事件流) | — |

**内部包边界**（Craft Agents `packages/ui` 的教训：agent UI primitives 要尽早从页面代码中抽离）。初期单 app，但目录按未来可拆包的边界组织：

```
src/components/   → 未来 packages/ui            （TurnCard 等 primitives，只消费 product model）
src/api/          → 未来 packages/runtime-client （HTTP/WS transport + 事件投影）
src/types/        → 未来 packages/product-model  （session/turn/task/artifact 前端类型）
```

组件不直接 import wire 格式（TaskEvent JSON），只消费 `reduceTurnState` 之后的 product model 类型。

### 5.2 色彩体系

> 基调、token 与琥珀纪律以 [multivac-design.md](./multivac-design.md) 为准（墨/纸/琥珀体系，暖中性基底替换原 slate 占位色板）。本节保留派生规则与组件级纪律。

```css
:root {
  /* 基底：墨与纸（暖中性，完整 token 见 multivac-design.md §2.2） */
  --background: #faf8f4;
  --foreground: #1c1a17;
  --surface: #f3f0ea;
  --border: #e4dfd5;

  /* 品牌色：琥珀——出场率 < 5%（deixis 高亮 / 聚焦态 / 品牌时刻） */
  --color-primary: #d97917;

  /* 语义色：独立状态系统，降饱和 */
  --color-success: #4d9960;
  --color-error: #c4554d;
  --color-warning: #d9a317;
  --color-info: #5b7e9e;

  &[data-theme="dark"] {
    /* 石墨夜：暖黑微偏褐 */
    --background: #161411;
    --foreground: #e8e4dc;
    --surface: #1f1c18;
    --border: #353029;
  }
}
```

**规则**：永远不直接用 `slate-600` 这种固定色。始终通过 `var(--foreground)` 或 `color-mix(in srgb, var(--info) 10%, transparent)` 派生。

**Agent 标识色**（MIMO 经验：multi-agent 时用户必须能区分「这步是谁做的」）：

```css
  --color-agent-build: #fb8147;
  --color-agent-plan: #c7e2a8;
  --color-agent-review: #a7a3d8;
  /* AgentFactory 动态生成的 agent 从预置色环分配 */
```

TurnCard 中每个 ActivityRow / ActivityGroupRow 标注执行者 agent 的 icon + color；同一颜色贯穿 surface 上的 agent attribution（设计原则 10）和 Inbox 条目。

**Overlay 纪律**：z-index、shadow 全部 token 化（`--z-overlay` / `--z-island` / `--shadow-minimal` 等量表），配 ESLint 自定义规则禁止硬编码。Craft Agents 用 `no-hardcoded-z-index` / `no-nonstandard-shadows` 管住了 overlay 失控——agent 产品的 overlay、menu、island、diff viewer 数量多，这条纪律必须从第一天建立。

### 5.3 组件树

```
MultivacApp
├── AppShell
│   ├── LeftRail
│   │   ├── UserAvatar
│   │   ├── WorkspaceSwitcher
│   │   ├── RunList
│   │   │   └── RunItem (icon, title, status dot, timestamp)
│   │   ├── InboxList (badge = inboxUnreadCount)
│   │   └── BottomActions (settings, help, feedback)
│   │
│   ├── SurfaceArea (主舞台, tab / 二分屏)
│   │   ├── FileSurface (tree + Reader: code/markdown/image/PDF/video, 只读为主)
│   │   ├── GitSurface (status / log / DiffViewer——review 主视图)
│   │   ├── TerminalSurface (xterm.js, 共享 PTY, agent sideband 操作带 attribution)
│   │   ├── BrowserSurface (实时预览 + console; agent 感知源)
│   │   └── OverlayHost (portal: fullscreen preview)
│   │
│   ├── ChatLane (右侧常驻, 可调宽)
│   │   ├── SessionHeader (permission mode, stop/interrupt 常驻可见)
│   │   ├── SessionViewer
│   │   │   ├── TurnCard
│   │   │   │   ├── PlanHeader (optional)
│   │   │   │   ├── ActivityRow[] (tool calls, agent color 标注)
│   │   │   │   ├── ActivityGroupRow[] (sub-agent tasks)
│   │   │   │   └── ResponseCard (assistant text)
│   │   │   └── UserMessageBubble
│   │   ├── ApprovalStrip (pending permissions, 常驻可见)
│   │   ├── ContextTray (agent 将看到什么, 可勾选可预览)
│   │   └── SessionInput
│   │       ├── ReferenceChips (deixis 引用)
│   │       ├── TiptapEditor
│   │       └── SendButton + ModelSelector
│   │
│   └── DeixisIsland (portal, 任意 surface 选区上浮现)
│
├── PermissionDialog (模态强化, ApprovalStrip 是常驻入口)
└── ToastContainer (sonner)
```

### 5.4 状态管理（Jotai）

```typescript
// stores/sessions.ts

// 轻量列表
export const sessionMetaMapAtom = atom<Map<SessionId, SessionMeta>>(new Map())
export const sessionIdsAtom = atom<SessionId[]>([])

// Per-session 详细状态
export const sessionAtomFamily = atomFamily((id: SessionId) => {
  const base = atom<SessionState>({
    id,
    status: 'idle',
    turnPhase: null,
    messages: [],
    permissionMode: 'explore',
    backgroundTasks: [],
    annotations: [],
  })
  return base
})

// stores/panel.ts
export const panelStackAtom = atom<Panel[]>([])

// stores/annotations.ts
export const activeAnnotationAtom = atom<AnnotationState | null>(null)

// stores/inbox.ts —— 后台任务完成、gate partial/blocked、待批 permission
export const inboxItemsAtom = atom<InboxItem[]>([])
export const inboxUnreadCountAtom = atom((get) => get(inboxItemsAtom).filter(i => !i.read).length)
```

### 5.5 Turn 状态模型

每个 turn 的 UI 状态从事件流纯函数推导——可序列化、可重放、可测试。不在 event handler 里散装 mutate atoms。

```typescript
interface TurnState {
  turnId: string
  phase: TurnPhase
  activities: ActivityRowState[]
  responseText: string
  subTasks: SubTaskState[]
  permissions: PendingPermission[]
}

// 纯函数：event → TurnState 转换
function reduceTurnState(prev: TurnState, event: TurnEvent): TurnState
```

`sessionAtomFamily` 中的 `messages` 数组替换为 `turns: TurnState[]`，每个 turn 从事件流纯函数构建。历史 session 加载时，重放 `task_events` 即可还原完整 UI 状态。

**Renderer 无关性**：`TurnState` 是中间状态，不引用任何 DOM/React 概念。`renderWeb(state)` 之外，未来飞书卡片 `renderCard(state)`、移动 push `renderPush(state)` 消费同一状态机——对齐 lark-bridge 验证过的 `AgentEvent → RunState → render` 三段式。平台 payload 限制（卡片大小、交互组件）集中在各自 renderer 里消化，不回流到状态层。

### 5.6 WebSocket 事件处理

```
Backend → WS → EventProcessor → Jotai Atom 更新 → React re-render
                                    │
                                    └── Toast（如果需要通知）

事件类型：
  session:started   → 设置 session status = running
  turn:phase        → 更新 turnPhase
  tool:started      → push ActivityRow
  tool:completed    → 更新 ActivityRow status
  text:delta        → 追加到 ResponseCard buffer
  text:flush        → force flush buffer
  turn:completed    → 标记当前 TurnState 为最终态，停止接收该 turn 的增量更新（AuditHook 此时写 DB）
  permission:need   → 打开 PermissionDialog + ApprovalStrip 入列
  task:event        → 更新 sub-agent task 状态
  task:gate         → completion gate 状态（evaluating / passed / partial+理由）
  inbox:item        → 后台任务完成/降级 → Inbox badge + toast
  session:error     → 显示错误 + Toast
  session:completed → 设置 status = idle, turnPhase = complete
```

---

## 六、核心交互流程

### 6.1 新建 Session

```
用户点击 New Session → POST /api/sessions → 获得 sessionId
→ pushPanel({ type: 'session', sessionId })
→ SessionViewer mount → 从 sessionAtomFamily(sessionId) 初始化
→ 空 SessionViewer 显示 welcome message + 建议 prompt
→ 用户输入 → POST /api/sessions/:id/messages { text }
  → 201 { message_id, turn_id }（同步返回，不等 agent）
  → 前端立即插入 TurnCard skeleton（phase: pending）
  → 后续 WS 事件通过 turn_id 匹配并填充骨架
```

消息发送是同步 HTTP POST，不是 WS 消息。前端在收到 201 后立即渲染 skeleton，不等第一个 WS 事件——避免「用户发送消息后 UI 空白等第一个 event」的延迟感。`api/client.ts` 需同时暴露 `sendMessage()` HTTP 方法和 WS 事件监听。

### 6.2 Agent 执行中的 UI

```
用户消息 → POST 返回 turn_id → 前端立即 TurnCard skeleton → TurnPhase: pending
模型返回 tool_call → TurnPhase: tool_active
  → ActivityRow 显示 "Running search_codebase..."
  → ActivityRow 更新 "✓ Found 3 files"
tool 执行完 → TurnPhase: awaiting (等待下一个模型调用或完成)
模型返回 text → TurnPhase: streaming
  → ResponseCard 缓冲渲染
模型完成 → TurnPhase: complete
  → WS relay hook 持久化 transcript
```

### 6.3 追问流程（deixis 在 chat 面的特例，见 §4.1）

```
用户在 ResponseCard 选中文字 → mouseup 触发
→ 计算选择范围的 DOM rect
→ 计算指针速度向量
→ 设置 activeAnnotationAtom = { anchor, rects, text, turnId }
→ AnnotationIsland 从 anchor 位置以 entryDistancePx 动画进入
→ 用户输入追问 → WS send({ type: 'follow_up', text, context })
→ 新 TurnCard 追加到 SessionViewer
→ 标注设置为 sent 状态（透明度 0.58）
```

### 6.4 Permission 弹窗

```
Agent 调用 tool（requires approval） → WS 收到 permission:need
→ PermissionDialog 弹出（模态）
  显示：tool name, input, risk level
  按钮：Approve Once / Approve All / Deny
→ 用户选择 → WS send({ type: 'permission_response', ... })
→ 对话框关闭，agent 继续
```

同一请求同时进入 ChatLane 的 ApprovalStrip——模态被关闭/失焦后 approval 仍有常驻可见入口，不会「丢」。多 run / 多 sub-agent 并发请求时以 strip 为主视图，模态只针对当前聚焦 session（设计原则 6）。

---

## 七、不做的事

| 不做 | 原因 |
|------|------|
| **不自定义 Lexical** | Tiptap 做输入，react-markdown 做输出——不需要双向编辑器 |
| **不做复杂 annotation overlay** | Craft Agents 的 `AnnotationOverlayLayer` 是 ~500 行的 DOM 几何计算。先做 `<mark>` + CSS `::after` |
| **不做 velocity-based island 动画** | framer-motion 的 `initial/animate/exit` 足够。速度计算是 polish，不是 MVP |
| **不做 PDF/图片 annotation** | 文本标注优先 |
| **不做 workspace 文件树** | 先做 session 列表 + task board。文件树是 knowledge workspace 阶段的事 |
| **不做多主题** | 一个暗色主题 + 一个亮色主题。不提供自定义主题引擎 |
| **不做离线模式 UI** | 先做 always-online。离线缓存后续 |
| **不做 hover-only 的关键动作** | approval / cancel / permission mode 必须常驻可见（Craft Agents 可发现性教训） |
| **不做自由拖拽分栏** | v0 布局做死：Surface 区 tab/二分屏 + Chat Lane 固定右侧，先建立空间记忆 |
| **不做编辑器内核** | 人读审指，agent 写（ADR-001 D6）；Reader 用成熟件，编辑外链 |
| **不做角色档案 / GenUI 布局引擎** | 角色是涌现的——内容驱动 surface 激活；v2 用 layout preset（ADR-001 D2） |
