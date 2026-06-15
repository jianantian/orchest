# ADR-001：产品定位——个体与 agent 协同的创作工作台

> 2026-06-12 | 状态：**待三方签署**
> 本文是 `multivac-reconstruction-analysis.md` 与 `multivac-frontend-design.md` 的上游。
> 此后所有「该不该进 v0」的争论对照本文裁决；与本文冲突的下游内容以本文为准。
> **命名说明**：「Multivac」目前是**工作代号（占位，待定）**，非确定产品名——存在三层撞名负债（见 §7.5），公开发布前重新决定。文档沿用此代号仅为指代方便，不构成名称承诺。

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

## 0.6 赌注的底层（比创世卡点更底的「为什么」）

创世卡点是「我们为什么开始」，本节是「我们到底在赌什么」。这是项目最底层的判断，分两层——一层可证伪，一层不可证伪，**必须分开放，不能用后者给前者背书**。

### 第一层：市场赌注——OPC 与小组织越来越主流（可证伪）

agent 把协调成本打穿（Coase 企业理论：公司存在是因为内部协调比市场交易便宜）；一个人能调动十个专家的产出时，「经济上有效的最小单位」在塌缩。独立开发者、micro-SaaS、创作者经济已在此方向跑了十年。

**诚实的反论据（全分量保留）**：生产塌缩，分发/信任/资本不塌缩；赋能个体的技术同样赋能大规模部署它的组织，规模效应常压倒一切——PC/互联网/no-code 每波都被预言瓦解大公司，结果 FAANG 更大。

**纪律：用弱版本下注，不碰强版本。**
- 强版本（危险，与历史先例对赌）：OPC 成为经济价值的**主体**。
- 弱版本（安全，且充分）：小型创作单元的**数量大到成为真实市场，且被「为经理/企业设计的工具」系统性忽视**。Multica/Vibeyard 全在为经理模型造，underserved 是现成的。

### 第一层的去风险：产品其实不骑在 OPC 赌注上

**共创介质依赖的是「创造在『做出来』的那一刻永远是个体行为」——这一条在大组织内部也永远成立。** 介质论赌的是**创造的单位**，不是**组织的单位**。大公司里那个正在创造的个体同样需要这个介质。

⇒ **OPC 主流化是加速器，不是承重墙。** 即使第一层判断错了，产品仍能活。这也焊死了内部分歧：创造在动手瞬间是个体的，沉淀在小团队里复利——甜点正是「OPC **和小组织**」这个小型创作单元（比 D1「Workspace 为根」更准地命名了同事的「团队」与个体创造两者皆真的单位）。

### 第二层：价值公理——人的意义在于创造（不可证伪，但承重）

不能判对错，但能判它对产品是承重还是装饰。在本项目它是**承重的设计约束**：

若意义在创造、而 agent 自动化生产，则老板模型推到极限 = 「人出规格、agent 创造、人消费署名」，**掏空创造体验**。于是价值判断逼出设计禁令：**人必须留在作者位——掌舵、决断者，而非委派、审批者。** deixis / 指 / 转向因此不只是功能，是把人留在创作行为之内的道德装置。这是在价值基础上选产品形态——稀有，给产品灵魂与信徒（tool-for-thought 一脉的号召力来源）。

**它带的纪律（必须直说）**：价值公理会让人对市场信号失聪。若用户用脚投票选老板模型（因省事），信「意义在创造」的创始人可能继续造更费劲、更少人要的工具，**把自己的价值观误当市场需求**。守则：**价值是灵魂，但仍验证市场——别让信念变成对反馈的免疫**；别拿信徒产品的留存曲线去对标效率产品的增长预期。GTM 性质随之确定：**先招信徒，再有人群**（bottom-up 创作者工具的常态）。

### 依赖结构小结

