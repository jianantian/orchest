# ADR-0005: 宿主集成契约（Host Integration Contract）

- 状态：**Accepted**
- 日期：2026-10-24（决策 1 于 2026-10-22 定，其余五项 2026-10-24 定）
- 附加约束（owner，2026-10-24）：**不维护向后兼容**。本 ADR 涉及的公开面变更一律硬切换——不提供兼容层、双写、弃用期或迁移说明；版本策略仍按 [ADR-0003](./0003-release-policy.md) 走主版本号。先例：v0.9.9 移除 deprecated API、v0.12 移除 legacy adapter/factory。
- 相关：[`../iteration/direction.md`](../iteration/direction.md)（方向与里程碑）、[ADR-0001](./0001-provider-unification.md)、[ADR-0002](./0002-protocol-provider-decoupling.md)、[ADR-0003](./0003-release-policy.md)、[ADR-0004](./0004-sdk-packaging.md)
- 取证：[`../research/sdk-comparison/agent-runtime-landscape-2026-10.md`](../research/sdk-comparison/agent-runtime-landscape-2026-10.md)

## 背景

Orchest 要同时满足两个身份：可脱离任何宿主独立使用的 runtime SDK，以及 Multivac 的默认 agent runtime。两个身份之间必须有一条**事实归属边界**：否则会出现两份真相（两套恢复语义）、两个权限分类器、两套子 agent 生命周期。

宿主侧（Multivac）的既有约束是硬的：产品状态只留一份在控制面；接触 Workspace 字节的能力属于 Execution Node；权限由 Runtime 执行，宿主不另加命令分类器；多端目标采用平台控制面 + 用户执行。

业界证据（51 个本地仓库的代码级调研）给出三条稳定结论，本 ADR 的多数取舍以此为准：会话是 append-only 事件日志、消息历史是派生投影；崩溃后的进行中副作用必须显式标记为 unknown 且绝不自动重跑；read-only 的硬保证来自 OS 沙箱而非工具分类。

## 决策

### D1 — 执行态持久化归属：Orchest `SessionStore`（2026-10-22）

执行态的权威是 Orchest 的会话契约；宿主实现 `SessionStore`，把落地放在自己的持久化里。理由：Orchest 必须能脱离宿主独立使用；宿主的恢复走自己的标准格式，由 adapter 转换成（或关联到）runtime 自己的格式。

粒度：**每个 step 写一次 checkpoint**；此外每次工具副作用派发前额外落一个标记——否则"已执行、结果未落盘"的窗口无法判定。

实现形态随之确定：会话改为 **append-only 事件日志 + 派生投影**（当前的全量快照 + `serde(skip)` hooks 的形态是样本中唯一异常，且表达不了"进行中副作用"），并新增 **in-flight 副作用账本**：崩溃恢复时合成闭包事件（outcome-unknown / not-started / aborted-before-dispatch），不重跑、不丢弃、不让模型自行猜测。

### D2 — read-only 承诺：先软后硬，界面必须标注（D2，M1/M2 软、M4 硬）

- M1/M2：策略级隔离（工具面 + 命令模式级 + 路径级规则），界面标注 **soft**。
- M4：宿主注入 OS 级沙箱后端后，read-only 变为 **hard**。
- **只读不等于禁止命令执行**：调研任务离不开 grep 与跑测试；砍掉命令执行会让只读档什么也干不了。正确做法是保留命令执行、按规则挡掉写类动作。
- Orchest 提供**原生权限模式表面**：工具级 + 命令模式级 + 路径级判定、未批准默认行为可配（fail-closed）、批准作用域支持一次性与会话级两档、沙箱/执行后端注入点。**档位到效果的映射属宿主**。

### D3 — 远端拓扑：远端常驻执行节点（D3）

- 目标形态：远端 Host 上常驻一个执行节点进程（内嵌 Orchest），控制面通过 Runtime Port 指挥它。
- **否决**"Orchest 跑在控制面、工具调用经 SSH 转发"：违反 Execution Node 边界，远端快照/回滚拿不到，权限判断发生在错误的机器。
- M4 之前远端继续使用厂商 CLI；"默认 runtime"在此期间只对本地成立。
- **常驻执行节点二进制由宿主提供**，Orchest 只承诺可嵌入：无全局单例、允许同进程多实例、不假设工作目录。

### D4 — 子 agent：宿主的半一等对象（D4）

- 子 agent 有**独立记录**（可查、可停、崩溃不丢），但**共享父工作区**（不建独立 Workspace，因此不引入多套写锁与交付合并）。
- **用户可手动派发；模型只能请求**（防止无限派生）。
- Orchest 侧只保留进程内原语，并把子 run 的状态与可恢复性暴露给宿主；持久委派能力属于生态包（`orchest-subagents`），不进 core。

### D5 — 生态包：语言包管理器 + manifest + 社区索引；不建 registry；不允许进程内插件（D5）

