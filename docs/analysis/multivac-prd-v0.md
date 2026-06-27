# Multivac PRD v0：自己的 Agent 创作工作台

> 版本：v0.2 draft
> 日期：2026-06-26
> 状态：初版 PRD 的 review 修订版，用于把 `docs/analysis` 中的定位、视觉、前端、架构文档收敛成可拆 issue 的产品定义
> 上游依据：[`adr-001-product-positioning.md`](./adr-001-product-positioning.md)、[`multivac-design.md`](./multivac-design.md)、[`multivac-frontend-design.md`](./multivac-frontend-design.md)、[`multivac-reconstruction-analysis.md`](./multivac-reconstruction-analysis.md)、[`external-research-synthesis.md`](./external-research-synthesis.md)、[`mimo-code-analysis.md`](./mimo-code-analysis.md)、[`create-squad-salvage.md`](./create-squad-salvage.md)
>
> 命名说明：Multivac 仍是工作代号，不是最终公开产品名。公开发布前必须重新评估命名、商标、SEO 与竞品撞名风险。

---

## 0. 裁决摘要

### 0.1 产品定位

**Multivac 是个体与 agent 协同的创作工作台：协作语法统一，创作介质可插拔，知识是工作的自动沉淀。**

更具体地说：

> Multivac 不是“另一个 agent 聊天窗口”，不是“AI 员工管理后台”，也不是“团队知识库”。它是一张以 Workspace 为根的创作桌：人、agent、文件、git、终端、browser、diff、产物、历史决策都在同一个现场里互相可见、可指、可审查、可沉淀。

### 0.2 v0 要证明什么

v0 对应 ADR 里的 **M1「自己的工作台」**：证明介质论，而不是证明团队协作、云 SaaS、多 runtime 或全自动 avatar。

换言之，v0 只验证 ADR §0.7 两阶段工作流里的 **阶段一 attended 共创**：真人填输入槽、真人路由、真人审查与转向。阶段二 unattended 托管执行、avatar 驱动、移动遥控与自动验收闭环属于 M2，不是 v0 的产品承诺。

v0 成立的标准：创始用户可以连续数周把真实开发/创作工作放进 Multivac，并且高频发生“指代替说”、diff 审查、跨 session 引用、自动沉淀复用。

### 0.3 v0 一条线

- **Host**：一个独立 daemon at `host:port`，host 取值为本机或用户自有 SSH / Tailscale 可达盒子。
- **Client**：web-first 瘦客户端，通过 `BACKEND_URL` 连接 daemon；当 host 经 LAN / SSH / Tailscale 可达时，另一台机器上的 web client 可连接同一 daemon 并 replay events 接续工作。原生壳、移动 app 与产品化 relay 不是 v0 阻塞项。
- **Storage**：SQLite + 本地文件系统。
- **Runtime**：Claude Code 单 runtime，走受支持的 headless / stream-json / SDK 类接口，转译为 normalized `TaskEvent`。
- **Workspace**：单 workspace。
- **Surface**：对话、文件/预览、git/diff、终端、browser 五个面必须进入 v0 的产品验证面。
- **Kernel**：Locator、Context Tray、Context Composer、TaskEvent、TurnState、run 前快照、写锁、脱敏、导出。

### 0.4 v0 明确不证明什么

- 不证明团队知识库。
- 不证明 IM 入口。
- 不证明多 CLI runtime。
- 不证明 mobile app。
- 不证明 cloud / managed runtime。
- 不证明 Orchest 的 supervised delegation 深度 API。
- 不证明生成式 UI / 多角色工作台。

---

## 1. 背景与原始问题

### 1.1 两个创世卡点

1. **“我想同时拥有 Gemini Canvas 和 Claude Code。”**
   Chat、实时画布、代码 agent、browser preview 与终端不应该是互相看不见的产品。

2. **“我希望 Codex 能看到我和 Claude Code 的交互。”**
   跨厂商 agent 共享上下文是任何单一厂商都不会主动解决的结构性空白。第三方工作台的机会不在重新做一个模型，而在建立中立的 session / task / artifact / knowledge 对象层。

### 1.2 当前替代方案的缺口

