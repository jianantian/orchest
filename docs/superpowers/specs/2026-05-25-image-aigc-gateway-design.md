# Image AIGC Gateway design

> Date: 2026-05-25
> Status: Review

## Goal

Provide a unified image generation and image editing gateway across AIGC providers. Callers should express image tasks in Orchest capability concepts such as text-to-image, image-to-image, masked edit, region edit, output format, size, quality, seed, and persistence. Provider adapters translate those concepts into provider-specific protocols without leaking provider output URLs or blocking model-specific features.

Initial image providers:

- Crazyrouter GPT Image API
- Alibaba Cloud DashScope Qwen-Image, Z-Image, and Wan image models
- OpenRouter image-output models
- Renderful image generation gateway

The first milestone is image only. Video, music, audio, and 3D generation should reuse the job, asset, storage, error, and telemetry foundations later, but they are not part of this image gateway contract.

## Non-goals

- Do not implement cross-provider routing, failover, retries, or ranking in provider adapters. Those are policy-layer concerns.
- Do not make `agent-runtime-core` branch by provider. Core should see only normalized gateway/tool contracts.
- Do not expose provider artifact URLs to callers. Provider URLs are ingestion inputs only.
- Do not force every provider feature into typed fields before it is stable. Model-specific features may use `provider_options`.
- Do not perform image decoding, resizing, recompression, or moderation unless explicitly requested by a later feature.

## Why this is separate from LLM providers

`agent-runtime-providers` owns the LLM `ModelAdapter::complete()` contract: messages, tools, reasoning, streaming text/tool events, and token usage. Image generation has a different shape:

- Asset inputs and outputs instead of chat messages.
- Synchronous, streaming, and async task execution all need to fit.
- Output persistence is part of the public contract.
- Cost and usage are image-count, resolution, model, and task based, not token based.
- Provider capabilities are media features such as masks, bboxes, references, output format, and resolution tiers.

Create a separate crate:

```text
crates/agent-runtime-aigc-providers/
  src/
    lib.rs
    types.rs
    image.rs
    storage/
      mod.rs
      traits.rs
      oss.rs
      noop.rs
      local.rs
    providers/
      crazyrouter.rs
      aliyun.rs
      openrouter.rs
      renderful.rs
    http.rs
    telemetry.rs
```

This crate should be standalone and should not depend on workspace-internal crates. `agent-runtime-core` can later wrap it as built-in tools such as `image_generate` and `image_edit`.

## Unification principles

- **Capability first.** Public APIs are organized by image capability, not provider endpoint shape.
- **No silent semantic loss.** If a provider cannot honor a requested field, the gateway records an `OptionAdjustment` or returns an error under strict compatibility.
- **Provider details are adapter-private by default.** Provider raw request and response details are preserved in internal metadata for debugging, but public output stays normalized.
- **Do not constrain provider-specific power.** Typed fields cover shared stable concepts. Rare or fast-moving model controls stay available through `provider_options`.
- **Public assets are controlled by Orchest.** Generated outputs returned to callers are either base64/data URL or an Orchest-controlled URL backed by the configured asset store.
- **Streaming and async are first-class.** A provider may return final assets synchronously, stream partial assets, or require task polling. The gateway presents a single job model.
- **Low runtime overhead.** Persistence uses streaming download/upload and bounded concurrency. It does not buffer full images in memory when avoidable.

## Provider facts

### Crazyrouter

Source: <https://docs.crazyrouter.com/en/images/gpt-image>

Crazyrouter exposes GPT Image through OpenAI-style image endpoints:

- `POST https://cn.crazyrouter.com/v1/images/generations`
- `POST https://cn.crazyrouter.com/v1/images/edits`

Documented model: `gpt-image-2`.

Generation parameters include:

- `prompt`
- `n`, default 1, range 1-10
- `size`, either `auto` or `WxH`; width/height multiples of 16 with documented pixel/range constraints
- `quality`: `auto`, `low`, `medium`, `high`; `hd` maps to `high`; `standard` is rejected
- `background`: `auto` or `opaque`; transparent is not supported
- `output_format`: `png`, `jpeg`, `webp`
- `output_compression`: 0-100 for jpeg/webp only
- `moderation`: `auto` or `low`
- `stream` and `partial_images`
- `user`

