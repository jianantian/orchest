# 006 · Aliyun adapter + examples

GitHub: [#114](https://github.com/jianantian/orchest/issues/114)

## 背景

实现阿里云 ASR provider adapter，feature-gated `aliyun`。首期目标 `aliyun/fun-asr-realtime`，adapter 设计预留 `aliyun/qwen3-asr-flash-realtime`。

## 参考资料

- Vendor docs: `docs/external/aliyun/asr-api-doc.md`, `docs/external/aliyun/asr-guideline.md`
- PRD provider-specific notes: `docs/iteration/v0_9_1/prd.md` → Aliyun 相关段落

## 影响范围

### 修改文件

- `crates/agent-runtime-asr-providers/Cargo.toml` — `aliyun` feature, dependencies
- `crates/agent-runtime-asr-providers/src/providers/aliyun.rs` — adapter 实现

### 新增文件

- `crates/agent-runtime-asr-providers/tests/aliyun_offline.rs`
- `crates/agent-runtime-asr-providers/tests/aliyun_live.rs` (`#[ignore]`)
- `crates/agent-runtime-asr-providers/examples/` — application-driven segmentation + full-duplex 示例

## 实现要点

### Fun-ASR / Paraformer Realtime

WebSocket duplex task lifecycle:

```
run-task → task-started → stream binary audio → result-generated* → finish-task → task-finished
```

- `task-failed` 是 fatal，当前 stream 终止
- `result-generated` → `TranscriptUpdate`
- `finish-task` → 触发 provider finalization
- `task-finished` / finish synthesis → `AsrFinal`

### Connection Reuse

- `ConnectionReuse::ReusableAfterProviderTaskFinished`
- 成功的 task finished 后可复用连接，但需要新 provider task ID
- 失败的 task 必须 discard connection

### Endpointing

- Fun-ASR: acoustic silence based (`max_sentence_silence`)
- Maps to `EndpointingMode::AcousticSilence` + `EndpointingOptions.silence_timeout`

### Flush / End Mapping

- `AudioChunkBoundary::Flush` → `finish-task` 触发 finalization，stream 保持
- `AudioChunkBoundary::End` → `finish-task` 触发 finalization + stream teardown

### Capabilities (Fun-ASR)

```rust
AsrModelCapabilities {
    model: "aliyun/fun-asr-realtime",
    streaming: true,
    batch: false,
    streaming_inputs: vec![/* PCM, WAV etc per vendor docs */],
    audio_timeline_modes: vec![AudioTimelineMode::ContinuousRealtime],
    endpointing_modes: vec![EndpointingMode::AcousticSilence],
    segment_flush: true,
    multi_segment_streaming: true, // via connection reuse after task-finished
    connection_reuse: ConnectionReuse::ReusableAfterProviderTaskFinished,
    word_timestamps: true, // Fun-ASR supports timestamps
    speaker_diarization: false, // Fun-ASR realtime 不支持
    hot_words: true,
    ..
}
```

### Qwen-ASR 设计预留（v0.9.1 不实现）

Adapter 内部结构需容纳以下差异，但不实现：

| 维度 | Fun-ASR | Qwen-ASR |
|------|---------|----------|
| Session 协议 | `run-task` / `finish-task` | `session.update` / `input_audio_buffer.commit` / `session.finish` |
| VAD | acoustic (`max_sentence_silence`) | server_vad (default) / manual (`turn_detection: null`) |
| Manual flush | `finish-task` | `input_audio_buffer.commit` |
| Connection reuse | ✅ after task-finished | ❌ |
| Word timestamps | ✅ | ❌ |
| Max audio (manual) | — | ≤60s |
| Transcript events | `result-generated` | `text` + `stash` (需合并) |
| Emotion | — | provider metadata，不提升 |

### 测试

- **Offline**: task lifecycle event parsing, connection reuse / discard, endpointing mapping, capability validation (reject `word_timestamps` for Qwen, reject `speaker_diarization` for Fun-ASR realtime)
- **Live** (`#[ignore]`): stream synthetic PCM → assert non-empty text + telemetry

### Examples

- Application-driven segmentation: send audio → `flush_and_wait_final()` → next segment
- Full-duplex: `split()` → capture task sends audio, consumer task reads events

## 验收标准

- [ ] Aliyun adapter implements `AsrProvider` trait behind `aliyun` feature flag
- [ ] Fun-ASR task lifecycle: `run-task` → `task-started` → audio → `result-generated` → `finish-task` → `task-finished`
- [ ] Failed tasks discard the connection; successful tasks allow reuse with new task IDs
- [ ] `max_sentence_silence` maps to `EndpointingOptions.silence_timeout` for acoustic silence mode
- [ ] `AudioChunkBoundary::Flush`/`End` maps to `finish-task`
- [ ] `AsrModelCapabilities` accurately reflects Fun-ASR capabilities (including no speaker_diarization)
- [ ] Adapter design leaves room for `aliyun/qwen3-asr-flash-realtime` without breaking changes
- [ ] Offline tests cover Aliyun task lifecycle events
- [ ] Offline tests cover connection reuse after `task-finished` and failed-task discard
- [ ] Capability tests reject `speaker_diarization=true` for Fun-ASR realtime
- [ ] (Deferred to Qwen issue) Capability tests reject `word_timestamps=true` for Qwen-ASR realtime
- [ ] (Deferred to Qwen issue) Offline tests cover Qwen-ASR server VAD vs Manual mode mapping
- [ ] (Deferred to Qwen issue) Offline tests cover Qwen-ASR `text` + `stash` transcript normalization
- [ ] Live tests are `#[ignore]` and gated by Aliyun-specific env vars
- [ ] Live tests stream generated PCM chunks and assert non-empty text plus telemetry
- [ ] README/example snippets for application-driven segmentation and full-duplex
- [ ] `cargo test -p agent-runtime-asr-providers --features aliyun` passes without live credentials
- [ ] `cargo test -p agent-runtime-asr-providers --no-default-features` passes

## 依赖

- 001 Crate scaffold + public types (#109)
- 003 Duplex streaming contract (#111)
- 004 Observability + trace (#112)

与 005 Volcengine adapter 并行，互不阻塞。
