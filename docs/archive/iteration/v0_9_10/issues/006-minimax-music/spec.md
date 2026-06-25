# 006 · Minimax Music(aigc 子模块)

## 背景

Minimax 3 个音乐 API(music_generation / lyrics_generation / music_cover_preprocess)有强逻辑
耦合(歌词 + 翻唱预处理 → 生成),且 hex/url 音频输出与图片/视频生成同属"生成型媒体"。

**决策变更(PRD delta 2)**:设计文档 §四 主张新建 `agent-runtime-music-providers` crate,
**本迭代改为放进 `agent-runtime-aigc-providers` 子模块** —— Step 3 要合 crate(见
[`docs/todo/provider-unification.md`](../../../../todo/provider-unification.md)),现在不加新 crate。
单开 `MusicProvider` trait 文件,**不污染** `ImageProvider`/`VideoProvider`;复用 aigc 的
`AigcError` / storage / asset 持久化。

设计来源:[`minimax-api-analysis.md`](../../../../research/minimax-api-analysis.md) §四。

## 6a. 模块布局(aigc 内)

```
crates/agent-runtime-aigc-providers/src/music/
├── mod.rs       # MusicProvider trait + 请求/响应 types
└── minimax.rs   # MinimaxMusicAdapter impl MusicProvider
```

- lib.rs:`pub mod music;` + re-export `MusicProvider`,加 `create_music_provider_from_config`。
- catalog:music 模型条目(`music-2.6` / `music-cover` / `music-2.6-free` / `music-cover-free`)。
- error:复用 `AigcError`(不新建 `MusicError`)。

## 6b. MusicProvider trait

```rust
#[async_trait]
pub trait MusicProvider: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    async fn generate(&self, req: GenerateMusicRequest) -> Result<GenerateMusicResult, AigcError>;
    async fn stream_generate(&self, req: GenerateMusicRequest) -> Result<MusicStream, AigcError>;
    async fn generate_lyrics(&self, req: GenerateLyricsRequest) -> Result<GenerateLyricsResult, AigcError>;
    async fn preprocess_cover(&self, req: CoverPreprocessRequest) -> Result<CoverPreprocessResult, AigcError>;
}
```

## 6c. 三个 API(字段表见设计文档 §4.3-4.5)

- **generate** `POST /v1/music_generation`:`model` / `prompt` / `lyrics` / `stream` /
  `output_format(url|hex,默认 hex;stream 时仅 hex)` / `audio_setting` / `aigc_watermark`;
  model 专属 `lyrics_optimizer` / `is_instrumental`(music-2.6)、`audio_url|audio_base64` /
  `cover_feature_id`(music-cover)。`data.audio` hex 或 URL → URL 走 aigc storage 持久化,hex 解码为 bytes。
- **generate_lyrics** `POST /v1/lyrics_generation`:`mode(write_full_song|edit)` / `prompt` /
  `lyrics` / `title` → `song_title` + `style_tags` + 含 14 种结构标签的 `lyrics`。
- **preprocess_cover** `POST /v1/music_cover_preprocess`:`model=music-cover` + `audio_url|audio_base64`
  → `cover_feature_id`(24h 有效) + `formatted_lyrics` + `structure_result` + `audio_duration`。

## 验收标准

- [ ] `src/music/{mod.rs,minimax.rs}` 存在,`MusicProvider` trait 在 aigc 内,未改 `ImageProvider`/`VideoProvider`
- [ ] error 复用 `AigcError`,**未**新建 `agent-runtime-music-providers` crate
- [ ] lib.rs 导出 `MusicProvider` + `create_music_provider_from_config`,catalog 含 4 个 music 模型
- [ ] `generate(music-2.6, prompt+lyrics, output_format=url)` 请求体正确,URL 输出走 storage 持久化(单元测试断言)
- [ ] `generate` hex 输出解码为 bytes(单元测试)
- [ ] `stream_generate(stream=true)` 走 hex chunk 路径(`output_format` 强制 hex)
- [ ] `generate_lyrics(write_full_song, prompt)` 请求体正确,响应解析出 `song_title`/`style_tags`/`lyrics`
- [ ] `preprocess_cover(audio_url)` 请求体正确,解析出 `cover_feature_id`
- [ ] `cargo test -p agent-runtime-aigc-providers` 全绿;`clippy -- -D warnings` 无 warning

> Live(手动,记录验证报告):lyrics → generate(url) 下载 → stream chunk → cover 全链路
> (设计文档 §4.6)。