Edit parameters add multipart `image` / `image[]` and optional `mask`. Up to 16 reference images are documented. Mask semantics: transparent areas are edited.

Rejected fields include `response_format`, `style`, `input_fidelity`, `background=transparent`, `quality=standard`, and `output_format=png` with `output_compression`.

### Alibaba Cloud DashScope

Sources:

- Qwen-Image generation: <https://www.alibabacloud.com/help/zh/model-studio/qwen-image-api>
- Qwen-Image editing: <https://www.alibabacloud.com/help/en/model-studio/qwen-image-edit-api>
- Wan image generation and editing: <https://help.aliyun.com/zh/model-studio/wan-image-generation-and-editing-api-reference>

DashScope has multiple image model families with different contracts:

- Qwen-Image: text rendering, text-to-image, editing, and multi-image fusion.
- Z-Image: fast text-to-image with `prompt_extend` returning optimized prompt and reasoning content when enabled.
- Wan image: photorealistic generation, image editing, sequential/group image generation, bbox region editing, color palettes, and 4K on selected models.

Common endpoint shape for current multimodal image models:

```text
POST /api/v1/services/aigc/multimodal-generation/generation
```

Async endpoints exist for selected models and older APIs:

```text
POST /api/v1/services/aigc/image-generation/generation
POST /api/v1/services/aigc/text2image/image-synthesis
GET  /api/v1/tasks/{task_id}
```

Qwen request shape uses `input.messages[].content[]` with `text` and optional `image` entries, plus `parameters`.

Qwen generation parameters include:

- `negative_prompt`
- `size`, as `width*height`
- `n`
- `prompt_extend`
- `watermark`
- `seed`

Qwen image inputs accept URL or base64 data URL. Current docs describe 1-3 input images for Qwen editing/fusion. Generated image URLs are documented as expiring after 24 hours.

Wan 2.7 adds:

- `size` as `"1K"`, `"2K"`, `"4K"`, or explicit pixels
- `n`, ordinary generation 1-4 and sequential mode up to 12
- `enable_sequential`
- `thinking_mode`
- `color_palette`
- `bbox_list`, up to 2 boxes per input image in documented examples
- `watermark`
- image editing with 0-9 image inputs by URL/base64

Region editing uses `bbox_list` under `parameters`.

### OpenRouter

Source: <https://openrouter.ai/docs/guides/overview/multimodal/image-generation>

OpenRouter image generation is exposed through Chat Completions and Responses, not OpenAI Images API. The documented Chat Completions request uses:

- `model`
- `messages`
- `modalities`: `["image"]` or `["image", "text"]`
- optional `image_config`
- optional `stream`

Model discovery:

```text
GET /api/v1/models?output_modalities=image
GET /api/v1/models?output_modalities=text,image
```

`image_config` includes shared and model-specific controls:

- `aspect_ratio`
- `image_size`: `0.5K`, `1K`, `2K`, `4K` depending on model
- Recraft-only `strength`
- Recraft V3 `text_layout`
- Recraft V3 `style`
- Recraft RGB color controls
- Sourceful font inputs
- Sourceful super-resolution references

Outputs are in `message.images` for non-streaming and `delta.images` for streaming. Documented image values are base64 data URLs, typically PNG.

### Renderful

Sources:

- Docs: <https://renderful.ai/docs>
- Public model list API: <https://api.renderful.ai/api/v1/models?type=text-to-image>

Renderful is itself an AIGC generation gateway. The current documented primary API is:

```text
POST /api/v1/generations
GET  /api/v1/generations/:id
GET  /api/v1/models?type=text-to-image
GET  /api/v1/models?type=image-to-image
POST /api/v1/uploads
```

`type` is required. Image-related types include at least:

- `text-to-image`
- `image-to-image`
- `upscale`
- `face-swap`

The create/get flow is async. Status values are:

- `queued`
- `processing`
- `completed`
- `failed`

