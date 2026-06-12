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
7. **厚度 = 时间 × 沉淀，不是功能数量。** 层（UI/adapter/壳）该薄且可替换；不可替换的是沉淀（objects/events/决策/跨 agent 上下文）。焦虑驱动的加功能是纸甲：铺开的每一寸面都要防守、都防不深，还烧掉做深一件事的时间。详见 §5.5。

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

**「创作者」的定义（2026-06 补充，区别于内容创作者）**：我们的创作者比 YouMind 式的「内容生产者」宽一个维度。当 agent 把工艺商品化，创作门槛从「会不会做」变成「想不想要、有没有品味」——**「会运行的东西」第一次成为普通人的表达形式**（situated software / home-cooked software 传统的 AI 兑现）。AI-native 表达的四个特征：① 创作动作 = 意图 + 品味 + 挑选 + 迭代——**deixis 不只是交互原语，它就是 AI 时代的画笔**；② 作品从静态内容变为有行为的造物（工具/交互作品/自动化/调教好的 agent 本身）；③ personal-scale——为一个人、一个场景而做；④ 作品是活的，带着自己的历史（沉淀即作品上下文）。这个未来创作者钳形两翼都接不住：YouMind 无执行层且哲学是「替你生成」，Vibeyard 假设用户自认开发者。D5 排序由此获得比「agent 能力分布」更深的理由：**今天的 builder 是未来创作者的种子人群**——我们站在类别扩张的起点上跟着长，而不是赌它哪天出现。（此判断尚在成形，随 M1/M2 的真实用户画像校准。）

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

### D9-D14 开工前必须定死的补充决策（2026-06-12 缺口审计：标准是「现在不定，以后改不动」）

**D9 沉淀的安全基线：先脱敏，后落盘。** 「所有上下文可感知 + 永久沉淀」同时是负债：终端回滚区有 API key，.env 会被读，browser 快照含登录态。脱敏器（token/secret/key 模式 + 可配路径黑名单）是 events/objects 持久化管线的 kernel 组件，不是后补功能——明文落盘或发给厂商后无法追回。跨厂商数据流向需要 workspace 级 policy（哪些对象类别允许进入哪个厂商 runtime）：把 Claude transcript 发给 OpenAI（创世卡点 2）必须是用户明示选择，context tray 的逐条勾选不够。

**D10 沉淀的寿命契约：版本化 + 永不破坏重放 + 可导出。** 厚度论的承重墙是数据：TaskEvent 带 schema version，新版本必须能重放全部历史事件；`knowledge://` URI 永久可解析（对象不可变，删除 = 墓碑）；**一键导出（JSONL + 文件）v0 即有**——「用户拥有自己的沉淀」是对抗厂商锁定的产品主张，自己先做到。

**D11 workspace 并发模型：写锁串行，读并行。** v0 同一 workspace 同时只有一个持写权的 run（scope lock，lark-bridge 模型）；并行 session 允许但默认只读（探索/问答），写权需等待或用户显式切换。worktree-per-session（Claude Code desktop 方案）留作 M2 选项——介质论要求「文件面显示的就是真相」，多 worktree 破坏单一真相直觉。影响阶段 0 schema（sessions 与 workspace 的锁关系）。

**D12 撤销故事先于 review 故事。** draft/review 在阶段 3，但 agent 从阶段 1 就写真实文件——每个 run 开始前自动 workspace 快照（git 仓库用 commit/stash 机制，非 git 目录用影子快照），一键回滚到 run 前。信任来自便宜的后悔药；实现为 RuntimeBackend `start_task` 的前置钩子，不是 UI 功能。

**D13 Claude Code 集成走受支持接口。** PtyRuntime 优先使用官方 headless / stream-json（或 Agent SDK），不做 PTY 屏幕抓取——稳定性与 ToS 双重理由。为 JSONL schema 写契约测试，Claude Code 版本升级先跑契约再放行。认证用用户自己的 Claude 订阅/登录，我们不代理计费。

