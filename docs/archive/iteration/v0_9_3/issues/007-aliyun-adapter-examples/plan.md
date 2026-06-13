# 007 实现路线

## 要读的资料

- `docs/external/aliyun/tts-api-doc.md` — model names, HTTP/WebSocket protocol and response fields
- `docs/external/aliyun/tts-guideline.md` — voice cloning/design and custom voice behavior
- `crates/agent-runtime-tts-providers/src/providers/volcengine.rs` — adapter pattern from 006
- `docs/archive/iteration/v0_9_3/prd.md` — provider scope and tool boundary

## 步骤

### 1. 添加 feature-gated dependencies

- Aliyun/DashScope HTTP/WebSocket deps 尽量作为 optional dependencies
- Wire deps to `aliyun` feature
- Verify `--no-default-features` still compiles fake/router tests

### 2. 实现 config 和 credential resolution

- Parse supported Aliyun target model strings
- Include `aliyun/qwen3-tts-instruct-flash-realtime` as the concrete instruct realtime target
- Resolve explicit key, explicit env and default env in order
- Preserve `api_url`, `region`, `timeout`, `provider_options`
- Add redaction for keys, auth headers and temporary URLs

### 3. 实现 capabilities

- Static capabilities for CosyVoice realtime target
- Static capabilities for Qwen3 realtime target
- Separate instruct model capability where supported
- Separate batch and streaming output formats

### 4. 实现 request mapping

- Batch synthesis where HTTP model supports it
- Single-stream committed text where realtime streaming supports it
- Duplex text chunk input where realtime protocol supports it
- Strictly reject unsupported instruction/SSML/format/duplex combinations

### 5. 实现 voice catalog behavior

- Return static/provider metadata for documented system voices
- Support existing custom voice ids for synthesis
- Do not implement custom voice creation, enrollment, deletion or governance

### 6. 添加 examples

- Prefer crate README snippets or examples matching local repo convention
- Show gateway construction with explicit model
- Show batch, single-stream, duplex stream and list voices
- Explicitly keep TTS outside core runtime Tool/Skill semantics

### 7. 测试

- Offline capability-difference tests across target models
- Direct provider request selector mismatch tests
- Instruction strict/coerce behavior
- Format validation differences
- Receiver-drop cancellation and no-hidden-unbounded-channel tests with fake transport
- Ignored live tiny fixture tests gated by env vars

## 验证

```bash
cargo test -p agent-runtime-tts-providers --features aliyun
cargo test -p agent-runtime-tts-providers --all-features
cargo clippy -p agent-runtime-tts-providers --all-features -- -D warnings
cargo fmt --check
```

## 关键决策

- Aliyun model names encode real capability differences; do not normalize them into one generic adapter behavior
- Examples are SDK usage references, not product-level voice session documentation
- Do not commit generated audio samples unless a later issue explicitly licenses and budgets them
- Adapter tests repeat transport-specific compliance checks from the shared streaming contract because real provider transports can break behavior that fake-provider tests cannot cover