Completed tasks contain `outputs` as URLs. Renderful also supports webhooks. Public model listing returns model capabilities such as aspect ratios, resolutions, max outputs, cost ranges, and webhook support.

Renderful model pages and responses currently show older `/v1/predictions` traces. A direct request to `/v1/predictions` returns a deprecated response with a successor link toward `/api/v1`. The adapter should treat `/api/v1/generations` as the primary contract and keep legacy endpoint handling out of the public API.

## Public API shape

### ImageProvider trait

```rust
#[async_trait]
pub trait ImageProvider: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn capabilities(&self) -> ImageModelCapabilities;

    async fn create_image_generation(
        &self,
        request: ImageGenerationRequest,
        tx: Option<mpsc::Sender<ImageGenerationEvent>>,
    ) -> Result<ImageGenerationJob, AigcError>;

    async fn get_image_generation(
        &self,
        provider_job_id: &str,
    ) -> Result<ImageGenerationJob, AigcError>;
}
```

`create_image_generation()` may return a completed job for synchronous providers or a running job for async providers. `get_image_generation()` polls provider-native task state when the provider supports async tasks. Providers without async tasks return a stable `unsupported_operation` error when polled.

Convenience helpers:

```rust
pub async fn generate_image(
    provider: &dyn ImageProvider,
    request: ImageGenerationRequest,
) -> Result<ImageGenerationJob, AigcError>;

pub async fn run_image_generation(
    gateway: &ImageGateway,
    request: ImageGenerationRequest,
    poll: PollOptions,
) -> Result<ImageGenerationResponse, AigcError>;
```

The gateway helper waits for provider completion and asset persistence before returning final outputs.

### Gateway orchestration

Provider adapters should not know about storage credentials or public delivery policy. The gateway coordinates:

1. Validate request against provider/model capabilities.
2. Call the selected provider adapter.
3. Drain stream events when present.
4. Poll async provider tasks when needed.
5. Persist assets through `AssetStore` or return base64 according to `ImageOutputDelivery`.
6. Return only normalized outputs.

```rust
pub struct ImageGateway {
    provider: Box<dyn ImageProvider>,
    asset_store: Arc<dyn AssetStore>,
    config: ImageGatewayConfig,
}
```

## Request types

### ImageGenerationRequest

```rust
pub struct ImageGenerationRequest {
    pub operation: ImageOperation,
    pub prompt: String,
    pub negative_prompt: Option<String>,
    pub inputs: Vec<ImageInput>,
    pub config: ImageGenerationConfig,
    pub execution: GenerationExecutionConfig,
    pub output: ImageOutputConfig,
    pub compatibility: CompatibilityPolicy,
    pub provider_options: Value,
}
```

`provider_options` is an explicit escape hatch. It is not a dumping ground for common fields. A field should be promoted into typed config when at least two providers support the concept or when it becomes important to the public tool contract.

### ImageOperation

```rust
pub enum ImageOperation {
    TextToImage,
    ImageToImage,
    EditImage,
    Upscale,
    FaceSwap,
}
```

Initial tool exposure should start with:

- `TextToImage`
- `ImageToImage`
- `EditImage`

`Upscale` and `FaceSwap` are represented because Renderful exposes them as image-related generation types, but they do not need built-in tools in the first implementation.

### ImageInput

```rust
pub struct ImageInput {
    pub role: ImageInputRole,
    pub source: AssetRef,
    pub mime_type: Option<String>,
    pub metadata: Value,
}

pub enum ImageInputRole {
    SourceImage,
    ReferenceImage,
    Mask,
    Font,
    SuperResolutionReference,
}
```

Do not collapse inputs into a single `image` field. Providers support different input roles:

- Crazyrouter uses multipart source/reference images plus optional mask.
- Aliyun Qwen and Wan use URL/base64 image entries.
- OpenRouter Sourceful supports font inputs and super-resolution references.
- Renderful image-to-image and upload flows require URL inputs.

### AssetRef

```rust
pub enum AssetRef {
    Url(String),
    DataUrl(String),
    Base64 {
        data: String,
        mime_type: String,
    },
    Bytes {
        bytes: bytes::Bytes,
        mime_type: String,
    },
    LocalPath(PathBuf),
    Stored {
        asset_id: String,
    },
}
```