| 层 | 性质 | 风险 | 产品依赖度 |
|----|------|------|-----------|
| 创造永远是个体行为 | 近乎不可证伪 | 极低 | **承重墙**——产品真正骑在这上面 |
| 小型创作单元是大且 underserved 的市场 | 可证伪（弱版本） | 中 | 决定市场规模与变现 |
| OPC/小组织成为主流 | 可证伪（强版本） | 高 | **加速器，非承重墙** |
| 人的意义在于创造 | 价值公理 | 不适用 | 灵魂 + 设计约束（要配市场验证纪律） |

## 0.7 核心使用模型：两阶段工作流 · 输入槽 · runtime 池 · avatar

> 这是「人怎么用 / Orchest 何时进 / Claude Code 如何被驱动」最完整的一张图，是 reconstruction §6.6/§八 的上游（那两节的「Orchest 大脑指挥 Claude Code」措辞按本节校正）。

### 工作流是一条两阶段的弧

```
阶段一（attended，重人协作）        交接棒              阶段二（unattended，自动）
人 ↔ agent 共创、明确想法    ──→ [设计文档+验收判据] ──→  对着判据自动执行  ──→ 验收
  人是作者/路由器                  一物两用                avatar 驱动            人在 gate 裁决
  meaning 在「决定什么」      spec + gate 判据                              meaning 在「判定对不对」
```

- **阶段一**：人与 agent 高带宽共创，产出设计文档 + 验收判据。meaning 在此。
- **交接**：设计文档是接力棒，一物两用——**spec**（造什么）+ **gate 判据**（怎么算完成）。「批准设计」= 授权阶段二。
- **阶段二**：对着判据自动执行，人参与度下降。
- **验收**（命门，杠杆最高/风险最高）：分层——测试（客观、loop 终止条件）+ LLM judge（软判据）+ 人在 gate 抽检（Inbox）。欠规格 → 自动执行自信地造错，故判据质量决定一切。

这条弧也解了「老板模型行不行」：老板模型的病是「薄规格上委派」；阶段一产出厚 spec 后，阶段二委派就成立。**Multica 只有阶段二（薄 ticket）；YouMind 只有阶段一（生成无验收闭环）；我们拥有整条弧。**

### 人类输入槽：谁填取决于阶段

每个 runtime（Claude Code 等）有一个「人类输入槽」（prompt / stream-json 输入）+ 一条输出流（TaskEvent）。谁填槽：
- **阶段一**：真人（经工作台 / deixis）
- **阶段二**：avatar（扮演人——读 TaskEvent、注入下一句、判完成）
- 共享总线：人随时可重新接管。**v0 桩：只建一条「填输入槽」路径**（RuntimeBackend inject-input），v0 人驱动、M2 avatar 驱动，不要分叉成两条。

### runtime 池 + 路由器（先人后 avatar）

- **WHAT（哪个 runtime 干这活）**：一池 binding `{inline:orchest, cli:claude-code, cli:codex}`。右活配右 runtime——**不是所有事都要 Claude Code**。
- **WHO（谁路由）**：v0 人选；M2 avatar 自动选/驱动。**「claude-code-as-skill」= M2 状态**（avatar 在池里按需调 Claude Code）。
- ⇒ **inline/cli 既非两种员工类型、也非固定 primary，而是「runtime 池 + 路由器」**。这彻底化解长期的 inline/cli 迷茫。

### Avatar = 人的授权代理 = 阶段二驱动者

- **avatar**：1:1 对人，带人的权限行事，是填输入槽 / 路由 / 驱动的代理。它**就是** M2 的外层 Orchest agent（inline binding）——我们从第一性重新推出它，旧设计给了它名字。
- **托管是光谱**：人自己做（不托管）←→ avatar 全做（全托管）。
- **与价值公理的张力（必须守）**：**全托管 = 人退化成「avatar 的老板」= §0.6 否定的空心老板模型**。故甜点与产品默认 = **共创（阶段一）+ 托管执行（阶段二）+ 自留验收**。全托管可做到，但不鼓励、非默认——否则就是「带 avatar 的 Multica」。
- **avatar 是我们 workforce 与 Multica 的分界**：它**携带共创出的上下文/沉淀进入执行**，而非派薄 ticket。
- **blast radius**：avatar 带人的完整权限自主行动，爆炸半径 = 人的全部授权。D9–D14 加倍适用（scoped permission、每 run 快照/回滚、预算闸、卡死升级、落盘前脱敏）。