| 替代方案 | 用户得到什么 | Multivac 看到的缺口 |
|---|---|---|
| 单厂商 Chat + Canvas | 对话和单一画布 | 跨 agent / 跨 runtime 上下文不可移植；执行过程仍然黑盒 |
| Coding agent CLI / IDE 插件 | 强代码执行能力 | 以 IDE 或 terminal 为中心，缺少可指代、可沉淀、跨 session 的工作台对象层 |
| 多 CLI session 管理器 | 并排跑多个 agent | 多 runtime 不等于共享上下文；session log 不等于可复用知识对象 |
| 企业协作套件 / IM agent | 分发、组织、审批 | 平台主场在入口与组织容器，不在本地执行现场 |
| 团队知识库 + RAG | 事后检索 | 写知识是额外税；没有工作现场，知识库冷启动 |
| AI 员工 / manager 模型 | 指派任务、看结果 | 人被推到审批者位置，失去创作中的作者位 |

### 1.3 核心判断

1. **瓶颈在共享上下文，不只在模型智能。** 高质量协作依赖“指着同一个东西说话”。
2. **协作语法稳定，创作介质可变。** 委派、指代、审查、裁决、沉淀在代码、文档、网页、视频、音频里都成立；变化的是 Locator 与 Surface adapter。
3. **知识是工作的副产品。** 可持续知识库不是让用户额外维护 wiki，而是让任务、对话、diff、产物、决策自然落成可寻址对象。
4. **Execution control plane 是基础设施，不是产品口号。** 产品对用户的形态是 workbench；task/runtime/artifact/event 是支撑这个 workbench 的内核。
5. **人读、审、指；agent 写。** Multivac 不和 IDE 拼人体工学，不重建 VSCode/Cursor；人的主动作是阅读、指代、裁决、回滚、转向。

---

## 2. 目标用户

### 2.1 首批用户

**高强度使用 coding agent 的独立开发者、产品型创始人、技术创作者。**

他们的共同特征：

- 已经在用 Claude Code / Codex / OpenCode / Cursor 等工具。
- 工作根对象是 repo / workspace，而不是一条聊天记录。
- 经常在文件、git diff、终端、browser、文档之间切换。
- 最大痛点不是“没有 agent”，而是“agent 彼此看不见、上下文散落、执行过程不可控、历史难复用”。

### 2.2 v0 Persona

#### Persona A：独立产品开发者

- 打开一个本地 git repo 开始工作。
- 让 Claude Code 修 bug、跑测试、改 UI。
- 希望把 browser console error、diff hunk、终端输出、之前 session 的结论直接指给 agent。
- 希望随时知道 agent 在做什么、改了什么、为什么等确认。

#### Persona B：技术型创始人 / maintainer

- 同时推进代码、文档、产品决策。
- 需要把每天的 agent 工作沉淀成后续可复用上下文。
- 需要在不同 session 之间迁移决策，而不是复制长 transcript。
- 对粗糙 UI 容忍度高，但对执行透明度、可回滚、数据归属要求高。

#### Persona C：未来创作者种子人群

- 今天表现为 builder / developer。
- 长期会把“可运行的东西”当作表达形式。
- v0 只服务其代码 / web 创作部分，但内核必须能扩展到文档、网页、视频、音频等 surface。

---

## 3. 产品原则

### P1. Workspace 是根对象

系统真相从 Workspace 开始。v0 的 Workspace 是一个本地目录或 git repo，包含：

- 文件树与 git 状态。
- sessions / turns。
- tasks / runs / events。
- terminal / browser 状态。
- artifacts / diffs。
- 自动沉淀的 knowledge objects。

Session 是通信 lane，Task 是执行记账，Knowledge 是工作投影；它们都挂在 Workspace 下。

### P2. Workbench = Surface 区 + Chat Lane

v0 不做自由布局，也不以空白聊天框为中心。固定信息架构：

- 左侧：Workspace / Run / Inbox 导航。
- 中间：Surface 区，承载文件/预览、git/diff、终端、browser。
- 右侧：Chat Lane，承载 TurnCard、Approval、Context Tray、输入框。

空间记忆先于灵活 panel；v0 布局做死，后续再引入折叠、推入和 layout preset。

### P3. Deixis 是第一交互原语

用户选中任何 surface 上的对象：文本、file:line、diff hunk、terminal 行、DOM 元素、session turn、task output，都应变成引用 chip 进入输入框。

