# 005 · Minimax Voice Management — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: `superpowers:executing-plans` /
> `superpowers:subagent-driven-development`。步骤用 checkbox 跟踪。

**Goal:** 新增 `VoiceManager` trait + Minimax clone/design/delete 实现,复用 004 的上传 helper。

**Architecture:** 独立 trait(不污染 `TtsProvider`);`MinimaxTtsAdapter` 双 impl。

**Tech Stack:** Rust, reqwest(json), serde_json, hex。

---

## 要读的现有代码

- `crates/agent-runtime-tts-providers/src/traits.rs`(`TtsProvider` 同文件加 `VoiceManager`)
- `crates/agent-runtime-tts-providers/src/types.rs`(`VoiceKind:157` / `VoiceInfo:183` / `AudioData:292`)
- `crates/agent-runtime-tts-providers/src/providers/minimax/{mod,files}.rs`(004 产物)
- `docs/external/minimax/voice_clone/clone.md`、`voice_design.md`、`delete_voice.md`

## 文件改动

- Modify: `crates/agent-runtime-tts-providers/src/traits.rs`(`VoiceManager` trait)
- Modify: `crates/agent-runtime-tts-providers/src/types.rs`(请求/响应类型)
- Add: `crates/agent-runtime-tts-providers/src/providers/minimax/voice.rs`
- Modify: `crates/agent-runtime-tts-providers/src/providers/minimax/mod.rs`(`mod voice;` + `impl VoiceManager`)

## 步骤

### 1. trait + types

- [ ] `traits.rs` 加 `VoiceManager`(spec 5a)。
- [ ] `types.rs` 加 `CloneVoiceRequest` / `ClonePrompt` / `CloneVoiceResponse` /
      `DesignVoiceRequest` / `DesignVoiceResponse`(spec 5b),serde 注意
      `skip_serializing_if = "Option::is_none"`。

### 2. voice.rs impl

- [ ] `clone_voice`:POST `/v1/voice_clone`,组装可选字段;解析 `demo_audio`(条件)/ `input_sensitive`。
- [ ] `design_voice`:POST `/v1/voice_design`,解析 `voice_id` + `trial_audio`(hex 解码,复用 004 `protocol::decode_audio`)。
- [ ] `delete_voice`:POST `/v1/delete_voice`,`VoiceKind` 映射(spec 5c);`System`/`Custom` 直接返回
      `UnsupportedOperation` 不打网络。
- [ ] `mod.rs` 加 `mod voice;` 并确保 `MinimaxTtsAdapter` 暴露 `VoiceManager` impl。

### 3. 测试

- [ ] clone 请求体:有/无 trial 两种情况,响应 `demo_audio` 对应 Some/None。
- [ ] design 响应解析(voice_id + trial_audio)。
- [ ] delete 4 个 `VoiceKind` 分支(2 个映射 voice_type、2 个 `UnsupportedOperation`)。

### 4. 验证

```bash
cargo test -p agent-runtime-tts-providers --features minimax
cargo clippy -p agent-runtime-tts-providers --features minimax -- -D warnings
cargo fmt --check
```

Live(手动,记录验证报告):upload → clone(trial)→ 合成;design → 合成;delete 后失败
(设计文档 §3.6)。
