# 002 · 核心类型定义

## 背景

所有模块都依赖同一套核心类型。先把类型定义稳定下来，后续 issue 才能并行推进，不会出现接口不对齐。

## 目标

在 `agent-runtime-core` 中定义全部公共类型，编译通过，无业务逻辑。

## 验收标准

- [ ] `Tool` trait 定义完整：`name()`、`description()`、`input_schema()`、`output_schema()`、`metadata()`、`execute()`
- [ ] `ToolOutput` enum：`Immediate(Value)`、`AsyncJob(JobHandle)`
- [ ] `JobHandle` 结构体：`job_id: String`、`poll: Arc<dyn Fn() -> BoxFuture<...>>`、`poll_interval: Duration`、`timeout: Option<Duration>`
- [ ] `JobStatus` enum：`Pending { progress, message }`、`Completed(Value)`、`Failed(String)`
- [ ] `ToolMetadata` 结构体：`side_effect`、`requires_approval`、`cost_hint`、`timeout`、`source`
- [ ] `ToolSource` enum：`InProcess`、`McpServer { server_id }`、`Skill { skill_name }`、`Builtin`
- [ ] `ToolContext` 结构体：`run_id`、`tool_call_id`、`on_update: Option<mpsc::Sender<Value>>`
- [ ] `AgentConfig` 结构体（含 `mcp_servers` 字段预留，类型为 `Vec<Value>` 占位）
- [ ] `BudgetConfig` 和 `BudgetUsage` 结构体
- [ ] `RunState` 结构体，含 `schema_version: String`（不用 `&'static str`，后者无法从 JSON 反序列化）
- [ ] `RunStatus` enum：全部变体包括 `WaitingForAsyncTool`
- [ ] `RuntimeEvent` enum：全部变体（参考 spec 中"Runtime Event"章节）
- [ ] `ModelStreamChunk` enum：`Text`、`Thinking`、`ToolCallArgsChunk`、`Done`
- [ ] 所有需要序列化的类型实现 `Serialize` / `Deserialize`
- [ ] `JobHandle.poll` 不实现 `Serialize`（文档注释说明跨进程恢复的限制）

## 注意

- `RunId` 用 `uuid::Uuid` 的 newtype wrapper
- `JsonSchema` 在 v0.1 用 `serde_json::Value` 作为类型别名，不引入 jsonschema crate
- `ToolError` 和 `ModelError` 各自定义为简单的 `struct { message: String, code: Option<String> }`
- `AgentConfig.mcp_servers` 在 v0.1 用 `Vec<serde_json::Value>` 占位，并加 `#[serde(default)]`；v0.2 替换为 `Vec<McpServerConfig>`。占位类型可以接受任意 JSON，但调用方不应在 v0.1 传入非空值
