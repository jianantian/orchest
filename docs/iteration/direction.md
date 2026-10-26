# Orchest 迭代方向与里程碑

> 状态：草稿（待 owner 确认）；前瞻方向文档，不是实现合同。
> 日期：2026-10-22
> 适用范围：Orchest 仓库、Rust crates 与 Python/Node SDK；跨仓库目标涉及 Multivac。
> 权威顺序：`docs/iteration/` / `docs/hotfix/` 的 PRD 与 issue spec > 本文 > 研究文档。版本状态以 [roadmap.md](./roadmap.md) 为准。

## 1. 一句话方向

Orchest 的方向是成为**可嵌入、可治理、可作为默认执行层的 agent SDK**，而不是另一个 Pi 产品。

成功的标志不是 Orchest 自己有一个 CLI，而是多个独立产品把 Orchest 作为默认 agent 执行层，并能随版本升级。

## 2. 当前坐标

| 维度 | 当前状态 |
|---|---|
| Rust crates | 1.1.0 已发布到 crates.io（8 个 crate；`orchest-py` / `orchest-node` 不发布，SemVer 策略见 [ADR-0003](../adr/0003-release-policy.md)） |
| Python / Node SDK | SDK 0.2 规划中，尚未发布到 PyPI / npm（[PRD](./sdk_v0_2/prd.md)）；0.2 只做打包，控制面能力对齐是后续 SDK 版本 |
| 第一个生产宿主 | Multivac 计划把 Orchest 作为默认 runtime；目前 **没有代码依赖** |
| Runtime 能力 | loop、tools、skills、MCP、subagents、approval、session store 已有；取消的可抢占性、恢复、沙箱、可观测性有明确缺口 |
| 执行态持久化 | `SessionStore`（内存 + SQLite）已有，但是全量快照形态：只在 run 结束/错误时保存，hooks/retry/handoffs 被 `serde(skip)` 而无法随快照恢复。决策 1 = SessionStore 为准 + 每 step 写 checkpoint；M1 需把会话改为 append-only 事件日志 + 派生投影（业界主流，见 [调研](../research/sdk-comparison/agent-runtime-landscape-2026-10.md) §1） |
| 生态 | 无独立第三方生产宿主；同一 workspace 的 Murmur 以 git tag 固定 v1.1.0，使用 Python `Agent` 做 LLM 调用；无已知外部 provider/tool/skill 包；除 crates.io 外无 skills/tools 安装层 |
| 与 Pi 的关系 | Pi 是产品优先的 harness；Orchest 是 SDK 优先的 runtime。不要用 Pi 的产品指标衡量 Orchest |

当前阶段：**L0（Buildable）到 L1（First host）之间**。下一步不是扩大功能面，而是证明一个真实宿主能默认使用并跨版本升级。

## 3. 成功定义

> 一个 agent SDK 成功，当且仅当：多个独立产品在生产中把它作为默认 agent 执行层，能在不 fork、不写厚适配层的前提下跟随版本升级，并且第三方能独立为它提供 provider/tool/skill。

四个测试：

| 测试 | 问题 | 通过条件 |
|---|---|---|
| **Build** | 新宿主能不能跑通真实 agent？ | 从安装到首个真实 Turn 不需改 core；适配层不含产品逻辑 |
| **Upgrade** | 宿主能不能跟着版本走？ | 跨两个 minor 升级不 fork；每版适配成本有界 |
| **Keep** | 宿主会不会退回 vendor CLI？ | 默认执行层份额稳定；没有“先用 SDK，出事换 CLI”的双轨 |
| **Extend** | 第三方能不能不靠 core 团队扩展？ | 外部维护的 provider/tool/skill 在生产中被使用 |

北极星指标：**Supported production hosts**。独立团队、真实生产负载、把 SDK 作为默认执行层、运行在最近两个 minor 内、没有 fork core、适配层不含产品逻辑、通过一致性测试的宿主数量。

本文中的**宿主（host）**指把 Orchest 嵌入自身产品的应用。Multivac 是第一个目标宿主，不是唯一宿主。

