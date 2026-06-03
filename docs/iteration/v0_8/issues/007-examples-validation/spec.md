# 007 · Examples + Final Validation

## 背景

001-006 各自实现核心功能，但缺少：
1. 端到端 API 使用示例（用户首次接触 v0.8 功能的入门材料）
2. 跨 issue 的集成验证测试（确保各功能在同一 run 中协同正确）
3. 最终 lint/CI 全通过的确认

本 issue 是 v0.8 的收尾，依赖 001-006 全部合入 main。

## 目标

补充 v0.8 功能的使用示例，编写集成验证测试，确保整个 workspace CI 全绿。

## 范围

### Rust Examples

在 `examples/rust/` 中新增以下示例（`cargo run --example <name>`，文件内联 mock model，不依赖真实 API key）：

| 文件 | 演示内容 |
|------|---------|
| `guardrail_keyword_filter.rs` | ToolInputGuardrail 拦截含禁用词的工具调用；含 Reject 路径 |
| `guardrail_output_sanitize.rs` | OutputGuardrail 替换模型输出中的敏感字段 |
| `approval_mode_side_effect.rs` | ApprovalMode::SideEffectOnly；side_effect tool 触发审批，其他 tool 不触发 |
| `session_persist_resume.rs` | InMemorySessionStore 保存 snapshot，AgentRun::resume 恢复并继续（消息历史延续） |
| `watcher_inject_message.rs` | attach_watcher 监听 ToolCallCompleted 事件，在特定条件下注入 steering 消息 |
| `watcher_abort_on_pattern.rs` | Watcher 检测到 ToolCallFailed 超过阈值时调用 Abort |

每个示例：
- 顶部注释说明演示内容和运行方式
- 不调用真实模型——examples 不能用 `#[cfg(test)]` 下的 `FakeModelAdapter`（examples 是独立编译单元，看不到测试模块）。沿用 v0.7 examples 的既有模式：每个 example 在文件内**内联定义**一个最小 mock `ModelAdapter`（按预设脚本返回 tool_call / text），参考 `examples/rust/hook_logging.rs` 的 `// ── Mock model ──` 段
- 能 `cargo run --example <name>` 成功执行并输出预期结果

### 集成验证场景（v08_integration.rs）

新建 `crates/agent-runtime-core/tests/v08_integration.rs`，包含跨功能验证：

| 测试 | 验证内容 |
|------|---------|
| `guardrail_and_approval_coexist` | 同一 run 中 ToolInputGuardrail + ApprovalMode::All，两个机制都正确工作（guardrail Reject 不触发审批；guardrail Allow 后正常审批） |
| `session_resume_with_hooks` | resume 后 hooks 重新注册，on_run_end 仍触发持久化 |
| `session_resume_after_handoff` | Agent A → Handoff → Agent B → resume from snapshot，active_config 为 Agent B |
| `watcher_inject_during_tool_loop` | watcher 在工具调用循环中注入消息，run 正确响应（不循环、不重复） |
| `all_v08_features_combined` | session_store + input_guardrail + approval_mode + watcher 同时注册，一次完整 run，各功能各司其职，无互相干扰 |

### SQLite 集成测试（sqlite_integration.rs）

在 `#[cfg(feature = "sqlite-session")]` 下：

| 测试 | 验证内容 |
|------|---------|
| `sqlite_persist_and_resume` | SqliteSessionStore + resume；open_in_memory() |
| `sqlite_crash_simulation` | 模拟 on_run_error 路径，snapshot 已保存 |

### 最终 CI 检查

以下全部通过：

```bash
# 默认 feature
cargo test --workspace
cargo clippy --workspace -- -D warnings
bash scripts/lint-check.sh

# sqlite-session feature
cargo test --workspace --features agent-runtime-core/sqlite-session
cargo clippy --workspace --features agent-runtime-core/sqlite-session -- -D warnings
```

## 验收标准

- [ ] 6 个 Rust example 存在，`cargo run --example <name>` 全部成功执行
- [ ] `examples/rust/guardrail_keyword_filter.rs` 展示 ToolInputGuardrail::Reject 路径
- [ ] `examples/rust/session_persist_resume.rs` 展示 resume 后消息历史延续
- [ ] `examples/rust/watcher_inject_message.rs` 展示 WatcherAction::Inject 生效
- [ ] `v08_integration.rs` 存在，含 5 个集成测试
- [ ] `all_v08_features_combined` 测试通过（最关键的集成验证）
- [ ] `sqlite-session` feature 下集成测试通过
- [ ] `cargo test --workspace` 全绿（默认 feature）
- [ ] `cargo test --workspace --features agent-runtime-core/sqlite-session` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
- [ ] `bash scripts/lint-check.sh` 全 PASS
