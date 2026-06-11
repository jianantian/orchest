飞书原生 Multi-Agent 对 Create Squad 的威胁与非对称价值

调研日期：2026-06-10
 背景问题：如果飞书未来通过类似 bridge 的插件或原生能力实现 multi-agent，Create Squad 是否还存在核心价值
 本文性质：战略判断文档，不是功能设计文档

1. 一句话结论

如果飞书做出原生 multi-agent，最容易被平台商品化的是“入口层价值”，不是“执行控制平面价值”。

因此，Create Squad 仍然有价值，但前提非常明确：

Create Squad 必须把自己定义为 agent execution control plane / engineering workbench，而不是另一个聊天插件。

如果我们最终只剩“在 IM 里调多个 agent”这件事，那会被平台快速压缩；如果我们把核心做在 task、runtime、workspace、terminal、artifact、execution transparency 上，飞书反而更可能成为我们的入口，而不是终结我们。

2. 飞书原生 Multi-Agent 大概率会怎么做

如果飞书自己做 multi-agent，我判断大概率不会先从“本地代码执行工作台”切入，而会从它已经占优势的组织协作表面切入。

2.1 首先强化入口层

飞书最自然的入口不是 terminal，也不是 repo，而是：

- 群聊
- 私聊
- Doc / Wiki
- 评论
- 多维表
- 审批 / 工作流
- 日历 / 会议

所以它最可能做的是：

- 在现有协作表面上增加 @agent
- 用卡片、按钮、表单、slash command 做调度
- 支持在文档、表格、任务上下文里直接发起 agent 协作

这意味着它天然会比第三方工具更强于：

- 组织内分发
- 用户教育成本
- 权限模型
- 触达频率
- 协作闭环

2.2 然后做 agent network，而不是单 bot

它不会停留在“一个 bot 回答问题”，而更可能做：

- 一个 coordinator agent 负责任务拆解
- 多个 specialist agents 负责不同域
- 根据场景自动选择 agent
- 将结果回写到飞书对象而不是只回一段文本

典型形态可能是：

- 写报告 agent
- 数据分析 agent
- 日程 / 邮件 agent
- 表格处理 agent
- 代码助手 agent
- 审批辅助 agent

也就是说，飞书真正会做强的，不是“本地 runtime 管理”，而是“组织协作上下文中的 agent 编排”。

2.3 权限与身份会强绑定飞书原生模型

飞书如果原生做，会天然吃到这些优势：

- 用户身份已经存在
- 群/文档/知识库/表格权限已经存在
- OAuth / 应用授权已经有成熟基础设施
- 审计、可见性、组织级开关、管理员控制也已经存在

这类平台优势不是 bridge 能轻易复制的。

2.4 运行时可能会有 bridge / runner，但不会是产品核心

如果飞书要接本地仓库、内网资源、桌面环境，它很可能也会需要类似 bridge / runner 的东西，但那更像：

- 连接器
- runner
- worker
- gateway

而不是产品本体。

也就是说，即使它背后使用了某种“本地 daemon + 云端协调”的模式，用户感知到的核心仍然会是：

“飞书里有一个原生 agent 网络”

而不是：

“我安装了一个本地 bridge 来调 CLI”

3. 这对 Create Squad 的直接威胁是什么

3.1 入口层价值会被平台吞掉

如果我们的主叙事是：

- 在飞书里调用 agent
- 飞书消息映射任务
- 飞书卡片回写结果
- 多 agent in IM

那么这些价值会非常容易被飞书原生覆盖。

这是最直接的威胁。

3.2 协作容器层会失去差异性

飞书天然已经拥有：

- 消息
- 卡片
- 文档
- 评论
- 表格
- 会议
- 组织通讯录

如果我们只是围绕“协作容器”本身做文章，那平台永远更强，因为这些容器本来就是它的地盘。

3.3 “多 agent”本身不是壁垒

“支持多个 agent”不是差异化，只要平台愿意做，它可以很快商品化。

真正构成壁垒的从来不是：

- agent 数量
- 路由命名
- 卡片形态

而是：

- 执行深度
- 状态真相
- runtime 抽象
- 可恢复性
- 可审计性
- 和工程工作流的咬合度

4. 我们还有什么非对称价值

前提：我们必须把产品中心放在“执行系统”而不是“入口层”。

4.1 深执行层

飞书最强的是协作入口，不是深执行。

Create Squad 的非对称价值在于它可以把 agent 的真实执行过程建成一等公民：

- terminal
- workspace
- file operations
- diff
- artifact
- runtime projection
- task lifecycle
- recovery / watcher / diagnostics

这些不是协作平台天然擅长的地方。

4.2 执行控制平面

如果 Create Squad 继续把以下对象做成系统真相：

- Task
- Channel
- Employee
- Runtime
- Artifact
- Knowledge

那它就不是一个“聊天插件”，而是一个控制平面。

这类系统的价值在于：

- 同一个任务可被持续跟踪
- 同一个 runtime 可被监控、恢复、切换
- 同一个 artifact 可被多终端消费
- 同一个任务上下文不依赖某个 IM 容器才能存在

这类价值比“消息里能不能 @agent”更硬。

4.3 本地 / 混合 / 私有执行

飞书就算提供本地 runner，也大概率会把它视为接入手段，不会深耕：

