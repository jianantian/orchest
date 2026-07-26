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

- [ ] 含图片消息走 Chat 协议时,丢弃在 adjustments/日志可见(User/System/Assistant 分支逐一核)
- [ ] 构造 tool_use 跨切点用例,compaction 后消息序列无孤儿 tool_result;边界对齐有单测
- [ ] 两条路径回插 role 一致(`Role::User`);受影响测试更新;迁移说明落字
- [ ] 五项检查全绿

## 备注

- 三个修复相互独立,分函数/文件提交在同一 commit 内,消息体分述。
