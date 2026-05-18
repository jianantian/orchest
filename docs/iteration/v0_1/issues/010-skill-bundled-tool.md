# 010 · Skill Bundled Script 执行与 Async Job 协议

## 背景

Skill 可以 bundle 可执行脚本作为 tool。Runtime 通过 spawn 子进程执行这些脚本，解析 stdout 作为 tool result。脚本也可以通过约定的 JSON 协议声明自己是异步 job，触发 runtime 轮询模式。

## 目标

实现 `SkillBundledTool`：spawn 子进程执行脚本，解析输出为 `ToolOutput::Immediate` 或 `ToolOutput::AsyncJob`。

## 验收标准

**同步执行：**
- [ ] `execute()` spawn `{executable} {script}` 子进程
- [ ] 通过 stdin 传入 JSON 序列化的 tool input
- [ ] 读取 stdout，解析为 `serde_json::Value`，返回 `ToolOutput::Immediate(value)`
- [ ] stderr 内容记录为 warning 日志（不影响结果）
- [ ] 子进程非 0 退出码视为 `ToolError`
- [ ] 使用 `ToolMetadata.timeout`（如有）限制子进程执行时间

**Async Job 协议：**
- [ ] 若 stdout JSON 包含 `"__async_job": true`，切换为异步模式
- [ ] 从 stdout 中读取 `job_id: String` 和 `poll_interval: u64`（秒）
- [ ] 构造 `JobHandle`：poll 闭包通过重新 spawn 子进程并传入 `--poll <job_id>` 参数实现
- [ ] poll 子进程的 stdout 格式：`{"status": "pending", "progress": 0.5}` 或 `{"status": "completed", "result": ...}` 或 `{"status": "failed", "error": "..."}`
- [ ] 返回 `ToolOutput::AsyncJob(handle)`

**安全：**
- [ ] `executable` 只允许白名单（`python`、`python3`、`node`、`bash`、`sh`）中的值，其他值返回 `ToolError`
- [ ] `script` 路径必须在 skill 目录内（防止路径穿越），否则返回 `ToolError`

## 说明

v0.1 不做进程沙箱，SKILL.md 中记录"用户须审核 skill 来源"警告。