### Employee = 可复用的 AgentConfig worker（deferred）

- **employee**：1:多，一个具名可复用的 AgentConfig（salvage 的 identity/soul/role），scoped 权限，被 avatar/人 实例化到某 runtime 上的 worker。
- 与 avatar 区分：avatar 是 1:1、带人全权的代理；employee 是 scoped 的 worker。
- **v0 两者都没有**：人 + workspace 里的 Claude Code 即可。

### Roadmap 映射

| 里程碑 | 工作流 | 角色配置 |
|--------|--------|----------|
| **M1** | 阶段一(共创介质 + 设计作为一等产出物) | 人填槽、人路由；无 avatar、无 employee |
| **M2** | + 阶段二(Ralph/自动执行 + gate + Inbox) | **avatar 入场**(= §7.3 Orchest 入场点 = 阶段二驱动者/路由器)；claude-code-as-skill 在此成真；跨厂商共享上下文经 avatar 实现 |
| **M3** | + workforce / team | employee + scope 共享沉淀 |

### 由本模型推出的 v0 桩（现在就埋，M2 零返工）

1. 单一「填输入槽」路径（人 now / avatar later 共用）
2. 进程组 runtime 生命周期（spawn/terminate/restart 整棵进程树）
3. RuntimeBackend 池抽象（v0 一个 binding，后续加）
4. 设计文档 + 验收判据作为一等 Locator 对象

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

**daemon 是独立进程 at host:port，host ∈ {本机 localhost，用户自有 SSH/Tailscale 可达盒子}（统一模型，见 D15）+ SQLite + Claude Code 单 runtime + 单 workspace；客户端瘦连接（v0 可 web-first，原生壳 Tauri/Electron 复审降级为后续打包项，[选型文档](../research/desktop-tools/electron-vs-tauri.md)）。** 本地与远程是同一模型 host 的两个取值，非两种架构；「内嵌/All-in-One」只是 host=localhost 的打包糖，不分叉客户端。

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

**D15 跨设备连续性是一等需求，不是「Cloud 模式」的子集（创始人 must-have）。** 在桌面重协作、在移动/web 接着看与轻量推进——这被错误地埋进了延后的「Cloud SaaS 模式」。纠正：它**几乎是已有架构的免费副产品**——daemon-first（工作活在 daemon，不在 UI）+ 事件溯源（任何客户端 replay events 还原 TurnState）+ 单一输入槽 + 单一 BACKEND_URL 客户端 ⇒ **「跨设备」= 另一个瘦客户端订阅同一个 daemon 的事件流**，不是新子系统。

