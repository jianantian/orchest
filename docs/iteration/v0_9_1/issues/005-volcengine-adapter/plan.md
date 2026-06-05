# 005 实现路线

## 要读的现有代码

- `docs/external/volceengine/asr.md` — vendor API 文档：鉴权 headers、二进制协议字段、full client request JSON payload、server response 结构、error 码
- `study_buddy/server/src/speech/asr.rs` — 完整的火山 ASR WebSocket handler：连接建立、full client request 构建、audio frame 发送、response 解析、definite/non-definite utterance 分离 + 去重、flush 状态机
- `study_buddy/server/src/speech/protocol.rs` — 二进制帧协议：legacy ASR 协议常量 + helper（`build_header`, `compress_gzip`, `decompress_gzip`），以及双向流式协议的 `Message` struct（本次不用）
- `crates/agent-runtime-asr-providers/src/streaming.rs` — 003 的 streaming contract

## 协议理解

### 关于两套协议

study_buddy `protocol.rs` 包含两套协议：
1. **Legacy ASR 协议**（`build_header` + flag 常量）— 用于 `bigmodel` / `bigmodel_async` / `bigmodel_nostream` 单向/双向流式 ASR
2. **双向流式事件协议**（`Message` struct + `EventType`）— 用于 TTS 等更复杂的双向流式场景

`asr.rs` 用的是 **legacy ASR 协议**，`bigmodel_async` 也用这套协议。区别只在服务端行为：`bigmodel_async` 只返回有变化的结果（性能优化），协议帧格式不变。

### 鉴权

鉴权信息全部通过 WebSocket 握手 HTTP headers 传递。这些都是**调用方配置**，SDK 不硬编码：

| Header | 含义 | 来源 |
|--------|------|------|
| `X-Api-App-Key` | 火山控制台 APP ID（旧版控制台） | 调用方 config |
| `X-Api-Access-Key` | 火山控制台 Access Token（旧版控制台） | 调用方 config |
| `X-Api-Key` | 火山控制台 APP Key（新版控制台） | 调用方 config |
| `X-Api-Resource-Id` | 资源 ID，标识计费方式和模型版本 | 调用方 config |
| `X-Api-Connect-Id` | 连接 ID，用于追踪 | SDK 生成 UUID |
| `X-Api-Request-Id` | 任务 ID | SDK 生成 UUID |
| `X-Api-Sequence` | 固定 `-1` | SDK 固定值 |

`app_id`、`access_key`/`api_key`、`resource_id` 和 LLM provider 的 API key 性质一样，是调用方的凭证配置。SDK 通过 `AsrProviderRuntimeConfig` 接收，不在 request-level `provider_options` 中。

### 音频帧

Study_buddy `asr.rs` 的 `build_audio_frame` 用 `COMP_NONE`（不压缩音频），vendor docs 示例用 `COMP_GZIP`（压缩音频）。两者都有效——压缩降带宽但增 CPU。首期跟 study_buddy 一致用 `COMP_NONE`，音频帧不压缩。Full client request 的 JSON payload 仍用 gzip 压缩。

### Endpointing

Vendor docs 的 `end_window_size` 参数（默认 800ms，最小 200ms）= 强制判停时间：静音超过此值输出 `definite`。这直接映射到 `EndpointingOptions.silence_timeout`。`vad_segment_duration` 是语义切句阈值，不决定判停，是不同概念。

### Response 结构

Vendor docs 确认 response 中 utterances 有 word-level timestamps（`words` 数组，每个 word 有 `start_time`/`end_time`/`text`）——只要开启 `show_utterances`。之前 plan 写 `word_timestamps: false` **是错的**。

### Speaker diarization

Vendor docs：`enable_speaker_info` 需要同时开启 `enable_nonstream=true`（二遍识别）+ `ssd_version="200"`，建议 ASR 2.0。首期不做为默认 capability，通过 `provider_options` 暴露。

## 步骤

### 1. 添加依赖 (`Cargo.toml`)

`tokio-tungstenite` 作为 non-optional dependency（005 和 006 都用 WebSocket）：

```toml
[dependencies]
tokio-tungstenite = { version = "0.24", features = ["native-tls"] }
flate2 = "1"  # gzip for full client request payload
byteorder = "1"  # binary frame parsing, 同 study_buddy
```

不需要 feature gate——两个首期 provider 都用 WebSocket 和 gzip。

### 2. 实现二进制帧协议 (`providers/volcengine/protocol.rs`)