目标是让用户少说“请看上面那个报错”，多做“指这个”。

### P4. Context 对人可见

agent 将看到什么必须在发送前可见、可勾选、可预览。Context Tray 是信任、权限、token budget 和跨厂商数据流向控制的共同起点。

### P5. 执行透明度是信任机制

TurnCard / ActivityRow / ApprovalStrip 不是装饰，而是告诉用户：

- agent 计划做什么。
- agent 调了什么工具或 runtime。
- 哪些步骤失败或重试。
- 哪些文件、diff、artifact 被影响。
- 哪些动作需要确认。
- run 完成 / 失败 / 取消及用户验收状态如何。

### P6. 先撤销，后 review

v0 阶段 agent 就可能写真实文件，因此 run 前快照与一键回滚必须先于复杂 review 系统进入内核。便宜的后悔药是建立信任的底线。

### P7. 沉淀必须安全、可导出、可重放

厚度来自长期沉淀，因此 v0 必须从第一天保证：

- 先脱敏，后落盘。
- events 带 schema version。
- `knowledge://` / Locator 长期可解析。
- 删除用 tombstone，不破坏历史重放。
- 一键导出 JSONL + 文件。

### P8. 视觉是一台安静的仪器

视觉基调遵守 `multivac-design.md`：**安静的书房 × 终端的血统**。暖纸、墨色、琥珀是定位延伸，不是装饰皮肤。琥珀只用于 deixis、高亮焦点、品牌时刻，不能大面积铺装。

---

## 4. v0 产品范围

### 4.1 必须包含

#### 4.1.1 Kernel / Control Plane

1. **独立 daemon**
   - daemon 监听 `host:port`。
   - web client 通过 `BACKEND_URL` 连接。
   - 关闭浏览器或未来桌面窗口不应杀掉正在运行的 task。

2. **SQLite 本地持久化**
   - 最小核心表：`workspaces`、`sessions`、`events`、`objects`。
   - `events` 用于事件溯源和 TurnState replay。
   - `objects` 是 Locator 注册表：URI + provenance + scope。

3. **Workspace binding**
   - 用户选择本地目录 / git repo。
   - 记录 workspace fingerprint、默认工作目录、git 状态。
   - v0 单 workspace；多 workspace switcher 可有壳但不承诺完整体验。

4. **RuntimeBackend + Claude Code 单 runtime**
   - 使用受支持的 Claude Code headless / stream-json / SDK 类接口。
   - runtime 原始事件转译为 normalized `TaskEvent`。
   - 前端与 knowledge 层不得依赖 Claude JSONL 原始 schema。
   - 用户使用自己的 Claude 订阅 / 登录 / API key；Multivac v0 不代理计费。

5. **Task / Run 生命周期**
   - `queued` / `running` / `waiting_approval` / `completed` / `failed` / `cancelled` / `interrupted`。
   - 每个 run 关联 session、workspace、runtime、events、artifacts。

6. **写锁与并发模型**
   - 同一 workspace 同时只有一个持写权 run。
   - 并行 session 默认只读探索。
   - 用户可以显式等待、取消或转移写权。

7. **run 前快照与回滚**
   - 每个写 run 开始前自动创建 workspace snapshot。
   - git repo 优先用 commit / stash 类机制；非 git 目录用影子快照。
   - UI 提供一键回滚到 run 前状态。

8. **脱敏与数据流向 policy**
   - events / objects 持久化前经过 secret / token / key 模式脱敏。
   - 支持路径 deny-list，例如 `.env`、credential 文件、私钥目录。
   - 把一个厂商 runtime 的 transcript 交给另一个厂商必须是用户明示选择，不由自动 context 默认跨发。

9. **一键导出**
   - 用户可导出 workspace 的 events / objects / summaries / metadata 为 JSONL + 文件包。
   - 导出是 v0 对“用户拥有自己的沉淀”的产品承诺。

10. **本地实验遥测**
    - 记录指代次数、沉淀引用率、Context Tray 修改率、回滚次数。
    - 本地明文可查，不默认上传。

#### 4.1.2 Workbench Surfaces

Surface 不是各自为政的面板集合。v0 的每个 surface 都必须实现同一组三件套合同：

