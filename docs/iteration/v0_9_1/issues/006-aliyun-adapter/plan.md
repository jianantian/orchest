# 006 实现路线

## 要读的现有代码

- `docs/external/aliyun/asr-api-doc.md` — 完整 API 文档：SDK 调用示例 + WebSocket 原始协议示例（Python/Java/Node.js）、事件生命周期、response 结构、断句配置、时间戳、情感识别
- `docs/external/aliyun/asr-guideline.md` — 模型选型：Fun-ASR vs Qwen-ASR 特性对比、音频规格、支持语言
- `crates/agent-runtime-aigc-providers/src/providers/aliyun.rs` — AIGC 的 Aliyun adapter，参考 DashScope API key 处理
- `crates/agent-runtime-asr-providers/src/streaming.rs` — 003 的 streaming contract
- `crates/agent-runtime-asr-providers/src/providers/volcengine.rs` — 005 的 adapter，参考 WebSocket + adapter task 模式

## 协议理解

### Fun-ASR 是 DashScope WebSocket 协议

与 Volcengine 的自定义二进制帧协议不同，Aliyun Fun-ASR 使用 **DashScope 标准 WebSocket 协议**：

- **控制消息**：JSON text frame（`run-task`, `finish-task`, server events）
- **音频数据**：raw binary frame（直接发送 PCM/WAV 字节，无自定义 header）
- **鉴权**：HTTP header `Authorization: bearer {api_key}`
- **WebSocket URL**：`wss://dashscope.aliyuncs.com/api-ws/v1/inference/`（北京地域）

这比 Volcengine 简单很多——不需要实现二进制帧协议。

### Task Lifecycle

```
Client                          Server
  |-- JSON: run-task ------------>|
  |<------------ task-started ----|
  |-- Binary: audio chunk ------->|
  |<------- result-generated -----|  (partial/sentence_end)
  |-- Binary: audio chunk ------->|
  |<------- result-generated -----|
  |-- JSON: finish-task --------->|
  |<------- result-generated -----|  (final results)
  |<------- task-finished --------|
  |         or task-failed -------|
```

### run-task 消息结构

```json
{
    "header": {
        "action": "run-task",
        "task_id": "uuid-32-hex",
        "streaming": "duplex"
    },
    "payload": {
        "task_group": "audio",
        "task": "asr",
        "function": "recognition",
        "model": "fun-asr-realtime",
        "parameters": {
            "sample_rate": 16000,
            "format": "pcm",
            "max_sentence_silence": 800
        },
        "input": {}
    }
}
```

### Server Event 结构

所有 server events 是 JSON text frame，共享结构：

```json
{
    "header": {
        "task_id": "...",
        "event": "task-started|result-generated|task-finished|task-failed",
        "error_code": "...",      // task-failed 时
        "error_message": "..."    // task-failed 时
    },
    "payload": {
        "output": {
            "sentence": {
                "text": "识别文本",
                "begin_time": 100,
                "end_time": 2000,
                "sentence_end": true,
                "words": [...]   // 字级时间戳
            }
        },
        "usage": {
            "duration": 10       // 计费时长（秒）
        }
    }
}
```

关键字段：
- `sentence_end: false` → partial result → `TranscriptUpdate { stability: Provisional }`
- `sentence_end: true` → sentence final → `TranscriptUpdate { stability: Committed }`（不等同于 `AsrFinal`）
- `task-finished` event → `AsrFinal`

### 鉴权

只有一个凭证：`DASHSCOPE_API_KEY`，通过 `Authorization: bearer {api_key}` header 传递。和其他 DashScope 服务（AIGC 等）用同一个 key。

### Endpointing

Fun-ASR：`max_sentence_silence` 参数（毫秒），静音超过此值判定句子结束。映射到 `EndpointingOptions.silence_timeout`。

### Capabilities 确认（来自 vendor docs）

| Feature | Fun-ASR Realtime | 来源 |
|---------|-----------------|------|
| 音频格式 | pcm, wav, mp3, opus, speex, aac, amr | guideline 音频规格表 |
| 采样率 | 16kHz | guideline |
| 声道 | 单声道 | guideline |
| Word timestamps | ✅ 句级 + 字级 | api-doc 时间戳章节 |
| Speaker diarization | ❌ | guideline 推荐模型表 |
| 情感识别 | ❌（仅 Qwen-ASR 支持） | guideline |
| 热词 | ✅ | guideline |
| 支持语言 | 中、英、日及方言 | guideline |
| 音频时长 | 无限制 | guideline |

### Qwen-ASR 差异（设计预留，不实现）

