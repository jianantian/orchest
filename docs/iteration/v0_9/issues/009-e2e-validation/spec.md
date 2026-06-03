# 009 · 端到端示例 + 集成测试

## 背景

v0.9 的所有组件就绪后，需要一个端到端示例和完整的集成测试套件验证 Supervised Delegation 全流程可用。

依赖所有前置 issue（001–008）。

## 契约

### 输入
- 全部 v0.9 API 就绪

### 输出
- `examples/rust/supervised_delegation.rs` — 端到端可运行的完整示例
- 集成测试覆盖 5 个核心场景

## 示例流程

`examples/rust/supervised_delegation.rs`：

1. 创建 `AgentConfig`，设置 `SupervisionStrategy::Restart { max_retries: 2 }`
2. 注册包含一个"模拟长任务"tool 和一个"模拟 panic"tool 的 registry
3. 创建 mock `ModelAdapter`（按剧本返回 tool call 序列）
4. `AgentRun::start()` → 得到 `RunHandle`
5. `handle.attach_watcher(LlmWatcher)` 或 mock watcher
6. worker 执行 tool → emit event → watcher 评估
7. 演示三种干预：
   - Inject：watcher 注入 "focus on error handling"
   - Steer：watcher 发出系统级重定向
   - Abort：watcher 判断 off-track → 终止
8. 演示 panic → restart → 从 snapshot 恢复

## 集成测试场景

| 场景 | 验证点 |
|------|--------|
| inject | `inject_message()` 后 messages 包含 user-role 消息，模型下一轮可见 |
| steer | `steer()` 后 messages 包含 system-role 指令，模型下一轮可见 |
| abort | watcher `Abort` → `RunAborted` 事件 → run 终止 |
| panic-restart | worker panic → `RunRestarted` → 从 snapshot 恢复 → 继续执行 |
| multi-watcher | 2+ watcher 协调：FIFO inject + abort 优先 + 跨 restart 存活 |

## 验收标准

- [ ] `examples/rust/supervised_delegation.rs` 可运行（`cargo run --example supervised_delegation`）
- [ ] 集成测试覆盖上述 5 个场景
- [ ] 现有 examples 未 broken（全部编译通过）
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
- [ ] `bash scripts/lint-check.sh` 全 PASS