Adapters decide how to convert `AssetRef` to the provider protocol. Examples:

- Crazyrouter may send `Bytes` or `LocalPath` as multipart.
- Aliyun may convert `Bytes` or `LocalPath` to base64 data URL.
- Renderful may upload `LocalPath` or `Bytes` first and pass the resulting URL.
- `Stored` resolves through `AssetStore` before provider submission.

## Generation config

### ImageGenerationConfig

```rust
pub struct ImageGenerationConfig {
    pub count: Option<u32>,
    pub size: Option<ImageSize>,
    pub quality: Option<ImageQuality>,
    pub output_format: Option<ImageFormat>,
    pub output_compression: Option<u8>,
    pub background: Option<ImageBackground>,
    pub seed: Option<u64>,
    pub watermark: Option<bool>,
    pub prompt_extend: Option<bool>,
    pub safety: Option<SafetyConfig>,
    pub edit: ImageEditConfig,
    pub style: ImageStyleConfig,
}
```

Use `Option` so callers can distinguish "provider default" from an explicit value.

### ImageSize

```rust
pub enum ImageSize {
    Auto,
    Pixels { width: u32, height: u32 },
    AspectRatio(String),
    ResolutionTier(String),
}
```

Mapping examples:

- Crazyrouter: `Auto` -> `"auto"`, `Pixels` -> `"WxH"`.
- Aliyun Qwen: `Pixels` -> `"W*H"`.
- Aliyun Wan: `ResolutionTier("1K")` / `"2K"` / `"4K"` maps directly; `Pixels` maps to `"W*H"`.
- OpenRouter: `AspectRatio` -> `image_config.aspect_ratio`; `ResolutionTier` -> `image_config.image_size`.
- Renderful: prefer model capability keys from `/api/v1/models`; adapter maps to provider body if supported.

If a provider supports only aspect ratio and the caller requests exact pixels, `CompatibilityPolicy::Coerce` may convert to the nearest supported aspect ratio and record an adjustment. `Strict` returns `unsupported_option`.

### Quality, format, and background

```rust
pub enum ImageQuality {
    Auto,
    Low,
    Medium,
    High,
    Standard,
    Hd,
    Provider(String),
}

pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
    Provider(String),
}

pub enum ImageBackground {
    Auto,
    Opaque,
    Transparent,
    Color(String),
    Provider(String),
}
```

Provider examples:

- Crazyrouter supports `auto`, `low`, `medium`, `high`; `hd` is coerced to `high`; `standard` is rejected; transparent background is not supported.
- Aliyun docs for the covered models primarily describe PNG outputs and watermark controls. If `output_format` is requested, strict mode should reject unless the selected model documents support.
- OpenRouter returns base64 data URLs, typically PNG. Format control is model-specific and should be driven by model metadata or `provider_options`.
- Renderful model list exposes capabilities but not a universal format parameter. Treat format as model-specific unless documented per model.

### Safety

```rust
pub struct SafetyConfig {
    pub moderation: Option<ModerationLevel>,
}

pub enum ModerationLevel {
    Auto,
    Low,
    Provider(String),
}
```

Crazyrouter documents `moderation`. Other providers should ignore only in coerce mode with an adjustment, or reject in strict mode.

### Edit config

```rust
pub struct ImageEditConfig {
    pub mask: Option<AssetRef>,
    pub regions: Vec<ImageRegion>,
    pub strength: Option<f32>,
    pub preserve_input_aspect_ratio: Option<bool>,
}

pub enum ImageRegion {
    BboxPixels {
        image_index: usize,
        x1: u32,
        y1: u32,
        x2: u32,
        y2: u32,
    },
    BboxNormalized {
        image_index: usize,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    },
    PolygonNormalized {
        image_index: usize,
        points: Vec<(f32, f32)>,
    },
}
```

Mapping examples:

- Crazyrouter: `mask` maps to multipart `mask`. `regions` are unsupported unless converted to a mask by a later image-processing feature; first implementation should reject regions for Crazyrouter.
- Aliyun Wan: pixel bboxes map to `bbox_list`.
- OpenRouter Recraft: `strength` maps to `image_config.strength`; text layout uses style config.
- Renderful: image-to-image support depends on selected model; adapter should consult model metadata where available.

### Style config

```rust
pub struct ImageStyleConfig {
    pub style: Option<String>,
    pub color_palette: Option<ColorPalette>,
    pub text_layout: Vec<TextLayout>,
    pub rgb_colors: Vec<RgbColor>,
    pub background_rgb_color: Option<RgbColor>,
    pub font_inputs: Vec<FontInput>,
    pub super_resolution_references: Vec<AssetRef>,
}
```

Mapping examples:

- Aliyun Wan: `color_palette` maps to `parameters.color_palette`.
- OpenRouter Recraft V3: `style`, `text_layout`, `rgb_colors`, and `background_rgb_color` map into `image_config`.
- OpenRouter Sourceful: `font_inputs` and `super_resolution_references` map into `image_config`.
- Other providers reject or adjust per compatibility policy.

### Execution config

```rust
pub struct GenerationExecutionConfig {
    pub prefer_async: Option<bool>,
    pub stream: Option<bool>,
    pub partial_images: Option<u8>,
    pub poll_interval: Option<Duration>,
    pub timeout: Option<Duration>,
    pub webhook_url: Option<String>,
    pub user: Option<String>,
}
```

Mapping examples:

- Crazyrouter: `stream` and `partial_images` map directly for image generation when supported.
- Aliyun: `prefer_async` selects async endpoints only for models that support async mode; old endpoint shape may differ by model family.
- OpenRouter: `stream` maps to Chat Completions streaming.
- Renderful: always async create/poll; `webhook_url` maps directly when supported.

### Output config

```rust
pub struct ImageOutputConfig {
    pub delivery: ImageOutputDelivery,
    pub signed_url_ttl: Option<Duration>,
    pub namespace: Option<String>,
    pub max_base64_bytes: Option<u64>,
}

pub enum ImageOutputDelivery {
    Url,
    Base64,
}
```

The public gateway response contains only `Url` or `Base64`. Provider output URLs are never returned as public outputs.

Default:

- `delivery = Url`
- `signed_url_ttl` from gateway config
- `max_base64_bytes` from gateway config

Base64 is useful for small inline assets, tests, and clients that cannot fetch URLs. It should be explicit because base64 inflates payload size and stresses streaming, logs, FFI, and memory.

## Output types

### ImageGenerationResponse

```rust
pub struct ImageGenerationResponse {
    pub id: String,
    pub provider: String,
    pub model: String,
    pub status: GenerationStatus,
    pub images: Vec<GeneratedImage>,
    pub usage: ImageUsage,
    pub option_adjustments: Vec<OptionAdjustment>,
    pub metadata: Value,
}
```

### GeneratedImage

```rust
pub struct GeneratedImage {
    pub id: String,
    pub mime_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub content: ImageOutput,
    pub sha256: Option<String>,
    pub bytes: Option<u64>,
    pub metadata: Value,
}

pub enum ImageOutput {
    Url(ImageUrlOutput),
    Base64(ImageBase64Output),
}

pub struct ImageUrlOutput {
    pub url: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub asset_id: String,
}

pub struct ImageBase64Output {
    pub data: String,
    pub mime_type: String,
}
```

`ImageUrlOutput.url` is an Orchest-controlled URL generated by the configured `AssetStore`. The first production store is Alibaba Cloud OSS, but the public response contract is storage-provider-neutral. It is not a provider URL.

`expires_at` describes the returned URL if it is signed. It may be `None` for public or CDN URLs. The underlying stored object is controlled by Orchest regardless of signed URL expiry.

### Generation status

```rust
pub enum GenerationStatus {
    Queued,
    Running,
    PersistingAssets,
    Completed,
    Failed,
    Cancelled,
}
```

Provider completion is not enough for `Completed`. A gateway job reaches `Completed` only after requested output delivery is ready.

### Events

