# Craft Agents UI 技术栈与产品经验

> 研究对象：`/Users/emile/Develop/externel/agent/craft-agents-oss`
>
> 目的：总结 Craft Agents 的 UI 技术栈、视觉系统和 agent 产品界面经验，为 Orchest 未来基于 Web React + Tauri 的产品化前端提供参考。

## 结论

Craft Agents 的 UI 路线可以概括为：

```text
Electron desktop shell
  + Vite renderer
  + React 18
  + TypeScript
  + Tailwind CSS v4
  + shadcn/ui-style local components
  + Radix UI primitives
  + Jotai state
  + custom @craft-agent/ui package
```

它没有使用 AI SDK UI，也没有把核心聊天体验交给 assistant-ui。它选择自研 chat/session/turn card/markdown/overlay/annotation 等 agent 产品 primitives。这对 Orchest 很有参考价值：复杂 agent 产品的事件语义、运行状态和协作结构最好由产品自己掌控，通用 chat library 只能作为局部组件或交互参考。

对 Orchest 的建议是：采用类似的 React + Tailwind + shadcn/Radix + 自研 agent UI primitives 路线，但桌面壳使用 Tauri，而不是 Electron；前端状态的事实来源应是 Orchest `RuntimeEvent` 经过 product projection 后的 UI model。

## 技术栈事实

### App Shell

Craft Agents 使用 Electron 作为桌面壳：

- `apps/electron/package.json`：Electron app，主进程用 `esbuild` 打包，renderer 用 `vite build`
- `apps/electron/vite.config.ts`：Vite root 指向 `apps/electron/src/renderer`
- `apps/webui/src/App.tsx`：Web UI 是 thin wrapper，通过 web adapter 设置 `window.electronAPI`，再 lazy-load Electron renderer 的 `App`

这说明它的主要 UI 资产在 Electron renderer 中，Web UI 复用同一套组件，而不是独立重写。

### Frontend Framework

核心前端依赖：

- React 18
- TypeScript
- Vite
- Tailwind CSS v4
- `@vitejs/plugin-react`
- `@tailwindcss/vite`

`apps/electron/src/renderer/index.css` 使用 Tailwind v4 的 CSS-first 模式：

```css
@import "tailwindcss" source(none);
@plugin "@tailwindcss/typography";
```

并通过 `@source` 显式声明扫描路径，包括 Electron renderer 和 `packages/ui`。这适合 monorepo 内部共享组件。

### Component System

Craft Agents 使用 shadcn/ui 风格，但不是依赖一个黑盒组件库：

- `apps/electron/components.json` 存在 shadcn 配置
- style 为 `new-york`
- `rsc: false`
- `tsx: true`
- Tailwind 使用 CSS variables
- 组件实际落在 `apps/electron/src/renderer/components/ui/*`

底层 primitive 主要来自 Radix：

- dialog
- dropdown menu
- popover
- context menu
- switch
- tabs
- tooltip
- scroll area
- select
- collapsible

其他 UI 基础库：

- `lucide-react`：图标
- `sonner`：toast
- `motion`：动画
- `cmdk`：command/menu 搜索体验
- `@dnd-kit/*`：拖拽排序
- `@tanstack/react-table`：表格
- `vaul`：drawer

### State

状态管理主要使用 Jotai：

- `apps/electron/src/renderer/atoms/sessions.ts`
- `apps/electron/src/renderer/atoms/sources.ts`
- `apps/electron/src/renderer/atoms/skills.ts`
- `apps/electron/src/renderer/atoms/panel-stack.ts`
- `apps/electron/src/renderer/atoms/browser-pane.ts`

从 `App.tsx` 可以看到它把 session、source、skill、panel stack、background tasks 等状态拆到 atom 或 atom family，而不是塞进一个全局 store。它还显式避免把完整 session 数组保存在一个大 atom 里，以降低内存泄漏和过量 rerender 风险。

### Shared UI Package

`packages/ui` 是最值得研究的部分。它是内部共享 React UI package，承载了 agent 产品 primitives：

- chat display
- session viewer
- turn card
- markdown rendering
- overlay previews
- terminal output
- code viewer
- annotation island
- file classification
- layout constants

核心目录：

```text
packages/ui/src/components/chat/
packages/ui/src/components/markdown/
packages/ui/src/components/overlay/
packages/ui/src/components/terminal/
packages/ui/src/components/code-viewer/
packages/ui/src/components/annotations/
packages/ui/src/components/ui/
```

这对 Orchest 的启发是：不要把 agent UI 直接写散在 app 页面里。应尽早抽出类似 `packages/ui` 或 `orchest-ui` 的内部包，承载可复用的 runtime-event rendering primitives。

## Agent UI Primitives