反指标：GitHub stars、包下载量、provider 数量、示例 agent 数量、LOC、测试数量、“Multivac 已经用了”。这些可以上升而 SDK 仍然失败。

## 4. 战略支柱

1. **Host-first**：每个新能力都要能指向一个宿主需求。Multivac 是第一个目标宿主，不是唯一宿主。
2. **Contract-first**：事件、权限、错误、恢复语义都是契约，必须版本化、可测试、可回退。
3. **Ecosystem**：SDK 发布、provider 目录、skills/tools 包、文档与评测，决定外部采用速度。
4. **Differentiation**：差异化是可嵌入、多语言、Skills/MCP 标准、可治理边界；不是 provider 数量或 TUI 功能对等。

明确不做：

- 不做 TUI、编辑器、主题、项目管理；产品壳由宿主提供。
- 不做 DAG/工作流引擎；委派与编排是生态包，不是 core。
- 不以 provider 数量、star 数、示例数量作为成功标准。
- 不做多租户、托管平台或跨控制面转移。
- **不维护向后兼容**：公开面变更一律硬切换，不提供兼容层、双写、弃用期或迁移说明；版本策略按 [ADR-0003](../adr/0003-release-policy.md) 走主版本号。先例：v0.9.9 移除 deprecated API、v0.12 移除 legacy adapter/factory。

### 4.1 边界：runtime 事实 vs 产品事实

原则：**每个事实只有一个归属方；接缝是协议，不是共享状态。**

| 事实 | 归属 |
|---|---|
| loop 节奏、模型调用与重试、压缩、预算检查 | Orchest |
| 工具调度机制（注册、顺序/并行、校验、超时、截断、结果回插） | Orchest |
| 事件发射、顺序、背压语义 | Orchest |
| 阻塞请求机制（挂起、等待、超时、按 id 回复） | Orchest |
| 执行态快照与恢复语义（schema、写入时机、resume 边界） | Orchest（决策 1 = B） |
| provider 目录与模型能力事实 | Orchest |
| 子 run 的进程内生命周期 | Orchest |
| skill 加载机制、MCP 传输实现 | Orchest |
| 可注入的执行后端接口（`Tool` / `ScriptExecutor`） | Orchest 定义，宿主实现 |
| Project / Task / Thread / Turn / Attempt / Delivery 对象与状态机 | 宿主 |
| canonical 事件日志（用户看到什么） | 宿主 |
| Workspace / Environment / Host、快照回滚写锁 | 宿主 |
| 权限**策略**（档位 → 工具面与审批行为）、hard/soft 标注 | 宿主 |
| 人的决定（批准、回答、接受计划、确认交付） | 宿主 |
| 工具**能力供给**（file/git/terminal/browser/delivery 的实现） | 宿主 |
| Attempt 语义、远端拓扑与常驻节点部署 | 宿主 |
| 子 agent 的持久记录与产品身份 | 宿主（Orchest 只暴露子 run 状态与可恢复性） |
| 沙箱/执行后端的**具体实现** | 宿主注入；Orchest 提供注入点与 hard/soft 事实 |
| 扩展的**分发与发现** | 生态（语言包管理器 + 社区索引，Orchest 不运营 registry） |
| `SessionStore` 的**存储落地** | 共同：trait 与快照语义在 Orchest，存哪里由宿主决定 |

接缝协议（宿主实现 Orchest 的接口，Orchest 不反向依赖任何宿主）：

- 宿主实现 `Tool`：把宿主能力暴露成工具。能力在宿主，调用机制在 Orchest。
- 宿主实现 `SessionStore`：快照语义在 Orchest，落地在宿主。
- 宿主消费 `RuntimeEvent`：翻译成产品事件。执行态真相在 Orchest，展示态真相在宿主。
- 宿主把权限档翻译成工具面 + 审批策略：映射表属宿主，Orchest 只提供效果词汇与原语。
- 宿主用 start / resume / resume_with_input / stop 表达 Turn 生命周期。

