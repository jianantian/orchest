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
- `AssetRef::{Url, DataUrl}`(`types/common.rs`)覆盖图片输入(URL / `data:image/...;base64,`)。

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

## 验收标准

- [ ] `providers/minimax.rs` impl `VideoProvider`,注册进 `create_video_provider_from_config` 的 `"minimax"` 分支
- [ ] catalog video 含上述 Minimax 模型条目
- [ ] T2V:`Vec<VideoContentItem>` 仅 `Text` → 请求体含 `prompt` + `model`,单元测试断言请求 JSON
- [ ] I2V:`Image{role:FirstFrame}` → `first_frame_image`;Frame2V:加 `Image{role:LastFrame}` → `last_frame_image`
- [ ] Subject Ref:`Image{role:ReferenceImage}` + S2V-01 → `subject_reference`
- [ ] `prompt_optimizer` / `fast_pretreatment` 经 `provider_options` 进请求体,**不**出现在共享 config 字段
- [ ] 状态映射单元测试:`Preparing`/`Queueing`/`Processing`/`Success`/`Fail` → 对应 `ProviderGenerationStatus`
- [ ] `Success` 路径用 `file_id` 拉 `/v1/files/retrieve` 取 `download_url`(可用 fake HTTP 断言调用)
- [ ] 无 `callback_url` webhook 代码
- [ ] `cargo test -p agent-runtime-aigc-providers` 全绿;`clippy -- -D warnings` 无 warning

> Live(手动,记录验证报告):`VideoGateway::generate(prompt, model="MiniMax-Hailuo-02")` 拿到本地
> asset URL;I2V / Frame2V / Subject Ref 各跑一次(设计文档 §5.6)。