### Session / Turn Model

Craft Agents 不是直接把所有内容渲染成普通 chat messages，而是把消息 group 成 turns：

- user message
- system message
- assistant turn
- activity item
- tool activity
- thinking / intermediate / status / plan activity
- background task
- todo visualization
- parent-child nesting for task / subagent display

参考文件：

- `packages/ui/src/components/chat/SessionViewer.tsx`
- `packages/ui/src/components/chat/TurnCard.tsx`
- `packages/ui/src/components/chat/turn-utils.ts`

这点对 Orchest 很重要。Multi-agent 产品不能只依赖 `message[]`。我们至少需要这些 UI model：

- session
- run
- turn
- agent
- sub-agent edge
- activity
- tool call
- approval
- artifact
- human annotation / comment

### TurnCard

`TurnCard` 是 Craft Agents 的核心 agent UI 单元。它把一个 assistant turn 的活动、结果、计划、tool call、diff stats、annotation、background task 都压缩在一个可展开的卡片中。

优点：

- 信息密度高
- tool activity 不直接污染主回答
- 支持折叠，能隐藏低价值中间过程
- 适合回放和 review

风险：

- 状态太多时，用户不容易理解“现在 agent 到底在等什么”
- 折叠层级、hover action、annotation island、details overlay 叠加后，交互学习成本高
- 如果没有明确的 run / agent graph，multi-agent 会被压平成一串 turn cards

Orchest 可以借鉴 turn card 的视觉密度，但要给 multi-agent 增加更清晰的空间模型。

### Overlay / Artifact System

Craft Agents 为 agent output 做了大量 preview overlay：

- code preview
- multi-file diff
- PDF preview
- image preview
- HTML preview
- JSON preview
- terminal preview
- mermaid preview
- formatted markdown document overlay

参考文件：

- `packages/ui/src/components/overlay/*`
- `packages/ui/src/components/code-viewer/*`
- `packages/ui/src/components/markdown/*`

这说明 agent 产品不能只做聊天。文件、diff、terminal、markdown、image、table 都是一等输出面。Orchest 产品也应该把 artifact panel 作为核心区域，而不是把 artifact 当作聊天附件。

## 视觉系统经验

### 低饱和工作台风格

Craft Agents 的 UI 整体是低饱和、低边框、低阴影、信息密集的工作台风格。它不是 marketing SaaS 风格，也不是卡片堆叠式 dashboard。

主要特征：

- 大量使用 `foreground / background` 派生 token
- 控制色彩数量，核心色约为 background、foreground、accent、info、success、destructive
- 使用 OKLCH 与 `color-mix()` 生成层级
- 使用 `shadow-minimal` 形成轻边界，而不是重阴影
- panel、dropdown、card 的边界很轻
- 图标尺寸偏小，文本尺寸偏小，适合高密度工作

这适合 Orchest：multi-agent 协作产品需要长期使用和快速扫视，不应做成大 hero、大卡片、大装饰背景的视觉语言。

### CSS Token Discipline

`apps/electron/src/renderer/index.css` 中定义了大量 CSS custom properties：

- color tokens
- shadow tokens
- z-index scale
- font tokens
- layout tokens
- shadcn compatibility tokens

它还用自定义 ESLint rule 管 z-index 和 shadow：

- `packages/ui/eslint-rules/no-hardcoded-z-index.cjs`
- `packages/ui/eslint-rules/no-nonstandard-shadows.cjs`
- `packages/ui/eslint-rules/no-floating-z-tokens-in-island.cjs`

这点非常值得借鉴。Agent 产品会有大量 overlays、menus、drawers、diff viewers、annotation islands。没有 z-index 和 overlay discipline，后期会快速失控。

## UX 风险与反面经验

用户感觉“UI 好看但 UX 难用”，从代码结构上能看到一些可能原因。

### 过多隐式交互

Craft Agents 使用很多 hover、alt-click、nested menu、context menu、annotation island、overlay、compact mode。视觉上很精致，但可发现性差。

Orchest 应避免把关键动作藏在 hover-only 或组合键里。尤其是：

- approval
- cancel / interrupt
- agent handoff
- tool retry
- artifact accept / reject
- permission mode change

这些动作必须有稳定、可见、可解释的位置。

### Panel Stack 复杂度高

Craft Agents 有 sophisticated panel stack：

- left sidebar
- navigator panel
- main content panel
- panel stack container
- compact panel transition
- focused mode
- mobile compact behavior

这让界面很灵活，但也容易让用户迷路。Orchest 如果做 multi-agent，应优先固定信息架构：

```text
Left: sessions / tasks
Center: selected run workspace
Right: inspector / approvals / artifacts
Bottom or inline: composer
```

