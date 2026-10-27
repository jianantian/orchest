# 002 · 方言补 Turn 与原生 finalize

GitHub issue: —

## Background

五个流式 ASR 方言的 Turn 与定稿行为不一致：

- ElevenLabs 只有 `committed_transcript`，从不发 `EndOfSpeech`（`crates/orchest-provider-stream/src/asr/elevenlabs.rs:62`）。
- Deepgram 只把 `speech_final` 映射为 `EndOfSpeech`，`UtteranceEnd` 被当作无内容消息丢弃（`deepgram.rs:33`、`:121`）；输入结束只发 `CloseStream`，没有发原生 `Finalize`。
- Soniox 支持 manual finalization，但没有接上。

## Goal

每个方言都按 ADR-0006 D6 产出 Draft → Stable → Turn，并对 `SessionInput::Finalize` 给出原生实现或明确的 `UnsupportedOption`。

## Acceptance Criteria

- [ ] ElevenLabs：每个 committed 之后合成一个 `EndOfSpeech`，携带该 committed 的 `segment_id`；`Finalize` 映射为 commit。
- [ ] Deepgram：`UtteranceEnd` 映射为 `EndOfSpeech`，与同段 `speech_final` 去重（一段只出一次 Turn）；`Finalize` 发原生 `Finalize` 控制帧，连接保持。
- [ ] Soniox：`Finalize` 发 manual finalization 请求；endpoint token 映射为 Turn。
- [ ] aliyun、volcengine：有原生手段的接上，没有的返回 `UnsupportedOption`，并在代码里写明依据（官方文档链接）。
- [ ] 每个方言的 `EndOfSpeech.segment` 都指向它结束的 Stable。
- [ ] 每条映射规则有单元测试（基于消息解析，不依赖网络）。

## Blocked by

001。

## Notes

- 是否支持 Finalize 先用方言内常量表达；能力目录字段 `client_finalize` 在语音 V2 补。
- Deepgram 的去重以 segment 为键，不按时间窗。