- **Renderer**：把该介质渲染为人可读、可操作的工作面。
- **Deixis adapter**：把用户选区 / 对象转成 Locator / Reference Chip，并能从 Locator 重新定位原对象。
- **Perception adapter**：把授权范围内的状态 / 事件转成机器可读摘要或 `TaskEvent`，供 Context Composer 与 agent 感知。

后续新增视频、音频、文档等 surface 时，只能扩展这组三件套，不能绕过 Locator / Context Composer / TaskEvent 另起临时代码路径。

1. **Chat Lane / TurnCard**
   - HTTP POST 创建 skeleton。
   - WebSocket 事件流填充 TurnState。
   - TurnCard 展示 plan、activity rows、response、approval、artifact links。

2. **File Surface**
   - 文件树 + 文件打开 / 搜索 / 最近文件。
   - 必须能查看 markdown、HTML、PDF、CSV、图片，并支持音频播放；代码文件也要能阅读。
   - 编辑功能可以弱化，但 v0 应提供简单文本编辑与保存；复杂编辑、重构、IDE 级能力外链到用户现有编辑器。
   - 文件操作必须包含复制路径：复制相对 workspace 路径、复制绝对路径，并能把路径作为 Reference Chip 插入输入框。
   - 支持类似 VS Code 的“从剪贴板粘贴创建文件”能力：剪贴板里是文件 / 图片 / 文本片段时，可在当前目录快速创建对应文件。
   - 所有可查看内容都应可被选中并生成 Locator；音频至少支持以时间区间生成 Locator。
   - 音频在 v0 只要求播放、时间定位与引用；转写、合成和音频生成不属于 v0。

3. **Git / Diff Surface**
   - 展示当前 branch、worktree 状态、git status、log、diff。
   - diff 是 review 主视图，但 git 面不只服务 review；它也是用户理解 workspace 当前真相的主面。
   - 支持简单 stage / unstage、commit、push、pull；冲突解决与复杂 rebase / cherry-pick 外链或后置。
   - 所有 git 操作必须写入 TaskEvent / audit trail，且 destructive / remote-changing action 需要明确确认。

4. **Terminal Surface**
   - xterm.js 共享 PTY，目标是全功能终端，而不是只读日志窗口。
   - 支持用户正常交互：shell、全屏 TUI、快捷键、resize、scrollback、复制粘贴。
   - replay buffer 与 scrollback 内容可以被 agent 捕获、摘要、引用；用户选中的 terminal 行区间可生成 Locator。
   - agent sideband input 带 attribution，避免和用户键盘输入混淆。

5. **Browser Surface**
   - v0 必须有 browser 面进入验证闭环，因为 web 创作的“可感知”是差异化招牌。
   - 最小能力：URL、页面截图或 screencast、console error、DOM 摘要中的至少一条可被 Locator 引用。
   - 深度 CDP / Electron / Tauri sidecar Chromium 的路线可 spike 后定，但不能把 browser surface 整体移出 v0。

#### 4.1.3 Deixis / Context

1. **Locator 系统**
   - 支持 file path、file range、diff hunk、terminal range、browser console / DOM、audio time range、session turn、task output、knowledge object。
   - Locator 可展示、复制、持久化、重新定位，并可作为 Reference Chip 注入 prompt。

2. **Reference Chips**
   - 用户选中 surface 对象后生成 chip。
   - chip 出现在输入框上方，可删除、预览、展开。

3. **Context Tray**
   - 发送前展示 agent 将看到的内容。
   - 内容分为常驻环境摘要、用户指代对象、按需查询工具。
   - 用户可勾选、取消、预览。

4. **Context Composer v1**
   - 常驻低成本摘要：打开文件、git dirty 状态、dev server 端口、当前 session/task metadata。
   - 精确注入：用户 chip 指向的内容。
   - 按需查询：`read_file`、`read_terminal`、`git_status`、`inspect_browser` 类工具。
   - 带 token budget，不全量注入 transcript。

5. **Session-as-context**
   - session / turn 本身是 Locator 对象。
   - v0 允许把 Session A 的 summary / decision / selected turns 注入 Session B。
   - v0 表现为 Claude↔Claude 并行 session 共享上下文；第二 runtime 接入后升级为跨厂商。

#### 4.1.4 Knowledge Projection

