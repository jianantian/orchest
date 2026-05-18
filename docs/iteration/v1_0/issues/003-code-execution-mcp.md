# 003 · Code Execution as MCP

## 背景

Anthropic 推荐的高级 agent 模式：与其为每种能力注册独立 tool，不如让 agent 写代码来调用能力。这种模式能大幅降低 tool call 次数和 token 消耗（agent 可以把多个操作组合成一段代码一次执行）。

实现方式：内置一个 Code Execution MCP server，提供 `execute_python` 和 `execute_javascript` tool。

## 目标

实现内置 Code Execution MCP server，每次 `agent.run()` 有独立的执行 session，stdout 流式输出，严格超时控制。

## 验收标准

**MCP Server：**
- [ ] 内置 `CodeExecutionMcpServer`，实现 MCP stdio transport（不需要用户单独启动进程）
- [ ] 提供两个 tool：`execute_python` 和 `execute_javascript`
- [ ] tool input schema：`{ "code": string, "timeout_seconds": integer (optional, default 30, max 300) }`
- [ ] tool output：`{ "stdout": string, "stderr": string, "exit_code": integer }`

**Session 隔离：**
- [ ] 每次 `agent.run()` 创建独立的 session（独立的 subprocess 或 interpreter context）
- [ ] 同一 run 内的多次 `execute_python` 调用共享 session（变量/导入保留）
- [ ] run 结束时销毁 session，释放资源

**Python 执行：**
- [ ] 通过 subprocess 运行 `python3 -c {code}`（或复用 persistent subprocess，见 v0.2 的 persistent script mode）
- [ ] stdout 流式转发为 `ToolCallUpdate` 事件（每行一个 update）
- [ ] stderr 包含在最终 output 的 `stderr` 字段

**JavaScript 执行：**
- [ ] 通过 `deno run --allow-net --allow-read -` 执行（Deno 提供比 Node 更好的权限控制）
- [ ] 若系统无 Deno，fallback 到 `node -e {code}`，并在 tool description 中标注沙箱级别降低

**超时控制：**
- [ ] 超过 `timeout_seconds` 时，kill 子进程，返回 `{ exit_code: -1, stderr: "timeout" }`
- [ ] budget guard 的 `max_duration` 也会终止正在执行的代码

**启用方式：**
- [ ] `AgentConfig.code_execution_enabled: bool`（默认 false）
- [ ] 启用时自动把 `CodeExecutionMcpServer` 注册为内置 MCP server

## 说明

v1.0 不做文件系统隔离和网络隔离。文档中明确标注：code execution 是可信代码执行（agent 生成的代码），不适合执行来自外部不可信来源的代码。
