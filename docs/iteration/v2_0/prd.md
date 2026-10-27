# v2.0 PRD: Runtime 契约与可靠性

> 落地 [`../../iteration/direction.md`](../../iteration/direction.md) 的 **M1** 里程碑。
> 决策依据：[ADR-0005](../../adr/0005-host-integration-contract.md)（宿主集成契约）、[Agent Runtime 全景调研](../../research/sdk-comparison/agent-runtime-landscape-2026-10.md)。
> 状态：草稿（待 owner 确认）。GitHub issue 已建（#331–#338、#349），编号已回填各 spec。

## Background

Orchest 1.1.0 的运行循环、工具系统、技能、MCP、委派原语都已具备，但作为**可嵌入的宿主 runtime** 还差一层契约。当前实现里有七处结构性问题，每一处都会被真实宿主（Multivac）立刻撞上：

1. **会话是全量快照**。`SessionSnapshot` 只在 run 正常结束或出错时保存（`crates/orchest/src/session/persistence_hook.rs`），`AgentConfig` 的 hooks / retry / handoffs / store 全部 `#[serde(skip)]`（`crates/orchest/src/run/config.rs:134-141`），恢复后行为与恢复前不同。业界主流是 append-only 事件日志 + 派生投影（dsh、Codex、LangGraph、openhuman、jcode、Claude Code 一致，见调研 §1）。
2. **崩溃后的进行中副作用无处安放**。工具调用一旦派发就没有记录，崩溃后既无法判定"是否已执行"，也无法合成一个让模型看得懂的结论。
3. **权限原语太粗**。`ToolMetadata` 只有 `side_effect: bool`（`crates/orchest/src/tool/metadata.rs:10`），没有命令模式级/路径级判定，没有批准作用域，没有沙箱注入点，MCP 工具默认 `approval: Never`（`crates/orchest/src/tool/mcp.rs:470`）。宿主无法把"只读/写前问/自动/全放行"四档映射成可执行策略。
4. **阻塞请求没有身份**。`ApprovalBus` 按 `RunId` 存 `bool`（`crates/orchest/src/run/handle.rs:265`），无法表达"提问""计划提案"这类带内容的一次一问，也无法按 id 回复、撤回或查询。
5. **取消不能抢占**。`abort` 只是投递一条消息，只在下一个 `RunStep` 边界检查（`crates/orchest/src/run/actor.rs:346-357`），进行中的模型调用、工具调用与最长一小时的审批等待都不可中断。
6. **事件会丢**。主事件通道 500ms 超时后会丢弃该事件（`crates/orchest/src/events.rs:273`、`:523-530`），宿主无法用它做 canonical 持久化。
7. **没有能力声明**。宿主无法知道 runtime 是否支持恢复、抢占取消、会话级批准、切换模型，只能靠猜。

## Goal

给 Orchest 一套**能被宿主依赖的运行时契约**：执行态有唯一真相、崩溃后有确定语义、权限可映射、取消可抢占、事件不丢、能力可查询。做到这些，Multivac 才有理由把 Orchest 设为默认 agent runtime。

## Decisions

| 主题 | 决定 | 来源 |
|---|---|---|
| 执行态持久化 | `SessionStore` 是执行态权威；宿主实现它，落地在宿主持久化 | ADR-0005 D1 |
| checkpoint 粒度 | 每个 step 写一次；**每次工具副作用派发前额外落一个标记** | ADR-0005 D1 |
| 会话形态 | append-only 事件日志 + 派生投影；**硬切换，不保留快照形态兼容层** | ADR-0005 D1 + 不兼容约束 |
| 进行中副作用 | in-flight 账本 + 崩溃时合成闭包事件（unknown / not-started / aborted）；不重跑、不丢弃 | ADR-0005 D1 |
| 权限 | Orchest 提供原生权限模式表面；**档位到效果的映射属宿主** | ADR-0005 D2 |
| 只读口径 | M1/M2 软隔离（宿主标 soft），M4 宿主注入沙箱后转 hard | ADR-0005 D2 |
| 阻塞请求 | `request id + kind + payload`；kind 词汇由宿主定义 | ADR-0005 §4.1 |
| 取消 | 令牌贯穿模型流 / 工具 / 审批；明确"安全工具可中断、危险工具跑完" | ADR-0005 §4.1 |
| 事件 | canonical 路径零丢失；产品事件不进 Orchest | ADR-0005 §4.1 |
| 向后兼容 | **不维护**。公开面变更一律硬切换，按 ADR-0003 走主版本号（2.0.0） | owner 2026-10-24 |
| 合入时机 | 在 `iteration/v2_0` 分支开发，SDK 0.2.0 发布后再合入 main，避免 0.2 构建在半成品 runtime 上 | 月度计划 2026-10-27 |
| 绑定 | 本迭代让 Python / Node 绑定跟上新契约并透传取消与按 id 回复（009）；完整控制面重设计属 SDK 0.3 | 月度计划 2026-10-27 |

## Scope

