# 003 · Minimax Video provider — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: `superpowers:executing-plans` /
> `superpowers:subagent-driven-development`。步骤用 checkbox 跟踪。

**Goal:** 在 aigc crate 加 `MinimaxVideoAdapter`(impl `VideoProvider`),5 个变体走同一
`POST /v1/video_generation`,轮询 + 下载复用现有 gateway。types 层零改动。

**Architecture:** 复制 `VolcengineVideoAdapter` 模式;minimax-only 字段进 `provider_options`。

**Tech Stack:** Rust, reqwest(json), serde_json, aigc gateway/storage。

---

## 要读的现有代码

- `crates/agent-runtime-aigc-providers/src/providers/volcengine/video.rs`(模板)
- `crates/agent-runtime-aigc-providers/src/types/video.rs`(`VideoProvider` / `VideoContentItem` / `VideoImageRole` / `VideoGenerationConfig`)
- `crates/agent-runtime-aigc-providers/src/types/common.rs`(`AssetRef` / `ProviderGenerationStatus`)
- `crates/agent-runtime-aigc-providers/src/gateway/video.rs`(`VideoGateway::generate` / `wait_for_completion`)
- `crates/agent-runtime-aigc-providers/src/lib.rs`(`create_video_provider_from_config`)
- `crates/agent-runtime-aigc-providers/src/providers/mod.rs`、`catalog/video.rs`
- `docs/external/minimax/video/{t2v,i2v,frame2v,refvideo,status,retrive}.md`

## 文件改动

- Add: `crates/agent-runtime-aigc-providers/src/providers/minimax.rs`
- Modify: `crates/agent-runtime-aigc-providers/src/providers/mod.rs`(声明 + re-export adapter/config)
- Modify: `crates/agent-runtime-aigc-providers/src/lib.rs`(`create_video_provider_from_config` 加 `"minimax"`)
- Modify: `crates/agent-runtime-aigc-providers/src/catalog/video.rs`(模型条目)

## 步骤

### 1. adapter 骨架

- [ ] 复制 `volcengine/video.rs` 结构,建 `MinimaxVideoAdapter` + `MinimaxVideoConfig`。
- [ ] `providers/mod.rs` 声明 `mod minimax;` 并 re-export。
- [ ] lib.rs `create_video_provider_from_config` 加 `"minimax"` 分支,默认 URL `https://api.minimaxi.com`,
      鉴权 `Authorization: Bearer`。

### 2. create_video_generation

- [ ] 解构 `Vec<VideoContentItem>`:`Text` → `prompt`;`Image{role:FirstFrame}` → `first_frame_image`;
      `LastFrame` → `last_frame_image`;`ReferenceImage` → `subject_reference`。
- [ ] `AssetRef` 6 variant 全部具体化为 URL 或 `data:<mime>;base64,...`(映射表见 spec §3e):
      `Url`/`DataUrl` 直传;`Base64`/`Bytes`/`LocalPath` 拼 `data:` URI(`LocalPath` 用 `tokio::fs::read`);
      `Stored` 走 `AssetRegistry::get` + `AssetStore::signed_url(asset, ttl)` 或返回 `AigcError::UnsupportedOperation`。
- [ ] 通用字段从 `VideoGenerationConfig` 取(`duration` / `resolution` / `aigc_watermark`);
      `prompt_optimizer` / `fast_pretreatment` 从 `provider_options` 取。
- [ ] POST `/v1/video_generation`,解析返回 `task_id`。

### 3. get_video_generation + 下载

- [ ] `GET /v1/query/video_generation?task_id=`,把 `status` 映射到 `ProviderGenerationStatus`
      (映射表见 spec 3c)。
- [ ] `Success` 时用 `file_id` 调 `GET /v1/files/retrieve` 取 `download_url`,交给 gateway 持久化。

### 4. catalog

- [ ] `catalog/video.rs` 加 Minimax video 模型条目(Hailuo-2.3 / 2.3-Fast / Hailuo-02 /
      T2V-01-Director / T2V-01 / I2V-01-Director / I2V-01-live / I2V-01 / S2V-01)。

### 5. 测试

- [ ] 4 变体请求体构造单元测试(用 fake/断言 JSON,不打真网络)。
- [ ] 状态映射单元测试。
- [ ] `provider_options` 字段进请求体、不进共享 config 的断言。

### 6. 验证

```bash
cargo test -p agent-runtime-aigc-providers
cargo clippy -p agent-runtime-aigc-providers -- -D warnings
cargo fmt --check
rg -n "callback" crates/agent-runtime-aigc-providers/src/providers/minimax.rs   # 应无命中
```

Live(手动,记录验证报告):T2V / I2V / Frame2V / Subject Ref 各一次,确认本地 asset 落库。