1. **Task post-process**
   - completed / failed / cancelled run 都产生结构化 summary。
   - 至少包含：目标、结果、changed files、decisions、open questions、artifacts、失败原因。

2. **Knowledge Objects**
   - summary / decision / artifact / annotation 均登记到 `objects`。
   - 带 provenance：来自哪个 workspace、session、turn、task、file。

3. **复用入口**
   - 用户可以从 knowledge / task / session 列表把对象引用到新 prompt。
   - 这是 v0 验证沉淀复利的最小闭环。

### 4.2 明确不包含

- Cloud / Postgres / multi-tenant SaaS。
- 组织、员工、RBAC、billing。
- 飞书 / Slack / IM ingress。
- Codex / OpenCode / Gemini 等第二 runtime。
- Orchest supervised delegation / avatar / worker 自动编排。
- Mobile app 与产品化 relay。
- Meeting / ASR / TTS；File Surface 的音频播放和时间区间 Locator 是消费 / 引用能力，不代表引入音频生产管线。
- 完整 MCP marketplace。
- 自研编辑器内核。
- 角色档案 / persona 配置 / GenUI 自由布局。
- 多媒体创作完整流；但 surface 合同必须为后续视频/音频留口。

---

## 5. UX 要求

### 5.1 固定信息架构

```text
┌──────────────┬─────────────────────────────┬────────────────────┐
│ Left Rail    │ Surface Area                │ Chat Lane          │
│              │                             │                    │
│ Workspace    │ File / Git / Terminal       │ Session Header     │
│ Runs         │ Browser / Overlay           │ TurnCards          │
│ Inbox        │                             │ ApprovalStrip      │
│ Settings     │                             │ ContextTray        │
│              │                             │ Input + Chips      │
└──────────────┴─────────────────────────────┴────────────────────┘
```

v0 不做用户自定义布局。Surface foregrounding 由规则触发：

- diff 到达 → Git / Diff Surface 前置。
- command 运行 → Terminal Surface 高亮。
- console error 捕获 → Browser Surface 高亮。
- approval request 到达 → ApprovalStrip 常驻显示。

Inbox 在 v0 是轻量注意力列表，不是 M2 的异步 unattended activity feed：只承载 `waiting_approval`、blocked / interrupted、以及 completed / failed 后需要用户查看或验收的事项。

### 5.2 TurnCard 最小结构

每个 TurnCard 至少包含：

- PlanHeader：可选，支持 plan accept / collapse。
- ActivityRow：tool / runtime event，默认折叠为一行。
- ResponseCard：agent 文本，流式缓冲，不逐 token 刷屏。
- Artifact links：diff、文件、terminal output、browser snapshot。
- Approval state：等待确认、已批准、已拒绝。
- Failure / retry state：失败原因、重试次数。
- Attribution：执行者 agent / runtime 的细色标。

### 5.3 关键动作永远可见

以下动作不能藏在 hover、context menu 或快捷键里：

- approve / deny。
- stop / interrupt。
- permission mode 切换。
- artifact accept / reject。
- rollback run。
- Context Tray 勾选。

### 5.4 Annotation / Deixis Island

v0 实现简化版：

1. 用户在任意支持 surface 中选中内容。
2. Deixis Island 从选区附近浮现。
3. 用户选择“引用给 agent / 解释 / 修改 / 追问”。
4. 系统生成 Locator + Reference Chip。
5. 发送前 Context Tray 展示该 chip 对应内容。

不要求实现 Craft Agents 全套 overlay annotation 系统，但必须保留物理来源感：菜单从用户指向的地方长出来。

### 5.5 Agent 在面上现身

agent 不只在 Chat Lane 报告自己做了什么，还应在对应 surface 上留下 attribution：

- 文件被读取 / 修改时，File Surface 有细标记。
- diff hunk 来自哪个 run / agent。
- terminal 命令是用户输入还是 agent sideband。
- browser console / DOM inspection 是哪个 run 触发。

---

## 6. 视觉与品牌要求

### 6.1 基调

**安静的书房 × 终端的血统——一台仪器，不是一个应用。**

v0 UI spike 必须用真实 TurnCard + terminal + diff 内容校准视觉，而不是空状态 mock。

### 6.2 色彩纪律