- **移动 = 阶段二 surface，不是阶段一**（§0.7）：移动端不做重共创(无终端/browser 感知/重 deixis)，它是**阶段二的遥控 + 验收面**——看 avatar 在跑什么、approve/answer/轻量 inject、读 artifact(diff/doc)、gate 验收。这与「移动端肯定不可能有很重的人机协作」一致，且和两阶段模型天然咬合(桌面=阶段一,移动/web=阶段二)。surface 子集化:渲染器无关的 TurnState(`renderWeb`/`renderCard`/`renderPush`)本就为多终端预留。
- **拓扑裁决:本地 daemon + 瘦 relay 可达,不是云 daemon。** 把执行平面(workspace/files/terminal/dev server/Claude Code)搬到云会牺牲 local-first(我们对 YouMind 的差异、执行深度护城河的根)。所以:daemon 留在用户机器(local-first),靠 daemon 主动外拨的瘦 relay(只转发 events + auth,非全后端)让移动/web 可达(复用已设计的 reverse-WebSocket 模式)。这是「第三种形态」,区别于 reconstruction 的 All-in-One 与 Cloud SaaS。
- **安全**:可达的 daemon + 带人全权的 avatar = 远程控制面必须强认证;移动端 approve/inject 动作借 lark-bridge 的 callback 签名模式(签 run+scope+operator+action+过期+nonce)。
- **roadmap**:web 跨设备**便宜且早**(前端本就是 web,Tauri/Electron 只是壳;`BACKEND_URL` 配置已在 §6.11——创始人 v0 即可用 tunnel/LAN 从另一台机器开 web 接上);**移动 app + 产品化 relay + 阶段二遥控 UI = M2**(与 avatar/阶段二/验收天然耦合)。v0 桩:daemon 可达 + web 客户端能从异机连上,**不分叉客户端**。
- **可达性 ≠ 可用性**:relay/tunnel 解决「找得到」(NAT 穿透),但必须有一台机器**开着**在跑 daemon(可用性)。会睡的笔记本给不了「离开后继续跑」——过夜自动执行 + 在外遥控**结构上需要常开宿主**。daemon 放置是用户在「可用性 vs 拥有权」上的选择(同一 daemon 二进制):会睡笔记本 / 常开自托管盒子 / 可选托管云。
- **v0(一期)部署裁决:统一模型——daemon 是独立进程,监听 host:port;客户端经 BACKEND_URL 连接;host ∈ {本机 localhost,用户自有 SSH/Tailscale 可达盒子}。同一份代码,host 只是参数。无云、无 relay、无 SaaS。** 本地与远程不是两种架构,是 host 的两个取值:
  - **host=本机**:daemon + Claude Code + workspace/terminal/files 全在这台 PC;满血 Phase 1、零延迟、**除 LLM 调用外可离线**。主力开发体验。
  - **host=远程盒子**:同上,只是在另一台常开机器;给「会睡的笔记本」补可用性 + 在外手机可达。
  - **daemon-first 本机也成立**:daemon 是独立进程(非嵌在 app 窗口进程),关窗口不杀 daemon——本机 Ralph loop 继续跑、可重连。这是「本地远程一致」的根。
  - **「内嵌/All-in-One」= 可选打包糖**(host=localhost + 把启动本机 daemon 打包进 app),非架构分叉。
  - 连带简化:① 可用性+可达性按 host 取值各自满足;② **shell 决策放松、不卡 v0**——shell 不内嵌 daemon、是纯渲染瘦客户端,**v0 可 web-client-first**,原生壳(Electron/Tauri 复审)降级为后续打包项;③ **CDP 在 daemon 所在机器**(dev server + 感知 Chromium 挨着 Claude Code),客户端是 screencast viewer + 输入转发,electron-vs-tauri spike 前提部分消解。
  - **「local-first」精确为「self-hosted-host-first」**:文件/执行在用户自有机器(本机或自有盒子,非 vendor 云)——对 YouMind 的差异与执行深度护城河保住,贴近开发者「在自己机器上干活 / SSH 到自己 dev box」的真实习惯。

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
| 跨设备桩：daemon 可达 + web 客户端异机可连（D15，创始人 tunnel 即用） | 产品化 relay + 移动 app + 阶段二遥控 UI（M2，D15） | |

---

## 4.5 三个里程碑（按验证解锁，不按日历推进）

每个里程碑由上一个的实验数据解锁。里程碑回答「证明了什么」，阶段（reconstruction §七）回答「建什么」。

### M1「自己的工作台」——证明介质论

