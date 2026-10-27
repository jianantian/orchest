# 001 implementation plan

## Files to read

- `crates/orchest-protocol/src/capability.rs`（`SessionInput`）
- `crates/orchest-protocol/src/stream.rs`（`SegmentRef`、`TranscriptUpdateKind`、`LifecycleEvent::EndOfSpeech`）
- `crates/orchest-provider-stream/src/asr/*.rs`、`omni.rs`（`SessionInput` 的 match 分支）
- `docs/adr/0006-voice-session-model.md` D2、D3、D6、D7

## Files to change

- `crates/orchest-protocol/src/capability.rs`：新增 `Finalize` 变体与文档
- `crates/orchest-protocol/src/stream.rs`：改写 `SegmentRef`、`EndOfSpeech` 注释
- `crates/orchest-provider-stream/src/asr/*.rs`、`omni.rs`、`tts/*`：`Finalize` 分支先返回 `UnsupportedOption`
- `CHANGELOG.md`

## Steps

1. 在 `SessionInput` 加 `Finalize`，写清语义与「不支持时返回 `UnsupportedOption`」。
2. 改写 `SegmentRef` 与 `EndOfSpeech` 的契约注释，引用 ADR-0006。
3. 给所有消费 `SessionInput` 的方言补 `Finalize` 分支，统一返回 `UnsupportedOption`，各加一条单元测试。
4. 跑 `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --check`；更新 `CHANGELOG.md`。