| 维度 | Fun-ASR | Qwen-ASR |
|------|---------|----------|
| WebSocket URL | `/api-ws/v1/inference/` | `/api-ws/v1/realtime` |
| 控制协议 | `run-task`/`finish-task` | `session.update`/`input_audio_buffer.commit`/`session.finish` |
| VAD | `max_sentence_silence` | `turn_detection.silence_duration_ms` / Manual mode |
| 情感识别 | ❌ | ✅ (provider metadata) |
| Word timestamps | ✅ | ❌ |
| 连接复用 | ✅ after task-finished | ❌ |

## 步骤

### 1. 依赖

`tokio-tungstenite` 已在 005 中作为 non-optional 依赖加入。006 不需要额外依赖——Fun-ASR 用 JSON text frame + raw binary frame，不需要 `flate2` 或 `byteorder`。

### 2. 定义 Aliyun 内部类型 (`providers/aliyun.rs`)

Client → Server:
```rust
#[derive(Serialize)]
struct DashScopeRunTask {
    header: DashScopeHeader,
    payload: DashScopePayload,
}
#[derive(Serialize)]
struct DashScopeFinishTask {
    header: DashScopeHeader,
    payload: DashScopeFinishPayload,
}
#[derive(Serialize)]
struct DashScopeHeader {
    action: String,
    task_id: String,
    streaming: String, // "duplex"
}
#[derive(Serialize)]
struct DashScopePayload {
    task_group: String,   // "audio"
    task: String,         // "asr"
    function: String,     // "recognition"
    model: String,        // "fun-asr-realtime"
    parameters: DashScopeParameters,
    input: Value,         // {}
}
#[derive(Serialize)]
struct DashScopeParameters {
    sample_rate: u32,
    format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_sentence_silence: Option<u64>,
    // ... other optional params from provider_options
}
```

Server → Client:
```rust
#[derive(Deserialize)]
struct DashScopeServerEvent {
    header: DashScopeServerHeader,
    payload: Option<DashScopeServerPayload>,
}
#[derive(Deserialize)]
struct DashScopeServerHeader {
    task_id: String,
    event: String,   // "task-started"|"result-generated"|"task-finished"|"task-failed"
    #[serde(default)]
    error_code: Option<String>,
    #[serde(default)]
    error_message: Option<String>,
}
#[derive(Deserialize)]
struct DashScopeServerPayload {
    output: Option<DashScopeOutput>,
    usage: Option<DashScopeUsage>,
}
#[derive(Deserialize)]
struct DashScopeOutput {
    sentence: Option<DashScopeSentence>,
}
#[derive(Deserialize)]
struct DashScopeSentence {
    text: String,
    begin_time: Option<i64>,
    end_time: Option<i64>,
    #[serde(default)]
    sentence_end: bool,
    words: Option<Vec<DashScopeWord>>,
}
#[derive(Deserialize)]
struct DashScopeWord {
    text: String,
    begin_time: i64,
    end_time: i64,
    punctuation: Option<String>,
}
#[derive(Deserialize)]
struct DashScopeUsage {
    duration: Option<f64>, // 计费时长（秒）
}
```

### 3. 实现 `AliyunAsrAdapter`

```rust
pub struct AliyunAsrAdapter {
    config: AliyunAsrConfig,
}
struct AliyunAsrConfig {
    model: String,              // "fun-asr-realtime"
    api_key: String,            // DASHSCOPE_API_KEY
    ws_url: String,             // default: wss://dashscope.aliyuncs.com/api-ws/v1/inference/
}
```

`AsrProviderRuntimeConfig` 映射:
- `config.model` → normalize → `model` 部分
- `config.api_key` / `config.api_key_env` → API key resolution, default env: `DASHSCOPE_API_KEY`
- `config.api_url` → `ws_url`, default `wss://dashscope.aliyuncs.com/api-ws/v1/inference/`

### 4. 实现 streaming adapter task

`start_stream()`:

1. Build WebSocket request:
   - URL = `self.config.ws_url`
   - Header: `Authorization: bearer {api_key}`
2. `tokio_tungstenite::connect_async(request)`
3. Build `DashScopeRunTask` JSON:
   - `task_id` = UUID hex 32 chars
   - `model` = `self.config.model`
   - `parameters.sample_rate` from `StreamingTranscribeRequest.format`
   - `parameters.format` from `StreamingTranscribeRequest.format`
   - `parameters.max_sentence_silence` from `EndpointingOptions.silence_timeout`
   - hot_words from `TranscribeOptions.hot_words` (通过 DashScope SDK 的 vocabulary_id 或 parameters 传递)
4. Send run-task as JSON text frame
5. Wait for `task-started` event
6. Create audio/event channels, emit `Started`, return `AsrStream`
7. Spawn adapter task:

