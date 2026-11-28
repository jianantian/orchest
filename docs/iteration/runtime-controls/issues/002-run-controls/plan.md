# 002 · 按运行控制与公开事件类型实施计划

## 先读

- `crates/orchest/src/{events,capabilities,bindings}.rs` 和 `crates/orchest/src/run/{request,cancel,handle}.rs`。核对 wire 转换、请求 ID 与核心取消原因。
- 两端绑定的 `respond`、`abort`、`capabilities`、错误转换和 001 的运行对象。
- `python/orchest/{__init__.pyi,exceptions.pyi}`、`js/{types,native,public}.d.ts`、`scripts/check-ts-event-{variants,wire-naming}.sh`。

## 修改范围

- 两端运行对象的控制方法、公开事件和控制错误类型、共用事件 fixture 与契约检查。更新 `.github/workflows/{sdk,release-sdk}.yml` 的已安装包测试步骤，为两端提供共享 fixture。

## 实施顺序

1. 从 Rust wire 转换列出事件判别字段、必选和可选字段、嵌套载荷，以及已有的 `run_depth`、`child_run_id`。沿用核心的 snake_case 事件字段。为 Python 声明可判别的 TypedDict 联合，为 Node 补充现有 discriminated union，并校验两个包的公开导出能访问这些类型。
2. 在 001 的运行对象上公开按 `RequestId` 回复、按核心原因取消和能力探测。回复使用现有 `RequestResponse` 形状，覆盖批准、拒绝、结构化答案及会话授权。请求只发往目标运行的 `RunHandle`，能力从该句柄读取。Agent 兼容方法沿用原签名，通过 Agent 的当前运行接线。
3. 沿用 001 的结构化错误机制，扩展运行控制错误。按现有核心响应解析规则识别非法 payload，按 `RequestError::NotFound` 识别无对应待处理请求的 ID。两端按结构化类别断言，错误文本供诊断。取消复用核心布尔结果和终止原因。
4. 用确定性的本地模型场景生成 Rust 主接收器的预期 wire 事件序列。Python 与 Node 使用已安装包的公开运行入口执行同一场景，将实际迭代输出逐项与预期比较。仅归一化本次运行生成的非确定字段，比较顺序、数量、判别字段、代表性 payload、元数据、取消和失败载荷。两条 SDK 工作流显式复制同一份 fixture 到两端的测试目录。扩展已有变体检查，让 Rust 与两端公开声明对缺失和多余变体双向报错。
5. 用模型流和审批 fixture 验证首次取消、重复取消和终止事件，覆盖两个 Agent 同时等待请求、合法与非法回复、其他运行 ID、过期或已回答 ID。通过断言事件和工具执行结果确认回复作用于正确运行。