- **用户**：我们自己（程序员/技术创作者，单人）
- **交付**：五个面（对话/文件阅读/git/终端/browser）+ deixis + context tray + review 裁决 + objects 沉淀；单壳 + SQLite + Claude Code 单 runtime。**daemon 可达 + web 客户端异机可连**（D15 跨设备桩——创始人靠 tunnel/LAN 即可在另一台机器开 web 接着干）。对应阶段 0-3
- **解锁判据（exit criteria）**：
  1. 创始用户连续 4 周把日常真实工作放在里面完成（不是 demo，是默认工具）
  2. 实验 1 正信号：「指代替说」高频发生，失败点收敛为可修的 bug 清单
  3. 实验 3 开始计量：沉淀引用率有第一条基线曲线
- **此阶段不做**：任何团队功能、任何第二角色 surface；移动 app（仅留 web 异机可连的桩）

### M2「会积累的工作台」——证明沉淀复利 + 跨 agent

- **用户**：个人创作者（bottom-up 自然扩散的第一批外部用户）
- **交付**：Codex 第二 runtime（session-as-context 升级为跨厂商，兑现创世卡点 2）；知识投影深化（单人记忆复用）；文字创作者/PM surface；Claude Code 配置一键导入；**移动 app + 产品化 relay + 阶段二遥控/验收 UI**（D15——移动是阶段二 surface，与 avatar/自动执行/Inbox 验收天然耦合）
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

**Multica（2026-06 发现，监督/协调侧翼的开源巨头，迄今最重的竞品）**：36.5k stars、Apache 2.0（含商用限制的修改版）、Go + Next.js + Postgres/pgvector，「open-source managed agents platform / 下一批员工不是人类」。agent 即队友（个人档案、自建 issue、评论、改状态）、Squads（leader 路由委派）、可复用 Skills、Autopilot（cron/webhook 定时）、12 个 runtime 统一面板（Claude Code/Codex/Cursor/Copilot/Gemini/OpenCode/OpenClaw 等）、本地执行、可自托管。

- **它是「另两个愿景」的规模化实体**：Multica ≈ 本文档 §1 中我们明确放弃的「execution control plane」定位 + 同事的「团队知识库 / AI 员工」愿景，被别人做成了 36.5k stars 的产品。**这是对 D7/D8 路线裁决最强的外部印证**——监督/协调侧翼已被开源巨头重占，若我们把产品核心放在那里，是正面撞墙；同事的愿景因此应保持为 v1 投影层（D4/D8），不能升级为 wedge。
- **它的交互模型与我们正交，这是分界的根**：Multica 是 **assign-and-wait**（人是经理，agent 是员工，异步——派任务、离开、agent 回报、看 activity feed / kanban）。**无共创介质**：无文件/终端/browser/diff pane，无实时指代，无 deixis。我们是 **sit-and-co-create**（人是创作者，同步，指着说，同一 workspace）。「下一批员工不是人类」vs「人与 agent 共享一个光标」——经理心智 vs 工匠心智，是两种产品、两个时刻。它加共创层 = 基因冲突（与轻办公下不来同理：整个 UX/品牌建在经理模型上）。
- **多 runtime ≠ 跨 agent 共享上下文**（与 Vibeyard 同病）：12 个 runtime 是统一调度面板，Squads 是任务路由不是上下文转移；agent 之间共享的是 Skills（能力），不是彼此的执行上下文。创世卡点 2 依然无人做。
- **它是「厚度 ≠ star 数」最锋利的例证**：36.5k stars 的协调层，assign-and-wait 无沉淀（agent 是无状态任务执行器），其护城河问题与我们同构且更弱——12 家 runtime 的任一厂商都可吸收「派单 + 看板」。star 是触达/声浪，不是留存。
- **命名注意**：其「Squads」与我们旧产品 Create Squad 同词（见 salvage 文档），对外叙事避免被归为同类。Apache-2.0-改（反托管服务）的「开放但防御」许可策略，是我们自身许可未决问题（未决 §7）的一个参考样本。

### 5.5.2 地形小结：三翼已占，楔子更清