- 基底：暖纸 / 石墨夜，不用 slate 蓝灰。
- 文本：墨色。
- 品牌色：琥珀。
- 琥珀出场率 < 5%，只用于 deixis 高亮、当前聚焦、品牌时刻。
- diff、warning、error、success 使用独立语义色，不和品牌色混用。
- agent attribution 使用降饱和分类色，只做细标记，不大面积铺。

### 6.3 字体声音

- UI：高密度人文主义无衬线。
- Agent 回复正文：可长读的衬线体。
- 代码 / diff / terminal：高质量等宽字体。

### 6.4 动效与材质

- 轻边界：1px border + minimal shadow。
- 禁止 spinner；agent working 使用骨架态或呼吸脉动。
- 动效预算集中在 Deixis Island；其他转场 ≤150ms 或无动效。
- 不用机器人、火花、气泡、大脑、电路板等 AI 俗套符号。

---

## 7. 核心用户旅程

### Journey 1：打开 repo，发起一次写 run

1. 用户打开 web client，连接本机 daemon。
2. 用户选择本地 repo 作为 workspace。
3. Multivac 展示文件树、git 状态、Chat Lane、terminal。
4. 用户输入：“修复登录页移动端布局问题。”
5. Context Tray 展示常驻环境摘要与将注入的上下文。
6. 用户确认后创建 run。
7. daemon 获取写锁并创建 run 前快照。
8. Claude Code runtime 启动，事件转译为 `TaskEvent`。
9. TurnCard 展示计划、文件读取、命令、修改、测试。
10. Git / Diff Surface 展示 agent 产出的 diff。
11. 用户接受部分 diff，拒绝或打回其他 diff。
12. Task 完成后生成 summary / decisions / artifacts，并登记为 knowledge objects。

### Journey 2：指着 browser error 让 agent 修

1. 用户在 Browser Surface 看到 console error。
2. 用户选中 error 或点击错误旁的 Deixis Island。
3. 系统生成 browser Locator，并创建 Reference Chip。
4. Context Tray 展示 URL、console stack、可用截图 / DOM 摘要。
5. 用户发送：“修这个。”
6. agent 根据 browser context 定位代码并修改。
7. TurnCard 与 Browser Surface 同步显示执行过程和验证结果。

### Journey 3：让一个 session 看见另一个 session

1. 用户在 Session A 中完成一次 API 重构讨论。
2. post-process 生成 Session A 的 summary / decision log。
3. 用户在 Session B 中引用 Session A 的最终决策。
4. Context Composer 注入 summary、decision log、关键 diff，而不是全量 transcript。
5. Session B 的 agent 基于该上下文继续写文档或测试。

### Journey 4：后悔并回滚

1. agent 修改多个文件后，用户发现方向不对。
2. 用户点击 run 的 rollback。
3. 系统回滚到 run 前 snapshot。
4. Task 标记为 cancelled / reverted，并保留事件与决策记录。
5. 用户可在新 run 中引用失败原因，让 agent 改方向。

---

## 8. 信息架构与对象模型

### 8.1 主要对象

| 对象 | 定义 | v0 要求 |
|---|---|---|
| Workspace | 工作现场根对象 | 单本地目录 / git repo |
| Session | 人与 agent 的通信 lane | 多 session 可存在，active session 先做稳 |
| Turn | 一次用户输入或 agent 输出 | 可被 Locator 引用 |
| Run / Task | agent 执行记账 | 生命周期、事件、artifact、snapshot |
| Runtime | 执行宿主 | Claude Code backend |
| Surface | 工作介质面 | chat、file、git、terminal、browser |
| Artifact | agent 产生或修改的产物 | 文件、diff、terminal output、browser snapshot |
| Locator | 对对象的稳定引用 | URI + provenance + scope |
| WorkbenchContext | 人当前看到 / 选择的上下文 | 打开文件、选区、diff、terminal、browser |
| KnowledgeObject | 工作副产品 | summary、decision、checkpoint、annotation |
| Snapshot | run 前状态 | 支持回滚 |
| InboxItem | 待处理事项 | v0 只承载 approval、blocked / interrupted、完成 / 失败 run；完整 unattended activity feed 后置 M2 |

### 8.2 Locator 初版示例