```rust
tokio::spawn(async move {
    loop {
        tokio::select! {
            chunk = audio_rx.recv() => {
                match chunk {
                    Some(chunk) => match chunk.boundary {
                        None => {
                            // Send raw audio bytes as binary frame
                            ws_write.send(Binary(chunk.data.to_vec())).await?;
                        }
                        Flush => {
                            // Send finish-task JSON text frame
                            let msg = build_finish_task(task_id);
                            ws_write.send(Text(msg)).await?;
                            flush_pending = true;
                        }
                        End => {
                            let msg = build_finish_task(task_id);
                            ws_write.send(Text(msg)).await?;
                            flush_pending = true;
                            end_requested = true;
                        }
                    }
                    None => { /* sender dropped → cancellation */ }
                }
            }
            msg = ws_read.next() => {
                // Parse JSON text frame as DashScopeServerEvent
                match event.header.event.as_str() {
                    "result-generated" => {
                        // sentence_end=false → TranscriptUpdate { Provisional }
                        // sentence_end=true → TranscriptUpdate { Committed }
                    }
                    "task-finished" => {
                        // Build AsrFinal from accumulated results
                        // If end_requested → close
                        // If flush only → prepare for next segment (connection reuse)
                    }
                    "task-failed" → {
                        // emit Error { fatal: true }
                        // discard connection
                    }
                }
            }
            _ = flush_timeout_future => { /* Timeout final */ }
        }
    }
});
```

### 5. 实现 connection reuse

After `task-finished`:
- If more segments (Flush, not End):
  - Generate new `task_id`
  - Send new `run-task` on same WebSocket connection
  - Wait for `task-started`
  - Resume audio streaming
- If End: close WebSocket

After `task-failed`:
- Close WebSocket immediately
- Do not attempt reuse

Track state:
```rust
enum TaskState {
    Idle,           // connection open, no active task
    Starting,       // run-task sent, waiting task-started
    Streaming,      // task-started received, accepting audio
    Finishing,      // finish-task sent, waiting task-finished
    Failed,         // task-failed received, connection must be discarded
}
```

### 6. 实现 capabilities

```rust
AsrModelCapabilities {
    model: "aliyun/fun-asr-realtime".into(),
    languages: vec![
        Language("zh-CN".into()),
        Language("en".into()),
        Language("ja".into()),
        // + 粤语、闽南语、吴语等方言（per vendor docs）
    ],
    streaming: true,
    batch: false,
    streaming_inputs: vec![
        AudioInputCapability {
            format: AudioFormat::Pcm,
            sample_rates_hz: SampleRateSupport::Exact(vec![16000]),
            channels: ChannelSupport::Exact(vec![1]),
            max_duration_ms: None,
            max_bytes: None,
        },
        AudioInputCapability {
            format: AudioFormat::Wav,
            sample_rates_hz: SampleRateSupport::Exact(vec![16000]),
            channels: ChannelSupport::Exact(vec![1]),
            max_duration_ms: None,
            max_bytes: None,
        },
        AudioInputCapability {
            format: AudioFormat::Mp3,
            sample_rates_hz: SampleRateSupport::Any,
            channels: ChannelSupport::Exact(vec![1]),
            max_duration_ms: None,
            max_bytes: None,
        },
        AudioInputCapability {
            format: AudioFormat::Opus,
            sample_rates_hz: SampleRateSupport::Any,
            channels: ChannelSupport::Exact(vec![1]),
            max_duration_ms: None,
            max_bytes: None,
        },
        // + aac, amr, speex per vendor docs
    ],
    batch_inputs: vec![],
    audio_timeline_modes: vec![AudioTimelineMode::ContinuousRealtime],
    endpointing_modes: vec![EndpointingMode::AcousticSilence],
    segment_flush: true,
    multi_segment_streaming: true, // connection reuse after task-finished
    connection_reuse: ConnectionReuse::ReusableAfterProviderTaskFinished,
    word_timestamps: true,    // 句级 + 字级时间戳
    speaker_diarization: false, // Fun-ASR realtime 不支持
    confidence: false,
    code_switching: true,     // 中英混合
    hot_words: true,
    context_prompt: false,
    provider_option_keys: vec![
        "max_sentence_silence",
        "semantic_punctuation_enabled",
        "vocabulary_id",
    ].into_iter().map(String::from).collect(),
    default_flush_timeout_ms: Some(3000),
    source: CapabilitySource::Static,
    diagnostic_metadata: Value::Null,
}
```

### 7. 预留 Qwen-ASR 扩展点

Adapter 内部按 model 名称分发协议：

