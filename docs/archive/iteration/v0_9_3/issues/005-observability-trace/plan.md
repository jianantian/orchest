# 005 实现路线

## 要读的现有代码

- `crates/agent-runtime-asr-providers/src/observability.rs` — telemetry style
- `crates/agent-runtime-asr-providers/src/error.rs` — upstream error preservation/redaction style
- `crates/agent-runtime-tts-providers/src/streaming.rs` — terminal summary from 003
- `docs/polaris/observability.md` — SDK-wide observability constraints

## 步骤

### 1. 添加 telemetry builders

- Helper to start/finish operation timing
- Helper to count input chars and output bytes
- Helper to derive `TtsOperation` from gateway method
- Helper to create streaming summary telemetry at terminal event

### 2. 接入 trace id propagation

- Gateway 在 route selection 前生成 trace id
- Gateway 在 provider request 缺 trace id 时注入
- Stream wrapper 在可行处校验/normalize forwarded event trace ids

### 3. 添加 spans

- Router selection span 只在 selection 后记录 provider/model
- Provider spans 避免 raw text/audio fields
- Voice listing span 记录 provider/model filters，但不记录 secrets

### 4. 添加 metrics

- 使用现有 `metrics` crate convention
- Labels 只用低基数字段：provider, model, operation, error code/status
- 默认不把 voice id 作为 metrics label

### 5. Redaction tests

- Error upstream body/header-like metadata 中包含 API key
- Error 中包含 temporary URL
- 确认 redacted error 仍保留 code/status/message

## 验证

```bash
cargo test -p agent-runtime-tts-providers --no-default-features telemetry
cargo test -p agent-runtime-tts-providers --no-default-features redaction
cargo test -p agent-runtime-tts-providers
cargo clippy -p agent-runtime-tts-providers -- -D warnings
cargo fmt --check
```

## 关键决策

- TTS observability stays inside the satellite crate; core runtime event model does not change
- Text/audio logging is application policy, not SDK default behavior
- `voice_id` is data telemetry, not a high-cardinality metrics label by default
