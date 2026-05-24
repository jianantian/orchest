# 003 · Playground v0.2 / v0.3 scenarios + REPL

## 背景

Issue 002 把 playground 骨架和 v0.1 scenario 跑通了。本 issue 补齐 v0.2、v0.3 两个 scenario，并实现 `repl` 子命令，让人能交互式验收。

## 目标

让 `cargo run -p playground -- scenario v0_2_mcp_and_compaction` 与 `... scenario v0_3_subagent_and_codeexec` 在无外部 API key 的环境下能跑通，并提供一个真正可用的 REPL。

## 验收标准

### `v0_2_mcp_and_compaction` scenario

- [ ] 启动一个内置的 mock MCP server（stdio 子进程或同进程内的 in-memory transport），暴露 2 个 tool
- [ ] 注册该 MCP server，scenario 完成后通过 `ToolRegistry` 公开 API 断言 2 个 MCP tool 的 `ToolSource` 为 `ToolSource::McpServer { .. }`（**runtime 当前没有 `ToolRegistered` 事件，不要预期该事件**）
- [ ] 启用 `tool_search_enabled: true`，注册 30+ 个 fake tool，让 mock model 调用内置 `search_tools` tool，scenario 断言：
  - `ToolCallStarted { tool: "search_tools", .. }` 事件出现
  - 后续 model 输入的 system tools 列表（通过 mock provider 的请求 captor 取得）token 数小于"全量暴露"基线的 50%
- [ ] 配置极小 `compaction_threshold`，让 mock model 输出超过阈值的 token，断言事件流中出现 `ContextCompacted { removed_messages, summary_tokens }`，且 `removed_messages > 0`
- [ ] 可选：通过环境变量 `PLAYGROUND_USE_OPENAI=1` 切换 mock provider 到 OpenAI shape，断言两个 adapter 在同样脚本下产生同样的 `ToolCallStarted` / `ToolCallCompleted` 序列

### `v0_3_subagent_and_codeexec` scenario

- [ ] 加载 issue 007 产出的 `skills/code-review/`（**这是 v0.4 唯一的真实 skill**；本 scenario 既验证 skill 基础设施，也验证 code-review skill 自身可加载）
- [ ] 断言 `SkillScanner::scan("skills/")` 返回的 `SkillManifest` 列表包含 `name == "code-review"`，且其 `capabilities` 非 None
- [ ] 验证 `CapabilityValidator::execution_env`：scenario 启动前在父进程注入 `ORCHEST_DECLARED=visible` 与 `ORCHEST_UNDECLARED=hidden`，code-review skill 的 capabilities.env 只声明 `ORCHEST_DECLARED`；脚本执行时通过 stdout 回显环境变量，断言子进程能看到 `visible` 但拿不到 `hidden`
- [ ] 触发 code-review skill 的 `summarize_diff` bundled tool（input 包含一段固定 diff），断言：
  - 事件流出现 `ToolCallStarted { tool: "summarize_diff", source: ToolSource::Skill { skill_name: "code-review" }, .. }`
  - `ToolCallCompleted` 的 output JSON 包含 `files_changed` / `insertions` / `deletions` 字段
- [ ] 在 mock model 的脚本中安排让 model 请求启动 sub-agent（具体机制：mock model 输出一个调用 sub-agent 触发 tool 的 tool call，或在 skill 脚本中通过 orchest_sdk 发出 sub-agent 请求），断言：
  - 事件流中按顺序出现 `SubAgentStarted { parent_run_id, child_run_id, .. }` 与 `SubAgentCompleted { child_run_id, budget_used, .. }`
  - 父 run 结束时的 `BudgetUsage.tokens_used` ≥ 子 run 上报的 `BudgetUsage.tokens_used`
- [ ] 触发 Code Execution MCP 的 `execute_python`（input：`print(1); time.sleep(0.05); print(2)`），断言事件流中出现 ≥ 2 条 `ToolCallUpdate { tool: "execute_python", .. }`
- [ ] 触发超时路径：`execute_python` input 为 `import time; time.sleep(10)`，timeout 设为 1s；断言事件流以 `ToolCallFailed { tool: "execute_python", error }` 收尾，且 error 字符串包含 "timeout"

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

- [ ] 在 issue 002 创建的 `.github/workflows/v0_4-playground.yml` 中追加 step：`v0_2_mcp_and_compaction` 与 `v0_3_subagent_and_codeexec`
- [ ] 两个 scenario 在该 workflow 中各自 exit code 0 即视为通过
- [ ] REPL 不进入 CI（交互工具）

## 注意

- v0.2 / v0.3 scenario 中如果需要 Python / Node 运行时（bundled script 测试、code exec），CI 必须保证这些可用；不行则 scenario 中跳过对应 step 并打印 `SKIP: <reason>` 而非失败
- v0.3 scenario **直接消费 issue 007 产出的 `skills/code-review/`**，不再单独造 echo-skill fixture；issue 007 完成前本 issue 的 v0.3 scenario step 留 `SKIP`
- mock provider 在 issue 002 已定下复用契约；本 issue 仅扩展 mock provider 的预设脚本，不重写其 API
- 不要用真实外部 MCP server（如官方 filesystem server）作为 CI 依赖；内置一个 fake mcp server 进程
