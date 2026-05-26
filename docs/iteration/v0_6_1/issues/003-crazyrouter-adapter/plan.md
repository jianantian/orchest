# 003 实现路线

## v0.7 依赖判断

不依赖 v0.7。Crazyrouter adapter 只依赖 v0.6.1 的 001/002 类型与 storage ingest 类型，不接入 runtime Hook 或 tool registry。

## 步骤

1. **建立 provider 模块**
   - 新建 `crates/agent-runtime-aigc-providers/src/providers/crazyrouter.rs`
   - 在 `providers/mod.rs` 显式导出 `CrazyrouterImageAdapter` 和 config
   - config 字段包括 model、api_key、api_url override、timeout

2. **实现 adapter 构造与能力**
   - `CrazyrouterImageAdapter` 实现 `ImageProvider`
   - `provider_name()` 返回稳定 provider id
   - `model_name()` 返回配置模型
   - `capabilities()` 返回 GPT Image generation/edit 能力，范围只写文档确认的字段
   - 默认 image base URL 使用 Crazyrouter 文档中的 image route host
   - HTTP 请求统一使用 crate shared client

3. **实现 text-to-image 请求映射**
   - `TextToImage` 映射到 `/v1/images/generations`
   - `prompt` 映射 Crazyrouter `prompt`
   - `count` 映射 `n` 并校验文档范围
   - `ImageSize::{Auto, Pixels}` 映射 `size`
   - quality、background、output format、compression、moderation、stream、partial images、user 只在文档确认时映射
   - unsupported 字段：Strict 报稳定错误，Coerce 记录 `OptionAdjustment`

4. **实现 edit 请求映射**
   - `ImageToImage` 和 `EditImage` 映射 `/v1/images/edits`
   - source/reference images 映射 multipart `image[]`
   - mask 映射 multipart `mask`
   - region edit 在 mask synthesis 实现前拒绝
   - 校验文档中的 reference image 最大数量

5. **实现响应和 streaming 解析**
   - `data[].url` 转为 `ProviderAsset { source: AssetIngestSource::Url, ... }`
   - adapter 不调用 `AssetStore`
   - adapter 不返回 `GeneratedImage`
   - streaming partial image 如文档和 fixture 支持，转为 `ProviderImageEvent::PartialAsset`

6. **写测试**
   - generation request construction
   - single-image edit multipart
   - mask edit multipart
   - multi-reference edit multipart
   - compatibility：`quality=hd`、`quality=standard`、transparent background、png compression、unsupported style/input fidelity
   - response parsing：单 URL 和多 URL
   - streaming parser：仅在有文档 fixture 时覆盖 partial image events

7. **验收**
   - `cargo test -p agent-runtime-aigc-providers crazyrouter`
   - 确认 adapter 代码中没有 `AssetStore` 调用
   - 确认 public response 类型没有从 adapter 返回

## 关键决策

- 不猜测 Crazyrouter 未文档化字段；provider-specific escape hatch 走 `provider_options`，并在 Strict/Coerce 下有明确行为。
- provider 原始 URL 只作为 `AssetIngestSource::Url` 留给 gateway 持久化，不直接暴露给调用方。
