# 003 · Playground v0.2 / v0.3 scenarios + REPL

## 背景

Issue 002 把 playground 骨架和 v0.1 scenario 跑通了。本 issue 补齐 v0.2、v0.3 两个 scenario，并实现 `repl` 子命令，让人能交互式验收。

## 目标

让 `cargo run -p playground -- scenario v0_2_mcp_and_compaction` 与 `... scenario v0_3_subagent_and_codeexec` 在无外部 API key 的环境下能跑通，并提供一个真正可用的 REPL。

## 验收标准

### `v0_2_mcp_and_compaction` scenario

- [ ] 启动一个内置的 mock MCP server（stdio 子进程或同进程内的 in-memory transport），暴露 2 个 tool
- [ ] 注册该 MCP server，验证 `tools/list` 流程，事件流中出现 `ToolRegistered { source: McpServer { ... } }`
- [ ] 启用 `tool_search_enabled: true`，注册 30+ 个 fake tool，让 mock model 调用 `search_tools`，验证渐进式 schema 暴露
- [ ] 配置极小 `compaction_threshold`，让 mock model 输出超过阈值的 token，验证 `ContextCompacted` 事件
- [ ] 可选：通过环境变量 `PLAYGROUND_USE_OPENAI=1` 切换 mock provider 到 OpenAI shape，验证两个 adapter 行为一致

### `v0_3_subagent_and_codeexec` scenario

- [ ] 加载 fixture skill（`playground/fixtures/skills/echo-skill/`），验证 `SkillScanner` discovery
- [ ] 验证 `CapabilityValidator` 行为：fixture skill 含 `scripts/` 目录且声明 `capabilities`，未声明的环境变量不应进入子进程
- [ ] 触发该 skill 的 bundled tool（Python 或 shell script），断言 `ScriptExecutor` 执行成功
- [ ] 在 skill 脚本中通过 SDK 启动 sub-agent，验证：
  - 事件流中出现 `SubAgentStarted` / `SubAgentCompleted`
  - 父 run 的 `budget_used` 累计了子 run 的消耗
  - 子 run 的事件带 `parent_run_id` / `child_run_id` 字段
- [ ] 触发 Code Execution MCP 的 `execute_python` tool，验证 stdout 通过 `ToolCallUpdate` 流式输出
- [ ] 触发超时路径：`execute_python` input 中 sleep 超过指定 timeout，断言 tool 返回错误

### REPL

- [ ] `orchest-playground repl` 进入交互模式
- [ ] 命令至少支持：
  - `:tool register <name> <inline-rust-or-stub>` —— 占位实现，注册一个返回固定值的 tool
  - `:skill load <path>` —— 加载指定路径下的 skill
  - `:budget <max-tokens>` —— 调整当前 budget
  - `:send <message>` —— 发送一条 user message 并开始 run
  - `:quit`
- [ ] 所有 `RuntimeEvent` 实时打印；run 结束后回到 prompt 等下一条指令
- [ ] REPL 默认使用 mock provider；通过 `:provider anthropic` / `:provider openai` 切换到真实 provider（需对应 API key）

### CI

- [ ] 三个 scenario 都加入 CI（`cargo run -p playground -- scenario <name>`，exit code 0 视为通过）
- [ ] REPL 不进入 CI（交互工具）

## 注意

- v0.2 / v0.3 scenario 中如果需要 Python / Node 运行时（bundled script 测试、code exec），CI 必须保证这些可用；不行则 scenario 中跳过对应 step 并打印 `SKIP: <reason>` 而非失败
- fixture skill 越简单越好；不要在 fixture 里塞太多业务逻辑，那是 issue 007 的事
- 不要用真实外部 MCP server（如官方 filesystem server）作为 CI 依赖；内置一个 fake mcp server 进程
