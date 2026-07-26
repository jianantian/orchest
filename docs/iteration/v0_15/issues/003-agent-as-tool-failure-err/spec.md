# 003 — agent-as-tool 子代理失败返回 Err

## 背景

`crates/orchest/src/tool/agent_as_tool.rs:212-248`: child run `RunFailed` 被包成
`details{"error": …}` 照常返回 `Ok(ToolOutput::Structured)`;消费方必须自己扒 details——demo
`countdown.rs` 的注释("Matching only on Immediate made every single countdown fail here")就是
踩坑记录。v0.9.4 的 ErrorKind taxonomy 已建立结构化失败语义,agent-as-tool 未遵循。

## 目标/范围

1. child `RunFailed` → tool 返回 `Err(ToolError)`;按错误性质选 ErrorKind(复用重试语义),
   kind 裁定规则写入 doc(如 budget 耗尽/模型侧失败 → 按现有 ToolError kind 惯例)。
2. 诊断保留: error message 带 `child_run_id`;`SubAgentFailed` 事件照常发射;`budget_used`
   进 ToolError 的诊断 payload(ToolError 有 details/诊断字段则带上)。
3. 失败路径不再构造 `Structured`,`output_extractor` 不再收到失败 details。
4. **demo 采用**: `countdown.rs` 删 `details["error"]` 匹配,改 `Err` 分支处理。

## 验收标准

- [ ] child 失败时消费方拿到 `Err(ToolError)`,kind 语义在 doc 注明
- [ ] countdown 类消费方无需匹配 `details["error"]`(demo 同步改,链路测试绿)
- [ ] `SubAgentFailed` 事件仍发射,含 `child_run_id` + error
- [ ] 测试:child RunFailed → Err(kind 正确、含 child_run_id 与 budget 诊断);成功路径行为不变
- [ ] 五项检查全绿

## 备注

- 004(输出契约)的"纠正后仍失败 → Err"复用本 issue 的 Err 语义,按编号顺序先做本 issue。
