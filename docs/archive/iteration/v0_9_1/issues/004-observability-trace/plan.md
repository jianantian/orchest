# 004 实现路线

## 要读的现有代码

- `crates/agent-runtime-providers/src/telemetry.rs` — LLM provider 的 telemetry 实现，span 命名和 metrics 记录方式
- `crates/agent-runtime-aigc-providers/src/telemetry.rs` — AIGC 的 telemetry 模式
- `crates/agent-runtime-providers/src/anthropic.rs` — 看 `tracing::instrument` 和 `metrics::counter!` / `metrics::histogram!` 的用法

## 步骤

以下步骤中，步骤 1/4/5/6/7 不依赖 003 的 streaming 实现，可以先行。步骤 2/3 需要 003 的 adapter task 和 event 流就位后才能接入。

### 1. 实现 trace ID 注入 (`routing.rs`) — 可先行

- `AsrGateway::transcribe()` / `start_stream()`:
  - 如果 `request.options.trace_id` 为 None → `uuid::Uuid::new_v4().to_string()`
  - 设置到 request.options.trace_id（需要 mutable 或 clone + override）
  - 传递给 provider adapter

### 2. 实现 trace ID 传播 (`streaming.rs`) — 依赖 003

- Adapter task 在 emit 每个 `AsrStreamEvent` 时注入 trace_id
- `RouteSelected { trace_id, model }` — gateway 在 router select 后、调 provider 前 emit
- `Started { trace_id, model }` — provider adapter 在连接建立后 emit
- `TranscriptUpdate { trace_id, ... }` — adapter 在 provider 返回 partial 时 emit
- `EndOfSpeech { trace_id, ... }` — adapter 在 provider endpoint 时 emit
- `AsrFinal { final_output: AsrFinalOutput { trace_id, ... } }` — adapter 在 finalization 时 emit
- `Error { trace_id, ... }` — adapter 在 error 时 emit

### 3. 实现 AsrTelemetry 填充 (`observability.rs`) — 依赖 003

`AsrTelemetryBuilder` 是 `pub` 的，adapter task（003 fake provider 和 005/006 真 provider）持有并使用。

- 定义 `pub struct AsrTelemetryBuilder`:
  ```rust
  struct AsrTelemetryBuilder {
      trace_id: String,
      model: String,
      started_at: Instant,
      first_update_at: Option<Instant>,
      rollback_count: u32,
      confidences: Vec<f64>,
      // ...
  }
  ```
- `on_started()` — record start time
- `on_transcript_update()` — record first_update_at (once), track rollbacks
- `on_final()` — compute latencies, build `AsrTelemetry`
- Adapter task 持有 builder，每个事件时调用对应 method
- `TranscribeResult.telemetry` 从 builder 最终构建

### 4. 实现 tracing spans (`observability.rs`) — 可先行

使用 `tracing::info_span!` + `Instrument::instrument()`:

```rust
pub fn gateway_transcribe_span(trace_id: &str, model: &str) -> tracing::Span {
    tracing::info_span!("asr.gateway.transcribe", trace_id, model)
}
// ... 同理其他 4 个 span
```

在 gateway 和 adapter 代码中 instrument:
- `AsrGateway::transcribe()` → `asr.gateway.transcribe` span
- `AsrGateway::start_stream()` → `asr.gateway.stream` span
- Provider `transcribe()` call → `asr.provider.request` span
- Provider streaming session → `asr.provider.stream` span
- Router select → `asr.router.select` span

### 5. 实现 metrics (`observability.rs`) — 可先行（定义），接入依赖 003

使用 `metrics` crate:

```rust
pub fn record_asr_request_duration(model: &str, duration: Duration) {
    metrics::histogram!("asr.request.duration_ms", "model" => model.to_string())
        .record(duration.as_millis() as f64);
}
// ... 同理其他 metrics
```

| Metric name | Type | Labels | Where recorded |
|-------------|------|--------|----------------|
| `asr.request.duration_ms` | histogram | model | gateway transcribe/stream end |
| `asr.first_update.latency_ms` | histogram | model | first TranscriptUpdate |
| `asr.final.latency_ms` | histogram | model | AsrFinal emit |
| `asr.audio.duration_ms` | histogram | model | AsrFinal |
| `asr.upstream.error_count` | counter | model, code, status | provider error |
| `asr.transcript.rollback_count` | counter | model | provisional → revised |
| `asr.option.adjustment_count` | counter | model | coerce adjustment |

### 6. 实现 secret redaction (`error.rs`) — 可先行

- `AsrError` 在 serialize upstream_body 前 redact known secret patterns
- 辅助函数 `fn redact_secrets(body: &mut Value)`:
  - 遍历 JSON object keys，匹配 `*key*`, `*secret*`, `*token*`, `*password*`, `*credential*` → replace value with `"[REDACTED]"`
- 在 `AsrError::new_with_upstream()` 构造时自动 redact
- 确保 `tracing::debug!` 输出 error 时不含 raw audio / API keys

### 7. 添加 log 安全 guard — 可先行（review 时执行）

- Review 所有 `tracing::*` 调用：
  - 不 log `AudioChunk.data` 的内容（可以 log len）
  - 不 log API key / access key values
  - 不 log transcript text at info level (只 debug level)
  - 不 log signed URLs

### 8. 写测试

- **Telemetry builder**: feed events → assert latencies and counts
- **Trace ID propagation**: fake provider stream → collect all events → assert all carry same trace_id
- **Auto trace ID**: omit trace_id → gateway generates one → all events have it
- **Secret redaction**: AsrError with upstream_body containing "api_key" → serialized output has `[REDACTED]`
- **Metrics**: 使用 `metrics-util` 的 `DebuggingRecorder` → assert metrics emitted with correct labels

### 9. 验证

```bash
cargo test -p agent-runtime-asr-providers
cargo clippy -p agent-runtime-asr-providers -- -D warnings
cargo fmt --check
```

## 关键决策

- `update_rollback_count` 的定义：当 provider 发送一个 `TranscriptUpdate { stability: Provisional }` 后又发送另一个覆盖同一 segment 的更新，计一次 rollback。计数逻辑在 adapter task 的 `AsrTelemetryBuilder` 中
- Metrics label 粒度：按 `model` (normalized provider/model) 分类，不按 trace_id（太高基数）
- Secret redaction 是 best-effort，不是 cryptographic guarantee。用 pattern matching on common key names
