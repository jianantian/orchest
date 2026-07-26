# 006 — ToolContext 一次性调用 helper

## 背景

seam-findings Finding 3/4: run 之外"调一次工具"必须手造 8 字段 `ToolContext`(`RunId::new()`、
`ApprovalBus::default()`、`run_depth: 0`、编的 `tool_call_id`、`event_tx: None`、默认预算、空
`parent_messages`……)——demo countdown 就是这么做的(`countdown.rs:286` 的 `tool_context()`);
`collect_info` 测试因构造太难,放弃执行工具本身,改为把校验逻辑复制进测试模块测副本
(`collect_info.rs` 测试模块),两份实现可静默漂移。

## 目标/范围

1. `ToolContext::oneshot()`:平凡值构造(新 RunId、run_depth 0、合成 tool_call_id、event_tx None、
   ApprovalBus::default、默认预算、空 parent_messages);doc 注明适用场景(run 外单次调用/测试)。
2. 视 API 手感可选加 `Tool::call_oneshot(input)` 便捷方法(内部构造 oneshot context)——二选一
   或都做,以实现时手感裁定,doc 写明。
3. **demo 采用**: countdown 改 `oneshot()`(删本地 `tool_context()`);collect_info 测试改执行
   真实 tool callback,删除复制的校验逻辑。

## 验收标准

- [ ] 一次性工具调用零样板;调用方无需知道哪些字段可安全伪造
- [ ] collect_info 测试走真实代码路径,复制的校验逻辑删除
- [ ] countdown 改 helper,链路测试绿
- [ ] 测试:oneshot context 可驱动一个真实 tool `execute`
- [ ] 五项检查全绿

## 备注

- Py/Node 绑定无需透传(绑定侧本就没有手造 ToolContext 的场景)。