```rust
pub enum ImageGenerationEvent {
    ProviderQueued { provider_job_id: String },
    ProviderRunning { provider_job_id: Option<String> },
    PartialImage { image: GeneratedImage },
    PersistingAssetsStart { count: u32 },
    AssetPersisted { asset_id: String, index: u32 },
    Completed { response: ImageGenerationResponse },
    Failed { error: AigcError },
}
```

Partial images follow the same public output rule. If a partial image is emitted publicly, it must be base64 or an Orchest-controlled URL. To keep latency low, first implementation may emit provider partial metadata internally and expose only final images unless `delivery=Base64` and size limits are satisfied.

## Asset persistence

### Public output rule

The gateway public API returns only:

1. Base64/data URL content.
2. Orchest-controlled URL backed by the configured asset store.

Provider URLs are internal ingestion sources. They may be stored in internal metadata for debugging and audit, but should not be serialized in public SDK responses by default.

### AssetStore

```rust
#[async_trait]
pub trait AssetStore: Send + Sync {
    fn provider_name(&self) -> &str;

    async fn put_stream(
        &self,
        input: AssetIngestSource,
        options: PutAssetOptions,
    ) -> Result<StoredAsset, AssetStoreError>;

    async fn signed_url(
        &self,
        asset_id: &str,
        ttl: Duration,
    ) -> Result<String, AssetStoreError>;
}
```

The storage API is intentionally not named after OSS. Alibaba Cloud OSS is the first production implementation, but the trait must be able to support S3-compatible storage, Cloudflare R2, MinIO, local development storage, or future internal asset services without changing the image gateway public API.

Initial production implementation:

```rust
pub struct OssAssetStore {
    // Alibaba Cloud OSS configuration.
}
```

Development/test implementations:

- `NoopAssetStore`, for tests that request `Base64` or inspect provider adapter output.
- `LocalAssetStore`, for integration tests and local playground usage.

### Stored asset

```rust
pub struct StoredAsset {
    pub asset_id: String,
    pub url: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub storage: StorageLocation,
    pub mime_type: String,
    pub bytes: u64,
    pub sha256: String,
    pub metadata: Value,
}

pub enum StorageLocation {
    Oss(OssObjectLocation),
    Local(LocalObjectLocation),
    External(Value),
}

pub struct OssObjectLocation {
    pub endpoint: String,
    pub bucket: String,
    pub object_key: String,
}

pub struct LocalObjectLocation {
    pub path: PathBuf,
}
```

`StorageLocation` is internal metadata, not a caller-facing dependency. Public callers should use `ImageUrlOutput.url` and `asset_id`.

### OSS implementation config

```rust
pub struct OssStorageConfig {
    pub endpoint: String,
    pub bucket: String,
    pub region: Option<String>,
    pub access_key_id: String,
    pub access_key_secret: String,
    pub public_base_url: Option<String>,
    pub signed_url_ttl: Duration,
    pub key_prefix: Option<String>,
}
```

Environment variable defaults for the OSS implementation should be explicit and storage-scoped:

- `ALIYUN_OSS_ENDPOINT`
- `ALIYUN_OSS_BUCKET`
- `ALIYUN_OSS_REGION`
- `ALIYUN_OSS_ACCESS_KEY_ID`
- `ALIYUN_OSS_ACCESS_KEY_SECRET`
- `ALIYUN_OSS_PUBLIC_BASE_URL`

Do not reuse DashScope API keys for OSS.

### Ingestion source

```rust
pub enum AssetIngestSource {
    Url {
        url: String,
        expected_expires_at: Option<DateTime<Utc>>,
    },
    DataUrl(String),
    Base64 {
        data: String,
        mime_type: String,
    },
    Bytes {
        bytes: bytes::Bytes,
        mime_type: String,
    },
}
```

The persistence layer streams data into the configured asset store. It should compute SHA-256, byte count, and content type during ingestion.

### Low-overhead rules