不进 Orchest 的东西：Project / Task / Turn / Workspace / Delivery 等产品对象；plan proposal、task plan 等产品事件；权限档语义；远端部署拓扑；"这一轮要不要换 runtime"这类调度决策。

## 5. 能力缺口与优先级

| 缺口 | 影响 | 优先级 | 里程碑 |
|---|---|---|---|
| 可抢占取消：`abort` 目前只在 step 边界生效，审批等待最长 1h | 宿主的 Stop 不可信 | P0 | M1 |
| 零丢失事件：主通道 500ms 超时会丢弃该事件并记 `record_event_drop("primary")`，secondary 明确有损 | 宿主无法做 canonical 持久化 | P0 | M1 |
| 阻塞请求通道：`ApprovalBus` 只按 run id 存 bool，没有 request id，也没有通用的 blocking request | 宿主的权限/提问/计划无法统一映射 | P0 | M1 |
| 执行态恢复：快照只在 run 结束/错误时保存，hooks/retry/handoffs 被 `serde(skip)` 而无法恢复；业界主流是 append-only 事件日志 + 派生投影 | 崩溃后不能续跑同一个 Turn，也无法表达"进行中副作用" | P0 | M1 |
| in-flight 副作用账本：没有记录"已派发未确认"的工具调用，崩溃后只能靠 transcript 线性推断 | 可能重跑或丢弃已发生的副作用 | P0 | M1 |
| 原生权限模式表面：`ToolMetadata` 只有 `side_effect` bool，无命令模式级/路径级判定，无沙箱注入点，MCP 默认不触发审批 | 宿主映射不出可执行的权限档 | P0 | M1 |
| 能力事实：已有 model descriptors 与 `ThinkingLevel`，但缺少“run 中能否切换”“切换要什么代价”这类宿主可消费的事实 | 宿主无法渲染选择器或决定何时生效 | P0 | M1 |
| 能力探测面：没有统一的"支持/不支持"声明（恢复、取消、审批、委派、切换模型） | 宿主只能靠猜或 try/catch 分支 | P0 | M1 |
| 消息生命周期：没有 message 生命周期事件，也没有通用 blocking request 事件 | 适配器只能聚合；产品事件不应进 Orchest | P0 | M1 |
| Multivac 适配器与工具桥（宿主实现 Orchest 的 `Tool` / `SessionStore`） | 无法成为默认 runtime | P0 | M2 |
| SDK 打包发布与后续控制面能力对齐 | 外部开发者无法安装或无法控制 | P1 | M3 |
| Provider 覆盖与动态目录 | 模型选择受限 | P1 | M3 |
| skills/tools/provider 包与安装层 | 生态无法自增长 | P1 | M3 |
| 子 agent 持久记录：Orchest 子 run 无持久状态，宿主无法记录"派了谁、跑到哪" | 崩溃即丢，宿主无法展示与接管 | P1 | M2 |
| 一等委派层（后台、监督、调度、worktree） | 只有原语，没有 pi-subagents 等价物 | P1 | M3 |
| SDK 1.0 冻结条件与版本矩阵 | 外部开发者无法判断何时可依赖 | P1 | M3 |
| 沙箱/硬隔离 | read-only 只能算 soft | P1 | M4 |
| 常驻 Execution Node | 远端 Host 无法默认 Orchest | P1 | M4 |
| Evals / telemetry 闭环 | 质量与成本不可持续度量 | P1 | M3–M4 |
| 参考 CLI / 示例宿主 | 采用门槛高 | P2 | M3 |

## 6. 里程碑

### M0 — 基线（当前）

目标：把当前状态与假设写清楚，避免在方向未定时继续堆 runtime 功能。

退出标准：

- 本文被 owner 接受；六项宿主集成决策已记录在 §11 与 [ADR-0005](../adr/0005-host-integration-contract.md)。
- [roadmap.md](./roadmap.md) 的版本状态与本文不冲突。
- 现有 Pi 对比研究标注为历史基线，不作为当前结论：`docs/research/sdk-comparison/orchest-vs-pi-agent-gap-analysis.md` 顶部补一条说明（当前只覆盖 Prime Agent，未覆盖 Orchest 自身的变化）。

