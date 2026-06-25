# 006 · Minimax Music(aigc 子模块)— 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: `superpowers:executing-plans` /
> `superpowers:subagent-driven-development`。步骤用 checkbox 跟踪。

**Goal:** 在 aigc crate 内新增 `music` 模块 + `MusicProvider` trait + `MinimaxMusicAdapter`,
实现 generation / lyrics / cover 三路,复用 aigc 的 error / storage。**不新建 crate。**

**Architecture:** music 作为 aigc 内与 image/video 平行的第三个生成型 trait family;hex/url 输出
复用 storage 持久化。

**Tech Stack:** Rust, reqwest(json), serde_json, base64/hex 解码, aigc storage。

---

## 要读的现有代码

- `crates/agent-runtime-aigc-providers/src/lib.rs`(`create_*_provider_from_config` 形态、export)
- `crates/agent-runtime-aigc-providers/src/storage/mod.rs`(`AssetStore` 持久化 URL/bytes)
- `crates/agent-runtime-aigc-providers/src/types/mod.rs`(`AigcError`、`AssetRef`)
- `crates/agent-runtime-aigc-providers/src/providers/volcengine/video.rs`(HTTP + 错误处理写法)
- `docs/external/minimax/music/{generation,lyrics,cover}.md`

## 文件改动

- Add: `crates/agent-runtime-aigc-providers/src/music/mod.rs`(trait + types)
- Add: `crates/agent-runtime-aigc-providers/src/music/minimax.rs`(adapter)
- Modify: `crates/agent-runtime-aigc-providers/src/lib.rs`(`pub mod music;` + export + `create_music_provider_from_config`)
- Modify: `crates/agent-runtime-aigc-providers/src/catalog/...`(music 条目)

## 步骤

### 1. trait + types

- [ ] `music/mod.rs` 定义 `MusicProvider` trait(见 spec 6b)。
- [ ] 定义 `GenerateMusicRequest` / `GenerateLyricsRequest` / `CoverPreprocessRequest` 及对应
      `*Result`(字段表见设计文档 §4.3-4.5)。错误类型用 `AigcError`。
- [ ] `MusicStream` 用现有 stream 形态(参考 aigc 内 streaming 写法,hex chunk)。

### 2. MinimaxMusicAdapter

- [ ] `music/minimax.rs` impl `MusicProvider`,默认 URL `https://api.minimaxi.com`,鉴权 Bearer。
- [ ] `generate`:POST `/v1/music_generation`;`output_format=url` → storage 持久化;`hex` → 解码 bytes。
      model 专属字段(`lyrics_optimizer`/`is_instrumental` vs `audio_url`/`cover_feature_id`)按 model 分派。
- [ ] `stream_generate`:`stream=true` 强制 `output_format=hex`,逐 chunk push。
- [ ] `generate_lyrics`:POST `/v1/lyrics_generation`,解析 `song_title`/`style_tags`/`lyrics`。
- [ ] `preprocess_cover`:POST `/v1/music_cover_preprocess`,解析 `cover_feature_id` 等。

### 3. 注册 + catalog

- [ ] lib.rs `pub mod music;`、re-export `MusicProvider`、加 `create_music_provider_from_config`("minimax")。
- [ ] catalog 加 `music-2.6` / `music-cover` / `music-2.6-free` / `music-cover-free`。

### 4. 测试

- [ ] generate(url / hex)请求体 + 输出处理单元测试。
- [ ] lyrics / cover 请求体 + 响应解析单元测试。
- [ ] stream_generate hex chunk 路径测试。
- [ ] 断言未新建 crate、未改 `ImageProvider`/`VideoProvider`。

### 5. 验证

```bash
cargo test -p agent-runtime-aigc-providers
cargo clippy -p agent-runtime-aigc-providers -- -D warnings
cargo fmt --check
test ! -d crates/agent-runtime-music-providers && echo "OK: no new music crate"
```

Live(手动,记录验证报告):lyrics → generate(url) → 下载 → cover 全链路(设计文档 §4.6)。