**D14 实验遥测进 v0。** 里程碑靠实验数据解锁（§4.5），故指标采集（指代次数、沉淀引用率、context tray 修改率）是 v0 功能而非运营工具：本地优先、明文可查、不上传——单人阶段不需要任何服务端。

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
| 脱敏器、run 前快照、写锁、本地遥测、一键导出（D9-D14） | | |

---

## 4.5 三个里程碑（按验证解锁，不按日历推进）

每个里程碑由上一个的实验数据解锁。里程碑回答「证明了什么」，阶段（reconstruction §七）回答「建什么」。

### M1「自己的工作台」——证明介质论

- **用户**：我们自己（程序员/技术创作者，单人）
- **交付**：五个面（对话/文件阅读/git/终端/browser）+ deixis + context tray + review 裁决 + objects 沉淀；单壳 + SQLite + Claude Code 单 runtime。对应阶段 0-3
- **解锁判据（exit criteria）**：
  1. 创始用户连续 4 周把日常真实工作放在里面完成（不是 demo，是默认工具）
  2. 实验 1 正信号：「指代替说」高频发生，失败点收敛为可修的 bug 清单
  3. 实验 3 开始计量：沉淀引用率有第一条基线曲线
- **此阶段不做**：任何团队功能、任何第二角色 surface

### M2「会积累的工作台」——证明沉淀复利 + 跨 agent

- **用户**：个人创作者（bottom-up 自然扩散的第一批外部用户）
- **交付**：Codex 第二 runtime（session-as-context 升级为跨厂商，兑现创世卡点 2）；知识投影深化（单人记忆复用）；文字创作者/PM surface；Claude Code 配置一键导入
- **解锁判据**：
  1. 实验 2 正信号：上周的沉淀让本周的 agent 可感知地更好用
  2. 实验 3 曲线持续上升：跨 session / 跨 agent 引用成为日常动作
  3. 第一批非团队成员的外部用户留存（哪怕个位数，但是真实回访）
- **此阶段不做**：scope 共享、组织功能

### M3「团队的上下文平面」——证明飞轮可乘人数

- **用户**：2-10 人小团队（同一 project）
- **交付**：objects 的 scope 共享（team plane）；多媒体 surface（视频/音频，视 agent 能力到位程度）；GenUI layout preset
- **解锁判据**：
  1. 一个真实小团队把共享上下文当日常（A 的 session 沉淀被 B 的 agent 引用）
  2. 离开成本可观测：团队尺度的沉淀引用率 > 单人基线——同事的知识库论在团队尺度被验证
  3. 多媒体 surface 至少一种（视频或音频）走通三件套合同
- **此后才讨论**：Cloud 模式、Org/权限体系、Meeting/ASR、IM 接入

**纪律**：里程碑之间不并行铺开——M1 没解锁就做 M2 的功能，就是回到「纸甲」老路（§5.5）。

## 5. 验证方式（用实验代替争论）

1. **介质论**：我们自己每天用 v0 工作台干真活，记录「指代替说」的使用频率与失败点。失败点 = deixis 适配器或注意力策略的 bug 清单。
2. **知识论（单人版）**：上周 session 的决策/产物，本周的 agent 是否因此**可感知地**更好用。单人不成立，团队也不会成立；成立了，team plane 就是把已验证的飞轮乘上人数。**先验证飞轮的物理，再投资飞轮的尺寸。**
3. **厚度指标（焦虑的替代品）**：每周 prompt 中引用已沉淀对象（deixis chip、历史 session、知识投影）的比例。在涨 = 层在变厚、离开成本在涨；不涨 = 沉淀设计有问题——那才是该恐慌并调整的信号。功能数量永远不是这个指标。

## 5.5 厚度论：何时该感到薄

「我们这一层够不够厚」的担忧是真实的，但它曾是 scope 蔓延的内因。固化判断标准，替代焦虑：

**厚度的定义**：用户离开时损失什么 + 相邻玩家复制不走什么。推论：**层薄，沉淀厚**——UI/adapter/壳可替换；objects/events/决策/跨 agent 上下文随时间复利，厚度单位是时间 × 使用量，不是功能数。

