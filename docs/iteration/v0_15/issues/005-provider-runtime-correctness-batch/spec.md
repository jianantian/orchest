# 005 — provider/run 正确性批(C5 多模态丢弃可见 / C6 compaction 边界 / C7 role 一致化)

## 背景

三个独立小修,均为"静默中招"类(SDK 优化计划 C5-C7):

- **C5**: `crates/orchest-provider-http/src/chat.rs` User 分支(:137-149)`_ => {}` 静默丢
  Image/Video/Audio block,无 adjustment、无警告;System 分支(:126-135)同样只取 Text。
  违反 `docs/polaris/observability.md`"错误不得静默"原则。
- **C6**: `crates/orchest/src/run/compaction.rs:77-79` 按条数硬切,切点落在 assistant ToolUse 与
  其 ToolResult 之间时,recent 窗口以孤儿 `tool_result` 开头 → Anthropic 400
  ("unexpected tool_use_id")。
- **C7**: 串行/handoff 路径工具结果以 `Role::User` 回插(`actor.rs:1599,1619,1644`),并行路径以
  `Role::Tool`(`actor.rs:1063-1066`);快照/钩子语义不统一。内部规范表示应为 `Role::User` +
  ToolResult blocks(Anthropic 惯例;OpenAI 方言由 chat.rs 在 wire 层转 `role:"tool"`,该转换已存在)。

## 目标/范围

1. **C5**: 丢弃非 Text block 时记录 `OptionAdjustment`(或等价诊断)+ `tracing::warn!`;
   逐一核 User/System/Assistant 分支的全部静默丢弃点。
2. **C6**: 切分点对齐 tool_use 边界——recent 窗口不得以"含 ToolResult 但其 ToolUse 在窗口外"的
   消息开头;assistant ToolUse 消息与其后续 ToolResult 消息不可拆散(必要时多保留一条)。
3. **C7**: 并行路径改 `Role::User` 回插;核查 `messages.rs`(Anthropic)对 `Role::Tool` 的现有处理
   并同步;快照/hook 消费方影响写入迭代实现记录或 migration notes。

## 验收标准

- [x] 含图片消息走 Chat 协议时,丢弃在 adjustments/日志可见(User/System/Assistant 分支逐一核)
- [x] 构造 tool_use 跨切点用例,compaction 后消息序列无孤儿 tool_result;边界对齐有单测
- [x] 两条路径回插 role 一致(`Role::User`);受影响测试更新;迁移说明落字
- [x] 五项检查全绿

## 备注

- 三个修复相互独立,分函数/文件提交在同一 commit 内,消息体分述。

### C5 核查结论(chat.rs `build_request_body`)

- 静默丢弃点共 4 处:System 分支只取 Text(丢其余一切);User 分支 `_ => {}` 丢
  Image/Video/Audio/Thinking/ToolUse/MidConvSystem,且 ToolResult 与 Text 混排时 Text 被静默
  丢弃;Assistant 分支 `_ => {}` 丢 Text/ToolUse 以外的 block;Tool 分支(旧快照遗留)忽略非
  ToolResult block。全部补 `OptionAdjustment { option: "content_block", reason:
  "chat_unsupported_content_block" }` + `tracing::warn!`(沿用
  `AnthropicProfile::encode_multimodal_block` 的 content_block 约定)。
- 例外:Assistant 分支的 Thinking block **不算丢弃**——它由 `profile.replay_reasoning` 显式接管
  (canonical 不重放是 protocol.rs 记录的设计决策),不重复记录。

### C6 边界算法

切点向前 walk(`split_at -= 1`)直到 recent 窗口首条消息不再携带"ToolUse 在窗口外"的
ToolResult;assistant ToolUse 消息与其 ToolResult 消息由此保持相邻,窗口必要时大于
`recent_messages`。若回退到 0 仍无安全切点,本轮放弃 compaction(无可摘要内容)。

### C7 迁移说明(快照/hook 消费方)

- **变更**:并行工具批路径(`actor.rs` `run_tool_and_handoff_phase`)回插消息由 `Role::Tool`
  改为 `Role::User` + ToolResult blocks,与串行/handoff 路径一致(Anthropic 惯例)。OpenAI 方言
  由 chat.rs 在 wire 层把 ToolResult blocks 转成 `role:"tool"` 消息(User 分支转换早已存在),
  Anthropic 方言本来就是 user + tool_result blocks——两条 wire 输出不变。
- **旧快照兼容**:`Role::Tool` variant 保留。旧快照中的 `Role::Tool` 消息仍可反序列化;
  messages.rs 的 canonical `messages_wire_role` 把 `Role::Tool` 映射为 wire `"user"`(内容即
  tool_result blocks),chat.rs 的 `CompatibleRole::Tool` 分支转为 wire `role:"tool"`——旧快照
  在两种方言下 wire 输出与新表示一致,无需迁移。
- **受影响方**:① 快照消费方——并行 run 之后保存的 SessionSnapshot 中,工具结果消息
  `"role"` 由 `"tool"` 变为 `"user"`;直接按 role 过滤/统计消息的外部消费者需知悉。② hook
  消费方——`before_compact` 等观察 `messages` 的 hook 所见历史中不再出现 `Role::Tool`(注意:
  配置非空 hooks 时并行路径本就走串行回插,故 hook 场景实际无变化)。③ bindings
  (`messages_from_wire_values`)按 serde 解析,`"user"`/`"tool"` 均可读,无影响。
