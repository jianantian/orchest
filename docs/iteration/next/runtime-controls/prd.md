# 运行控制面 · Python / Node

## 背景与目标

当前发布版本为 `0.2.0`，覆盖 Rust、Python `orchest-py` 和 Node `orchest-sdk`。Python 的 `run` 仍同步收集事件，Node 的 `runSync` 返回 Promise，两个 SDK 都提供回调式 `run_stream` / `runStream`。现有 `abort` 与 `respond` 通过 Agent 上的当前运行句柄工作。Rust 已提供 `RunHandle`、类型化请求、无损 canonical 事件流和 Session v2 恢复，绑定尚未提供可独立控制的异步运行对象。[v2.0 绑定契约](../../v2_0/issues/009-bindings-follow-contract/spec.md)

交付一条两端对称的调用路径。宿主启动运行，异步读取事件，在同一运行上回复请求或取消，并可用显式存储恢复会话。运行状态、权限、事件顺序和恢复规则沿用 Rust 内核。这是路线图中提前交付的 [M3 SDK 控制面](../../direction.md#m3--生态地基)。

## 范围与约定

- 新增运行对象，承载异步事件迭代、等待完成、取消、按请求 ID 回复和能力探测。实现前在 001 确定 Python 与 Node 的公开签名。保留 SDK 0.2 的同步与回调入口及其返回形状。
- 首版每个 Agent 实例同时持有一个运行。两个实例可以独立并发。遗留入口与新入口共用占用规则，启动冲突有明确错误。
- 将 `RuntimeEvent`、请求回应、取消原因、能力和错误映射成 SDK 可检查的类型。新流保持 canonical 事件顺序和背压。公开事件形状由共享测试数据约束。
- 提供显式的本地 Session v2 存储配置，以及恢复原会话和追加新输入两条路径。继续由宿主提供模型与工具配置。
- Node 补齐 Python 已有的 Agent 作为工具注册及内建写文件工具注册。权限、工具 schema 与执行行为由 Rust 共用实现产生。
- 运行对象及会话控制属于本迭代。额外事件订阅、steering、持久委派、托管会话、语音双工和 1.0 冻结另行排期。

### 与 DSH 启发的 Session 证据的衔接

[Session 证据](../../../archive/iteration/session_evidence/prd.md)已经作为 Rust 可选功能合入主线。它记录逐次模型调用、受控的 adapter 输入重建、会话入账与压缩来源。证据采集显式开启，查询沿用原有会话身份与授权。

控制面沿用核心的会话和运行标识。运行事件供宿主实时处理，Session v2 日志用于恢复，独立证据存储用于事后查询。Python / Node 的显式采集和受控查询范围见 [下一版发布计划](../release-plan.md)。

## 发布验收

- [ ] 同一组 Python 与 Node fixture 能在模型流仍运行时读取事件、回复请求或取消，宿主事件循环可继续运行。
- [ ] 慢消费者能收到完整、有序的 canonical 事件。Python 退出运行上下文与 Node 关闭迭代器时，运行和等待项按公开契约回收。
- [ ] 两个 Agent 并行时控制动作只作用于目标运行。同一 Agent 重叠启动有明确错误，结束后可再次启动。
- [ ] 使用显式存储重建宿主后，可按 Session ID 恢复原运行或追加新输入。缺失会话和副作用账本场景沿用核心语义。
- [ ] Node 的两项工具注册通过权限与参数契约测试。现有 SDK 0.2 调用路径通过回归测试。
- [ ] 三平台 SDK 构建与安装测试通过。候选版附 Python、Node 示例、迁移说明和 SDK 对应的实际 Rust 基线。[SDK 发布策略](../../../adr/0004-sdk-packaging.md)

## 任务顺序

| Issue | 交付 | 依赖 |
| --- | --- | --- |
| [001](issues/001-async-run/spec.md) | 异步运行对象、事件迭代与生命周期 | v2.0 |
| [002](issues/002-run-controls/spec.md) | 运行控制、公开事件与错误类型 | 001 |
| [003](issues/003-session-resume/spec.md) | Session 配置、跨实例恢复与追加输入 | 001、002 |
| [004](issues/004-tool-registration/spec.md) | Node 工具注册对齐与双端集成验收 | 注册实现可先行，集成验收依赖 002、003 |

控制面和语音绑定分别按契约验收，共享的绑定入口、导出与声明文件在合并时核对。候选版编号和发包顺序见 [下一版发布计划](../release-plan.md)。