```text
file://src/auth/login.tsx#L42-L68
diff://task/123?file=src/auth/login.tsx&hunk=2
turn://session/abc/turn/17
task://123/output/summary
terminal://workspace/main/session/dev/lines/840-920
browser://workspace/main/page/console/err-42
knowledge://workspace/main/task/123/decision-log
```

v0 不要求 URI 语法永久定型，但必须保证：

- 可序列化。
- 可展示给用户。
- 可从 UI 重新定位原对象。
- 可被 Context Composer 转成 agent 上下文。
- 带 provenance 与 scope。
- 删除后保留 tombstone，不破坏历史引用。

---

## 9. 非功能要求

### 9.1 可靠性

- daemon 重启后可恢复 workspace、sessions、events、objects。
- 前端断线重连后可 replay 最近 TurnState。
- runtime 丢失连接时 task 标记为 `interrupted` 或 `unknown`，不得静默 completed。
- CLI event adapter 有契约测试，Claude Code 版本升级前必须跑。

### 9.2 性能

- 本地 workspace 启动到可交互：P50 < 3s。
- WebSocket event 到 UI 展示：P50 < 200ms。
- ResponseCard 流式缓冲，不逐 token 导致 UI 抖动。
- Context Composer 有 token budget，并优先注入用户显式指代对象。

### 9.3 安全

- v0 默认 self-hosted-host-first：文件与执行在用户自有机器。
- 默认不上传用户 workspace 到 Multivac 托管云。
- destructive action 必须可审查：文件删除、命令执行、credential 访问、外部网络敏感调用。
- secrets 不进入 knowledge projection。
- Context Tray 必须明确展示跨厂商数据流向。

### 9.4 可观测性

- 每个 event 有 `schema_version`、workspace_id、session_id、task_id、timestamp、source runtime。
- trace_id 贯穿 daemon、runtime adapter、WS event、前端 TurnState。
- 日志本地结构化，自动脱敏。
- 支持 `/doctor` 类自诊断的日志基座。

---

## 10. 成功指标与验证方式

### 10.1 M1 / v0 exit criteria

| 指标 | 成功信号 |
|---|---|
| 默认工具使用 | 创始用户连续 4 周把真实日常工作放在 Multivac，而不是只做 demo |
| 指代替说 | 每周 prompt 中包含 Locator / chip 的比例持续上升 |
| Context Tray 有用 | 用户经常查看、勾选或删减 agent 将看到的内容 |
| Review safety | agent 写文件后，用户能通过 diff / rollback 安全裁决 |
| 沉淀复用 | 上周 task / session / decision 被本周 prompt 引用 |
| Browser 感知 | browser error / URL / console / DOM 至少一种成为真实修 bug 的上下文来源 |
| 数据归属 | 用户可成功导出自己的 events / objects / summaries |

### 10.2 本地实验指标

- `deixis_chip_count_per_run`
- `context_tray_open_rate`
- `context_tray_edit_rate`
- `knowledge_object_reference_rate`
- `run_rollback_count`
- `approval_wait_time`
- `task_interrupted_rate`
- `browser_locator_usage_count`

这些指标本地记录，不默认上传；用于决定是否进入 ADR 的 M2「会积累的工作台」。

### 10.3 v0 验收清单

以下清单用于把 PRD 拆成 issue 时校验闭环是否真的可用：

- 当用户打开一个 git repo 时，系统能展示当前 workspace、branch、worktree 状态和最近 runs。
- 当任一 Surface 进入 v0 实现时，必须同时交付 renderer、deixis adapter、perception adapter，并能完成 Locator → Reference Chip → Context Composer 的闭环。
- 当用户在 File Surface 打开 markdown / HTML / PDF / CSV / 图片 / 音频时，内容可预览；文件本身、文件片段与音频时间区间都可生成 Locator；相对路径和绝对路径都可复制。
- 当用户从剪贴板粘贴文件 / 图片 / 文本片段到目录时，系统能创建对应文件并在 File Surface 中定位。
- 当用户选中文件片段、diff hunk、terminal 行区间、browser console error 或历史 turn 时，系统能生成 Reference Chip，并在 Context Tray 中显示 agent 将看到的内容。
- 当 agent 发起写 run 时，系统能获取 workspace 写锁、创建 run 前快照，并在需要时一键回滚。
- 当 agent 修改文件时，Git / Diff Surface 能展示 diff，用户可以 stage / unstage、commit、push、pull 或打回。
- 当用户在 Terminal Surface 操作 shell / TUI 时，终端保持全功能交互；agent 能读取授权范围内的 scrollback 摘要或被用户引用的行区间。
- 当 task 完成、失败或被取消时，系统能生成带 provenance 的 summary / decisions / artifacts，并可在后续 prompt 中引用。
- 当用户导出 workspace 沉淀时，系统能导出 events / objects / summaries / metadata，且不破坏历史 Locator。

