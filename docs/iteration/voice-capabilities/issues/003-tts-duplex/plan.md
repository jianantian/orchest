# 003 · 实施计划

## 先读

- `docs/adr/0006-voice-session-model.md` D7.5、`docs/iteration/voice/plan.md` V4、`crates/orchest-protocol/src/{capability,stream}.rs`。
- `crates/orchest-provider-stream/src/tts/{aliyun,volcengine,minimax}.rs` 及仓库协议资料。

## 修改范围

- TTS 输入与事件契约、三个 adapter、registry 能力声明及共享 transport fixture。

## 步骤

1. 建立逐方言能力矩阵，先证明输入开放时可收到音频、继续追加和正常关闭，再分别验证 Flush 与取消。
2. 实现输入与输出并行和上下文归属，保留一次性、整段流式合成的行为。
3. 对各方言执行共同契约和各自声明的操作测试，覆盖不支持的操作。有凭证时补真实服务验证，将覆盖和证据写入能力矩阵。

## 方言验收矩阵

| 方言 | 追加 | Flush | 取消 | 关闭 | 词级对齐 | 实际服务覆盖 |
| --- | --- | --- | --- | --- | --- | --- |
| Aliyun CosyVoice | 支持，等待 `task-started` 后发送 `continue-task` | 支持，`task-finished` 确认当前任务结束，下一段使用同连接新任务 | 支持，`finish-task` 的 `directive: cancel`，同连接继续 | 支持，等待任务完成后释放 WebSocket | 待验证 | 未运行，当前环境无 DashScope TTS 凭证 |
| Volcengine | 支持，`SessionStarted` 后发送 `TaskRequest` | 支持，`SessionFinished` 确认边界，下一段使用同连接新会话 | 支持，等待 `SessionCanceled`，按上游会话 ID 丢弃旧音频 | 支持，结束会话后等待 `ConnectionFinished` | 待验证 | 未运行，当前环境无已确认可用于语音的凭证 |
| MiniMax | 支持，`task_started` 后发送 `task_continue` | 支持，`task_flushed` 位于当前 Flush 的音频之后 | 支持，等待 `task_canceled`，下一句开始前过滤迟到音频 | 支持，`task_finished` 后释放 WebSocket | 待验证 | 中国端点完成连接与任务握手，首段合成返回余额不足（1008）。国际端点拒绝凭证，未拿到音频 |

整段流式保持原有端点。双工使用对应的供应商入口。Volcengine 的官方双工入口在未配置具体资源时使用 `seed-tts-2.0`，显式资源配置优先。三家双工目录均不声明词级对齐能力，要求该能力会在建连前返回 `UnsupportedOption`。CosyVoice 双工不支持 SSML，因为厂商只允许为 SSML 发送一次文本。

共同 transport fixture 覆盖首段音频先于关闭输入、Flush 发出与确认的顺序、同连接续接、取消后过滤旧音频、重复关闭、握手失败和意外断连。

[Aliyun 客户端事件](https://www.alibabacloud.com/help/en/model-studio/cosyvoice-client-events) · [Aliyun 服务端事件](https://www.alibabacloud.com/help/en/model-studio/cosyvoice-server-events) · [Volcengine 双向流式协议](../../../../external/volceengine/tts_bidirection.md) · [MiniMax 双工协议](https://platform.minimax.io/docs/api-reference/speech-t2a-websocket-bidi)
