# 003 · Tool Registry 与 InProcess Tool

## 背景

Tool registry 是 run loop 查询和调用 tool 的中心。v0.1 支持两种来源：InProcess（用户通过 SDK 注册的 FFI callback）和 Builtin（runtime 内置）。Skill bundled tool 在 issue 010 中添加，MCP tool 留到 v0.2。

## 目标

实现 `ToolRegistry`，支持注册和查询 tool；实现 `InProcessTool`，把 FFI callback 包装成 `Tool` trait。

## 验收标准

- [ ] `ToolRegistry` 支持 `register(tool: Arc<dyn Tool>)`
- [ ] `ToolRegistry` 支持 `get(name: &str) -> Option<Arc<dyn Tool>>`
- [ ] `ToolRegistry` 支持 `list() -> Vec<ToolDef>`（返回供模型使用的 tool 定义列表）
- [ ] `ToolRegistry` 支持 `filter_by_allowed(allowed: &Option<Vec<String>>) -> ToolRegistry`（按 allowed_tools 过滤）
- [ ] `InProcessTool` 实现 `Tool` trait，`execute()` 通过存储的 async callback 调用
- [ ] `ToolDef` 结构体包含 `name`、`description`、`input_schema`，用于序列化给模型
- [ ] 重复注册同名 tool 时返回 `Err`

## 说明

InProcess tool 的 callback 签名（Rust 侧）：

```rust
type ToolCallback = Arc<dyn Fn(Value, ToolContext) -> BoxFuture<'static, Result<ToolOutput, ToolError>> + Send + Sync>;
```

Python/TS 侧的 async 函数通过 FFI 包装成这个签名，具体实现在 issue 012/013 中处理。本 issue 只定义 Rust 侧接口。
