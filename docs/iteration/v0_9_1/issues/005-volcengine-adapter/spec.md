# 005 · Volcengine adapter

GitHub: [#113](https://github.com/jianantian/orchest/issues/113)

## 背景

实现火山引擎 ASR provider adapter，feature-gated `volcengine`。首期目标 `volcengine/bigmodel_async` 优化双工流式模式。

## 参考资料

- Vendor docs: `docs/external/volceengine/asr.md`
- 既有实现（参考，非权威）: `study_buddy/server/src/speech/asr.rs`, `study_buddy/server/src/speech/protocol.rs`
- PRD provider-specific notes: `docs/iteration/v0_9_1/prd.md` → "Volcengine binary protocol" 等段落

## 影响范围

### 修改文件

- `crates/agent-runtime-asr-providers/Cargo.toml` — `volcengine` feature, WebSocket/gzip dependencies
- `crates/agent-runtime-asr-providers/src/providers/volcengine.rs` — adapter 实现

### 新增文件

- `crates/agent-runtime-asr-providers/tests/volcengine_offline.rs`
- `crates/agent-runtime-asr-providers/tests/volcengine_live.rs` (`#[ignore]`)

## 实现要点

### 二进制帧协议

火山引擎不是 JSON-over-WebSocket，需实现自定义二进制帧协议：

- **Header** (4 bytes): message type, flags (last-frame / no-sequence), serialization type, compression type
- **Full client request**: JSON payload → gzip 压缩 → binary frame (`MSG_FULL_CLIENT_REQUEST`)
- **Audio frames**: raw audio data → binary frame (`MSG_AUDIO_ONLY_REQUEST`, `FLAG_NO_SEQUENCE` / `FLAG_LAST_NO_SEQUENCE`)
- **Response parsing**: 4-byte header → 4-byte sequence/error code → 4-byte payload size → gzip decompression → JSON parse
- **Error response**: `MSG_ERROR_RESPONSE` type → error code + message extraction

### 流式行为

- `AudioChunkBoundary::Flush` / `End` → `FLAG_LAST_NO_SEQUENCE` (空 data last-frame) 触发 provider final
- Provider 响应中 `definite: true` 的 utterances → `TranscriptUpdate { stability: Committed }`
- Provider 响应中 `definite: false` 的 utterances → `TranscriptUpdate { stability: Provisional }`
- 去重：`show_utterances=true` 时 provider 会 replay 之前的 definite utterances，用 `(start_time, end_time, text_hash)` 去重
- Provider endpointing / stream completion → `AsrFinal { reason: ProviderEndpoint }`

### Capabilities

```rust
AsrModelCapabilities {
    model: "volcengine/bigmodel_async",
    streaming: true,
    batch: false,
    streaming_inputs: vec![AudioInputCapability {
        format: AudioFormat::Pcm,
        sample_rates_hz: SampleRateSupport::Exact(vec![16000]),
        channels: ChannelSupport::Exact(vec![1]),
        ..
    }],
    audio_timeline_modes: vec![AudioTimelineMode::ContinuousRealtime],
    endpointing_modes: vec![/* provider-specific modes */],
    segment_flush: true,
    multi_segment_streaming: false, // single session per connection
    connection_reuse: ConnectionReuse::NotReusable,
    word_timestamps: false, // utterance-level only
    speaker_diarization: /* mode-dependent */,
    hot_words: true, // via corpus/context provider_options
    ..
}
```

### Config

- WebSocket auth headers: `X-Api-App-Key`, `X-Api-Access-Key`, `X-Api-Resource-Id`, `X-Api-Connect-Id`
- API key env default: `VOLCENGINE_ACCESS_KEY`
- Provider-specific `provider_options` keys: `resource_id`, `enable_nonstream`, `enable_itn`, `enable_punc`, `enable_speaker_info`, `show_utterances`, `end_window_size`, corpus/context

### 测试

- **Offline**: binary frame building/parsing roundtrip, definite utterance dedup, error response parsing, last-frame flag mapping
- **Live** (`#[ignore]`): stream synthetic PCM → assert non-empty text + telemetry fields

## 验收标准

- [ ] Volcengine adapter implements `AsrProvider` trait behind `volcengine` feature flag
- [ ] Binary frame protocol: header building, audio frames, last-frame flags, gzip compression/decompression, response parsing
- [ ] `definite` utterance deduplication before emitting committed `TranscriptUpdate`
- [ ] `AudioChunkBoundary::Flush`/`End` maps to provider last-frame (`FLAG_LAST_NO_SEQUENCE`)
- [ ] `AsrModelCapabilities` accurately reflects Volcengine streaming capabilities
- [ ] Offline tests cover binary frame protocol parsing/building (roundtrip)
- [ ] Offline tests cover `definite` utterance normalization and deduplication
- [ ] Offline tests cover error response (`MSG_ERROR_RESPONSE`) parsing
- [ ] Live tests are `#[ignore]` and gated by Volcengine-specific env vars
- [ ] Live tests stream generated PCM chunks and assert non-empty text plus telemetry
- [ ] `cargo test -p agent-runtime-asr-providers --features volcengine` passes without live credentials
- [ ] `cargo test -p agent-runtime-asr-providers --no-default-features` passes

## 依赖

- 001 Crate scaffold + public types (#109)
- 003 Duplex streaming contract (#111)
- 004 Observability + trace (#112)

与 006 Aliyun adapter 并行，互不阻塞。