- **会话事件日志**：`SessionLog` / `SessionEvent` / 派生投影；`SessionStore` 改为 append + read + flush + delete + list；移除 `SessionSnapshot` 与终止时自动保存钩子。
- **checkpoint 与恢复**：语义化落盘点（step 起点、工具副作用派发前、step 终点、run 终止）；`flush` 耐久屏障；`resume` 从日志重建 run；恢复边界与"未闭合 step"的处理。
- **in-flight 副作用账本**：已派发未确认的工具调用被记账；崩溃恢复时合成闭包事件并写回日志。
- **原生权限模式表面**：工具效果分类（取代 `side_effect: bool`）、单一决策点、命令模式级与路径级规则、批准作用域（一次性/会话级）、默认 fail-closed、沙箱/执行后端注入点与 hard/soft 事实。
- **阻塞请求通道**：带 id 与载荷的请求；按 id 回复/撤回/查询；超时可配且超时 fail-closed。
- **取消语义**：取消令牌贯穿模型流、工具执行、审批等待；工具可中断性分类；进程组终止作为宿主可注入后端。
- **事件模型补齐**：message 生命周期、通用请求事件、效果账本事件；能力探测面。
- **零丢失事件**：主通道不再丢事件；次要订阅者维持有损但可观测。
- **绑定跟上**：`orchest-py` / `orchest-node` 在新契约下编译通过，现有 SDK 测试全绿，并透传 `abort` 与按 request id 的回复。

## Out of Scope

- 任何产品语义：计划提案、任务计划、Task/Thread/Workspace、交付确认。
- 宿主的权限档映射表、UI、Attempt/Unknown 语义、远端执行节点部署。
- OS 级沙箱实现与平台覆盖矩阵（属 M4；本迭代只提供注入点与 hard/soft 事实）。
- 持久委派与后台子 agent（属 M3 的生态包）。
- SDK（Python/Node）控制面的完整重设计（真异步 run、session / resume、能力探测、事件类型等，属 [SDK 0.3](../roadmap.md)）。本迭代只做 009 的最小跟进。
- 向后兼容层、弃用期、迁移说明。

## Issues

依赖顺序执行；同组内可并行。

| Doc | Issue | Work | Type | Blocked by |
|---|---|---|---|---|
| 001 | [#331](https://github.com/jianantian/orchest/issues/331) | 会话事件日志与派生投影 | AFK | — |
| 002 | [#332](https://github.com/jianantian/orchest/issues/332) | checkpoint 落点、耐久屏障与恢复契约 | AFK | 001 |
| 003 | [#333](https://github.com/jianantian/orchest/issues/333) | in-flight 副作用账本与合成闭包 | AFK | 001, 002 |
| 004 | [#334](https://github.com/jianantian/orchest/issues/334) | 阻塞请求通道（id + kind + payload） | AFK | 001 |
| 005 | [#335](https://github.com/jianantian/orchest/issues/335) | 原生权限模式表面与执行后端注入点 | AFK | 004 |
| 006 | [#336](https://github.com/jianantian/orchest/issues/336) | 取消语义与工具可中断性 | AFK | 003 |
| 007 | [#337](https://github.com/jianantian/orchest/issues/337) | 事件模型补齐与能力探测面 | AFK | 001, 004 |
| 008 | [#338](https://github.com/jianantian/orchest/issues/338) | canonical 事件零丢失 | AFK | 001 |
| 009 | [#349](https://github.com/jianantian/orchest/issues/349) | 绑定跟上新契约（最小透传） | AFK | 001, 004, 006 |

1. [会话事件日志与派生投影](issues/001-event-log-session/spec.md)
2. [checkpoint、耐久屏障与恢复契约](issues/002-checkpoint-and-resume/spec.md)
3. [in-flight 副作用账本与合成闭包](issues/003-in-flight-effect-ledger/spec.md)
4. [阻塞请求通道](issues/004-blocking-request-channel/spec.md)
5. [原生权限模式表面](issues/005-native-permission-surface/spec.md)
6. [取消语义与工具可中断性](issues/006-cancellation-semantics/spec.md)
7. [事件模型补齐与能力探测面](issues/007-event-model-and-capabilities/spec.md)
8. [canonical 事件零丢失](issues/008-lossless-event-delivery/spec.md)
9. [绑定跟上新契约](issues/009-bindings-follow-contract/spec.md)

主要风险在 001：它改的是公开面（`SessionStore` trait、快照类型），牵动 run 生命周期、bindings 与示例。002/003 的语义正确性依赖 001 的日志模型定死之后才能测。

## Acceptance Criteria

- [ ] 会话以 append-only 事件日志为唯一执行态真相；消息历史、step、预算由日志投影得出，代码里不存在第二份状态源。
- [ ] 每个 step 与每次工具副作用派发前都有落盘点；`flush` 是显式耐久屏障；崩溃后能恢复到最后一个完整 step 边界。
- [ ] 崩溃恢复后，模型能读到每个未完成工具调用的明确结论（outcome-unknown / not-started / aborted-before-dispatch），且没有任何工具被自动重跑。
- [ ] 权限判定只有一个决策点，支持工具级、命令模式级、路径级；未批准默认 fail-closed；批准作用域支持一次性与会话级；宿主可注入沙箱后端并据此标注 hard/soft。
- [ ] 阻塞请求带 id、kind 与载荷；可按 id 回复、撤回、查询；超时可配且超时按未批准处理。
- [ ] 取消能在模型流、工具执行与审批等待中被观察；不可中断的工具跑完后再结束；不产生取消之后的新副作用。
- [ ] canonical 事件路径零丢失、顺序稳定；慢消费者产生背压而不是丢事件。
- [ ] runtime 显式声明能力（恢复、抢占取消、批准作用域、请求类型、并行工具、委派、效果分类、沙箱等级），宿主据此分支。
- [ ] `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --check` 全绿；`scripts/check-ts-event-variants.sh`（若涉及 TS 事件面）通过。
- [ ] 破坏性公开面变更在 `CHANGELOG.md` 的 `## [Unreleased] → ### Breaking Changes` 记录，并按 ADR-0003 走主版本号。
