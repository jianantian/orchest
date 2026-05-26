# 002 实现路线

## v0.7 依赖判断

不依赖 v0.7。资产存储是 AIGC gateway crate 内部能力，不需要 Hook 框架、core runtime 或 tool registry。未来 core tool 调用 gateway 时只使用公开 API 和 `asset_id`。

## 步骤

1. **建立 storage 模块结构**
   - 新建或完善 `crates/agent-runtime-aigc-providers/src/storage/mod.rs`
   - 拆分文件：`traits.rs`、`types.rs`、`registry.rs`、`local.rs`、`noop.rs`、`oss.rs`
   - `mod.rs` 只声明模块和显式 re-export，不放业务逻辑

2. **定义 storage trait 和核心类型**
   - `AssetStore` 定义 `put_stream()` 和 `signed_url()`
   - `signed_url(&StoredAsset, ttl)` 返回 `AssetAccessUrl { url, expires_at }`
   - `AssetRegistry` 定义 `save(scope, asset)` 和 `get(scope, asset_id)`
   - 定义 `AssetScope`，字段覆盖 tenant/workspace/app/namespace 类 scope
   - 定义 `StoredAsset`、`StorageLocation`、`OssObjectLocation`、`LocalObjectLocation`
   - 定义 `AssetIngestSource::{Url, DataUrl, Base64, Bytes}`
   - 定义 `PutAssetOptions`，包含 namespace/key_prefix、content type hints、persistence options

3. **实现 public resolution helper**
   - 增加 `resolve_asset_url(scope, asset_id, ttl)` 的 crate 内公共 helper 或 gateway 可复用函数
   - 返回 `ImageUrlOutput`，不返回 `StorageLocation`
   - wrong scope 返回稳定错误码，错误信息不能泄露另一个 scope 下 asset 是否存在

4. **实现 registry**
   - `InMemoryAssetRegistry` 用于单元测试和 mock gateway flow
   - local/file-backed registry 用 JSON 文件持久化，支持跨实例读取
   - 保存时记录 asset_id、scope、location、content_type、sha256、byte_count、created_at、expires_at

5. **实现 Noop 和 Local store**
   - `NoopAssetStore` 用于不需要持久化的测试，返回稳定错误或测试专用固定输出，行为要明确
   - `LocalAssetStore` 把对象写到 local root
   - Local URL 输出支持直接可用的 dev URL 或 path，由配置决定
   - provider URL ingestion 尽量 stream download 到 upload/write，不先读完整大文件

6. **实现 OSS store contract**
   - `OssAssetStore` 作为第一种生产实现
   - `OssStorageConfig` 包含 endpoint、bucket、region、access_key_id、access_key_secret、public_base_url、signed_url_ttl、key_prefix
   - 环境变量命名使用 storage scope，不能复用 DashScope API key
   - 没有真实凭证时，真实网络测试用 ignored test 或 feature-gated integration test

7. **实现低开销与安全约束**
   - ingest 过程计算 SHA-256、byte count、MIME type
   - 不做图片解码
   - Base64 ingest 按配置 max bytes 拒绝超限
   - tracing 字段不能包含大 payload、base64、signed URL、secret

8. **写测试**
   - local store put/resolve
   - in-memory registry save/get
   - file-backed registry 跨实例持久化
   - scoped lookup success 和 wrong-scope rejection
   - signed/non-expiring URL metadata
   - base64 max-size failure
   - public response helper 不暴露 storage descriptor

9. **验收**
   - `cargo test -p agent-runtime-aigc-providers storage`
   - `cargo clippy -p agent-runtime-aigc-providers -- -D warnings`

## 关键决策

- `asset_id` 是端侧和业务侧可持久化的稳定引用；signed URL 是短期访问凭证，过期后通过 `asset_id + scope` 刷新。
- 公共 API 只暴露可直接使用的 URL，不暴露 bucket、object key、endpoint 或签名参数构造能力。
- storage trait 保持 provider-neutral，OSS 只是短期第一实现。
