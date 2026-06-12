# Multivac 前端设计参考

> 2026-06-01 | 参考源：Claude macOS app (Epitaxy 设计体系), Craft Agents OSS (组件架构), Codex macOS app (插件系统)

---

## 一、设计原则

1. **安静** — 界面不抢注意力。Agent 在工作时用户应该感觉到「事情在被处理」，而不是被 UI 噪音打断。
2. **渐进式披露** — 默认折叠，按需展开。参考 Craft Agents 的 TurnCard：tool calls 折叠为一行，点开才看细节。
3. **空间即状态** — panel stack 的推入/弹出反映用户的工作深度。不是 tab，不是抽屉——是「我进入了这个 session，现在在这里」。
4. **动效表达物理关系** — 参考 Craft Agents 的 Island 菜单：从文本选择点发出，速度决定动画距离。不是花哨，是指引注意力。
5. **每 session 独立** — 状态管理用 Jotai atom family，不是全局 store。不同 session 的状态互相不可见。
6. **关键动作永远可见** — approval、cancel/interrupt、permission mode 切换、artifact accept/reject 不藏在 hover、context menu 或组合键里。Craft Agents 的反面教训：视觉精致但可发现性差，「UI 好看但 UX 难用」的主要来源就是过多隐式交互。
7. **固定信息架构先于灵活 panel** — 先让用户形成空间记忆（左：列表；中：工作区；右：inspector），再逐步引入 panel 折叠与推入。Panel stack 的灵活性只在中区生效。

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

## 四、信息架构：三区布局

Craft Agents 的反面经验：sophisticated panel stack + 隐式交互让界面灵活但用户迷路；同时它的 chat-centric 偏置把 multi-agent 压平成一串 turn cards。Multivac 的回答是固定三区布局——**chat 是 human-agent communication lane，不是产品根模型**；Task / Approval / Artifact / sub-agent 关系在 Inspector 有独立于聊天流的稳定位置（这正是「execution control plane 而非聊天插件」在 UI 上的体现）。

```
┌──────────────┬──────────────────────────────┬─────────────────────┐
│  Left Rail   │  Center: Run Workspace        │  Right: Inspector   │
│              │                               │                     │
│  Sessions    │  PanelStack                   │  Pending Approvals  │
│  Tasks       │   - SessionViewer             │  Artifacts          │
│  Inbox       │   - TurnCard timeline         │  Sub-agent Tree     │
│  Knowledge   │   - SessionInput              │  Task / Runtime     │
│              │                               │  Projection         │
└──────────────┴──────────────────────────────┴─────────────────────┘
```

- **Left Rail**：四个一级入口——Sessions（对话列表）、Tasks（task board，独立于 session 的执行真相）、Inbox、Knowledge。每个 item 是 icon + label + 状态点，当前选中高亮。底部用户头像 + 设置。
- **Center**：panel stack 的领地（§3.5 的物理栈模型只发生在这里）。Session、Task 详情、Knowledge 文档都以 panel 推入。
- **Inspector**：执行真相的常驻视图——approval queue、artifact index、当前 session 的 sub-agent provenance 树、task lifecycle + runtime projection。可折叠，默认展开。Approve/Deny、Stop/Interrupt、permission mode 是 Inspector 与 SessionHeader 上的**常驻可见控件**，PermissionDialog 只是它们的模态强化，不是唯一入口。
- **Inbox**：后台 task / background sub-agent 完成（或 gate 判定 partial/blocked）后的非侵入式通知面——sidebar badge + 列表，点击 pushPanel 进入对应 session/task。Daemon 模式下 agent 在窗口关闭后仍在工作，Inbox 是用户回来时「发生了什么」的入口。

MVP 先做死这个布局，不提供自由拖拽分栏；空间记忆建立后再考虑 compact mode 等弹性。

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

### 5.2 色彩体系（借鉴 Epitaxy + 自建）

