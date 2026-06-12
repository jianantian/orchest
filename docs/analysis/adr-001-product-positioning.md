# ADR-001：产品定位——个体与 agent 协同的创作工作台

> 2026-06-12 | 状态：**待三方签署**
> 本文是 `multivac-reconstruction-analysis.md` 与 `multivac-frontend-design.md` 的上游。
> 此后所有「该不该进 v0」的争论对照本文裁决；与本文冲突的下游内容以本文为准。

---

## 0. 一句话定位

> **个体与 agent 协同的创作工作台：协作语法统一，创作介质可插拔，知识是工作的自动沉淀。**

---

## 0.5 创世卡点（产品的原始需求，所有抽象的检验锚点）

定位的抽象论证之外，记录两个最初让创始用户「不爽」的具体时刻——任何设计如果解决不了这两个卡点，就偏离了原点：

1. **「我想同时拥有 Gemini Canvas 和 Claude Code」**——chat + 实时画布 + coding agent 不该是两个产品。
   现状（2026-06）：单一厂商内部已在收敛——Claude Code 桌面端的 preview 面板、Claude Design（2026-04，chat 左 + canvas 右 + handoff 给 Claude Code）。**推论：「canvas 挨着 coding agent」正在被厂商商品化，不能作为我们的差异化本体**；它验证了 Surface 区方向，但我们的增量在多介质创作面（超出 web/code）、全工作区 deixis 与 team plane。
2. **「我希望 Codex 能看到我和 Claude Code 的交互」**——跨厂商的 agent 共享上下文。
   **这是结构上没有任何厂商会解决的卡点**（Anthropic 不会把 Claude 的 transcript 喂给 OpenAI 的 Codex，反之亦然），天然是第三方工作台的生意。它把介质论推广到完整形态：**工作台是「人 ↔ N 个 agent」以及「agent ↔ agent」的共享介质**。我们的架构已为此成形——normalized TaskEvent（厂商协议不外泄）+ session/turn 作为 Locator 对象 + Context Composer 注入——机制即：把一个 session 作为 deixis 对象引用进另一个 agent 的会话。v0 单 runtime 下表现为 Claude↔Claude（并行 session 共享上下文），第二 runtime 接入即升级为跨厂商。

## 1. 背景：三个愿景的张力

团队内部对产品定位存在三种表述，导致设计文档 scope 蔓延（Org/Employee/Meeting/Knowledge/多 runtime 全部挤进早期阶段）：

| 愿景 | 主张 | 单独成立时的死法 |
|------|------|----------------|
| **执行控制面**（原架构文档） | task/runtime/artifact 是系统真相 | 定义的是基础设施身份，不是用户时刻；建完阶段 0-3 得到的是「有漂亮 TurnCard 的聊天应用」 |
| **团队知识库**（协作优先） | 文档/对话/任务/代码都是知识，agent 在团队知识库中高效工作 | 冷启动空库；且走进 feishu.md 自己警告过的平台主场（协作容器、文档、组织分发） |
| **个体生产力**（介质优先） | 人和 agent 的沟通需要介质；每个角色有最适合的协作界面 | 「适合每个人」横向铺开 = v0 不适合任何人 |

**裁决结论：三者不是竞争关系，是同一产品的三层**（见 §3 D8）。

---

## 2. 第一性判断（决策依据）