从 study_buddy `protocol.rs` 的 **legacy ASR 部分**移植（不需要双向流式事件协议）：

**常量**:
- `MSG_FULL_CLIENT_REQUEST = 0b0001`, `MSG_AUDIO_ONLY_REQUEST = 0b0010`, `MSG_FULL_SERVER_RESPONSE = 0b1001`, `MSG_ERROR_RESPONSE = 0b1111`
- `FLAG_NO_SEQUENCE = 0b0000`, `FLAG_LAST_NO_SEQUENCE = 0b0010`
- `SER_JSON = 0b0001`, `SER_NONE = 0b0000`, `COMP_GZIP = 0b0001`, `COMP_NONE = 0b0000`
- `PROTOCOL_VERSION = 1`, `HEADER_SIZE_4B = 1`

**Functions**:
- `build_header(msg_type, flags, serialization, compression) -> [u8; 4]` — 同 study_buddy
- `build_full_client_request(config, options) -> Result<Vec<u8>, AsrError>`:
  - 构建 JSON payload（`user`, `audio`, `request` 三层）
  - `request.model_name` = "bigmodel"（对 bigmodel_async 也是 "bigmodel"，区别在 endpoint URL）
  - `request.show_utterances` = true（必须，否则没有 definite/word timestamps）
  - `request.result_type` = "single"（增量返回，减少重复传输）
  - 映射 `TranscribeOptions`：`enable_itn` ← `punctuate`（近似），`enable_punc` ← `punctuate`，`end_window_size` ← `endpointing.silence_timeout`
  - 映射 `provider_options`：`enable_nonstream`, `enable_speaker_info`, `ssd_version`, `vad_segment_duration`, etc.
  - 映射 `hot_words` → `corpus.context` hotwords JSON
  - Serialize → gzip → binary frame (`MSG_FULL_CLIENT_REQUEST`, `SER_JSON`, `COMP_GZIP`)
- `build_audio_frame(data, is_last) -> Vec<u8>`:
  - `MSG_AUDIO_ONLY_REQUEST`, `SER_NONE`, `COMP_NONE`（音频不压缩）
  - flag = `FLAG_LAST_NO_SEQUENCE` if is_last else `FLAG_NO_SEQUENCE`
  - 4-byte payload size (big-endian) + raw audio data
- `parse_response(data) -> Result<VolcengineFrame, AsrError>`:
  - 4-byte header → extract msg_type, flags, serialization, compression
  - `MSG_ERROR_RESPONSE` → 4-byte error code + 4-byte msg size + error message
  - `MSG_FULL_SERVER_RESPONSE` → 4-byte sequence + 4-byte payload size → decompress if gzip → JSON parse
  - 返回 structured enum（ServerResponse / ErrorResponse）

### 3. 定义 Volcengine 内部类型

```rust
/// Parsed server response payload
#[derive(Deserialize)]
struct VolcenginePayload {
    result: Option<VolcengineResult>,
    audio_info: Option<AudioInfo>,
}
#[derive(Deserialize)]
struct VolcengineResult {
    text: String,
    utterances: Option<Vec<VolcengineUtterance>>,
}
#[derive(Deserialize)]
struct VolcengineUtterance {
    text: String,
    definite: bool,
    start_time: i32,
    end_time: i32,
    words: Option<Vec<VolcengineWord>>,
    additions: Option<Value>, // speaker_info, emotion, etc.
}
#[derive(Deserialize)]
struct VolcengineWord {
    text: String,
    start_time: i32,
    end_time: i32,
    blank_duration: Option<i32>,
}
#[derive(Deserialize)]
struct AudioInfo {
    duration: Option<u64>,
}
```

### 4. 实现 definite utterance 去重

同 study_buddy `process_asr_response()` 的逻辑：

```rust
struct UtteranceDeduplicator {
    seen: HashSet<(i32, i32, u64)>,
}
impl UtteranceDeduplicator {
    fn is_new(&mut self, u: &VolcengineUtterance) -> bool {
        let mut hasher = DefaultHasher::new();
        u.text.hash(&mut hasher);
        let key = (u.start_time, u.end_time, hasher.finish());
        self.seen.insert(key)
    }
}
```

- definite + `is_new()` → emit `TranscriptUpdate { stability: Committed }`，附 word timestamps
- definite + !is_new → skip
- non-definite → emit `TranscriptUpdate { stability: Provisional }`

### 5. 实现 `VolcengineAsrAdapter`

