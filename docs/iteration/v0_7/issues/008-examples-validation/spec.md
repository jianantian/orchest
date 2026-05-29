# 008 · Examples + Final Validation

## 背景

v0.6 deferred 了 examples。v0.7 引入了多个新 API（Hook、Agent-as-Tool、Handoff、Retry、Loop Detection），需要补齐使用示例并做最终集成验证。

## 目标

补齐 v0.7 新 API 的使用示例，确保所有 issue 组合后端到端可用。

## 范围

### Examples

在 `examples/` 目录下新增：

| 示例 | 演示内容 | 路径 |
|------|---------|------|
| `hook_logging.rs` | 注册自定义 Hook，在 before_model / after_tool 打印日志 | happy path |
| `hook_modifier.rs` | before_model hook 修改 messages（如注入 context） | happy path |
| `hook_abort.rs` | hook 返回 Abort 终止 run + hook panic 后 run 继续 | **error path** |
| `agent_as_tool.rs` | 用 `AgentConfig::as_tool()` 创建子 agent 工具，父 agent 调用 | happy path |
| `handoff_routing.rs` | Triage agent 通过 Handoff 路由到 Billing / Support agent | happy path |
| `handoff_input_filter.rs` | Handoff 时用 input_filter 裁减上下文 | happy path |
| `retry_exhausted.rs` | 配置 RetryPolicy，演示 429 重试 + 重试耗尽后的错误处理 | **error path** |
| `loop_detection.rs` | 启用 loop detection，演示循环检测 → 警告 → 最终终止 | **error path** |

每个示例应该：
- 可独立 `cargo run --example <name>` 运行
- 使用 mock model/tool（不依赖真实 LLM API key）
- 有简要注释说明关键 API 用法
- 控制台输出能看到效果（事件、日志、结果）

### 集成验证

确认所有 v0.7 issue 组合后无冲突：

| 场景 | 验证内容 |
|------|---------|
| Hook + Agent-as-Tool | 子 run 的 hook 独立于父 run |
| Hook + Handoff | on_handoff hook 在切换时触发，新 agent 的 hook 链生效 |
| Retry + Hook | 重试不跳过 hook（每次重试都经过 before_model/after_model） |
| Loop Detection + Handoff | Handoff 后 loop detection 窗口重置 |
| Agent-as-Tool + Handoff | 父 agent 既有 as_tool 子 agent 又有 handoff 目标 |

### Lint + CI

- `cargo test --workspace` 全绿
- `cargo clippy --workspace -- -D warnings` 全绿
- `bash scripts/lint-check.sh` 全 PASS
- examples 编译通过

## 需要修改的文件

| 文件 | 变更 |
|------|------|
| `examples/` 目录 | 7 个新示例 |
| 可能的集成测试 | 组合场景验证 |

## 不在范围内

- API 参考文档（v0.9）
- SDK 示例（v0.9）
- 性能 benchmark

## 依赖

- 002（Hook Framework）
- 003（Agent-as-Tool）
- 004（Handoff）
- 005（AgentRun Actor，如果执行）
- 006（LLM Retry）
- 007（Loop Detection）

本 issue 在所有其他 issue 完成后执行。

## 验收标准

- [ ] 8 个示例全部可 `cargo run --example <name>` 运行
- [ ] 示例包含至少 3 个 error path 场景（hook abort、retry exhausted、loop termination）
- [ ] 示例不依赖真实 LLM API key
- [ ] 5 个集成验证场景全部通过
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
- [ ] `bash scripts/lint-check.sh` 全 PASS