1. **瓶颈在共享上下文，不在模型智能。** 靠一两句话让 agent 自动完成想法不现实——没有人做得到，除非他是你三十年的助理。人类高效协作靠指着同一个东西说话，不靠描述。介质的本质是**让「指」代替「说」**。
2. **协作语法不变，创作介质可变。** 委派、指代、审查、裁决、积累——五个动作对程序员/PM/测试/视频创作者/写歌的人完全一致；变的只是 artifact 类型和「指」的形态（file:line、diff hunk、时间码区间、波形区域、文档 span、DOM 元素）。
3. **知识是工作的副产品。** 团队 wiki 失败从来不在存储和检索，在「写下来是没人付的税」。唯一可持续的知识库从工作现场自动捕获。先有现场，才有沉淀；反向不成立。「三十年助理」之所以好用，是因为共享上下文积累了三十年——介质是交换，知识是留存，同一个飞轮的两半。
4. **不防守平台必赢的地盘**（feishu.md 的结论继续有效）。协作容器、文档、组织分发是飞书/Notion 的主场；执行现场、运行时、介质深度是平台不会深耕的地方。
5. **角色排序由 agent 能力分布决定，不由偏好决定。** Claude Code 让程序员工作台今天就成立；文本类 agent 也够强；视频/音频 agent 还碎且弱。
6. **冷启动检验：第一个用户是我们自己。** 工作台打开已有 git repo 第一小时就有用；「和我空空的知识库对话」构不成任何人的第一个用户时刻。

---

## 3. 决策

### D1 根对象是 Workspace

系统真相不是 Session（聊天），也不是知识容器，而是**工作发生的现场**：Workspace（v0 = 一个本地目录/git repo）+ 其中的 Runs 与 Artifacts。

- Chat 是 workspace 的一个面（human-agent communication lane）
- Task 是对 agent 工作的记账
- Knowledge 是工作历史的投影（见 D4）

### D2 Kernel / Surface 分层

「每个角色有最适合的交互方式」的架构翻译**不是**做 N 个角色工作台，而是：

- **Kernel（做一次）**：session runtime、双向上下文流（D3）、deixis 原语、review/裁决循环、对象寻址（Locator，D4）与权限 scope、daemon 生命周期。
- **Surface（可插拔）**：每种 artifact 类型一个工作面，插件合同是**三件套**：
  1. **渲染器** — 怎么展示（代码 / markdown / 图片 / PDF / 视频 / 音频 / 网页）
  2. **指代适配器** — 怎么「指」（文本 span、file:line、diff hunk、终端行区间、时间码、波形区域、DOM 元素）
  3. **感知适配器** — agent 看到什么（文本、diff、截图、console、转写）

**角色是涌现的**：工作台按内容自动激活 surface（规则驱动：diff 到达 → git 面前置；dev server 起 → browser 面打开）。不做角色档案，不做 GenUI 布局引擎——v2 用 layout preset 近似，生成式布局等介质论验证后再说。

### D3 双向上下文流（介质的技术核心）

现有设计只有 agent→人 的流（TaskEvent → TurnState → 渲染）。必须补齐对称的另一半：

```
人 → agent：WorkbenchContext
  打开的文件 + 选区 / git 状态 + 未提交 diff
  终端尾部 N 行 / browser 的 URL + console + 截图/DOM
    → Context Composer 组装 → 随消息注入，或暴露为 agent 查询工具
```

**「可感知」≠ 全量注入**。三层注意力策略：

1. **廉价环境摘要常驻**（打开了什么文件、dev server 端口、git 脏不脏——几百 token）
2. **用户指代的内容精确注入**（deixis chip 指什么给什么）
3. **其余按需查询**（agent 主动调 `read_file` / `read_terminal` / `inspect_browser` 等工具）

配套 UX：**context tray**——发送前人能看见并勾选「agent 将看到什么」。信任与权限控制从这里长出，而不是从 Org RBAC 长出。这个注意力策略是介质论真正的技术 crux。

**跨 agent 共享上下文**（创世卡点 2）：WorkbenchContext 的来源不限于五个面——**另一个 agent 的 session/turn 也是可指代、可注入的上下文对象**。一个 session 的 normalized transcript（token-budgeted 摘要 + 按需展开）可以作为引用 chip 进入另一个 agent 的会话。这要求 session 归一化从第一天就厂商中立（TaskEvent 不泄漏 Claude JSONL / Codex 协议细节）。

### D4 Knowledge = 工作的投影（给团队路线的结构性让步）

v0 的一切对象（session、turn、文件、diff、终端片段、annotation、browser 快照）都是**可寻址、带 provenance、带权限 scope 的上下文对象**（`knowledge://` URI——salvage 文档的 Locator 模式）。