```rust
pub struct VolcengineAsrAdapter {
    config: VolcengineAsrConfig,
}

pub struct VolcengineAsrConfig {
    /// Provider-native model identifier, e.g. "bigmodel"
    pub model: String,
    /// WebSocket endpoint URL
    pub ws_url: String,
    /// Auth: APP ID or API Key (new console)
    pub app_key: String,
    /// Auth: Access Token (old console only, empty for new console)
    pub access_key: String,
    /// Billing resource ID, e.g. "volc.bigasr.sauc.duration"
    pub resource_id: String,
}
```

`AsrProviderRuntimeConfig` 映射:
- `config.model` → `normalize_asr_provider_model()` 拿到 `model` 部分（如 "bigmodel_async"）
- `config.api_key` / `config.api_key_env` → API key resolution → `app_key`（新版控制台）或 `access_key`（旧版控制台）
- `config.api_url` → `ws_url`，默认 `wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async`
- `config.provider_options` → extract `resource_id`（必填）、`access_key`（旧版控制台可选）

默认 API key env var: `VOLCENGINE_API_KEY`

### 6. 实现 streaming adapter task

`start_stream()`:

1. Build WebSocket request:
   - URL = `self.config.ws_url`
   - Headers: `X-Api-App-Key` / `X-Api-Key`, `X-Api-Access-Key`, `X-Api-Resource-Id`, `X-Api-Connect-Id` (UUID), `X-Api-Request-Id` (UUID), `X-Api-Sequence` (-1)
2. `tokio_tungstenite::connect_async(request)`
3. Send `build_full_client_request(config, options)` as binary frame
4. Create audio/event channels, return `AsrStream`
5. Spawn adapter task:

```rust
tokio::spawn(async move {
    let (mut ws_write, mut ws_read) = ws_stream.split();
    let mut dedup = UtteranceDeduplicator::new();
    let mut flush_pending = false;
    let mut flush_started: Option<Instant> = None;

    loop {
        tokio::select! {
            chunk = audio_rx.recv() => {
                match chunk {
                    Some(chunk) => {
                        let is_last = matches!(chunk.boundary, Flush | End);
                        if is_last { flush_pending = true; flush_started = Some(Instant::now()); }
                        let frame = build_audio_frame(&chunk.data, is_last);
                        ws_write.send(Binary(frame)).await?;
                        if matches!(chunk.boundary, End) { /* mark for teardown */ }
                    }
                    None => { /* sender dropped → cancellation */ }
                }
            }
            msg = ws_read.next() => {
                match msg {
                    Some(Ok(Binary(data))) => {
                        match parse_response(&data)? {
                            ServerResponse(payload) => {
                                // process utterances → emit TranscriptUpdate / AsrFinal
                            }
                            ErrorResponse(code, msg) => {
                                // emit Error event
                            }
                        }
                    }
                    // ws closed, error handling...
                }
            }
            _ = flush_timeout_future => {
                // emit AsrFinalOutput { reason: Timeout }
            }
        }
    }
});
```

Flush 映射:
- `AudioChunkBoundary::Flush` → `build_audio_frame([], true)` (empty data + last-frame flag)
- `AudioChunkBoundary::End` → same + mark stream for close after final

Provider endpointing:
- `bigmodel_async` 的 `end_window_size` 判停后服务端返回 `definite: true` utterance
- Adapter 检测到 definite utterance 后，如果是 last response 或 flush response，emit `AsrFinal`

### 7. 实现 capabilities

```rust
AsrModelCapabilities {
    model: "volcengine/bigmodel_async".into(),
    languages: vec![
        Language("zh-CN".into()),
        // bigmodel_async 默认支持中英文 + 方言
        // 指定 language 参数仅 bigmodel_nostream 支持
    ],
    streaming: true,
    batch: false,
    streaming_inputs: vec![AudioInputCapability {
        format: AudioFormat::Pcm,
        sample_rates_hz: SampleRateSupport::Exact(vec![16000]),
        channels: ChannelSupport::Exact(vec![1, 2]),
        max_duration_ms: None,
        max_bytes: None,
    }],
    batch_inputs: vec![],
    audio_timeline_modes: vec![AudioTimelineMode::ContinuousRealtime],
    endpointing_modes: vec![], // ProviderDefault (end_window_size) 隐式可用
    segment_flush: true,
    multi_segment_streaming: false, // 每个连接一次 session
    connection_reuse: ConnectionReuse::NotReusable,
    word_timestamps: true,  // show_utterances 开启后有 words 数组
    speaker_diarization: false,  // 需要 enable_nonstream + ssd_version=200，不做默认 capability
    confidence: false,
    code_switching: true,  // 默认支持中英文混合
    hot_words: true,
    context_prompt: true,  // 支持 corpus.context 上下文
    provider_option_keys: vec![
        "resource_id", "enable_nonstream", "enable_itn", "enable_punc",
        "enable_ddc", "enable_speaker_info", "ssd_version",
        "show_utterances", "show_speech_rate", "show_volume",
        "enable_lid", "enable_emotion_detection", "enable_gender_detection",
        "result_type", "end_window_size", "vad_segment_duration",
        "force_to_speech_time", "enable_accelerate_text", "accelerate_score",
        "sensitive_words_filter", "output_zh_variant",
    ].into_iter().map(String::from).collect(),
    default_flush_timeout_ms: Some(2000),
    source: CapabilitySource::Static,
    diagnostic_metadata: Value::Null,
}
```

