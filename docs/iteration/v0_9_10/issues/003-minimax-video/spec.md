# 003 · Minimax Video provider

## 背景

Minimax 5 个 video 生成变体(T2V / I2V / Frame2V / Subject Ref + 状态查询)都打到
`POST /v1/video_generation`,只是 `model` 字段和必填字段不同 —— 这正是 aigc 的
"提交 task → 轮询 → 下载"模式,与 `VolcengineVideoAdapter` 形态完全一致。

现有类型已覆盖,**types 层零改动**:
- `VideoProvider` trait(`crates/agent-runtime-aigc-providers/src/types/video.rs:163`)的
  `create_video_generation` + `get_video_generation` 匹配 Minimax 提交+轮询。
- `VideoContentItem`(`types/video.rs:42-60`)的 `Text` / `Image{role}` 覆盖 4 个变体输入。
- `VideoImageRole`(`types/video.rs:62-67`)的 `FirstFrame`/`LastFrame`/`ReferenceImage` 对齐
  I2V `first_frame_image` / Frame2V `last_frame_image` / Subject Ref `subject_reference`。
- `VideoGateway::generate`(`gateway/video.rs`)已封装"创建 → `wait_for_completion` 轮询 → 下载持久化"。
- `AssetRef`(`types/common.rs:14-21`,**6 个 variant**)的全部输入形态都需具体化到 Minimax
  请求体的 URL 或 base64(见 §3e 映射)。

设计来源:[`minimax-api-analysis.md`](../../../../research/minimax-api-analysis.md) §五。
模板:`crates/agent-runtime-aigc-providers/src/providers/volcengine/video.rs`。

## 3a. 新建 adapter

`crates/agent-runtime-aigc-providers/src/providers/minimax.rs`,impl `VideoProvider`,以
`volcengine/video.rs` 为模板。`VideoProvider` 实现只负责把 `Vec<VideoContentItem>` 按 `role`
解构,挑对应字段填 Minimax 请求体。

注册:`providers/mod.rs` 声明 + lib.rs `create_video_provider_from_config` 加 `"minimax"` 分支。

可用模型(`video/t2v.md:64-105` 等):Hailuo-2.3 / 2.3-Fast / Hailuo-02 / T2V-01-Director / T2V-01 /
I2V-01-Director / I2V-01-live / I2V-01 / S2V-01。catalog video 条目据此加。

## 3b. minimax 专属字段进 provider_options

`prompt_optimizer`(默认 true,`t2v.md:77`)、`fast_pretreatment`(仅 Hailuo-2.3/2.3-Fast/02,
`t2v.md:80`)走 `provider_options` JSON,**不污染共享 `VideoGenerationConfig`**。
通用字段(`duration` / `resolution` / `aigc_watermark` 等)用现有 config 字段(设计文档 §5.3 映射表)。

## 3c. 状态查询与下载

- `get_video_generation`:`GET /v1/query/video_generation?task_id=xxx`(`video/status.md`)。
- 状态映射到 `ProviderGenerationStatus`(`types/common.rs`):
  `Preparing`/`Queueing` → `Queued`;`Processing` → `Running`;`Success` → `Completed`;`Fail` → `Failed`。
  (Minimax 无显式 timeout 状态;`TimedOut` 由 gateway 轮询超预算时本地产生。)
- `Success` 带 `file_id` → `GET /v1/files/retrieve` 拿 `download_url`(有效期 1 小时)→ gateway
  asset 持久化在过期前完成(`video/retrive.md:90-93`)。

## 3d. 非目标

`callback_url` webhook **不实现**(PRD 非目标 / 设计文档 §5.5 / §七 Q4):v0.9.10 只做轮询。

## 3e. `AssetRef` → Minimax 输入映射(全 6 variant)

Minimax 图片输入(I2V `first_frame_image` / Frame2V `last_frame_image` / Subject Ref
`subject_reference`)接受 URL 或 `data:<mime>;base64,<...>` 字符串。`AssetRef` 6 个 variant
逐一具体化:

| `AssetRef` | 处理 |
|---|---|
| `Url(s)` | 直传 `s`(Minimax 拉取) |
| `DataUrl(s)` | 直传 `s`(已是 `data:<mime>;base64,...`) |
| `Base64 { data, mime_type }` | 拼成 `data:<mime_type>;base64,<data>` |
| `Bytes { bytes, mime_type }` | base64 编码 `bytes` → 拼 `data:<mime_type>;base64,<...>` |
| `LocalPath(p)` | `tokio::fs::read(p)` → 推断 mime(扩展名;失败时 `application/octet-stream` 并报 `AigcError`)→ 同 `Bytes` 路径 |
| `Stored { asset_id }` | `AssetRegistry::get` 取 `StoredAsset`,再 `AssetStore::signed_url(asset, ttl)` 拿短期外链 URL,走 `Url` 路径;若 store 无法签发外链(例如 `NoopAssetStore`),返回 `AigcError::UnsupportedOperation`("Minimax 视频图片输入需要可外网访问的 URL 或内联 base64") |

错误形态:`LocalPath` 读失败或 `Stored` 无法解析为外链均返回 `AigcError`(不 panic)。Minimax video 上传的 `signed_url` 推荐 ttl ≥ 1h(轮询 + 下载窗口对齐 §3c 的 `download_url` 1 小时过期)。

## 验收标准

- [ ] `providers/minimax.rs` impl `VideoProvider`,注册进 `create_video_provider_from_config` 的 `"minimax"` 分支
- [ ] catalog video 含上述 Minimax 模型条目
- [ ] T2V:`Vec<VideoContentItem>` 仅 `Text` → 请求体含 `prompt` + `model`,单元测试断言请求 JSON
- [ ] I2V:`Image{role:FirstFrame}` → `first_frame_image`;Frame2V:加 `Image{role:LastFrame}` → `last_frame_image`
- [ ] Subject Ref:`Image{role:ReferenceImage}` + S2V-01 → `subject_reference`
- [ ] `prompt_optimizer` / `fast_pretreatment` 经 `provider_options` 进请求体,**不**出现在共享 config 字段
- [ ] 状态映射单元测试:`Preparing`/`Queueing`/`Processing`/`Success`/`Fail` → 对应 `ProviderGenerationStatus`
- [ ] `AssetRef` 6 variant 全部具体化的单元测试(`Url`/`DataUrl` 直传;`Base64`/`Bytes`/`LocalPath` 拼 `data:` URI;`Stored` 走 `AssetRegistry::get` + `AssetStore::signed_url` 或返回 `UnsupportedOperation`)
- [ ] 无 `callback_url` webhook 代码
- [ ] `cargo test -p agent-runtime-aigc-providers` 全绿;`clippy -- -D warnings` 无 warning

> Live(手动,记录验证报告):`VideoGateway::generate(prompt, model="MiniMax-Hailuo-02")` 拿到本地
> asset URL;I2V / Frame2V / Subject Ref 各跑一次(设计文档 §5.6)。