**两个威胁的停止线**：
- 飞书/Linear agent 化（平台下压）：会商品化 @agent、任务分派、知识库 RAG、会议纪要；**结构上不会深耕本地执行现场**（repo/终端/dev server/跨厂商 CLI 生命周期）——它们为组织协作服务，不为创作者的双手服务
- Claude Code/Codex/Cursor 强化（厂商上顶）：**单厂商工作台体验我们守不住**（创世卡点 1 已被商品化）。但厂商有三条结构性停止线：①中立性（不会互喂 transcript）②上下文归属（厂商要锁孤岛，用户要自己拥有）③介质宽度（代码/web 之外在其基因外）。且厂商强化一半是利好——我们是 runtime 的互补品，Claude Code 越强，我们工作台里跑的引擎越强

**我们的厚长在三个位置**：①中立的跨 agent 上下文平面（结构位：对手要抄得先放弃厂商身份）②随时间复利的用户沉淀（知识库论的兑现方式：厚度是沉出来的，不是建出来的）③创作介质宽度（Surface 合同）。

**残余风险**（诚实记录）：Cursor 证明「跨厂商层」可行但有人争（分界：它是编辑器中心、代码限定、不拥有跨 agent 产品的 session 真相）；若未来出现 session 可移植标准，我们的 normalized 事件设计是最快采纳者——标准化世界里中立工作台位置更好而非更差。

**行动推论**：厚度 = 时间 × 沉淀 ⇒ **最危险的事是为穿盔甲推迟上线**。v0 的任务是让计时开始：薄薄的五个面尽快跑起来，每次交互都落沉淀。今后任何焦虑性需求，先对照实验 3 的厚度指标，不对照竞品功能列表。

### 5.5.1 竞品地形图：为什么大家都聚在轻办公（2026-06）

观察：绝大多数 agent 产品聚在轻办公/文档场景——飞书 Aily（企业知识问答 + 流程自动化）、OpenClaw 系（IM 入口的个人助理）、Moxt（Agent-Native Workspace：AI 员工 + 团队共享记忆，活在 Slack 里，场景为社媒/销售/法务/调研）。

**成因：能力供给决定场景选择，不是需求深度。** 纯文本 LLM 成熟且便宜，文档/聊天场景不需要 runtime、本地执行、沙箱、桌面应用；IM bot 分发零安装好传播；「AI 员工」叙事好讲。低门槛的另一面：这是商品化最快、平台结构性占优的一层——恰好是本文判定不防守的位置。

**三重含义**：
1. **厚度焦虑的经验性回答**：绞杀区在轻办公，不在深执行。执行介质（终端/git/browser/runtime/deixis/沉淀）的占位者目前只有两个：Claude Code 桌面端（厂商，停止线见 §5.5）与 Vibeyard（见下）。
2. **不对称是单向的**：轻办公产品下不来（要重建执行层 + 桌面应用 + 本地信任模型，与其基因和成本结构相反）；我们上得去——v1 文字/PM surface 带着沉淀 + deixis + 跨 agent 进入文档市场，不是又一个 RAG 聊天。同一片市场，晚进，但武器不同。
3. **Moxt 是内部争论的活体对照组**：它几乎是「团队知识库优先」愿景的产品化。每季度回看：若它撞上冷启动/留存墙 = 验证「知识必须是工作的沉淀」；若起飞 = 研究我们漏了什么。另注意其「Agent-Native Workspace」叫法与我们定位词相邻，对外叙事需措辞区隔。

**Vibeyard（2026-06 发现，离我们最近的占位者）**：MIT 开源 Electron 桌面应用，「为 AI 编码代理构建的 IDE」——多 CLI runtime 并行（Claude Code/Codex/Copilot/Gemini，node-pty）、swarm 网格监控、看板、成本/token 追踪、session 持久化与 P2P 共享、嵌入浏览器（点击 DOM 元素 → 发送 selector 给 agent）。

