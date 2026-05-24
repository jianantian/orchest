# 002 · MCP Streamable HTTP Transport 集成

## 背景

Streamable HTTP transport 允许连接远程 MCP server（无需本地进程），适用于云端 MCP 服务。与 stdio 不同，HTTP transport 需要处理认证和网络重连。

## 目标

实现 `McpHttpClient`：通过 Streamable HTTP 连接远程 MCP server，与 stdio client 共享相同的 `Tool` trait 包装。

## 验收标准

- [ ] `McpHttpClient::connect(url: &str, auth: Option<McpAuth>) -> Result<McpHttpClient, McpError>`
- [ ] 支持 `McpAuth::Bearer { token: String }`
- [ ] `list_tools()` 和 `call_tool()` 接口与 stdio client 相同
- [ ] 使用 Server-Sent Events（SSE）接收 streaming 响应（Streamable HTTP 协议要求）
- [ ] 网络断开时自动重连，重连间隔指数退避（最多 3 次，之后返回 `ToolError`）
- [ ] 请求超时（默认 30s，可通过 `ToolMetadata.timeout` 覆盖）
- [ ] `McpServerConfig.transport` 为 `McpTransport::StreamableHttp { url }` 时使用本 client

## 说明

Streamable HTTP 是 MCP spec v0.2 中定义的 transport 之一，基于 HTTP POST + SSE。每次 `tools/call` 是一个 POST 请求，响应通过 SSE stream 返回（支持中间进度推送）。