```rust
impl AliyunAsrAdapter {
    fn protocol(&self) -> AliyunProtocol {
        match self.config.model.as_str() {
            m if m.starts_with("fun-asr") => AliyunProtocol::FunAsr,
            m if m.starts_with("qwen") => AliyunProtocol::QwenAsr,
            _ => AliyunProtocol::FunAsr, // default
        }
    }
}

enum AliyunProtocol {
    FunAsr,
    QwenAsr, // not implemented in v0.9.1
}
```

`start_stream()` 中 match on protocol：
- `FunAsr` → 当前实现
- `QwenAsr` → return `AsrError { code: UnsupportedOperation }`（v0.9.1 不实现）

Capabilities 也按 protocol 分发：
- `FunAsr` → 当前 capabilities
- `QwenAsr` → 不同的 capabilities（no word_timestamps, different endpointing_modes, no connection_reuse）

### 8. 写 offline 测试

- **run-task JSON**: build_run_task(config, options) → assert JSON 结构正确、model/format/sample_rate 正确
- **finish-task JSON**: build_finish_task(task_id) → assert JSON 结构正确
- **task-started parsing**: mock JSON → parse → assert event = task-started
- **result-generated parsing**: mock JSON with sentence.text + sentence_end → parse → assert text and sentence_end
- **result-generated word timestamps**: mock JSON with words array → parse → assert word-level timestamps
- **task-finished parsing**: mock JSON → parse → assert event = task-finished
- **task-failed parsing**: mock JSON with error_code/error_message → parse → assert error fields
- **Connection reuse state machine**: Idle → Starting → Streaming → Finishing → TaskFinished → Idle (new task_id)
- **Failed task discard**: Idle → Starting → Streaming → Failed → assert cannot reuse
- **Endpointing mapping**: `EndpointingOptions { silence_timeout: 500ms }` → parameters.max_sentence_silence = 500
- **Capability rejection**: `speaker_diarization: true` + fun-asr-realtime → strict `UnsupportedOption`
- **Qwen-ASR model → UnsupportedOperation**: `aliyun/qwen3-asr-flash-realtime` → `start_stream()` returns `UnsupportedOperation`

### 9. 写 live 测试 (`#[ignore]`)

```rust
#[ignore]
#[tokio::test]
async fn live_aliyun_fun_asr_streaming() {
    // skip if DASHSCOPE_API_KEY not set
    // create adapter: model="fun-asr-realtime"
    // start_stream(format: Pcm16 { 16000, 1 })
    // send 1s synthetic PCM silence
    // flush_and_wait_final()
    // assert no error
    // assert telemetry fields populated
}
```

### 10. 写示例代码 (`examples/`)

**`examples/asr_segmented.rs`** — PTT style:
```rust
// 使用 aliyun/fun-asr-realtime
// create gateway with route config
// start_stream()
// simulate 3 segments:
//   segment 1: send 1s audio → flush_and_wait_final() → print text
//   segment 2: send 2s audio → flush_and_wait_final() → print text
//   segment 3: send 1s audio → end_and_wait_final() → print text
```

**`examples/asr_full_duplex.rs`** — Full-duplex:
```rust
// 使用 aliyun/fun-asr-realtime
// start_stream()
// split()
// task 1: read audio file → send_audio() in loop
// task 2: while let Some(event) = events.next() { print event }
// task 1 finishes → sink.end_stream()
// task 2 receives AsrFinal → done
```

示例不需要真实音频设备，用 synthetic PCM 或文件。注释说明如何换成 mic input。

### 11. 验证

```bash
cargo test -p agent-runtime-asr-providers
cargo test -p agent-runtime-asr-providers --no-default-features
cargo clippy -p agent-runtime-asr-providers -- -D warnings
cargo fmt --check
```

## 关键决策

- **Fun-ASR 用 DashScope WebSocket 协议，不是自定义二进制协议**。控制消息是 JSON text frame，音频是 raw binary frame。比 Volcengine 简单很多
- **鉴权只有一个 `DASHSCOPE_API_KEY`**，通过 `Authorization: bearer` header。和 AIGC provider 用同一个 key
- **Connection reuse 的 task_id**：每次新 task 生成新 UUID。vendor docs 示例中 task_id 是连接级别的，但 reuse 时必须换新 task_id
- **`sentence_end` 不等于 `AsrFinal`**。`sentence_end: true` 是 provider 的句级 committed 结果，映射到 `TranscriptUpdate { Committed }`。`task-finished` 才是 stream/task 级别的 final，映射到 `AsrFinal`
- **Flush 映射到 `finish-task`**。`AudioChunkBoundary::Flush` 发送 `finish-task` → wait `task-finished` → emit `AsrFinal`。如果是 multi-segment，之后发新 `run-task` 开始下一个 segment
- **不需要 `flate2` 或 `byteorder`**。Aliyun 的音频帧是 raw bytes，没有 gzip 或自定义 header
