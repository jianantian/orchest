# 006 实现路线

## 要读的资料

- `docs/external/volceengine/tts_bidirection.md` — protocol, auth, session lifecycle and response shape
- `crates/agent-runtime-tts-providers/src/config.rs` — factory hooks
- `crates/agent-runtime-tts-providers/src/streaming.rs` — stream contract
- `crates/agent-runtime-tts-providers/src/voices.rs` — voice/control validation

## 步骤

### 1. 添加 feature-gated dependencies

- HTTP/WebSocket deps 尽量作为 optional dependencies
- Wire deps to `volcengine` feature
- 保持 fake/router tests 在 `--no-default-features` 下可编译

### 2. 实现 config 和 credential resolution

- Parse `volcengine/seed-tts-2.0`
- Resolve explicit key, explicit env and default env in order
- Add redaction helpers for Volcengine credential names and request fields

### 3. 实现 capabilities 和 catalog

- Static capabilities for target model/resource
- Static voice metadata where provider docs support it
- Conservative assumptions marked as such

### 4. 实现 request mapping

- Batch synthesis for supported HTTP/chunked mode
- Single-stream synthesis for unidirectional streaming mode
- Duplex streaming for bidirectional WebSocket mode
- Reject unsupported operation/mode combinations

### 5. 实现 stream/session lifecycle

- No concurrent sessions on one connection
- Direct provider events start with `Started`
- Release provider session/connection on terminal event or cancellation
- Preserve first-audio latency

### 6. 测试

- Offline request construction and error mapping tests
- Direct provider request selector mismatch tests
- Session lifecycle tests with fake transport
- Receiver-drop cancellation and no-hidden-unbounded-channel tests with fake transport
- Ignored live tiny fixture test gated by env vars

## 验证

```bash
cargo test -p agent-runtime-tts-providers --features volcengine
cargo test -p agent-runtime-tts-providers --all-features
cargo clippy -p agent-runtime-tts-providers --features volcengine -- -D warnings
cargo fmt --check
```

## 关键决策

- 不暴露 Volcengine connection pooling/reuse as public SDK API
- Keep protocol modes explicit internally so bidirectional text input is not mistaken for unidirectional output streaming
- Live tests are ignored unless explicit credentials are set
- Adapter tests repeat transport-specific compliance checks from the shared streaming contract because real provider transports can break behavior that fake-provider tests cannot cover