- 开发机上的真实 CLI 生命周期
- 本地 repo / 内网仓库 / SSH runtime
- 长生命周期 task runtime
- 本地工作区与任务/产物的系统化映射

而这些恰恰是 Create Squad 可以做深的方向。

4.4 执行透明度

平台产品为了通用性，通常更偏向“结果导向”，不一定强调：

- 中间推理如何分阶段产生
- 调了哪些工具
- 哪一步失败
- 文件如何变化
- 为何需要用户确认
- 恢复时到底恢复了什么

而技术型用户和工程团队往往非常在意这些。

如果 Create Squad 能把“执行透明度”持续做强，它就会和飞书形成明显分层：

- 飞书负责协作容器
- 我们负责执行真相

4.5 多引擎统一执行层

平台大概率只会优先支持它定义好的那一套 agent/runtime 接口。

Create Squad 的机会在于成为：

- Claude Code
- Codex
- 未来更多 CLI / agent runtime

之上的统一执行抽象层。

也就是说，我们可以把平台看作上游入口，把多执行引擎适配、任务状态统一、产物归一和恢复策略放在自己这里。

5. 什么情况下我们会失去价值

下面这些方向如果成为产品核心，我们会非常危险：

5.1 如果我们只做成一个更复杂的 bridge

如果用户最终理解我们的方式是：

- 在聊天里调用 agent
- 回来几张卡片
- 能分配给几个 agent

那我们和飞书原生能力的差异会非常脆弱。

5.2 如果系统真相仍然依赖聊天消息

如果 task、artifact、runtime 只是消息流上的附属信息，而不是独立的一等公民，那么平台一旦把消息侧做强，我们的存在感就会变弱。

5.3 如果我们没有形成工程工作台体验

飞书不会天然提供一个对工程执行友好的本地工作台。

如果 Create Squad 自己也没把这块做出来，用户就会问一个致命问题：

“我为什么不直接在飞书里用原生 agent？”

6. 应如何重新定义 Create Squad

如果未来存在飞书原生 multi-agent，Create Squad 最健康的定义方式不是“飞书的替代品”，而是：

6.1 作为 execution OS / control plane

Create Squad 应该强调：

- task 是执行真相
- runtime 是执行宿主
- channel 是协作视图，不是唯一真相
- artifact 是执行产物，不是聊天附件
- knowledge 是任务与项目上下文，不是 IM 历史的副产物

6.2 作为 engineering workbench

Create Squad 的主战场应该是：

- 技术型任务
- 长生命周期任务
- 需要可恢复 runtime 的任务
- 需要 workspace/terminal/file/diff 的任务
- 需要清晰验收和追踪的任务

这和飞书原生 agent 的“广泛组织协作”不是同一层。

6.3 把飞书视为上游入口，而不是竞争者本体

最现实的格局不是二选一，而是分层：

- 飞书：身份、消息、文档、协作容器、组织分发
- Create Squad：task、runtime、workspace、terminal、artifact、execution transparency

如果这样看，飞书原生 multi-agent 不是一定要击败的对象，而可能是未来必须接上的入口层。

7. 战略判断：我们要防守什么，不要防守什么

不要防守的东西

这些东西平台一定更强：

- 消息入口
- 卡片容器
- 组织内分发
- 飞书对象权限
- 文档/表格/会议原生绑定

跟平台硬打这些，不会赢。

必须防守的东西

这些东西如果我们不做强，就真的会失去意义：

- task 作为系统真相
- runtime 作为执行真相
- local / hybrid execution
- terminal / workspace / file / diff 深体验
- artifact 与执行状态的系统化投影
- 可恢复、可诊断、可审计的执行链路
- 技术型用户真正需要的执行透明度

8. 对当前产品方向的启发

8.1 外部 IM 接入是加分项，不应成为产品定义

飞书、Slack、Telegram 等接入应该看作：

- distribution surface
- command surface
- notification surface

而不应该反向定义产品本体。

8.2 Employee 的价值不能只停留在“会聊天的 bot”

如果 Employee 最终只是飞书里可被 @ 的 bot，那么平台原生能力一来，差异就没了。

Employee 必须继续向更深的执行实体发展：

- 有 role/soul/identity
- 可承接 task
- 可拥有 runtime context
- 可与 artifact / knowledge / task history 绑定

8.3 task/workspace/runtime 三角必须继续做硬

这是 Create Squad 和任意 IM-native agent 产品最根本的潜在差异。

只要这三角是硬的，我们就不是“另一个入口”；
 一旦这三角是软的，我们就很容易退化成“另一个插件”。

9. 最终结论

飞书如果做原生 multi-agent，会对 Create Squad 构成真实威胁，但这个威胁主要集中在：

- 入口层
- 协作容器层
- agent 分发表面

而不会自动消灭下面这些价值：

- 深执行层
- execution control plane
- engineering workbench
- local / hybrid runtime
- execution transparency

所以真正的问题不是：

“飞书做 multi-agent 后，我们还有没有价值？”

而是：

“我们到底是在做入口层产品，还是在做执行系统？”

如果答案是前者，价值会被快速压缩。
 如果答案是后者，飞书越强，我们越应该把自己做成它上方或下方的执行控制平面，而不是试图重复它的协作表面。