不包含：新功能、新版本号、发布承诺。

### M1 — Runtime 契约与可靠性

目标：Orchest 具备成为“可靠宿主 runtime”的最低契约。

范围：§5 中标记为 M1 的 runtime P0 缺口；宿主集成契约见 [ADR-0005](../adr/0005-host-integration-contract.md)；一致性测试。取证见 [调研：Agent Runtime 全景](../research/sdk-comparison/agent-runtime-landscape-2026-10.md)。落地迭代：[v2.0 PRD](v2_0/prd.md)（8 个 issue，硬切换，Rust 2.0.0）。

退出标准：

- Stop 在模型流、工具执行、审批等待中都能在有界时间内终止（建议目标 p95 < 1s），且不产生后续副作用。
- 事件路径零丢失、顺序稳定；慢消费者不会导致丢事件。
- 阻塞请求统一为 `request id + kind + payload`：可按 id 回复、撤回、查询；超时可配置。**kind 词汇由宿主定义**，Orchest 不认识 plan / question 这类产品语义。
- 执行态恢复（决策 1 = Orchest SessionStore 为准）：会话改为 append-only 事件日志 + 派生投影；**硬切换，不保留快照形态的兼容层**；日志 schema 版本化；**每个 step 写 checkpoint，且每次工具副作用派发前额外落一个标记**；可从日志重建 run（工具与 hook 由宿主重新注册）。
- in-flight 副作用账本：记录已派发未确认的工具调用；崩溃恢复时合成闭包事件（outcome-unknown / not-started / aborted-before-dispatch），**不重跑、不丢弃、不让模型自行猜测**。
- 原生权限模式表面（Orchest 定义原语，**档位到效果的映射属宿主**）：工具级 + 命令模式级 + 路径级判定；未批准默认行为可配（fail-closed）；批准作用域支持一次性与会话级两档；提供沙箱/执行后端注入点。M1/M2 阶段宿主侧只读标注 **soft**，M4 沙箱就位后转 **hard**。
- 取消语义：取消令牌贯穿模型流、工具执行与审批等待；明确"安全工具可中断、危险工具跑完"的边界；进程组终止作为可选后端。
- 能力探测面：runtime 显式声明支持/不支持（恢复、取消、审批、委派、切换模型），宿主据此分支，而不是靠猜。
- Orchest 提供宿主可消费的能力事实：模型、推理档、是否支持在 run 中切换、切换需要什么代价；**何时生效由宿主决定**。
- 新增 message 生命周期与通用 blocking request 事件；**不新增产品事件**（plan proposal、task plan 属宿主）；新增变体保持 `#[non_exhaustive]` 兼容，不用 `RuntimeWarning` 承载结构化事件。
- 上述每一项有一致性测试，不依赖真实 provider。

不包含：Multivac 代码集成、沙箱、远端执行节点。

### M2 — Multivac 本地默认 runtime

目标：Multivac 新本地 Task 默认使用 Orchest；已有 Task 保持原 binding；vendor CLI 仍是可选适配器。

范围：Multivac 侧 `ProviderId`、in-process provider spec、`RuntimeAdapter`、host tool bridge、事件翻译、权限映射、feature flag、parity matrix。

退出标准：

- 同一组真实 Turn 在 Claude / Codex / OMP / Orchest 上契约等价：事件形状、审批、Stop、恢复、错误分类；不要求模型输出相同。
- Stop / 重启 / 审批中断不产生重复副作用；无法确认时显示 Unknown，而不是假完成。
- 权限档映射一致：ADR-003 的四档（`read_only` / `approve_each_write` / `auto` / `bypass_all`；代码当前仍是 `approve_on_request`）与界面标注一致；没有硬隔离时明确标 soft。
- 默认切换可由配置回退；已有 Thread 不静默换 runtime。
- Multivac 侧实现 Orchest 的 `Tool` 与 `SessionStore`（快照落地在 Multivac 的持久化里），adapter 只做格式翻译，不含产品逻辑。
- 子 agent 是**半一等对象**：有独立记录（可查、可停、崩溃不丢），共享父工作区；用户可手动派发，模型只能请求（防止无限派生）。
- 权限档与 Enforcement 标注一致：M1/M2 的 read-only 明确标 soft，不做静默降级。
- Multivac 不新增第二套 agent loop 或权限分类器。

