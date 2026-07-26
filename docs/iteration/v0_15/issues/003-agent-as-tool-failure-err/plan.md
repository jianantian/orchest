# 003 — 实施计划

## 要读的文件

- `crates/orchest/src/tool/agent_as_tool.rs`(execute 全貌 :150-249、SubAgentBuilder、现有单测)
- `crates/orchest/src/tool/mod.rs`(ToolError/ErrorKind 定义与 kind 惯例)
- v0.9.4 failure semantics 文档(`docs/archive/iteration/` 下,ErrorKind 裁定参照)
- `examples/demo/music-gift/src/tools/countdown.rs`(details["error"] 消费点)
- 其他 AgentAsTool/SubAgentBuilder 使用方(rg 找齐,评估破坏面)

## 要改的文件

- `crates/orchest/src/tool/agent_as_tool.rs`
- `examples/demo/music-gift/src/tools/countdown.rs`
- 测试(含受影响的其他使用方)

## 步骤

1. 失败分支改 `Err(ToolError)`:kind 裁定 + child_run_id/budget 诊断;`SubAgentFailed` 事件保留。
2. 成功分支保持不变(Structured + output_extractor + external_usage)。
3. 单测:stub child 失败 → Err;成功路径回归。
4. demo countdown 改 Err 分支;全 workspace 其他使用方核查适配。
5. 五项检查。
