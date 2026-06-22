# 005 · Minimax Voice Management

## 背景

Minimax 3 个音色管理 API(Voice Clone / Voice Design / Delete Voice)不属于"合成"语义,
**不应塞进 `TtsProvider`**(`traits.rs` 只覆盖 synthesize/stream/duplex/list_voices)。新增独立
`VoiceManager` trait,`MinimaxTtsAdapter` 同时 impl `TtsProvider`(004)和 `VoiceManager`。

依赖 004:Voice Clone 用 004 的 `files::upload_file`(原音频 + 示例音频上传)。

依赖确认:`VoiceKind`(`types.rs:157`)的 `System`/`Cloned`/`Designed`/`Custom`、
`VoiceInfo`(`types.rs:183`)已存在,直接复用。决策见设计文档 §七 Q2-A(独立 trait,只 Minimax 实现)。

设计来源:[`minimax-api-analysis.md`](../../../../research/minimax-api-analysis.md) §3.5。

## 5a. VoiceManager trait(traits.rs 同文件)

```rust
#[async_trait]
pub trait VoiceManager: Send + Sync {
    async fn clone_voice(&self, req: CloneVoiceRequest) -> Result<CloneVoiceResponse, TtsError>;
    async fn design_voice(&self, req: DesignVoiceRequest) -> Result<DesignVoiceResponse, TtsError>;
    async fn delete_voice(&self, voice_id: &str, kind: VoiceKind) -> Result<(), TtsError>;
}
```

## 5b. 请求/响应类型(types.rs,字段表见设计文档 §3.5)

- `CloneVoiceRequest`:`file_id` / `voice_id` / `clone_prompt?` / `trial_text?` + `trial_model?`
  (两者同时存在才合成试听)/ `language_boost?` / `need_noise_reduction` / `need_volume_normalization` /
  `aigc_watermark`(后三默认 false)。`ClonePrompt { prompt_audio: u64, prompt_text: String }`。
- `CloneVoiceResponse { voice: VoiceInfo, demo_audio: Option<String>, input_sensitive: u8 }`
  (`demo_audio` 仅当 trial_text+trial_model 都给;`input_sensitive` 风控 0-7,0=通过)。
- `DesignVoiceRequest`:`prompt` / `preview_text`(≤500 字符)/ `voice_id?`(不传自动生成)。
- `DesignVoiceResponse { voice: VoiceInfo, trial_audio: AudioData }`(hex 试听音频)。

## 5c. impl 与映射

`providers/minimax/voice.rs`:`impl VoiceManager for MinimaxTtsAdapter`。
- `clone_voice`:POST `/v1/voice_clone`(`voice_clone/clone.md`)。
- `design_voice`:POST `/v1/voice_design` → `VoiceInfo{kind:Designed}` + `trial_audio`(hex 解码)。
- `delete_voice`:POST `/v1/delete_voice`,`VoiceKind` 映射:
  - `Cloned` → `voice_type:"voice_cloning"`
  - `Designed` → `voice_type:"voice_generation"`
  - `System` / `Custom` → `Err(TtsError{code: UnsupportedOperation, ..})`(系统音色不可删)

> 复刻音色 7 天内未调用会被系统删除(`clone.md:8`)—— 记入 `CloneVoiceResponse` 或 doc 注释。

## 验收标准

- [ ] `VoiceManager` trait 在 `traits.rs`,`MinimaxTtsAdapter` 同时 impl `TtsProvider` + `VoiceManager`
- [ ] `CloneVoiceRequest` / `ClonePrompt` / `CloneVoiceResponse` / `DesignVoiceRequest` / `DesignVoiceResponse` 类型如 5b
- [ ] `clone_voice` 请求体正确;`trial_text`+`trial_model` 都给时响应含 `demo_audio`,否则为 `None`(单元测试)
- [ ] `design_voice` 解析 `voice_id` + `trial_audio`(hex → `AudioData`)
- [ ] `delete_voice`:`Cloned`→`voice_cloning`、`Designed`→`voice_generation`、`System`/`Custom`→`UnsupportedOperation`(单元测试覆盖 4 个 VoiceKind)
- [ ] 复用 004 的 `files::upload_file`,无重复上传实现
- [ ] `cargo test -p agent-runtime-tts-providers --features minimax` 全绿;`clippy --features minimax -- -D warnings` 无 warning

> Live(手动,记录验证报告):upload → clone(带 trial)→ 新 voice_id 跑 sync 合成;
> design → 合成;delete 后合成应失败(设计文档 §3.6)。