先让用户形成空间记忆，再逐步加入可折叠和多 panel。

### Chat-Centric Bias

Craft Agents 的核心仍然围绕 session chat 和 turn card。对单 agent 或 Claude Code-like 体验足够，但 multi-agent 协作需要额外抽象：

- agent roster
- run graph
- ownership / assignee
- blocked / waiting state
- human intervention queue
- sub-agent provenance
- cross-agent artifact dependency

Orchest 不能只复制 chat UI。我们应该把 chat 视为 human-agent communication lane，而不是整个产品的根模型。

## 对 Orchest 的建议

### 技术路线

采用：

```text
Tauri desktop shell
  + React
  + TypeScript
  + Vite
  + Tailwind CSS
  + shadcn/ui local components
  + Radix primitives
  + Jotai or Zustand
  + custom orchest-ui package
```

不采用：

- Electron 作为默认桌面壳
- AI SDK UI 作为产品协议
- assistant-ui 作为核心状态模型
- Rust GUI 作为主产品 UI

assistant-ui 可以只作为局部 chat primitive 参考；如果它压制 runtime event semantics，就不用。

### UI Package Boundary

建议未来建立：

```text
apps/desktop/              # Tauri + React app
apps/web/                  # optional web host
packages/ui/               # design system + agent UI primitives
packages/runtime-client/   # RuntimeEvent transport + state projection
packages/product-model/    # session/run/agent/artifact frontend types
```

`packages/ui` 不应直接理解 Rust core 内部结构，而是消费稳定的 product model。

### RuntimeEvent Projection

Craft Agents 的 `TurnCard` 经验说明，agent UI 需要一个中间 projection 层。Orchest 应显式设计：

```text
RuntimeEvent[]
  -> RunTimeline
  -> AgentGraph
  -> ApprovalQueue
  -> ArtifactIndex
  -> ChatTranscript
```

不要把 `RuntimeEvent` 直接一条条渲染，也不要先转换成通用 chat messages 再丢失结构。

### MVP Layout

建议第一个产品 spike 做这个结构：

```text
┌──────────────┬──────────────────────────┬─────────────────────┐
│ Sessions     │ Run Workspace            │ Inspector           │
│ Tasks        │ - conversation lane      │ - agent graph       │
│ Filters      │ - timeline / turn cards  │ - approvals         │
│ Sources      │ - active task bar        │ - artifacts         │
└──────────────┴──────────────────────────┴─────────────────────┘
```

借鉴 Craft Agents：

- 低饱和工作台视觉
- 高密度列表
- 轻边界 panel
- turn / activity 卡片
- artifact overlays
- markdown / code / diff 渲染

主动规避：

- 关键动作隐藏在 hover / context menu
- 过早引入复杂 panel stack
- 把 multi-agent 压平成 chat transcript
- overlay / z-index 缺少统一规范

## 可借鉴清单

- React + Tailwind + shadcn/Radix 是适合 agent workbench 的 UI 基础
- 自研 agent UI primitives 是必要的，不能完全依赖通用 chat library
- `TurnCard` 是表达 agent intermediate work 的有效形态，但要补 multi-agent graph
- Artifact preview 是核心产品面，不是附件功能
- CSS token、z-index、shadow discipline 应从早期建立
- Web UI 和 desktop UI 可以共享大部分 renderer/component 代码
- Thin web wrapper 复用 desktop renderer 的思路值得参考，但 Orchest 可通过 Tauri + web server adapter 实现更清晰的边界

## 参考文件

- `/Users/emile/Develop/externel/agent/craft-agents-oss/apps/electron/package.json`
- `/Users/emile/Develop/externel/agent/craft-agents-oss/apps/webui/package.json`
- `/Users/emile/Develop/externel/agent/craft-agents-oss/apps/electron/vite.config.ts`
- `/Users/emile/Develop/externel/agent/craft-agents-oss/apps/electron/components.json`
- `/Users/emile/Develop/externel/agent/craft-agents-oss/apps/electron/src/renderer/index.css`
- `/Users/emile/Develop/externel/agent/craft-agents-oss/apps/electron/src/renderer/App.tsx`
- `/Users/emile/Develop/externel/agent/craft-agents-oss/apps/electron/src/renderer/components/app-shell/AppShell.tsx`
- `/Users/emile/Develop/externel/agent/craft-agents-oss/packages/ui/package.json`
- `/Users/emile/Develop/externel/agent/craft-agents-oss/packages/ui/src/components/chat/SessionViewer.tsx`
- `/Users/emile/Develop/externel/agent/craft-agents-oss/packages/ui/src/components/chat/TurnCard.tsx`
- `/Users/emile/Develop/externel/agent/craft-agents-oss/packages/ui/src/lib/layout.ts`