- **重叠**：多厂商 CLI 工作台、监督面、浏览器元素指代的雏形——它做的是本 ADR 之前那版「监工台」形态，且免费开源、多 runtime day-1。「多 session 管理」这一层从此是免费商品，不能作为我们的卖点。
- **我们的差异仍然成立但更紧迫**：① 它是 N 个并排的孤岛——多 runtime ≠ 跨 agent 共享上下文（创世卡点 2 仍无人做：P2P 共享是人对人，不是 agent 对 agent）；② 无沉淀——session 持久化是日志，不是可指代、可注入未来上下文的 objects；③ code-only IDE 框架，无创作介质宽度；④ 元素 → selector 是浅指代，非感知适配器（无截图/console/DOM 语义进上下文）。
- **行动**：MIT 许可 = 可研究其 PTY 管理、多 CLI adapter、嵌入浏览器实现（又一个 Electron 数据点）；与 Moxt 同列季度回看名单。它的存在把 M1/M2 的时间窗收紧了——介质 + 沉淀必须是 v0 就可感知的差异，否则我们只是第二个 session 管理器。

**YouMind（2026-06 发现，创作侧翼的占位者）**：玉伯（语雀创始人、前飞书开放平台负责人）2024 年创办，「AI Creation Studio / AI 时代的纸和笔」——收集（网页/视频/播客/PDF 灵感捕捉）→ 思考（发现连接）→ 产出（文章/幻灯片/视频/网页）。Web + 移动 SaaS，目标用户即我们 D5 的角色清单（YouTuber/学生/独立开发者/PM/创作者）。

- **两个独立趋同，分量很重**：① 「AI 时代的纸和笔」= 介质论的另一种表述；② 玉伯公开判断**「当前的知识管理工具是伪需求，要输出驱动」**——语雀的创始人亲口否定知识容器路线，这是对 D4「知识是工作的沉淀，不是待填的容器」最强的外部证词，内部争论时直接引用。
- **钳形地形成形**：Vibeyard 占工程侧翼（执行无介质/沉淀），YouMind 占创作侧翼（创作无执行——无本地 workspace/git/终端/dev 预览，内容在其云端，生成中心而非共创中心）。我们的楔子恰在交点：**创作 × 执行 × 沉淀归属用户本地**。
- **对 v1 的含义**：进入文字创作者市场时 YouMind 是在位者。我们的入口仍是「也写作的 builder」——创作长在 workspace 上（项目文档/README/博客随 repo 沉淀），不是灵感捕捉云。另注意命名冲突升级：「AI 创作工作台」与我们的定位词几乎同字面，对外叙事必须先于它锚定差异（执行深度 + 本地拥有 + 共创非生成）。
- **GTM 上必须承认它选得好**：内容创作者是互联网上声浪最大的人群——每个满意用户本身就是扩音器（用它做的视频/文章自动成为它的营销）。它会先占领「AI 创作」的公共叙事。我们的应对不是去抢声浪，而是两条：① builder 渠道声浪第二大但信任度更高（Cursor/Claude Code 均由此引爆），且 builder 分享的是**作品本身**（repo/「built with」时刻）；② 把「可分享的创作过程」做成产品能力——事件溯源的 session 天然可重放，「看我和 agent 怎么把这个东西做出来的」回放是自带演示性的传播物（M2+ 的增长功能候选）。声浪人群同时是迁移成本最低、流失最快的人群；我们赌留存（沉淀）而非触达。
- 与 Moxt、Vibeyard 同列季度回看名单；其「创作者的 GitHub 社区」愿景若兑现，将形成分发网络效应，值得跟踪。

**显式承认的赌注**：纵向排序（D5）= 主动放弃当下声量最大的市场入口，赌「从执行侧进入文档市场的武器」比「现在和几十家 RAG-chat 挤同一扇门」更值钱。三方对齐时明确说出：文字/办公市场不是不去，是换一条进攻路线去。

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
5. **命名与商标**：Multivac 出自阿西莫夫，且有同名既有项目——公开发布前完成商标与重名检查（不阻塞开发）
6. **仓库组织**：在 orchest monorepo 内孵化还是独立 multivac repo——阶段 0 init 前定，影响 CI 与发布流水线

---

## 签署

| 角色 | 签署 |
|------|------|
| 个体生产力 / 介质论（Emile） | ☐ |
| 团队知识库 / 协作 | ☐ |
| 架构 | ☐ |
