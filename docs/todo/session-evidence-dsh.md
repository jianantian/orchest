# Orchest Session：补齐模型调用证据

状态：设计草案，未排期。基线：[Orchest 源码 `eedf05d`](https://github.com/jianantian/orchest/tree/eedf05d42646b8a2de3bc0605b57a03ce1c98fa2)。

## 为什么要做

今天重读一个 Session，可以恢复对话和执行状态，但不一定能回答：**某次请求实际给了模型什么、模型实际回了什么、哪些内容最终进入了会话。** 这三个答案在 hook 修改、重试和上下文压缩时会分开。把它们区分开，才能定位同一段输入为何在不同配置下表现不同，或者一次失败究竟发生在模型请求、流中断，还是运行时筛选输出之后。

保留现有 Session v2 的状态和恢复合同，不改用外部格式。本案所说“重建输入”是重建已有证据能支持的请求对象，不是再次请求模型并复现同一输出，也不重放工具。

## 现状与目标

| 问题 | 现有证据 | 缺口与目标 |
|---|---|---|
| 某次调用的模型与工具是什么？ | actor 将当前 `tool_defs` 和模型 options 传给 adapter；响应可上报 `option_adjustments`。 | 不能从当前配置倒推历史。对每次尝试绑定实际 provider/model、发送的有效 options、当时传入的有序工具定义；适配层进一步转换的部分单独标记来源和完整度。 |
| 模型实际看到了什么？ | `before_model` 每次从干净的会话投影重新构造输入；单次插入不回写原会话。[actor](https://github.com/jianantian/orchest/blob/eedf05d42646b8a2de3bc0605b57a03ce1c98fa2/crates/orchest/src/run/actor.rs#L1086-L1178) | 区分已提交对话、这次调用可见的上下文、用户作者身份，不把相同 role 当相同来源。 |
| 模型回了什么、最终接受了什么？ | 原始结构化响应先用于实时消息，随后可能被 `after_model` 改写或拒绝；错误可以重试，不会为每次尝试生成持久 `AssistantMessage`。[adapter 调用与 hook](https://github.com/jianantian/orchest/blob/eedf05d42646b8a2de3bc0605b57a03ce1c98fa2/crates/orchest/src/run/actor.rs#L1226-L1350) | 能对齐原始 adapter 响应、尝试结果、最终进入投影的事件。部分输出可被看到但不一定进入会话。 |
| 压缩怎么改变了上下文？ | `Compacted { messages }` 已保存压缩后的有效替换，足以推进 replay。[Session log](https://github.com/jianantian/orchest/blob/eedf05d42646b8a2de3bc0605b57a03ce1c98fa2/crates/orchest/src/session/log.rs#L21-L105) · [压缩调用](https://github.com/jianantian/orchest/blob/eedf05d42646b8a2de3bc0605b57a03ce1c98fa2/crates/orchest/src/run/actor.rs#L1495-L1519) | 标定被替换的上下文版本、执行方式与结果、保留/替换来源；只对确实生效的压缩推进上下文版本。 |

## 具体对照：DSH 保存了什么

以下以 [DSH 固定提交 `4b399c3`](https://github.com/deepseek-ai/deepseek-harness/commit/4b399c39a6d1479cd84ca591b4d7c88e24d38a96) 为准。借鉴可核对的证据，不照搬事件名、状态机或采集策略。

| DSH 原生记录 | 如何解释 | Orchest 取舍 |
|---|---|---|
| [`request/header`](https://github.com/deepseek-ai/deepseek-harness/blob/4b399c39a6d1479cd84ca591b4d7c88e24d38a96/docs/subsystems/session.md#L180-L209)：`header.{config,adapterDefaults?,tools?}`，以及 `reason`（initial / resume / change / series）、`startsSeries?` | config 记录 provider、模型、推理与采样；adapterDefaults 标记从精确 adapter 物化出来的默认值；tools 是当时组合出的定义。普通下一步／重试可沿用当时生效的头。`system/message` 属于消息投影，不在 header；单独的 `request/context` 是路由、容量等元数据，不参与重建或 header 等价判断。 | 用不可变的有效 profile 和上下文引用关联每次真实尝试，保留 adapter 默认和已做转换；不引入 DSH 的 epoch／series 状态机。 |
| [`user/message`、`system/message`](https://github.com/deepseek-ai/deepseek-harness/blob/4b399c39a6d1479cd84ca591b4d7c88e24d38a96/packages/core/session/src/types.ts#L318-L345) | 人工消息、运行时注入、目标续跑可以都是 model role = user；消息来源另以 `source` 区分。系统指令有自身来源；不要根据展示位置或 role 推导作者。 | 保留作者／注入来源、模型角色、母事件或 hook 引用；不把每条单次临时注入升级为持久会话消息。 |
| [`assistant/message` / `assistant/attempt`](https://github.com/deepseek-ai/deepseek-harness/blob/4b399c39a6d1479cd84ca591b4d7c88e24d38a96/packages/core/session/src/types.ts#L346-L370) | 前者有 `turn, step, message, stream, usage?, interrupted?`；中断后已提交的可见文本前缀仍是 message，标记 `interrupted: true`，不含未派发调用。后者记 `turn, step, stream`，适用于已结算却没有投影消息的尝试，含重试、取消或流失败；不声明每个失败都有完整流。 | 将原始结构化 adapter 响应／可确认的流、hook 后结果与入账引用分别记录；不改变 Orchest 在中断时原本是否提交前缀的行为，也不默认保存完整流。 |
| [`compaction/*`、`surfaceOp`、`sourceEventSeqs`](https://github.com/deepseek-ai/deepseek-harness/blob/4b399c39a6d1479cd84ca591b4d7c88e24d38a96/docs/subsystems/compaction.md#L7-L17) | `compaction/start`、`summary`、`end` 是日志记录，不是模型消息。summary 保存安全摘要、替换范围/具体节点及摘要模型信息，end 可标错。真正更改投影的是单独的 `user/message`，带 `surfaceOp` 的替换及来源序号。 | 保留现有 `Compacted { messages }` 作为恢复事实，独立补录生效前后上下文、替换/保留来源和压缩尝试；不把局部 `surfaceOp` 改成新的 replay 机制。 |
| [`tool/call` / `tool/result`](https://github.com/deepseek-ai/deepseek-harness/blob/4b399c39a6d1479cd84ca591b4d7c88e24d38a96/packages/core/session/src/types.ts#L371-L400) | 前者是请求的工具名和原始参数字符串，以 `callId` 配对；后者区分模型可见结果与可选失败和展示元数据。 | 复用 Orchest 的 dispatch/result ledger，补来源关联；模型提出调用不等于实际派发。默认不记录原始参数。 |

**例一：请求头不等于完整请求。** 假设位置 10 记录 profile（某 provider/model、reasoning 选项、两个工具定义）；第一次尝试与重试都引用 profile 10，但是第二次 `before_model` 多注入了一条提示。两个尝试的有效上下文引用应不同。即使原生 Session 的消息列表完全一样，模型看到的也可能不同。DSH 把系统消息、用户消息和相关投影规则放在 header 之外，二者共同决定输入；Orchest 用相同的区分，不承诺未采集的临时输入也能重建。

**例二：模型看到的顺序不等于日志顺序。** DSH 当前 surface 假设为 `[4, 9, 12]`，压缩结果在日志位置 21，随后在位置 22 提交摘要消息，其 `surfaceOp: {op:"replace", startSeq:9, endSeq:12}` 指当前投影上的两端，`sourceEventSeqs` 须包含被遮蔽的 9、12，可同时引用生成它的摘要记录 21。新 surface 为 `[4,22]`，原始事件仍保留；这不是删除日志上 9～12 的所有事件。普通消息用 `append`；`assistant/message` 自带流而不设 `sourceEventSeqs`，证据事件也不冒充 surface。见 [DSH 类型定义](https://github.com/deepseek-ai/deepseek-harness/blob/4b399c39a6d1479cd84ca591b4d7c88e24d38a96/packages/core/session/src/types.ts#L448-L479)。

**重建的前提与例外。** [DSH 的 reconstructability note](https://github.com/deepseek-ai/deepseek-harness/blob/4b399c39a6d1479cd84ca591b4d7c88e24d38a96/.agents/notes/implemented/architecture/2026-07-05-reconstructable-requests.md) 的字节级主张限定为由 agent loop 标记的请求，要有完整日志、可读且经过摘要校验的引用附件，以及固定的投影／adapter 代码；缺失或破损对象不能假装重建。独立 compactor 一次性调用不在这个 loop 不变量内：它另记摘要模型、范围与可用配置，可用固定代码重建其特定已知输入，不代表任意第三方 backend 可重建。Orchest 首先区分本地可证明的 adapter 输入和 provider 请求，不把外部服务的内部变换、未返回 token 或隐藏推理算作已知。

## 建议设计

### 1. 以实际尝试为单位记录有效输入

以 `(session_id, run_id, step, attempt_ordinal)` 定位运行时尝试；已分配且可用稳定的内部 `attempt_id` 则一并贯穿请求、响应和投影提交。不能只用 step：恢复会重新执行未完成的同位置，重试也共用 step。adapter 内部若透明重试，须有可观测的 transport attempt 才另记，不把不可见内部次数写为 1。

在通过校验、即将调用 adapter 时关联 `request_profile_ref`（请求态）和 `effective_context_ref`。Profile 是不可变、按内容去重的版本化快照，包含 adapter/provider 及版本、模型或已知 revision、有效采样／推理／输出限制等非密钥参数、工具选择选项、当次实际传入的工具 schema（顺序、名称、描述、参数及本次实际可用集合）。明确区分未设置、显式清空、不支持、被 adapter 调整、仅服务端知道；不要从解析到的工具调用反推当时工具集合。

分层标记证据：`orchest_input` 是 adapter 调用边界实际收到的对象；`adapter_request` 仅在 adapter 能提供经策略处理后的最终请求时才记，包括 provider 方言映射和已知默认值/剔除项。两者都不等于服务端内部最终 prompt 或模型权重版本。无证据处写未观测，不承诺字节级重建；工具层执行时的动态授权也不从工具 schema 推导。

### 2. 把流、尝试结果与入账结果分开

- 尝试结果至少区分：成功返回、provider/transport 错误、流错误、取消、进程中断未知。额外记录可确认的 retry 原因、usage、stop reason；尚未 dispatch 的 hook 拒绝/本地校验失败另记决策，不冒充 provider 请求。
- 捕获层级明确为 adapter 输出，不自动宣称收到 provider 逐字节原始流。可记录有界的 adapter chunks 或其受控引用；到达顺序与流片段边界仅在确实观察到时保留。前端显示过的前缀与持久消息分别记录。
- `after_model` 改写、拒绝与实际入账是独立事实；用已提交的 `AssistantMessage.seq` 或等价确认标记入账，不能把 adapter 返回成功直接当作已入账。日志说明工具是否实际可见／实际 dispatch，未派发的建议不成为调用事实。证据不进入原有 SessionProjection，不改变历史工具的执行、预算或恢复规则。

### 3. 记录上下文替换与内容来源

对成功的 compaction，把 `before_context`、`after_context` 和现有 `Compacted.seq` 关联起来；记录触发原因、保留段、被替换段、摘要来源或摘要调用引用（如有）。失败／取消只有尝试记录，无生效替换。按确知的来源引用映射，不对两段同文消息做猜测匹配；重复压缩后仍能区分原始消息、保留内容和生成摘要。禁止因为看到一段摘要，就把它倒推出早先一定存在的具体上下文。

每份有效上下文保留顺序以及每段的 model role 和来源类型。来源至少区分人工输入、系统指令／运行时注入、工具反馈、压缩生成、恢复或继承内容、未知；具体来源可指向原生事件、hook、受控产物或前序摘要。人工用户通过 steer 提供的指令，与运行时作为系统消息注入，需按语义分别保留作者与 model role；无法证明的旧日志来源不默认标为人工。只存在于单次请求的 hook 重排或插入，不得偷偷进入恢复投影。

## 存储、隐私和兼容

首选**独立版本、按 Session 派生的可选证据记录**，与 Session v2 已提交序号关联；不增加 v2 枚举类型去试探旧 reader 对未知事件的容忍度。原 `SessionMeta` 明确拒绝不匹配 schema；原 `SessionEvent` 要求连续序号。[store](https://github.com/jianantian/orchest/blob/eedf05d42646b8a2de3bc0605b57a03ce1c98fa2/crates/orchest/src/session/store.rs#L32-L84) · [log](https://github.com/jianantian/orchest/blob/eedf05d42646b8a2de3bc0605b57a03ce1c98fa2/crates/orchest/src/session/log.rs#L21-L39)。如未来要让某类新证据参与恢复，另行设计 schema 迁移，不在本案混入。现有 store 对步骤边界及有副作用工具的 checkpoint 次序不变；证据写入独立限界、可重试，不能阻塞模型流或工具安全闸。未捕获、捕获失败、进程中断、已裁剪等原因能确认时分别标记，无法确定时明确写未知；不会为了日志完整度重发模型或工具调用。

逐次记录是采集目标，不是崩溃后无缺口的保证。「准备调用 adapter」只证明走到了本地边界，不证明请求已发送或被 provider 接收。只有明确的本地拦截或取消证据才标为**未发出**；有 transport 发送或响应证据而无终态时标为**已发出、结果未知**；捕获关闭、丢失或进程崩溃后无充分证据时标为**是否发出未知**。缺失不是未发出，不用现有 v2 步骤 checkpoint 推断网络发送，也不为证据额外加同步写入。

默认不保存原始请求／响应、完整私有推理、认证材料、未授权的附件或 hook 输入；是否开启结构化精确采集由显式策略控制。记录允许的非敏感形状和覆盖状态；工具描述、参数默认值、错误字符串和模型流也可能含敏感信息，应用相同的字段白名单与脱敏规则。富内容仅以权限继承的引用保留，限定体积、留存期及读取者；不得把原始 token 序列、凭证 URL、系统私有路径混进日志、遥测或导出。删除同时撤销由其派生的引用／索引；不能靠 hash 或可猜测的稳定摘要识别已删敏感内容。明确区分精确、已脱敏、仅元数据和未知，不能宣传默认完整。证据仅存于受控的会话存储：不自动上传，不随模型请求附带，不进入第三方诊断或遥测。

## 验证

- [ ] 老 Session v2 fixture 可用旧路径恢复；加、删、损坏证据记录都不改变旧日志投影、权限、未完成工具的处理和预算。重读只报可辨识的证据缺口。
- [ ] 一个 step 出现 retry、adapter options 调整与 `after_model` 改写：每次实际 dispatch 可分别找到当时有效请求，只有真正入账的消息进入投影。
- [ ] 对照明确在 dispatch 前拦截、transport 已发送后崩溃、仅看到本地准备记录或证据写入丢失三类 fixture，分别显示未发出、已发出但结果未知、是否发出未知；流中出现可见前缀也不虚构已入账消息。
- [ ] 两次压缩夹一次失败压缩；可以按来源定位前后上下文，失败不生效。人工消息、工具反馈与一次性 hook 注入即使同 role 或同正文也不会串来源。
- [ ] 工具集改变、默认值由 adapter 调整、托管端不暴露 wire 信息时，分别显示确知差异与未知；配置同值快照可复用，使用不同 capture 策略的记录不得误称物理同一份。
- [ ] 在假 adapter 中用精确输入对照验证受控捕获与请求一致；用含密钥/私有内容的 fixture 验证默认不留原文、权限撤回和过期有效；故障/慢写场景验证正常流与原 checkpoint 语义不变。

开放项：证据的 backend 与保留策略、各 adapter 可报告的最高精度，以及如何在有界体积内记录流。先从本地合成和可控 adapter 验证证据模型，再决定是否排期。