| 侧翼 | 占位者 | 缺什么（= 我们的差异） |
|------|--------|----------------------|
| 工程/session 管理 | Vibeyard（MIT，Electron） | 无介质宽度、无沉淀、浅指代 |
| 创作/内容生产 | YouMind（玉伯，Web/移动） | 无执行层、替你生成而非共创 |
| 监督/协调/AI 员工 | **Multica（36.5k，开源）** | assign-and-wait 非共创、无跨 agent 上下文、无沉淀 |

三翼都有强占位者，但**没有一个站在交点**：创作 × 执行 × 共创介质 × 用户本地沉淀。三者的共同缺失高度一致——**没有沉淀、没有跨 agent 共享上下文、没有同步共创介质**，这恰好是本文 D3/D4 + 创世卡点 2 锁定的位置。结论不是「我们没有竞争」，而是「我们赌的那个点至今无人站」——但三翼的存在把验证窗口收紧了：M1 必须让介质 + 沉淀在 v0 第一天就可感知，否则会被归入任一已占侧翼。

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
3. **Orchest 进入产品的时机与角色**：**已收敛（见 §0.7）——Orchest 作为 inline binding 入场的身份就是 avatar / 阶段二驱动者（替缺席的人填输入槽 + runtime 路由），时点 M2。** 其余候选切入点（Context Composer 智能化、知识投影提炼 agent / Dream/Distill）是 avatar 能力的子集或近亲，同属 M2+。
4. **v1 文字创作角色的引擎**：Claude Code 通用化使用，还是 Orchest 原生 agent
### 7.5 命名（已升级为明确待决项：建议改名，时点定在发布前）

「Multivac」是创始人对阿西莫夫《最后的问题》的致敬——而那个故事的内核（跨时间累积上下文 → 数据足够后完成创造，「LET THERE BE LIGHT」；以及 "INSUFFICIENT DATA FOR MEANINGFUL ANSWER"）恰好就是本文 §0.6 的两层赌注（沉淀复利 + 意义在创造）。**结论：要保留的是这个魂，不是这八个字母——魂已在产品里，与名字无关。**

但字符串本身有三层独立撞名负债（2026-06-13 调研）：
1. **直接竞品**：Multica（同品类、共享 Multi- 词根、`Multica` 近乎 `Multivac` 子集拼写、36.5k stars 先发心智）——会被误认为其拼写变体/fork/山寨，是名字相邻里最差的一种。
2. **在位商标大厂**：MULTIVAC Sepp Haggenmüller SE & Co. KG（德国包装机械全球龙头，活跃商标组合，持有 `multivac.com`）——商标注册更复杂、`.com` 不可得、SEO 首页被占。
3. **文化占用**：阿西莫夫的 Multivac 是著名专有名词，非空地。

**时点理由**：现处 phase 0，名字仅存于 docs（repo 仍叫 orchest），零品牌资产——**此刻改名成本在谷底，发布后建了品牌再改伤筋动骨**。故决策不必今天拍，但须在公开发布前完成；在此之前 Multivac 仅作工作代号。

**换名方向（待创始人启动）**：从同一语义场长出——琥珀/沉淀、墨与纸、终端血统、块状光标、deixis/指、共创/同一个光标、本地拥有，或阿西莫夫同源主题（累积到足够才能创造 / 不足以回答 / 终局之光）。硬性筛子：`.com` 或 `.ai` 可得、GitHub org 可得、商标软件类无强占用、不含被竞品与品类稀释的 Multi-/Agent/AI 词根、一眼会读会拼。

**附带的命名卫生**：品类词「AI 创作工作台」与 YouMind「AI Creation Studio」、定位词「工作台」与 Moxt「Agent-Native Workspace」均相邻——对外叙事须先于邻居锚定差异。产品名最急，品类词其次。

7.6 **仓库组织**：在 orchest monorepo 内孵化还是独立 repo——阶段 0 init 前定，影响 CI 与发布流水线

---

## 签署

| 角色 | 签署 |
|------|------|
| 个体生产力 / 介质论（Emile） | ☐ |
| 团队知识库 / 协作 | ☐ |
| 架构 | ☐ |
