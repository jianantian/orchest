# 006 · Webhook 模式异步 Tool

## 背景

Polling 模式下，runtime 需要反复调用 `poll()` 来获取 job 状态，对于有 push 能力的服务（如支持 webhook 的视频生成 API）来说是不必要的轮询开销。Webhook 模式让外部服务主动推送结果，runtime 只需等待。

## 目标

扩展 `JobHandle` 支持 webhook 模式：runtime 暴露本地 HTTP endpoint，tool 提交任务时把 callback URL 传给外部服务，外部服务完成后 POST 到该 URL 唤醒等待中的 job。

## 验收标准

**JobHandle 扩展：**
- [ ] `JobHandle` 新增 `webhook: Option<WebhookConfig>` 字段
- [ ] `WebhookConfig { expected_job_id: String }` — runtime 用此 ID 匹配入站 webhook；**不**包含 callback URL（URL 由 runtime 通过 `ctx.webhook_base_url` 注入，tool 自行拼接传给外部服务）
- [ ] `JobHandle.poll` 在 webhook 模式下可以为 `None`（纯 webhook，无 polling fallback）

**本地 HTTP Server：**
- [ ] Runtime 初始化时在随机可用端口启动本地 HTTP server（`0.0.0.0:0`）
- [ ] 监听 `POST /webhooks/async-job/{job_id}`
- [ ] request body 格式：`{"status": "completed", "result": ...}` 或 `{"status": "failed", "error": "..."}`
- [ ] 收到 webhook 后，通过 channel 唤醒对应 job 的等待 task
- [ ] HTTP server 在所有 run 完成后关闭

**Callback URL 注入：**
- [ ] `ToolContext` 新增 `webhook_base_url: Option<String>` 字段（这是 callback URL 的唯一来源）
- [ ] Tool 通过 `format!("{}/webhooks/async-job/{}", ctx.webhook_base_url.as_ref().unwrap(), job_id)` 构造完整 callback URL，传给外部服务
- [ ] 未启用 webhook server 时该字段为 `None`；tool 在构造 `JobHandle` 时应检查此字段是否存在

**Fallback：**
- [ ] 若 `JobHandle` 同时有 `poll` 和 `webhook`，优先等待 webhook；超过 `poll_interval * 3` 无 webhook 收到时，fallback 到 polling

**安全：**
- [ ] webhook endpoint 只监听 localhost（`127.0.0.1`），不对外暴露
- [ ] job_id 使用 UUID v4，防止猜测

## 说明

webhook server 只在 `AgentConfig.webhook_enabled: bool`（默认 false）时启动，不影响不使用 webhook 的用户。