---

## 11. 风险与缓解

| 风险 | 缓解 |
|---|---|
| v0 退化成漂亮聊天应用 | 入口从 Workspace 开始；五个 surface 与 Context Tray 必须进 v0 验证 |
| 退化成 IDE 竞赛 | 文件面支持简单编辑但不追求 IDE 级人体工学；差异化放在多介质查看、deixis、event、knowledge、agent attribution |
| 执行透明度变噪音 | ActivityRow 默认折叠；失败、approval、artifact、用户验收优先展示 |
| 沉淀变成隐私负债 | 先脱敏后落盘；路径黑名单；跨厂商传递明示确认 |
| CLI runtime 协议不稳定 | adapter 隔离原始协议；契约测试；前端只看 normalized `TaskEvent` |
| context token 爆炸 | 常驻摘要 + 精确指代 + 按需查询；Context Composer token budget |
| browser surface 技术路线不确定 | v0 先验证 URL / console / screenshot / DOM 摘要闭环；深 CDP 路线 spike 后定 |
| 多 session 写冲突 | workspace 写锁；并行只读；run 前快照与回滚 |
| 名字 Multivac 撞名 | 继续作为内部代号；公开发布前完成命名决策 |

---

## 12. 里程碑与阶段

### Phase 0：Kernel 地基

- daemon + web client 连接。
- SQLite schema：`workspaces` / `sessions` / `events` / `objects`。
- RuntimeBackend trait + `TaskEvent` schema。
- Claude Code adapter 契约测试框架。
- 脱敏管线、schema version、export skeleton。

### Phase 1：对话 + 单 runtime 闭环

- HTTP POST → skeleton → WS events → TurnCard。
- Claude Code run 启动、停止、失败、完成。
- run 前 snapshot + rollback。
- 写锁。
- 本地 telemetry skeleton。

### Phase 2：五个 surface 与 deixis

- File Surface：md / HTML / PDF / CSV / 图片查看、音频播放、代码阅读、简单文本编辑、复制相对/绝对路径、从剪贴板粘贴创建文件。
- Git / Diff Surface：branch / worktree / diff 展示，简单 stage / commit / push / pull。
- Terminal Surface：全功能 PTY，scrollback 可被 agent 捕获并引用。
- Browser Surface 最小感知。
- Deixis Island → Locator → Reference Chip。
- Context Tray + Context Composer v1。

### Phase 3：裁决与沉淀闭环

- diff accept / reject /打回。
- Task post-process summary。
- KnowledgeObject projection。
- Session-as-context。
- 一键导出。

### Phase 4：M2 准备，不进入 v0 承诺

- 第二 runtime：Codex。
- 更深 checkpoint writer。
- mobile / relay / 阶段二遥控 UI。
- avatar / supervised delegation。
- 完整 unattended Inbox / activity feed。
- team plane scope sharing。

---

## 13. 与现有文档的关系

- 本 PRD 定义“v0 做什么、为什么、做到哪里算成立”。
- `adr-001-product-positioning.md` 仍是定位与 v0 范围的上游裁决文档；冲突时 ADR 优先。
- `multivac-design.md` 约束视觉基调、色彩纪律、字体声音与品牌隐喻。
- `multivac-frontend-design.md` 约束 Workbench 信息架构、TurnCard、Context Tray、Deixis Island、Jotai / panel 方向。
- `multivac-reconstruction-analysis.md` 约束 daemon、RuntimeBackend、数据库、事件、阶段拆分与 Orchest 边界。
- Orchest iteration PRD 只定义 SDK 能力；Multivac 产品需求不得直接扩大 Orchest core 范围。v0 基本不依赖 Orchest 编排深度，M2 avatar 阶段才验证 supervised delegation API。
