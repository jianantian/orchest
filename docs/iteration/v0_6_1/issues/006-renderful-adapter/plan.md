# 006 实现路线

## v0.7 依赖判断

不依赖 v0.7。Renderful adapter 虽然自身也是 gateway 型服务，但在 Orchest 中只作为普通 provider adapter 接入，不依赖主线 Hook 或 tool 抽象。

## 步骤

1. **建立 Renderful provider 模块**
   - 新建 `crates/agent-runtime-aigc-providers/src/providers/renderful.rs`
   - 在 `providers/mod.rs` 显式导出 `RenderfulImageAdapter` 和 config
   - config 字段包括 model、api_key、api_url override、timeout、optional webhook behavior
   - 主 API 使用 `/api/v1/generations`
   - 不把 legacy `/v1/predictions` 写入 public adapter contract
   - HTTP 请求统一使用 crate shared client

2. **实现 model metadata 解析**
   - 支持 `GET /api/v1/models?type=text-to-image`
   - 支持 `GET /api/v1/models?type=image-to-image`
   - capabilities 包含 aspect ratios、resolutions、max outputs、cost ranges、webhook support
   - metadata 缺失时按 compatibility policy 决定静态能力或 assumed capability

3. **实现 create request mapping**
   - `TextToImage` 映射 `type: "text-to-image"`
   - `ImageToImage` 和 `EditImage` 仅在所选模型文档支持时映射对应 task type
   - prompt、model、webhook URL 直接映射
   - 需要 URL 的 inputs 只接受 gateway/input preprocessing 后的 provider-usable URL 或 data URL
   - unresolved `LocalPath` 或 `Stored` refs 返回稳定 adapter error，不在 adapter 内调用 `AssetStore`
   - `Upscale`、`FaceSwap` 保留类型能力，但第一里程碑不默认暴露，除非显式启用

4. **实现 task lifecycle**
   - create 返回 provider job id 和 queued/running status
   - poll 把 `queued`、`processing`、`completed`、`failed` 映射为 provider statuses
   - completed `outputs[]` URLs 转为 provider asset ingest sources
   - failed task 保留 provider error details

5. **实现 input preprocessing 边界**
   - adapter 接受 URL/data URL 输入并映射到 Renderful 请求
   - adapter 遇到 unresolved local input 或 stored asset input 时返回稳定错误
   - local/stored 到 provider-usable URL 的转换由 gateway 负责，不在 adapter 内实现

6. **写测试**
   - model metadata parsing：text-to-image 和 image-to-image fixtures
   - create mapping：text-to-image、image-to-image
   - polling：queued、processing、completed、failed、timeout
   - input preprocessing：已解析 URL/data URL 输入成功，unresolved local/stored 输入稳定失败

7. **验收**
   - `cargo test -p agent-runtime-aigc-providers renderful`
   - 确认 adapter 返回 provider job/provider assets，不返回 public `GeneratedImage`

## 关键决策

- Renderful 是上游 provider，不是 Orchest gateway 的替代品；Orchest 仍负责最终资产持久化和 public output contract。
- 对需要 URL 的输入，adapter 只做上游调用所需准备，不把 storage 配置泄露给调用方。
