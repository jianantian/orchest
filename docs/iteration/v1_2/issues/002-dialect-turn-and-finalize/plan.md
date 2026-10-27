# 002 implementation plan

## Files to read

- `crates/orchest-provider-stream/src/asr/{deepgram,elevenlabs,soniox,aliyun,volcengine}.rs`
- `crates/orchest-provider-stream/src/transport.rs`（控制帧发送）
- `docs/external/` 下对应厂商文档（若有），以及 ADR-0006 参考链接

## Files to change

- `crates/orchest-provider-stream/src/asr/elevenlabs.rs`：committed 后合成 Turn；commit 映射 Finalize
- `crates/orchest-provider-stream/src/asr/deepgram.rs`：`UtteranceEnd` → Turn、去重；`Finalize` 控制帧
- `crates/orchest-provider-stream/src/asr/soniox.rs`：manual finalization；endpoint → Turn
- `crates/orchest-provider-stream/src/asr/{aliyun,volcengine}.rs`：原生 finalize 或 `UnsupportedOption`

## Steps

1. ElevenLabs：在 committed 分支后追加 `EndOfSpeech { segment }`；`Finalize` 发 commit 消息。
2. Deepgram：解析 `UtteranceEnd`；按 segment 记录已发 Turn，`speech_final` 与 `UtteranceEnd` 只发一次；`Finalize` 发 `{"type":"Finalize"}`。
3. Soniox：`Finalize` 发 manual finalization；endpoint token 映射 Turn。
4. aliyun、volcengine：查官方协议，有就接，没有就返回 `UnsupportedOption` 并注释依据。
5. 每个方言补解析级单元测试；跑 workspace 检查。