不包含：远端 Host、沙箱、完整生态发布。

### M3 — 生态地基

目标：外部开发者能安装、构建、扩展；出现第一个独立宿主和第一个外部包。

范围：SDK 0.2 打包发布（若尚未完成）、后续 SDK 版本控制面能力对齐（abort/steer/subscribe/session/MCP/approval）、provider 目录扩展、skills/tools/provider 包与安装、一等委派 crate、参考 CLI/示例、evals/telemetry 基础。

退出标准：

- 在支持的平台上 `pip install orchest-py` / `npm install orchest-sdk` 无需 Rust 工具链即可跑通 SDK 测试。
- 从安装到首个真实 agent 的路径有文档、有示例、有验收。
- 至少 1 个独立生产宿主在不改 core 的情况下跑通真实 Turn。
- 至少 1 个外部维护的 provider/tool/skill 包在生产中被使用。
- 宿主跨两个 minor 升级不 fork；适配层不含产品逻辑。
- 委派层支持后台运行、监督、失败证据和基本的 worktree 隔离；不要求第一版就有完整调度器。
- 扩展包可用：manifest（name/version/description/author + skills/agents/prompts/mcp 目录）随语言包管理器分发；不建第一方 registry；工具默认进程外（MCP 或编译期 Rust 依赖），不开放进程内插件。
- SDK 侧给出 1.0 冻结条件与冻结范围（API 形状 + 事件 payload + 错误分类 + 工具/技能发现方式），每次发布附 SDK↔crate 版本矩阵；0.x 期间直接演进，不留兼容层。

SDK 0.2 只承担打包与发布；abort/steer/subscribe/session/MCP/approval 等控制面能力进入后续 SDK 版本，不塞进 0.2。

不包含：Windows / Intel macOS、Pi 的全量 TUI/主题/编辑器功能、多租户。

### M4 — 远端与高级 runtime

目标：远端 Host 可默认 Orchest；read-only 有硬隔离选项；长任务可恢复。

范围：常驻 Execution Node、Orchest 在 Host 上运行、沙箱后端、后台委派与 checkpoint 原语（持久记录仍归宿主）、评测闭环。

退出标准：

- 远端 Turn 与本地 Turn 的语义、事件、审批、Stop 一致。
- 远端由宿主提供的常驻执行节点承载 Orchest；Orchest 侧保证可嵌入（无全局单例、同进程多实例、不假设工作目录）。
- 至少一种 OS 级硬隔离可用；界面能区分 hard / soft enforcement，且只读档在该后端就位后由 soft 转 hard。
- 进程/daemon 崩溃后，长任务能进入明确的恢复路径，不重复已确认的副作用。
- 后台委派有持久记录、监督和停止能力。

不包含：平台托管、多租户、跨控制面转移。

### M5 — 生态标准

目标：多个独立宿主默认使用；外部 provider/tool/skill 能自我维持；SDK 进入稳定期。

退出标准（建议目标，按阶段校准）：

- 3 个以上 supported production hosts。
- 最近两个 minor 的宿主升级率 ≥ 80%。
- 外部维护且在生产中使用的 provider/tool/skill 包 ≥ 5。
- fork 数为 0；厚适配层宿主数为 0。
- SDK 的 SemVer 破坏次数与升级成本在可接受范围。

## 7. 工作流映射

