# 001 实现路线

## v0.7 依赖判断

不依赖 v0.7。这个 issue 只创建独立 crate 和公共类型，不能引用 v0.7 Hook、core tool 注册、AgentConfig 或任何 workspace-internal crate。后续把 gateway 包装成 tool 是集成事项，不属于本 issue。

## 步骤

1. **创建 crate 骨架**
   - 新建 `crates/agent-runtime-aigc-providers/Cargo.toml`
   - 在根 `Cargo.toml` 的 workspace members 中加入 `"crates/agent-runtime-aigc-providers"`
   - 新建 `src/lib.rs`、`src/types.rs`、`src/image.rs`、`src/http.rs`、`src/telemetry.rs`
   - 新建空模块目录：`src/providers/mod.rs`、`src/storage/mod.rs`
   - 001 只加入公共类型和 shared HTTP client 需要的基础依赖：`tokio`、`serde`、`serde_json`、`async-trait`、`thiserror`、`reqwest`、`bytes`、`chrono`、`tracing`、`metrics`
   - 运行 `cargo check -p agent-runtime-aigc-providers`

2. **定义 provider 边界**
   - 在 `image.rs` 定义 `ImageProvider` trait，方法为 `provider_name()`、`model_name()`、`capabilities()`、`create_image_generation()`、`get_image_generation()`
   - `create_image_generation()` 和 `get_image_generation()` 只返回 provider-layer 类型：`ProviderImageJob`、`ProviderImageEvent`、`ProviderGenerationStatus`
   - 明确不要返回 `ImageGenerationResponse` 或 `GeneratedImage`

3. **定义公共请求类型**
   - 在 `types.rs` 定义 `ImageGenerationRequest`
   - 按 spec 覆盖：operation、prompt、negative prompt、inputs、generation config、execution config、output config、compatibility policy、provider options
   - `ImageOperation` 至少包含 `TextToImage`、`ImageToImage`、`EditImage`、`Upscale`、`FaceSwap`
   - `AssetRef` 覆盖 URL、data URL、base64、bytes、local path、stored asset
   - 定义 `AssetIngestSource::{Url, DataUrl, Base64, Bytes}`，作为 provider adapter 返回资产时使用的 provider-boundary 类型

4. **定义公共响应类型**
   - 定义 `ImageGenerationResponse`、`GeneratedImage`、`ImageOutput`
   - `ImageOutput` 只能有 `Url(ImageUrlOutput)` 和 `Base64(ImageBase64Output)`
   - `GeneratedImage.asset_id` 是顶层字段，URL 和 Base64 两种 delivery 都必须存在
   - `ImageUrlOutput` 只包含可直接使用的 URL 和可选过期时间，不包含 bucket、endpoint、object key、storage credentials

5. **定义能力、错误、usage 和运行时 config**
   - 定义 `ImageModelCapabilities`、`ImageOperationCapability`、`GenerationExecutionMode`
   - 定义 `CompatibilityPolicy::{Strict, Coerce}` 和 `OptionAdjustment`
   - 定义 `AigcError`，保留 normalized code/message/provider/status/upstream details
   - 定义 `ImageUsage`
   - 定义 `AigcProviderRuntimeConfig`，字段包括 provider、model、api_key、api_key_env、api_url、region、timeout、provider_options

6. **实现共享 HTTP client helper**
   - 在 `http.rs` 定义 `shared_client() -> &'static reqwest::Client`
   - 用 `std::sync::OnceLock` 保证进程内 singleton
   - 只负责构造 client，不放 provider 业务逻辑

7. **写类型测试**
   - 在 `types.rs` 或 `tests/types.rs` 覆盖 serde round-trip：`ImageGenerationRequest`、`ImageGenerationResponse`、`ProviderImageJob`、`GeneratedImage`、`ImageModelCapabilities`
   - 增加 URL/Base64 output 都含 `asset_id` 的测试
   - 增加 public URL output 不包含 bucket/object key/endpoint/access key/provider URL 字段的序列化测试
   - 在 `http.rs` 加 singleton 测试，比较两次 `shared_client()` 的指针地址

8. **验收**
   - `cargo test -p agent-runtime-aigc-providers`
   - `cargo clippy -p agent-runtime-aigc-providers -- -D warnings`
   - 确认 `Cargo.toml` 没有 workspace-internal path dependency

## 关键决策

- `ProviderGenerationStatus` 不包含 `PersistingAssets`；这是 gateway-public 状态。
- `ImageGenerationEvent` 只属于 gateway public event，不出现在 `ImageProvider` trait。
- 这个 issue 允许创建 module stub，但不能实现 provider HTTP 调用、storage backend 或 gateway orchestration。
