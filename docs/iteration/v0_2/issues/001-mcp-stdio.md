# 001 · MCP Stdio Transport 集成

## 背景

MCP stdio transport 是最简单也最通用的 MCP 接入方式：runtime spawn 一个子进程，通过 stdin/stdout 用 JSON-RPC 通信。官方 MCP server（`mcp-server-filesystem`、`mcp-server-github` 等）均支持 stdio。

## 目标

实现 `McpStdioClient`：连接 stdio MCP server，发现其 tool 列表，调用 tool，注册到 registry。

## 验收标准

- [ ] `McpStdioClient::connect(command: &str, args: &[&str]) -> Result<McpStdioClient, McpError>` spawn 子进程并完成 MCP 握手
- [ ] 握手：发送 `initialize` 请求，接收 capabilities 响应
- [ ] `client.list_tools() -> Result<Vec<McpToolDef>, McpError>` 调用 `tools/list`
- [ ] `client.call_tool(name: &str, input: Value) -> Result<Value, McpError>` 调用 `tools/call`
- [ ] `McpTool` 实现 `Tool` trait，`execute()` 委托给 `client.call_tool()`，`source` 为 `ToolSource::McpServer { server_id }`
- [ ] `AgentConfig.mcp_servers` 字段（v0.1 预留的占位）替换为 `Vec<McpServerConfig>`
- [ ] Runtime 初始化时自动连接所有配置的 MCP server 并注册 tool
- [ ] MCP server 进程异常退出时，对应 tool 的 `execute()` 返回 `ToolError`，不 panic
- [ ] 通过 `mcp-server-filesystem` 的端到端测试（列出工具、调用 `read_file`）

## MCP 协议说明

使用 MCP spec v0.2（JSON-RPC 2.0 over stdio）。消息格式：
```json
{"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "read_file", "arguments": {...}}}
```
响应：
```json
{"jsonrpc": "2.0", "id": 1, "result": {"content": [{"type": "text", "text": "..."}]}}
```