| 工作流 | 主要内容 | 主要里程碑 |
|---|---|---|
| Runtime core | 取消、事件、请求模型、恢复、工具效果 | M1 |
| Host integration | Multivac adapter、工具桥、事件/权限映射 | M2、M4 |
| SDK & bindings | SDK 0.2 打包发布；后续版本控制面能力对齐、异步/事件 | M3 |
| Providers & catalog | provider 覆盖、动态目录、推理档 | M3 |
| Ecosystem packaging | skills/tools/provider 包、安装、发现 | M3、M5 |
| Delegation | 后台运行、监督、调度、worktree、证据 | M3、M4 |
| Safety & sandbox | 效果策略、OS 隔离、容器 | M1、M4 |
| Observability & evals | span/metric 落地、一致性测试、评测 | M1、M3、M4 |

里程碑不是串行门禁：M1 与 M3 的部分工作可以并行，但 M2 必须等 M1 的 P0 契约通过。

## 8. 成功指标

建议初始目标，按阶段校准：

| 指标 | M1 | M2 | M3 | M4 |
|---|---|---|---|---|
| 独立生产宿主 | 0 | 0（Multivac 为第一方） | 1 | 2+ |
| Multivac 本地新 Task 默认 Orchest | 否 | 是（可回退） | 是 | 是 |
| Stop p95 | < 1s | < 1s | < 1s | < 1s |
| canonical 事件丢失 | 0 | 0 | 0 | 0 |
| 重复副作用 | 0 | 0 | 0 | 0 |
| 跨两个 minor 升级 | n/a | 内部验证 | 1 个外部宿主 | 2+ |
| 外部 provider/tool/skill 包（生产中） | 0 | 0 | 1 | 5+ |
| SDK 安装方式 | 源码 | 源码 | PyPI + npm | PyPI + npm |

“独立生产宿主”必须同时满足：不是 core 团队、真实生产负载、默认执行层、最近两个 minor、无 fork、适配层无产品逻辑、通过一致性测试。

“跨两个 minor”指宿主实际使用的版本线（Rust crates 或 SDK）。在任一版本线还没有产生足够 minor 之前，该项以“同一版本线内无破坏性变更、宿主无需改代码即可升级”衡量。

评审节奏：每季度复核 §8 指标与 §11 决策，结果写回 §12 变更记录；指标未达成时先改里程碑与范围，不改指标定义。

## 9. 风险与失败信号

风险：

- **产品化漂移**：为了“对标 Pi”加 TUI、编辑器、主题、工作流引擎，违背 [non-goals](../polaris/non-goals.md)。
- **Pi 对等陷阱**：用 provider 数量、功能数量、star 数当目标，忽略嵌入与升级体验。
- **软隔离冒充硬隔离**：没有沙箱时把 read-only 说成强保证，制造产品事故。
- **Multivac 长出第二套 loop**：Orchest 缺的能力被产品层重新实现，边界失效。
- **远端走捷径**：在控制面里跑 Orchest 再代理工具，破坏 Execution Node 边界。
- **API churn**：SDK 发布后频繁破坏，外部宿主无法升级。
- **支持成本失控**：每个宿主都需要 core 团队定制。

失败信号：

- Multivac 在 M2 后仍主要用 vendor CLI 跑真实任务。
- M3 后没有独立宿主。
- 宿主 fork core，或适配层出现产品逻辑。
- 外部包无人维护。
- Stop / 事件 / 恢复指标回退。
- 文档与代码再次出现 observability 式漂移。

## 10. 与现有文档的关系

| 文档 | 作用 |
|---|---|
| [roadmap.md](./roadmap.md) | 已发生/已规划的版本状态、依赖图、能力缺口历史 |
| `docs/iteration/<version>/prd.md` + `issues/` | 当前实现合同与验收标准；M1 的落地迭代是 [v2.0](v2_0/prd.md) |
| [polaris/](../polaris/) | 跨版本长期约束：概念边界、设计原则、non-goals、可观测性 |
| [adr/](../adr/) | 已拍板的架构与发布决策；宿主集成契约见 [ADR-0005](../adr/0005-host-integration-contract.md) |
| `docs/research/` | 证据与比较；[Agent Runtime 全景（2026-10）](../research/sdk-comparison/agent-runtime-landscape-2026-10.md) 覆盖 51 个本地仓库的持久化/权限/委派/取消，是 M1 的取证；旧版 [Pi 差距分析](../research/sdk-comparison/orchest-vs-pi-agent-gap-analysis.md) 是历史基线，不代表当前实现 |
| 本文 | 前瞻方向、里程碑、成功指标；不替代 PRD/issue |

