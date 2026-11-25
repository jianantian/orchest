# Session 证据迭代

状态：迭代分支已实现，发布版本待定。依赖已发布的 [Session v2](../v2_0/prd.md)。

## 背景与目标

SDK 使用者排查一次 run 时，需要知道某次尝试传给 adapter 的输入、adapter 返回的结果，以及实际提交到会话的消息。Session v2 保存恢复所需的事实。重试、一次性 hook 改写和上下文压缩会让会话历史与实际调用输入产生差异。

DSH 从已写入 Session 的消息和请求配置构造冻结请求，并用 mock adapter 实收对象对照历史重建结果。Orchest 采用同样的对照方法，在每次 adapter 调用边界采集证据。证据按 Session 关联和查询，原有 Session v2 日志继续驱动恢复。背景见 [PR #357 设计草案](../../todo/session-evidence-dsh.md) 和 [DSH 重建测试](https://github.com/deepseek-ai/deepseek-harness/blob/4b399c39a6d1479cd84ca591b4d7c88e24d38a96/packages/core/agent-loop/tests/request-reconstruction.spec.ts#L848-L895)。

## 使用场景

1. SDK 使用者按 Session 查出某个 step 中可查询的尝试，看到可确认的采集缺口，并区分本地取消与发送状态未知。
2. 排查者对比两次重试的参数、工具定义和有序消息，定位一次性 hook 带来的差异。
3. 排查者区分 adapter 返回、hook 处理与会话提交，理解可见文本或工具建议最后去了哪里。
4. 排查者追踪压缩前后的上下文，判断摘要调用是否成功以及运行时真正采纳了哪些变更。
5. 宿主为受控的本地会话开启内容采集，限制体积、读取权限和留存时间，并在撤销或过期后停止提供内容。

## 实现边界

- 首版覆盖绑定 Session v2 存储的 Rust run。宿主按次显式开启，resume 同样由宿主提供策略。启用时默认仅采集允许的元数据。内容采集需要宿主按字段授权，并在排队前处理正文、工具 schema、错误详情等可能承载敏感数据的值。已识别的凭证、完整私有推理和未经授权的附件始终排除。任意文本或自定义 JSON 中的秘密只能由宿主依据内容来源和策略判断，缺乏授权时整个字段跳过。
- 读取由宿主在受控本地存储内授权，作用域绑定到所属 SessionStore、Session id 和会话实例。不同存储或删除后重建的同名 Session 各自隔离。无有效权限时禁止返回正文、可解引用的引用或用于辨识内容的摘要。
- 每次实际进入 adapter 调用分配独立的 `attempt_id`。run、step、尝试序号用于展示和定位。Supervisor 重启 worker 会沿用 run id，允许重新执行相同 step。主循环调用和独立摘要调用各有自己的尝试标识和用途。
- 查询分别展示 adapter 调用边界、发送证据、结算和会话提交。证据关联 `SessionEvent.seq` 时，必须能验证会话实例和目标事件属于原调用。内存 append 与现有 checkpoint 的耐久确认分开表示，掉电前未确认的引用不得被后来复用的序号误绑定。SDK 观察流、Session schema、checkpoint、副作用账本、工具审批和恢复规则沿用 v2 契约。
- 使用独立版本的可选证据存储。首版随 Rust SDK 通过可选的 `session-evidence` feature 提供独立 SQLite 数据库。限制队列、单条记录、数据库文件大小及读取窗口。优先使用有界且增量的采集成本，证据写入与 Session checkpoint 不组成事务。读端区分可确认的丢弃、裁剪、过期、损坏和缺口原因未知；空记录不能证明没有发生调用。
- 重建范围以 `ModelAdapter::complete()` 收到的消息、工具和 options 为准。精确表示结构化值相等，且范围内每个字段均有授权和完整证据。adapter 报告的调整另记来源。发送或 provider 响应状态需要 transport 或上游证据；普通 adapter 返回值、流片段或泛化错误本身仅证明 adapter 边界处观察到的结果。首版通过可控 adapter 验证状态，兼容无 transport 证据的现有实现。
- 内容来源以有证据的事件、hook 或摘要引用为准。任意 hook 改写和旧会话历史可能缺少逐条来源，无法证明的关联标为未知。`Compacted` 的实际 append 决定替换是否生效，既有 checkpoint 决定耐久状态，摘要调用单独结算。压缩后的 Session 投影仍可被下一次请求的临时 hook 改写，两处各保留自己的上下文引用。

## 成功指标

- 核心 fixture 在同一 step 内产生两次 adapter 调用，共用同一配置，`before_model` 注入不同内容。受控采集下，从历史证据重建的消息、工具和 options 与 mock adapter 的实收对象逐项相等。成功返回经 `after_model` 改写后，只有实际提交的消息带入账引用。
- 外部 resume 和同一 run 内的 Supervisor 重启均有可验证的尝试标识。历史事件序号复用时，旧证据无法误关联到新的消息。
- 旧 Session v2 fixture 在证据关闭、存在、过期、损坏、积压或丢失时得到相同的恢复投影、待决工具、权限和预算。阻塞证据写入端时，模型流、原 checkpoint 和工具派发仍能完成。
- 同时覆盖压缩成功、摘要失败且无替换、摘要失败但 hook 改写已提交。下一次请求能定位真正生效的上下文；来源缺失时读端明确显示未知。
- 用合成敏感数据覆盖默认元数据、允许内容、禁止字段、体积裁剪、过期和撤销。检查持久文件、可读引用和既有遥测出口。

## 范围外

工具重放、再次请求模型复现输出、provider 请求的通用字节级重建、provider 隐藏状态、全量 transport 插桩、完整流归档、附件仓库、无 Session 绑定的 run、跨进程查询服务、Python/Node 绑定、UI、远程上传，以及 Session schema 迁移。

按 [ADR-0003](../../adr/0003-release-policy.md) 走增量 Rust API。保留现有 `SessionStore` 与 `ModelAdapter` 的必实现方法和既有配置的序列化行为。发布版本由路线图另行分配。

## Issue 顺序

| Issue | GitHub | 交付 | 依赖 |
| --- | --- | --- | --- |
| [001](issues/001-attempt-record/spec.md) | [#358](https://github.com/jianantian/orchest/issues/358) | 持久化并读取逐次调用的元数据 | Session v2 |
| [002](issues/002-input-reconstruction/spec.md) | [#359](https://github.com/jianantian/orchest/issues/359) | 受控采集及重建 adapter 输入 | 001 |
| [003](issues/003-response-settlement/spec.md) | [#360](https://github.com/jianantian/orchest/issues/360) | 对齐返回、hook 和会话提交 | 002 |
| [004](issues/004-compaction-lineage/spec.md) | [#361](https://github.com/jianantian/orchest/issues/361) | 关联压缩尝试、提交与上下文来源 | 002 |
