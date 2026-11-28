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