跨仓库约束：Multivac 的 `docs/polaris/component-boundaries.md` 规定接触 Workspace 字节的能力属于 Execution Node；`docs/analysis/adr-003-layering-and-permissions.md` 规定权限由 Runtime 执行。Orchest 作为默认 runtime 的设计必须服从这两条，不能把工具执行或权限判断搬回控制面。

## 11. 决策记录

以下决策已由 owner 拍板；完整论证、备选与后果见 [ADR-0005](../adr/0005-host-integration-contract.md)。

| 决策 | 归属 | 状态 | 结论 |
|---|---|---|---|
| 执行态持久化归属 | Orchest（宿主实现 `SessionStore`） | **已定 2026-10-22** | SessionStore 是执行态权威；checkpoint 粒度 = 每 step + 每次工具副作用派发前；会话改事件日志 + 派生投影；新增 in-flight 副作用账本与合成闭包 |
| read-only 承诺口径 | 宿主策略 + Orchest 原语 | **已定 2026-10-24** | M1/M2 软隔离（界面必须标注 soft），M4 宿主注入沙箱后转 hard；只读不等于禁命令执行；Orchest 提供原生权限模式表面，映射表归宿主 |
| 远端拓扑 | Multivac | **已定 2026-10-24** | 远端常驻执行节点（内嵌 Orchest）由 Multivac 提供二进制；控制面内代理工具方案否决；M4 前远端继续用厂商 CLI；Orchest 保证可嵌入、无全局单例、允许多实例 |
| 子 agent 定位 | 宿主产品记录 / 生态包 | **已定 2026-10-24** | 半一等对象：独立记录（可查/可停/崩溃不丢）但共享父工作区；用户可手动派、模型只能请求；Orchest 暴露子 run 状态，持久委派进生态包 |
| 生态包格式 | 生态 | **已定 2026-10-24** | 语言包管理器 + manifest + 社区索引；不建第一方 registry；工具默认进程外（MCP 或编译期 Rust 依赖）；不开放进程内插件 |
| SDK 1.0 冻结 | Orchest | **已定 2026-10-24** | M3 控制面补齐 + 1 个独立宿主跨两个 minor 后冻结；冻结范围含事件 payload 与错误分类；每次发布给 SDK↔crate 版本矩阵；新能力先进 experimental 命名空间 |

## 12. 变更记录

| 日期 | 变更 |
|---|---|
| 2026-10-22 | 初稿：方向、成功定义、P0 缺口、M0–M5 里程碑、指标与开放决策 |
| 2026-10-22 | 记录决策 1 = B（Orchest SessionStore 为执行态权威）；新增 §4.1 边界（runtime 事实 vs 产品事实），按边界重写 P0 表述与 M1/M2 退出标准 |
| 2026-10-22 | 决策 1 补 A：checkpoint 粒度 = 每个 step 写一次 |
| 2026-10-22 | 新增 Agent Runtime 全景调研（51 个本地仓库）；据此把 M1 P0 扩到 9 项：事件日志会话、in-flight 副作用账本、原生权限模式表面、能力探测面；决策 2 与 5 的建议按业界证据复核 |
| 2026-10-24 | 决策 2–6 全部拍板并写入 [ADR-0005](../adr/0005-host-integration-contract.md)（read-only 先软后硬、远端常驻执行节点由宿主提供、子 agent 半一等对象、不建 registry 且禁进程内插件、SDK 1.0 冻结条件）；§4.1 边界、§5 缺口表与 M1–M4 退出标准同步更新 |
| 2026-10-24 | owner 指示不维护向后兼容：公开面变更一律硬切换（无兼容层/双写/弃用期/迁移说明），写入 §4 与 ADR-0005 |