- 单人模式下服务于 deixis 引用与 agent 按需查询
- 团队模式 = 调整对象 scope，**不需要重做数据模型**

团队知识库的终局保留，且换成平台抢不走的版本：**知识由工作自动生成、生来 agent 可消费**，长在执行现场上而不是文档容器里。这是几张表的设计纪律，不是一个 v0 产品模块。

### D5 纵向排序（愿景横，落地纵）

| 阶段 | 角色 | 依据 |
|------|------|------|
| **v0** | 程序员/技术创作者（我们自己） | Claude Code 引擎现成；五个面：对话、文件、git、终端、browser；此人群容忍粗糙、反馈质量高 |
| **v1** | 文字创作者 / PM + team plane | surface 增量小（文档面已覆盖大半）；单人知识复用验证后再乘人数 |
| **v2+** | 视频、音频 + GenUI preset | 等 agent 能力到位；Surface 三件套合同 day-1 定型，届时加插件不改内核 |

纪律：**愿景可以是横的，承诺和工程必须是纵的**。官网说「创作工作台」，v0 只为一种人做穿。

### D6 人读、审、指；agent 写（IDE 引力逃逸）

五个面做全就是在重建 IDE——那是 VSCode/Cursor 的地盘，必输。逃逸路线：

- 文件面以**阅读/预览**为主（代码、markdown、图片、PDF、视频），编辑能力极轻或外链到外部编辑器
- 写作主力是 agent；人负责指方向和裁决产出
- 自研只留 TurnCard 与 deixis 层；终端、阅读器、diff、browser 用成熟件（xterm.js、CodeMirror 只读态、pdf.js、CDP）

差异化永远在「创作介质 + agent 共享上下文」，不在编辑器人体工学。

### D7 v0 工程剖面：一条线

**桌面单壳（Tauri vs Electron 复审中，见[选型文档](../research/desktop-tools/electron-vs-tauri.md)，复审建议 Electron）+ SQLite + Claude Code 单 runtime + 单 workspace。**

移出 v0（接口形态保留，工程预算为零）：Cloud/Postgres、多 CLI runtime（Codex/OpenCode）、MessageIngress（IM 接入）、Org/Employee、Meeting/ASR、Orchest 编排深度（gate/checkpoint/factory）。

保留不变的架构资产：normalized TaskEvent、TurnState 事件溯源、SessionIdentity、daemon 语义、RuntimeBackend trait（v0 只有一个实现）、双层权限模型。

### D8 三个愿景的统一

```
个体生产力（介质论）      → 产品的 Kernel + Surface（v0，单人工作台）
团队知识库               → Kernel 废气的积累 + scope 共享（v1，team plane）
execution control plane → Kernel 底下的 runtime 基础设施（始终是手段，不是定位）
```

Go-to-market 推论：个体优先 = bottom-up 采用（Cursor / Claude Code 的路径）——单个创作者自己下载、自己爱用、带进团队。与小团队做桌面应用的现实匹配。

---

## 4. v0 范围表

| 进 v0 | 出 v0（接口保留） | 不做 |
|-------|------------------|------|
| 对话面（TurnCard + TurnState） | Cloud / Postgres 模式 | 编辑器内核军备竞赛 |
| 文件阅读面 + 多媒体预览 | 多 CLI runtime（**Codex 是 v0 后第一扩张项**——创世卡点 2，优先于其他一切后置项） | 知识库作为独立容器产品 |
| git 面（status / log / diff） | Org / Employee / Meeting / ASR | GenUI 自由布局引擎 |
| 终端面（共享 PTY + sideband） | MessageIngress（IM 接入） | 和平台打入口层 |
| browser 面 + 感知适配器 | Orchest 编排深度（§6.7） | 角色档案 / persona 配置 |
| deixis 原语 + context tray（含 **session/turn 作为引用对象**——跨 session 上下文移植，创世卡点 2 的 v0 形态） | team plane（scope 共享） | |
| Locator 对象模型（URI + provenance + scope） | Claude Code 配置导入（v0.x 增长杠杆） | |
| review / draft 裁决循环 | | |

