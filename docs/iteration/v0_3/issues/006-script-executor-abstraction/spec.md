# 006 · ScriptExecutor 抽象层与 ExecutionContext

## 背景

v0.1/v0.2 中，skill bundled script 的子进程执行逻辑直接内嵌在 `skill_bundled.rs` 里，子进程继承父进程的完整环境变量和工作目录。这带来两个问题：

1. 未来加沙箱需要改动 run loop 内部逻辑，不是纯替换
2. 子进程能访问父进程的所有环境变量（含密钥），暴露面过大

本 issue 抽取 `ScriptExecutor` trait 和 `ExecutionContext`，让执行后端可插拔，同时收紧执行上下文。

## 目标

实现 `ScriptExecutor` trait 和 `BareSubprocessExecutor`，重构 skill bundled script 执行路径，使 run loop 不再直接 spawn 子进程。

## 验收标准

**ScriptExecutor trait：**
- [ ] 定义 `ScriptExecutor` trait，签名见 spec.md "Script 执行层"
- [ ] `BareSubprocessExecutor` 实现 trait，行为与当前 `skill_bundled.rs` 一致
- [ ] `AgentRuntime` 通过 `Arc<dyn ScriptExecutor>` 持有执行器，支持测试时注入 mock

**ExecutionContext：**
- [ ] 每次执行创建独立的临时工作目录（`tempdir()`），执行结束后清理
- [ ] `env` 字段只包含 `SkillCapabilities.env` 声明的变量；skill 未声明 `capabilities` 时，默认传入空 env（不继承父进程）
- [ ] 现有行为保持兼容：`on_update` 推送逻辑不变，`timeout` 来自 `ToolMetadata.timeout`

**回归：**
- [ ] 现有 skill bundled tool 的 e2e 测试全部通过
- [ ] async job 协议（stdout JSON `{"__async_job": true, ...}`）行为不变

## 说明

`BareSubprocessExecutor` 是 v0.3 的唯一实现，行为与重构前等价。沙箱实现（`FirejailExecutor` 等）不在本 issue 范围内。
