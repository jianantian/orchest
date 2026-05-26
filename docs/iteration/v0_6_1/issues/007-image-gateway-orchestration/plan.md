# 007 实现路线

## v0.7 依赖判断

不依赖 v0.7。`ImageGateway` 是 `agent-runtime-aigc-providers` crate 内的编排层，不需要 Hook 框架。未来 v0.7 或之后的 core 只需要把它包装成 tool。

## 步骤

1. **建立 gateway 模块**
   - 新建 `crates/agent-runtime-aigc-providers/src/gateway.rs`
   - 在 `lib.rs` 显式导出 `ImageGateway`、gateway config、public helper
   - `ImageGateway` 持有 `ImageProvider`、`AssetStore`、`AssetRegistry` 和 gateway config

2. **实现请求校验**
   - 根据 selected provider 的 `ImageModelCapabilities` 校验 operation、输入数量、尺寸、格式、execution mode
   - Strict compatibility 遇到 unsupported field 返回稳定错误
   - Coerce compatibility 对安全调整记录 `OptionAdjustment`

3. **实现 provider 调用和 async polling**
   - gateway 调用 `ImageProvider::create_image_generation()`
   - 对 async job 按 poll interval 轮询 `get_image_generation()`
   - timeout/failure 返回 public error，同时保留 provider error details
   - provider event 和 public event 分开，public event 使用 `ImageGenerationEvent`

4. **实现资产持久化**
   - provider assets 返回后进入 `PersistingAssets`
   - 用 `AssetStore` 写入对象，并用 `AssetRegistry` 保存 `StoredAsset`
   - public response 在所有资产持久化完成后才返回 `Completed`
   - provider raw URL 默认不进入 public response

5. **实现 URL delivery**
   - `ImageOutputDelivery::Url` 返回 `GeneratedImage.asset_id` 和 `ImageUrlOutput`
   - `ImageUrlOutput.url` 必须是直接 fetchable URL
   - 过期信息只放 `expires_at`
   - 不包含 bucket、endpoint、object key、storage credentials、ACL 或签名逻辑

6. **实现 Base64 delivery**
   - `ImageOutputDelivery::Base64` 返回 `ImageBase64Output`
   - 默认仍持久化 asset，并返回 `GeneratedImage.asset_id`
   - Base64 payload 受 max bytes 配置约束

7. **实现 asset refresh**
   - 提供 `resolve_asset_url(scope, asset_id, ttl)` 或 gateway 等价方法
   - 正确 scope 返回新的 `ImageUrlOutput`
   - wrong scope 返回稳定错误，不泄露跨 scope asset 是否存在

8. **写 mock provider 测试**
   - mock provider + local asset store 端到端 text-to-image
   - mock async task：queued/running/completed/persisting/completed public flow
   - mock provider failure 保留 provider error
   - serialized public response 不包含 provider raw URL
   - URL output 直接可用且不需要 storage metadata
   - Base64 output 仍包含 `asset_id`
   - wrong-scope asset resolution rejected

9. **验收**
   - `cargo test -p agent-runtime-aigc-providers gateway`
   - 确认 public partial image events 默认关闭，除非能满足 public output contract

## 关键决策

- gateway 是唯一能把 provider asset 变成 public `GeneratedImage` 的层。
- `asset_id` 是长期稳定引用；URL 是短期可用访问方式，过期后通过 scoped resolve 刷新。
