# 003 · 实施计划

## 先读

- `docs/adr/0006-voice-session-model.md` D7.5、`docs/iteration/voice/plan.md` V4、`crates/orchest-protocol/src/{capability,stream}.rs`。
- `crates/orchest-provider-stream/src/tts/{aliyun,volcengine,minimax}.rs` 及仓库协议资料。

## 修改范围

- TTS 输入与事件契约、三个 adapter、registry 能力声明及共享 transport fixture。

## 步骤

1. 逐方言钉住 Flush、结束及取消语义，确认音频能在输入结束前到达。
2. 实现输入与输出并行和上下文归属，保留一次性、整段流式合成的行为。
3. 跑三方言同一套契约断言，有授权凭证时补真实服务 smoke，记录能力证据。
