# Nimbalyst — Visual Workspace for Building with Codex and Claude Code

**仓库**: https://github.com/nimbalyst/nimbalyst
**一句话描述**: 一个免费、本地、交互式的可视化编辑器与 session 管理器，让开发者与 Codex 和 Claude Code 在文件、会话和任务上可视化协作。

## 概述

Nimbalyst 是一个 AI 原生的桌面应用（Electron + React + Lexical），面向使用 Codex、Claude Code、Opencode、Copilot 的开发者。它将 agent 会话、文件编辑、任务管理和团队协作统一到一个可视化工作空间中。

核心命题：不是另一个 terminal wrapper——是一个让人类和 AI agent 在同一个编辑器中**可视化协作**的工作环境。agent 的修改以红/绿差异显示，人类可以直接在所见即所得编辑器中审批、编辑、批注。

## 核心功能

### 可视化编辑器
内置多种编辑器，人类和 agent 在同一界面协作：
- Markdown（WYSIWYG）
- 带批注的线框图（Mockups）
- Mermaid 图表
- Excalidraw 手绘图
- CSV 表格
- 数据模型
- 代码（Monaco Editor）

所有编辑器通过同一个 `EditorHost` 契约接入，自定义编辑器与内置编辑器享有同等地位。

### Session 管理
- 支持多个并行 agent session
- Session 与文件双向关联
- 看板视图管理 session 状态
- Session 搜索和恢复
- 开发者专用功能：Git 状态管理、AI 提交、Ghostty 终端、Worktree

### 任务追踪
- 计划、bug、功能、待办事项统一管理
- Agent 可以编辑、添加、移动和执行任务
- 人类也可以直接编辑任务

### 移动端
- 原生 iOS（SwiftUI）和 Android（Capacitor）应用
- Session 看板：查看哪些 agent 需要你，哪些还在工作
- 文字/语音回复，agent 立即继续
- 可视化差异审查：滑动浏览变更，点击批准
- 下一任务排队：保持 pipeline 满载
- 推送通知：agent 需要你时主动通知

### 扩展系统
- `packages/extension-sdk` 提供扩展开发 SDK
- `packages/extensions` 包含内置扩展：Astro 网站编辑器、可视化 Git log、思维导图、幻灯片、3D 对象编辑器
- 扩展通过 `manifest.json` 注册，所有编辑器共享 `EditorHost` 接口

### 团队协作
- 端到端加密（客户端 AES-256-GCM，服务端零信任）
- 基于 Yjs CRDT 的实时文档同步
- 协作服务端（collabv3）是独立项目，部署在 Cloudflare Workers (Durable Objects)

## 架构

### 工作区结构（TypeScript/Electron monorepo，npm workspaces）

| 包 | 职责 |
|----|------|
| `packages/electron` | Electron 桌面应用（主进程 + 渲染进程） |
| `packages/runtime` | 跨平台运行时服务：AI 提供商、Lexical 编辑器、Jotai 状态管理、转录流水线、扩展加载器、Yjs 文档同步 |
| `packages/extension-sdk` | 扩展开发套件 |
| `packages/extensions` | 内置扩展 |
| `packages/ios` | 原生 iOS 应用（SwiftUI） |
| `packages/android` | Android 应用（Capacitor） |
| `packages/collab-protocol` | 协作同步协议的线格式类型（与同步服务器共享） |
| `packages/collab-adapters` | 协作适配器 |

### AI 提供商架构（packages/runtime）

双层抽象：

- **AIProvider**：底层模型访问（OpenAI、Anthropic 等）
- **AgentProtocol**：agent 会话管理（Codex、Claude Code、OpenCode、Copilot）

支持的 coding agent：
- Codex
- Claude Code
- Opencode（alpha）
- Copilot（alpha）

### 数据与状态

- **PGLite**（WebAssembly PostgreSQL）：本地数据持久化
- **Jotai atoms**：前端状态管理
- **Transcription pipeline**：两层存储——原始日志 → 规范化事件管道

### 协作架构

- 客户端：Yjs CRDT 实现实时文档同步，AES-256-GCM 加密
- 服务端：独立项目（nimbalyst-collab），Cloudflare Workers + Durable Objects
- 零信任：服务端无法解密协作内容

### 扩展架构

- 每个扩展 = 一个 npm 包，通过 `manifest.json` 注册
- 所有编辑器（包括内置）通过同一个 `EditorHost` 接口接入
- 扩展可以定义自定义文件类型、编辑器 UI、与 agent 的交互方式

## 关键设计决策

1. **本地优先** — 免费、本地运行的应用，不在云端处理用户文件内容
2. **编辑器即平台** — 不是 terminal wrapper，是真正的可视化编辑器。所有编辑器通过 `EditorHost` 契约实现可插拔
3. **双层 AI 抽象** — `AIProvider`（模型访问）+ `AgentProtocol`（agent 会话），支持多种 coding agent
4. **客户端加密协作** — AES-256-GCM 客户端加密，服务端零信任架构
5. **PGLite 作为本地数据库** — WebAssembly PostgreSQL，无需外部数据库依赖
6. **Agent 修改可审批** — agent 的所有变更以红/绿差异显示，人类审批后才保存
7. **移动端不是"只读监控"** — iOS 和 Android 原生应用支持完整的 session 管理、差异审查和任务排队
8. **扩展一等公民** — 内置编辑器与第三方扩展通过同一接口接入
9. **匿名遥测** — PostHog 收集使用分析（无 PII、无文件内容、无 API key），可选择退出

## 局限

1. **平台限制** — 桌面端通过 Electron，需要安装应用（不是纯 Web）
2. **AI agent 依赖** — 应用本身不包含 AI 能力，需要用户安装 Codex / Claude Code 等 agent CLI
3. **协作需要独立服务** — 同步服务器（collabv3）是独立项目，不是开箱即用的本地功能
4. **扩展生态刚起步** — 扩展数量和社区规模有限
5. **Copilot/Opencode 仍为 alpha** — 部分 agent 支持尚不稳定
6. **Electron 资源消耗** — 桌面应用的内存和启动时间高于纯 terminal 方案
7. **iOS/Android 功能不对称** — 移动端缺少部分桌面编辑器的完整能力