---

## 5. 验证方式（用实验代替争论）

1. **介质论**：我们自己每天用 v0 工作台干真活，记录「指代替说」的使用频率与失败点。失败点 = deixis 适配器或注意力策略的 bug 清单。
2. **知识论（单人版）**：上周 session 的决策/产物，本周的 agent 是否因此**可感知地**更好用。单人不成立，团队也不会成立；成立了，team plane 就是把已验证的飞轮乘上人数。**先验证飞轮的物理，再投资飞轮的尺寸。**

---

## 6. 对两份下游文档的影响

**multivac-reconstruction-analysis.md**：
- 「产品定位」章按本文重写；execution control plane 降级为基础设施层身份
- 阶段计划按五个面重排：用户 PTY / 文件预览 / draft review 从阶段 4 提前进 v0；Cloud / 多 runtime / Org / Meeting / 编排深度 / IM 接入移出 v0
- §6.7（编排深度）、§6.9（MessageIngress）、§6.10-6.13（双模式）标注为「目标架构，非 v0」
- 数据模型从 6+ 张表收敛为 4 张起步：workspaces / sessions / events / objects（Locator）

**multivac-frontend-design.md**：
- 信息架构从「三区监督布局」改为「**Surface 区（主舞台）+ Chat Lane**」
- deixis 升为第一交互原语；annotation 从「聊天回复标注」泛化为「全工作区指代」
- 新增 context tray；agent 在各 surface 上现身（attribution）
- Inbox / approval 可见性原则保留，但从产品中心降为支撑能力

---

## 7. 未决问题

1. **browser 面技术路线**：Tauri 子 webview vs CDP 驱动的外部浏览器——影响感知适配器能做多深（截图/DOM/console 的获取方式）。
   **调研补充（2026-06-12，Claude Code 桌面端）**：Claude Desktop 是 Electron（自带 Chromium），其 preview 面板即 Electron 内嵌 webContents——天然带全量 CDP（`webContents.debugger`），所以截图/DOM 检查/元素选取/表单交互开箱即得，且双平台一致。它还验证了我们的两个核心设计：① pane 工作台（chat/diff/preview/terminal/file/tasks 可拖拽布局）≈ 我们的 Surface 区；② 「点击元素 = 给下一条 prompt 提供指代上下文」（Cmd+Shift+S Select an element）= 我们的 deixis 原语。
   **对决策的影响**：第三个选项浮现——Electron 壳。原架构反对 Electron 的主要论据（napi-rs FFI 桥）在我们的实际架构下不成立：前端本来就通过 HTTP/WS 连 Rust daemon，壳与 core 是进程解耦的，换壳不影响 multivac-core。真实代价只剩二进制体积与内存。三选项重述：(A) Tauri + 系统 webview（感知浅，平台不一致）；(B) Tauri + sidecar Chromium via CDP（感知满血，嵌入要做 screencast）；(C) Electron（感知满血 + 原生嵌入，体积大）。spike 改为 B vs C 对比。
   **它的空档 = 我们的差异化**：Claude Code preview 是 solo-centric（一个 agent、一个本地 session、无共享）、code-only；我们的多角色创作介质 + v1 team plane 不与之正面冲突。
   **完整复审**：见 [electron-vs-tauri](../research/desktop-tools/electron-vs-tauri.md)——结论是建议改选 Electron；该决策须在阶段 1（介质面）动工前定死。
2. **文件面编辑的「轻」**到什么程度：只读 + 行内小改，还是完全外链
3. **Orchest 进入产品的时机与角色**：候选切入点是 Context Composer 的智能化（注意力策略由 agent 决策）与知识投影的提炼 agent（Dream/Distill 的近亲）
4. **v1 文字创作角色的引擎**：Claude Code 通用化使用，还是 Orchest 原生 agent

---

## 签署

| 角色 | 签署 |
|------|------|
| 个体生产力 / 介质论（Emile） | ☐ |
| 团队知识库 / 协作 | ☐ |
| 架构 | ☐ |
