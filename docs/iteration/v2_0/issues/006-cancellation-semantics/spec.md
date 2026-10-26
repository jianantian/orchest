# 006 · 取消语义与工具可中断性

GitHub issue: #336

## Background

`RunHandle::abort()` 只是投递一条 `Cancel` 消息；actor 只在处理下一个 `RunStep` 时检查取消标志（`crates/orchest/src/run/actor.rs:346-357`）。因为 actor 在一次 step 内 await 整个 step，**进行中的模型调用、工具调用与最长一小时的审批等待都不可中断**（`crates/orchest/src/run/actor.rs:36`、`:1298`）。

业界的成熟模型（调研 §6）：

- 协作式令牌 + 明确边界：dsh 每个 step 一个 `AbortController`，`agent.cancel()` abort 之，未派发的调用记 `TOOL_ABORTED_BEFORE_DISPATCH`；Claude Code 有 `AbortSignal` 与 `turn.abort`；Codex / prime-agent / openhuman / jcode 各有等价物（jcode 的 `InterruptSignal` 还带单调 epoch 防丢取消）。
- **取消有硬边界**：Kocoro 让安全工具（read/grep）可中断、危险工具（bash/write）跑完（`internal/agent/interrupt_filter.go:34`）；moumantai 让进行中工具完成（副作用已提交）、只丢弃缓冲文本；Claude Code 把运行中的命令转后台并标 `interrupted`，不杀。
- 进程组终止是常见兜底（openhuman、jcode、orca、ZCode）。

Multivac 的产品不变量要求"用户永远能停"。

## Goal

让取消在模型流、工具执行与审批等待中都可被观察；明确"安全工具可中断、危险工具跑完"的边界；取消之后不再产生新副作用；终止动作可由宿主注入的进程组终止后端兜底。

## Acceptance Criteria

- [ ] 存在可复制的取消令牌，贯穿：模型调用（provider 流）、工具执行、审批等待、子 run。
- [ ] `abort` 之后：进行中的模型流被中断；等待中的审批立即按取消处理（不是等超时）；未派发的工具调用不再派发。
- [ ] 工具声明**可中断性**：只读类工具默认可中断；有副作用的工具默认不可中断（跑完再结束）。宿主可覆盖。
- [ ] 不可中断的工具在取消后跑完，其结果正常落账；取消之后**不发起新的工具调用**。
- [ ] 被取消的未派发调用按 `TOOL_ABORTED_BEFORE_DISPATCH` 记账（与 003 的账本一致）。
- [ ] 终止事件带原因（用户取消 / 超时 / 上游要求 / 宿主断开），可被宿主区分。
- [ ] 宿主可注入进程组终止后端：取消时对仍在运行的外部进程做终止（尽力而为，不保证成功，失败要如实上报）。
- [ ] 取消是幂等的：重复调用 `abort` 不产生额外副作用；取消与正常结束竞争时，终态唯一且确定。
- [ ] 测试覆盖：模型流中断、工具执行中取消、审批等待中取消、取消后无新工具调用、幂等取消、进程组后端调用。

## Blocked by

003（账本词汇与 aborted 记账）。

## Notes

- provider 侧的中断依赖 `ChatModel::complete` 拿到取消信号：这是协议 trait 的破坏性变更（新增参数或改 `RequestOptions`），硬切换即可。
- 取消令牌与"可中断性"是两个概念：令牌是机制，可中断性是每个工具的属性。
- 进程组终止属于宿主能力：runtime 提供注入点，不自己实现跨平台终止逻辑。