- 包形态：一个目录 + 一份 manifest（name / version / description / author）+ 可选子目录（skills / agents / prompts / hooks / mcp 配置）。
- 分发：随各语言包管理器发布（crates.io / PyPI / npm），索引靠社区清单。**不运营第一方 registry**。
- 扩展方式只有两种：**工具**走 MCP 进程外或编译期 Rust 依赖；**知识资产**（技能、提示词、工作流）是纯数据，不执行代码。
- **不开放进程内插件**：第三方代码进宿主进程即可读 prompt、文件与凭据；且在 Rust 核心里"进程内"实际会退化为 FFI/子进程，只会模糊安全边界。

### D6 — SDK 1.0 冻结：M3 控制面补齐后，且至少 1 个独立宿主跨两个 minor（D6）

- 冻结范围：**API 形状 + 事件 payload 结构 + 错误分类 + 工具/技能发现方式**（事件 payload 是 Rust 与 SDK 共享的耦合点，必须一起冻）。
- 每次 SDK 发布给出 **SDK ↔ crate 版本矩阵与兼容性声明**（两条版本线独立，必须显式绑定）。
- 0.x 期间直接演进，不保留兼容层与弃用期；1.0 时一次性冻结。
- 现状的绑定面很薄（`run` / `run_sync` / `run_stream` / `respond_approval`，Node 另有 `register_tool`），此刻冻结会在 M3 补控制面能力时必然破坏。

## 后果

**对 Orchest**

- M1 工作量显著上升，且涉及公开面变更：`SessionSnapshot` → 事件日志属破坏性变更。**硬切换，不做兼容层、弃用期或迁移说明**，按 ADR-0003 走主版本号。
- 需要提供：事件日志会话 + 派生投影、每 step checkpoint + 工具副作用前标记、in-flight 副作用账本与合成闭包、取消令牌贯穿模型流/工具/审批、命令模式级与路径级权限判定、沙箱注入点、能力探测面（显式声明支持/不支持）。
- 不做：产品语义（计划提案、任务计划）、权限档语义、远端部署拓扑、第二套 agent loop。

**对宿主**

- 实现 Orchest 的 `Tool`（暴露自身能力）与 `SessionStore`（落地自身持久化）；adapter 只做格式翻译，不含产品逻辑。
- 负责权限档映射、只读 soft/hard 标注、Attempt 与 Unknown 语义、远端执行节点部署与升级。
- 不新增第二套 agent loop 或权限分类器。

**对生态**

- 不建 registry 换来的低成本，代价是发现性依赖社区；缓解手段是 manifest 约定与清晰的安装文档。
- 禁止进程内插件换来的安全边界，代价是 MCP 工具的通信往返成本。

## 备选与未采纳

| 决策 | 未采纳的备选 | 未采纳原因 |
|---|---|---|
| D1 | 宿主 canonical、Orchest 每 Turn 无状态重灌 | 两份真相；且 Orchest 作为独立 SDK 必须自己能用 |
| D1 | 快照 + 增量 journal（jcode 式） | 可作为实现细节，但表达不了"进行中副作用"；事件日志更通用 |
| D1 | 为快照 → 事件日志提供兼容层 / 双写 / 弃用期 | owner 明确不维护向后兼容；双写会让"执行态真相"再次分裂成两份 |
| D2 | 只做硬隔离 | 沙箱必须跑在接触文件的进程上，与 D3 强耦合，远端节点落地前无处安放 |
| D2 | 只读即禁命令执行 | 只读档将无法调研；与"完成任务优先于权限管控"冲突 |
| D3 | 控制面内跑 Orchest + SSH 工具代理 | 违反 Execution Node 边界；远端快照拿不到 |
| D3 | Orchest 自带常驻节点二进制 | 等于承担半个产品的发行责任；部署拓扑不是 runtime 的事 |
| D4 | 子 agent 为完全一等对象（独立 Workspace） | 多套写锁与交付合并，复杂度陡增；先取"可观察/可干预/不丢"的收益 |
| D4 | 子 agent 纯执行细节 | 与厂商现实脱节；父 Turn 崩溃即全丢 |
| D5 | 自建 registry | 零外部包的情况下运营索引没有用户；托管/审核/签名成本高 |
| D5 | 允许进程内插件 | 安全边界模糊；Rust 核心里实际退化为进程外 |
| D6 | 立即冻结 | 现有面过薄，M3 必然破坏 1.0 承诺 |
| D6 | 永不冻结 SDK | 外部开发者无法依赖，生态长不起来 |

## 非目标

- 不在 Orchest 内实现产品语义或宿主概念。
- 不在宿主内实现第二套 agent loop、权限分类器或恢复语义。
- 不运营第一方扩展 registry。
- 不在本 ADR 内确定沙箱的具体后端实现与平台覆盖矩阵（属 M4）。