- Stream provider URL downloads into asset-store uploads. Avoid buffering full images.
- Use bounded concurrent persistence for multiple outputs. Default concurrency should be small, for example 2-4 assets.
- Do not decode or transform image pixels by default.
- Do not include base64 image payloads in default tracing output.
- Apply a hard `max_base64_bytes` limit when `delivery=Base64`.
- Reuse the shared reqwest client for provider calls and provider artifact downloads.
- Treat provider URL download failure as generation failure unless caller requested an internal debug mode.

### Object keys

Use deterministic, collision-resistant keys:

```text
{prefix}/images/{yyyy}/{mm}/{dd}/{job_id}/{index}-{sha256_prefix}.{ext}
```

If SHA-256 is only known after upload starts, use a temporary object key and finalize by copy/rename only if the selected storage backend makes that cheap. Otherwise use `job_id` plus random UUID in the object key and store SHA-256 metadata.

## Compatibility policy

```rust
pub enum CompatibilityPolicy {
    Coerce,
    Strict,
}
```

`Strict` returns errors for unsupported or ambiguous options.

`Coerce` prefers a working request and records changes:

```rust
pub struct OptionAdjustment {
    pub option: String,
    pub requested: Value,
    pub applied: Value,
    pub reason: String,
}
```

Examples:

- Crazyrouter `ImageQuality::Hd` -> `high`, adjustment recorded.
- Crazyrouter `ImageQuality::Standard` -> error even in coerce mode because docs say it is rejected and there is no safe equivalent unless caller allows a fallback quality.
- `ImageBackground::Transparent` on Crazyrouter -> error in strict mode; in coerce mode either reject or apply `Auto` only if gateway policy explicitly allows semantic fallback.
- Exact `Pixels` requested for OpenRouter model that only documents `aspect_ratio` -> coerce to aspect ratio only when dimensions map cleanly.
- `output_format=Png` for Aliyun Qwen may be accepted because docs describe PNG output; other formats should be strict errors unless model docs support them.

## Capability metadata

```rust
pub struct ImageModelCapabilities {
    pub operations: Vec<ImageOperation>,
    pub input_roles: Vec<ImageInputRole>,
    pub max_input_images: Option<u32>,
    pub max_outputs: Option<u32>,
    pub sizes: ImageSizeCapabilities,
    pub formats: Vec<ImageFormat>,
    pub quality: Vec<ImageQuality>,
    pub supports_streaming: bool,
    pub supports_async: bool,
    pub supports_webhook: bool,
    pub supports_seed: bool,
    pub supports_negative_prompt: bool,
    pub supports_prompt_extend: bool,
    pub supports_watermark: bool,
    pub supports_mask: bool,
    pub supports_regions: bool,
    pub source: CapabilitySource,
    pub provider_metadata: Value,
}
```

Capability sources:

- Provider metadata endpoint, when available and precise enough. Renderful and OpenRouter model lists are useful here.
- Static table maintained in the crate for documented provider/model behavior.
- Assumed capability only for best-effort coerce mode. Strict mode should not rely on assumptions.

## Provider mapping

### Crazyrouter adapter

Generate:

- `operation=TextToImage`
- endpoint `/v1/images/generations`
- JSON body
- `size`: `Auto` or `Pixels` only
- `count` -> `n`
- `quality`, `background`, `output_format`, `output_compression`, `moderation`, `stream`, `partial_images`, `user` map directly where valid

Edit:

- `operation=ImageToImage` or `EditImage`
- endpoint `/v1/images/edits`
- multipart body
- source/reference images -> `image[]`
- `mask` -> `mask`

Response:

- `data[].url` becomes internal `AssetIngestSource::Url`
- Gateway persists through `AssetStore` or returns base64

### Aliyun adapter

Qwen and Z-Image:

- endpoint `/api/v1/services/aigc/multimodal-generation/generation`
- `prompt` -> `input.messages[0].content[].text`
- inputs -> `content[].image`
- `size` -> `parameters.size` as `"W*H"`
- `count` -> `parameters.n`
- `negative_prompt`, `prompt_extend`, `watermark`, `seed` map into `parameters`

Wan:

- same multimodal endpoint for current sync calls
- `ResolutionTier` -> `parameters.size` such as `"1K"`, `"2K"`, `"4K"`
- `regions` -> `parameters.bbox_list` when pixel bboxes are supplied
- `color_palette`, `thinking_mode`, `enable_sequential` map from typed fields or `provider_options`

Response:

- `output.choices[].message.content[].image` URLs become internal ingestion sources
- URL expiration should be marked when known; docs state generated image URLs expire after 24 hours

### OpenRouter adapter

Request:

- endpoint `/api/v1/chat/completions` for first implementation
- prompt and inputs are serialized into `messages`
- `modalities` includes `"image"` and may include `"text"` when the model is known text+image
- `stream` maps directly
- `AspectRatio` and `ResolutionTier` map to `image_config.aspect_ratio` and `image_config.image_size`
- Recraft/Sourceful style fields map into `image_config`

Response:

- non-streaming: `choices[].message.images[]`
- streaming: `choices[].delta.images[]`
- image URL values are usually data URLs and become internal ingestion sources

### Renderful adapter

Request:

- endpoint `/api/v1/generations`
- `operation` maps to `type`
- `model`, `prompt`, `webhook_url` map directly
- inputs requiring URL are uploaded through `/api/v1/uploads` or resolved from existing stored assets
- model capabilities may be read from `/api/v1/models?type=...`

Response:

- `id` becomes provider job id
- `status` maps to gateway status
- `outputs[]` URLs become internal ingestion sources

## Tool surface

Core tools should be capability tools, not provider tools:

- `image_generate`
- `image_edit`

The tool schema should expose the normalized request fields and keep provider/model selection in runtime config by default. Advanced users may pass explicit provider/model only when application policy allows it.

Tool output should be normalized `ImageGenerationResponse`. It must not include provider raw URLs.

## Error handling

```rust
pub struct AigcError {
    pub message: String,
    pub code: Option<String>,
    pub provider: Option<String>,
    pub status: Option<u16>,
    pub upstream_code: Option<String>,
    pub upstream_message: Option<String>,
    pub upstream_body: Option<Value>,
}
```

Error codes should include:

- `missing_api_key`
- `invalid_api_key`
- `unknown_provider`
- `unknown_model`
- `unsupported_operation`
- `unsupported_option`
- `invalid_request`
- `provider_http_error`
- `provider_task_failed`
- `asset_download_failed`
- `asset_upload_failed`
- `output_too_large_for_base64`
- `timeout`

Provider response bodies should be preserved in errors for debugging, with secrets redacted.

## Observability

Provider adapters and gateway orchestration should emit spans:

- `aigc.image.create`
- `aigc.provider.request`
- `aigc.provider.poll`
- `aigc.asset.persist`
- `aigc.asset.signed_url`

Metrics:

- request duration
- provider first-output latency when streaming
- provider total duration
- asset persistence duration
- generated image count
- persisted bytes
- provider error count by code/status
- storage error count by code

Do not log base64 payloads or signed URLs by default.

## Testing strategy

Unit tests:

- Request mapping per provider.
- Response parsing per provider.
- Compatibility policy adjustment behavior.
- Capability metadata for documented models.
- Asset output public contract: only URL or base64.
- Provider raw URL is not serialized in public response.
- Base64 max size enforcement.

Integration tests:

- Mock provider URL streamed into local asset store.
- Renderful async create/poll lifecycle with mocked HTTP.
- Aliyun expiring URL persisted to local asset store.
- Crazyrouter multipart edit request body construction.
- OpenRouter streaming `delta.images` parsing.

Workspace checks when implementation begins:

```bash
cargo test -p agent-runtime-aigc-providers
cargo clippy -p agent-runtime-aigc-providers -- -D warnings
cargo fmt --check
```

## Open questions

- Which URL style should the first OSS implementation use by default: signed OSS URL, public bucket URL, or CDN/custom domain in front of OSS?
- Should partial streaming images be exposed publicly in v1, or should v1 emit only final persisted images?
- Should the first tool schema allow explicit provider/model override, or should provider/model be runtime-only for safety and consistency?
- Should `output_format` be treated as desired final stored object format or only provider-native generation format? First implementation should treat it as provider-native only and avoid post-processing.