```css
:root {
  /* 语义色 */
  --color-primary: #3b82f6;
  --color-primary-rgb: 59, 130, 246;
  --color-info: #6366f1;
  --color-info-rgb: 99, 102, 241;
  --color-success: #22c55e;
  --color-success-rgb: 34, 197, 94;
  --color-warning: #f59e0b;
  --color-warning-rgb: 245, 158, 11;
  --color-error: #ef4444;
  --color-error-rgb: 239, 68, 68;

  /* 表面色 */
  --background: #ffffff;
  --foreground: #0f172a;
  --surface: #f8fafc;
  --surface-secondary: #f1f5f9;
  --border: #e2e8f0;

  /* 暗色 */
  &[data-theme="dark"] {
    --background: #0f172a;
    --foreground: #f1f5f9;
    --surface: #1e293b;
    --surface-secondary: #334155;
    --border: #475569;
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

TurnCard 中每个 ActivityRow / ActivityGroupRow 标注执行者 agent 的 icon + color；同一颜色贯穿 Inspector 的 sub-agent 树和 Inbox 条目。

**Overlay 纪律**：z-index、shadow 全部 token 化（`--z-overlay` / `--z-island` / `--shadow-minimal` 等量表），配 ESLint 自定义规则禁止硬编码。Craft Agents 用 `no-hardcoded-z-index` / `no-nonstandard-shadows` 管住了 overlay 失控——agent 产品的 overlay、menu、island、diff viewer 数量多，这条纪律必须从第一天建立。

### 5.3 组件树

```
MultivacApp
├── AppShell
│   ├── Sidebar (Left Rail)
│   │   ├── UserAvatar
│   │   ├── SessionList
│   │   │   └── SessionItem (icon, title, status dot, timestamp)
│   │   ├── TaskList (task board 入口)
│   │   ├── InboxList (badge = inboxUnreadCount)
│   │   ├── KnowledgeNav
│   │   ├── OrgSwitcher (如果是 SaaS 模式)
│   │   └── BottomActions (settings, help, feedback)
│   │
│   ├── MainPanel
│   │   ├── PanelStack
│   │   │   ├── SessionView
│   │   │   │   ├── SessionHeader (title, permission toggle, actions)
│   │   │   │   ├── SessionViewer
│   │   │   │   │   ├── TurnCard
│   │   │   │   │   │   ├── PlanHeader (optional)
│   │   │   │   │   │   ├── ActivityRow[] (tool calls)
│   │   │   │   │   │   ├── ActivityGroupRow[] (sub-agent tasks)
│   │   │   │   │   │   └── ResponseCard (assistant text)
│   │   │   │   │   └── UserMessageBubble
│   │   │   │   ├── AnnotationIsland (portal, floats above text)
│   │   │   │   └── SessionInput
│   │   │   │       ├── AttachmentBar
│   │   │   │       ├── TiptapEditor
│   │   │   │       └── SendButton + ModelSelector
│   │   │   │
│   │   │   ├── TaskView (panel push from session)
│   │   │   └── KnowledgeView (panel push from session)
│   │   │
│   │   └── OverlayHost (portal target)
│   │       ├── CodePreviewOverlay
│   │       ├── DiffPreviewOverlay
│   │       ├── JSONPreviewOverlay
│   │       └── ImagePreviewOverlay
│   │
│   ├── InspectorPanel (right, 可折叠)
│   │   ├── ApprovalQueue (常驻 approve/deny 控件)
│   │   ├── ArtifactIndex
│   │   ├── SubAgentTree (agent color + provenance)
│   │   └── TaskRuntimeStatus (lifecycle + runtime projection)
│   │
│   └── TransportStatusBar (连接状态, 仅 cloud 模式)
│
├── PermissionDialog (全局, approval gate 触发)
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
  permission:need   → 打开 PermissionDialog + Inspector approval queue 入列
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

### 6.3 追问流程

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

同一请求同时进入 Inspector 的 ApprovalQueue——模态被关闭/失焦后 approval 仍有常驻可见入口，不会「丢」。多 session / 多 sub-agent 并发请求时以 queue 为主视图，模态只针对当前聚焦 session（设计原则 6）。

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
| **不做自由拖拽分栏** | 三区固定布局先建立空间记忆；panel stack 限定在中区 |
| **不把 multi-agent 压平成 chat transcript** | sub-agent provenance / task / artifact 在 Inspector 有独立稳定位置 |
