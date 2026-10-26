# 005 implementation plan

## Files to read

- `crates/orchest/src/tool/{mod.rs,metadata.rs,registry.rs,mcp.rs,agent_as_tool.rs,code_exec.rs,search.rs}`
- `crates/orchest/src/run/{config.rs,actor.rs}`（`ApprovalMode`、审批判定点 `should_approve`）
- `crates/orchest/src/skill/{executor.rs,types.rs}`（`ScriptExecutor`、capabilities 声明）
- `crates/orchest/src/run/request.rs`（004 的通道，询问的出口）
- `crates/orchest/src/events.rs`（审批事件）

## Files to change

- `crates/orchest/src/tool/metadata.rs`：`ToolEffect` 分类、`ToolMetadata.effects`、可中断性与作用域标记
- 新增 `crates/orchest/src/tool/policy.rs`：规则类型（工具 / 命令模式 / 路径）、匹配器、单一决策点 `decide()`
- `crates/orchest/src/run/config.rs`：`PermissionPolicy`（宿主提供的规则集）、默认 fail-closed、批准作用域
- `crates/orchest/src/run/actor.rs`：审批判定改为走 `policy::decide`，MCP 工具同样走
- `crates/orchest/src/tool/mcp.rs`：MCP 工具的默认批准策略与效果标注
- 新增 `crates/orchest/src/tool/sandbox.rs`：`ToolSandbox` 注入点与 `Enforcement`（hard/soft/none）
- `crates/orchest/src/skill/executor.rs`：与新执行后端的关系（不再假定无隔离）
- `crates/orchest/src/events.rs`：权限判定结果事件（允许/询问/拒绝 + 原因）
- bindings 与示例：跟随公开面变更

## Steps

1. 定义 `ToolEffect` 枚举与 `ToolMetadata.effects`（硬切换移除 `side_effect`）；把既有内置工具与 MCP 工具逐一标注效果。
2. 定义规则模型：`PermissionRule`（作用域=工具/命令模式/路径、动作=allow/ask/deny、模式与路径 glob）；实现匹配器（最后命中规则胜出），并把匹配输入规范化为"工具名 + 归一化参数"（命令取解析后的命令链，路径取规范化路径）。
3. 实现单一决策点 `decide(tool, input, metadata, policy, session_memory) -> Decision`；未知效果或未匹配 → 询问（fail-closed）。
4. 接入 actor：工具执行前决策；询问走 004 的通道；拒绝生成带原因的拒绝结果。
5. 会话级记忆：按"工具 + 归一化参数摘要"缓存决定；宿主可选择不记忆（默认只记显式授予）。
6. MCP 工具改为与其他工具同样经过决策点；移除默认 `approval: Never`。
7. 执行后端注入点：定义 `ToolSandbox` 与 `Enforcement`；默认实现为无隔离（soft）；runtime 对外暴露当前强度事实（供界面标注）。
8. 写清 skill capabilities 与工具效果的关系（声明 ≠ 执行边界），更新 `docs/guide/tool-surface.md`。
9. 测试：四档典型规则、fail-closed、会话级记忆、MCP 不绕过、注入后端后强度事实变化。
10. 跑 `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --check`；`CHANGELOG.md` 记破坏性变更。