### 8. 写 offline 测试

- **Binary frame roundtrip**: `build_header()` → 验证 4 bytes 各 bit 正确
- **Audio frame**: `build_audio_frame(data, false)` → parse → verify flag = `FLAG_NO_SEQUENCE`; `build_audio_frame(data, true)` → verify `FLAG_LAST_NO_SEQUENCE`
- **Full client request**: `build_full_client_request()` → parse frame → decompress → JSON → assert `model_name`, `show_utterances`, `rate`
- **Response parsing**: 构造已知 binary response → `parse_response()` → assert utterances 结构
- **Error response**: 构造 `MSG_ERROR_RESPONSE` frame → `parse_response()` → assert error code + message
- **Definite dedup**: 同一 `(start_time, end_time, text)` 喂两次 → 第一次 true 第二次 false
- **Word timestamps**: response 带 words 数组 → 正确 parse 为 `Vec<VolcengineWord>`
- **Hot words mapping**: `TranscribeOptions.hot_words = ["热词1"]` → full client request JSON 中 `corpus.context` 包含 hotwords
- **Endpointing mapping**: `EndpointingOptions { silence_timeout: Some(500ms) }` → JSON `end_window_size: 500`

### 9. 写 live 测试 (`#[ignore]`)

```rust
#[ignore]
#[tokio::test]
async fn live_volcengine_streaming() {
    // skip if VOLCENGINE_API_KEY and VOLCENGINE_RESOURCE_ID not set
    // create adapter from env
    // start_stream(StreamingTranscribeRequest { format: Pcm16 { 16000, 1 }, ... })
    // send 1s synthetic PCM silence (16kHz 16-bit mono)
    // flush_and_wait_final()
    // assert no error (silence → may return empty text, that's OK)
    // assert telemetry fields populated (trace_id, model, latency_final_ms)
}
```

### 10. 验证

```bash
cargo test -p agent-runtime-asr-providers
cargo test -p agent-runtime-asr-providers --no-default-features
cargo clippy -p agent-runtime-asr-providers -- -D warnings
cargo fmt --check
```

## 关键决策

- **用 legacy ASR 协议，不用双向流式事件协议**。`bigmodel_async` 虽然是"优化版双向流式"，但仍用 legacy 二进制帧协议（header + payload），不用 `Message` struct 的 event-based 协议。vendor docs 和 study_buddy 实际代码都确认了这一点
- **音频帧不压缩**（`COMP_NONE`）。同 study_buddy `build_audio_frame`。减少 CPU 开销，实时流场景带宽不是瓶颈
- **`show_utterances` 默认开启**。必须开启才能拿到 definite/word timestamps，这是 PRD streaming 语义的基础
- **`result_type` 默认 `"single"`**（增量返回）。同 study_buddy。减少 bigmodel_async 优化版的冗余数据
- **`resource_id` 通过 `AsrProviderRuntimeConfig.provider_options` 传入**。和 `app_key`/`access_key` 一样是调用方的计费/凭证配置，不是 request-level 参数，但 `AsrProviderRuntimeConfig` 没有专门的 `resource_id` 字段，所以通过 `provider_options` 传。未来可以提升为 typed config field
- **`speaker_diarization` 首期 capabilities = false**。vendor docs 说需要 `enable_nonstream=true` + `ssd_version="200"`，且只推荐 ASR 2.0 使用。调用方可以通过 `provider_options` 手动开启
- **`byteorder` crate 用于 big-endian 整数读写**。同 study_buddy protocol.rs 的做法
