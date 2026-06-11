# 001 实现路线

## 要读的现有代码

- `crates/agent-runtime-providers/Cargo.toml` — provider crate 依赖、feature 和 lint 风格
- `crates/agent-runtime-providers/src/lib.rs` — provider/model normalization 风格
- `crates/agent-runtime-aigc-providers/src/types.rs` — media/provider-neutral 类型风格
- `crates/agent-runtime-asr-providers/src/types.rs` — satellite crate 类型组织参考
- `crates/agent-runtime-asr-providers/src/error.rs` — stable error code 和 redaction 风格参考
- workspace `Cargo.toml` — members 列表格式

## 步骤

### 1. 创建 crate 骨架

- 新增 `crates/agent-runtime-tts-providers/Cargo.toml`
- 依赖包含 `tokio`, `serde`, `serde_json`, `async-trait`, `thiserror`, `bytes`, `uuid`, `chrono`, `tracing`, `metrics`
- 不加入 `anyhow`
- 不加入任何 `agent-runtime-*` workspace 内部依赖
- 定义 features: `volcengine`, `aliyun`, `default = ["volcengine", "aliyun"]`
- 在 workspace `Cargo.toml` 新增 member

### 2. 定义模块和 re-export

- `lib.rs` 只声明模块和显式 re-export
- `providers/mod.rs` 只声明 provider submodules，不放业务逻辑
- provider stubs 用 feature gate 暴露

### 3. 定义公共类型

- 在 `types.rs`, `voices.rs`, `streaming.rs`, `observability.rs`, `routing.rs`, `config.rs`, `error.rs` 按 spec 类型清单补齐 public structs/enums
- 需要跨 FFI 或 config 边界的类型 derive `Serialize` / `Deserialize`
- 含 `serde_json::Value` 的类型避免 derive `Eq`
- `AudioData::Url` 使用 `chrono::DateTime<Utc>`

### 4. 定义 trait 和 provider stubs

- `traits.rs` 定义 `#[async_trait] pub trait TtsProvider`
- provider stub adapter 实现 trait，可暂时返回 `TtsErrorCode::UnsupportedOperation`
- `create_tts_provider_from_config()` 可先根据 provider prefix 分发到 stub constructor

### 5. 定义 model normalization

- `normalize_tts_provider_model()` 接受 explicit `"provider/model"` 形状
- 空 provider、空 model、缺少 `/` 均返回稳定 `TtsError`
- 不实现 unprefixed default provider fallback

### 6. 验证

```bash
cargo check -p agent-runtime-tts-providers
cargo check -p agent-runtime-tts-providers --no-default-features
cargo test -p agent-runtime-tts-providers
cargo clippy -p agent-runtime-tts-providers -- -D warnings
cargo fmt --check
```

## 关键决策

- `Language` 用 `pub struct Language(pub String)`，因为 BCP-47 是开放集合
- `AudioFormat` 不使用含糊的 `Pcm` / `Wav` 名称，首期直接表达 PCM endian/container
- `TtsStreamSummary` 独立于 `SynthesizeResult`，避免 streaming completion 携带完整 audio bytes
