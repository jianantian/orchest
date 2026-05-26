# 004 实现路线

## v0.7 依赖判断

不依赖 v0.7。Aliyun adapter 是独立 provider adapter，只依赖 v0.6.1 的 001 公共类型、shared HTTP client 和 `AssetIngestSource` provider 边界。

## 步骤

1. **建立 Aliyun provider 模块**
   - 新建 `crates/agent-runtime-aigc-providers/src/providers/aliyun.rs`
   - 在 `providers/mod.rs` 显式导出 `AliyunImageAdapter` 和 config
   - config 字段包括 model、api_key、region/endpoint、api_url override、timeout

2. **实现 endpoint 和 region 选择**
   - 支持北京、新加坡、Virginia 风格 endpoint
   - region 和 endpoint 只决定请求地址，不混用 API key
   - API URL override 优先级高于 region 默认 endpoint
   - HTTP 请求统一使用 crate shared client

3. **实现 Qwen 和 Z-Image 请求映射**
   - 当前 multimodal endpoint 使用 `input.messages[].content[]`
   - prompt 映射为 `text` content entry
   - URL/base64 image inputs 映射为 `image` content entry
   - `ImageSize::Pixels` 映射 `parameters.size`，格式为 `W*H`
   - count、negative prompt、prompt extend、watermark、seed 映射到 `parameters`
   - Z-Image 的 `prompt_extend` response metadata 保存在 provider metadata

4. **实现 Wan 请求映射**
   - Wan sync 请求使用文档确认的 multimodal endpoint
   - `ResolutionTier("1K" | "2K" | "4K")` 和 pixel size 映射 `parameters.size`
   - pixel bbox region edits 映射 `parameters.bbox_list`
   - color palette、thinking mode、sequential generation 使用 typed config 或 `provider_options`
   - per-operation capability 校验 Wan image input limits

5. **实现 async task 支持**
   - 只对文档明确支持 async 的模型/endpoint 开启 task create/poll
   - task status 映射 `ProviderGenerationStatus`
   - success output URLs 转为 `ProviderAsset`
   - 已知 24 小时 URL 过期写入 provider asset metadata
   - request id、usage、actual prompt、reasoning/prompt extension metadata 尽量保留

6. **写测试**
   - request mapping：Qwen text-to-image、Qwen edit/fusion、Z-Image、Wan text-to-image、Wan sequential generation、Wan bbox edit
   - async polling：success、failure、timeout、unsupported async model
   - response parsing：current multimodal responses 和 legacy async task responses
   - compatibility：unsupported format、unsupported region、input image limits、Strict/Coerce

7. **验收**
   - `cargo test -p agent-runtime-aigc-providers aliyun`
   - 确认 adapter 不调用 `AssetStore`
   - 确认 URL 过期只记录在 provider metadata/asset metadata，公共 URL 由 gateway 生成

## 关键决策

- Aliyun 不同模型族不是同一个 API 形状，代码按 Qwen/Z-Image/Wan 分支处理，避免把差异压平到错误抽象。
- 只实现官方文档确认的 async 行为；不对未知模型做猜测。
