# 003 实现路线

## 要读的现有代码

- `crates/agent-runtime-asr-providers/src/streaming.rs` — stream handle style
- `crates/agent-runtime-asr-providers/tests/streaming.rs` — async stream tests and timeout style
- `docs/archive/iteration/v0_9_3/prd.md` — streaming lifecycle rules

## 步骤

### 1. 实现 stream handle ergonomics

- 保持 owned `mpsc::Receiver<TtsStreamEvent>` 可供调用方直接使用
- 如本地 crate 风格允许，补 `next()` / collect-until-terminal 测试 helper
- 让测试 helper 使用 timeout，避免死锁静默挂住

### 2. 实现 gateway stream forwarding wrapper

- Gateway wrapper 先发 `RouteSelected`
- Forward provider events，并在可行处 enforcing terminal behavior
- Public receiver dropped 时停止 forwarding

### 3. 实现 duplex input lifecycle

- Track 是否已经发送 final text chunk
- final 后再次 send 返回稳定 caller error 或关闭 sender
- sender 在 final 前被 drop 时，除非 provider 已正常 terminal，否则视为 cancellation

### 4. Backpressure 和 cancellation 测试

- 使用小容量 bounded channels
- 断言 stream handle 返回不依赖 caller 预先 drain provider-internal channel
- drop event receiver 后，用 timeout 断言后台任务退出

### 5. Error semantics 测试

- Non-fatal error 后仍可继续 audio/completion
- Fatal error 后不再 completion/audio
- Completed 后不再 fatal/audio

## 验证

```bash
cargo test -p agent-runtime-tts-providers --no-default-features streaming
cargo test -p agent-runtime-tts-providers
cargo clippy -p agent-runtime-tts-providers -- -D warnings
cargo fmt --check
```

## 关键决策

- 不实现 "duplex via buffer then single-stream" fallback
- Cancellation 对外部 provider 是 best-effort，但本地 task 不能永久泄漏
- Event validation 用测试固化，而不是只靠文档描述
