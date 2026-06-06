# 003 · Duplex streaming contract

GitHub: [#111](https://github.com/jianantian/orchest/issues/111)

## 背景

实现双工流式合约：`AsrStream` 的两种使用模式、flush/end/cancel 生命周期、事件排序、超时处理。

## 契约

### 输入

- 001 定义的 stream/event types
- 002 的 gateway 框架

### 输出

- `AsrStream` 完整实现，支持 application-driven segmentation 和 full-duplex 两种模式
- Event ordering 保证
- Flush timeout 机制
- Sink-drop cancellation 语义

## 影响范围

### 修改文件

- `crates/agent-runtime-asr-providers/src/streaming.rs` — `AsrStream`, `AsrAudioSink`, `AsrEventStream` 实现

### 新增/修改测试

- `crates/agent-runtime-asr-providers/tests/fake_provider.rs` — 扩展 fake provider 支持 streaming 路径

## 实现要点

### 两种互斥使用模式

1. **Owned convenience** — `AsrStream` 整体使用 `flush_and_wait_final()` / `end_and_wait_final()`
2. **Split full-duplex** — `split(self)` 消耗 stream，返回 `(AsrAudioSink, AsrEventStream)` 各自独立

`split(self)` 后 owned 方法不可用，类型系统保证。

### AsrAudioSink

- `send_audio(data, timestamp_ms)` — 发送音频 chunk
- `flush_segment()` — 发送 `AudioChunkBoundary::Flush`，触发 provider last-frame，不关流
- `end_stream()` — 发送 `AudioChunkBoundary::End`，触发 provider last-frame + stream teardown

### AsrEventStream

- `next()` — 返回下一个 `AsrStreamEvent`
- `next_final()` — 跳过非 final 事件，返回 `AsrFinalOutput` 或 error

### Sink-Drop Cancellation

Drop `AsrAudioSink` without prior `End`:
1. Adapter closes provider connection
2. Emits `Error { code: Cancelled, fatal: true }` when events channel still open
3. Pending `next_final()` returns `AsrErrorCode::Cancelled`

### Flush-to-Final Timeout

- `TranscribeOptions.flush_timeout` 限制 flush 后等 final 的时间
- 超时 → `AsrFinalOutput { reason: Timeout }` + 已有 committed text
- Provider 超时后的 late final → 抑制，不发 duplicate `AsrFinal`
- 超时后允许下一个 segment 继续（除非 provider connection 本身失败）

### Provider Endpointing

- Provider 可以不依赖 caller Flush 自行 emit `EndOfSpeech` + `AsrFinal { reason: ProviderEndpoint }`
- 同一 normalized segment：provider endpoint + caller Flush → 最多一个 `AsrFinal`
- `flush_and_wait_final()` 可以返回 provider-endpoint final 而非强制 caller-flush final

### FinalResultScope

- `Segment` — `TranscribeResult.text` 只含当前 segment
- `Stream` — `TranscribeResult.text` 累积所有 committed segments

### Event Ordering

```
RouteSelected → Started → (TranscriptUpdate | EndOfSpeech)* → AsrFinal (per segment) | Error (fatal)
```

- `TranscriptUpdate { stability: Committed }` ≠ `AsrFinal`
- `EndOfSpeech` ≠ `AsrFinal`
- 每个 normalized segment 最多一个 `AsrFinal`

### Committed Segment Deduplication

Adapters 必须在 emit `TranscriptUpdate { stability: Committed }` 前去重。Gateway 消费者只看到每个 committed segment 一次。

### Backpressure

Bounded channel + capacity 文档化。SDK 不 sleep、不合成静音、不 drop 静音、不选择 VAD 策略。

## 验收标准

- [ ] Public streaming API is duplex-first: `AsrAudioSink` + `AsrEventStream`
- [ ] `AsrStream::split()` consumes the stream and is mutually exclusive with convenience methods
- [ ] `flush_and_wait_final()` sends flush and returns `AsrFinalOutput` while keeping stream reusable
- [ ] `AudioChunkBoundary::Flush` finalizes without dropping sink
- [ ] `AudioChunkBoundary::End` finalizes then allows teardown
- [ ] Dropping `AsrAudioSink` without prior `End` → fatal `Cancelled` error, pending waits return `Cancelled`
- [ ] Flush timeout emits `AsrFinalOutput { reason: Timeout }` instead of blocking indefinitely
- [ ] Late provider final after timeout does not emit duplicate `AsrFinal`
- [ ] Provider endpointing emits `EndOfSpeech` + `AsrFinal { reason: ProviderEndpoint }` without caller Flush
- [ ] Same segment: provider endpoint + caller Flush → at most one `AsrFinal`
- [ ] `FinalResultScope::Segment` vs `Stream` controls `TranscribeResult.text`
- [ ] `AsrFinalOutput` carries trace ID, segment ID, `AsrFinalReason`, and `TranscribeResult`
- [ ] `AsrFinalReason` distinguishes caller flush, caller end, provider endpointing and timeout finalization
- [ ] Streaming event ordering is stable and tested
- [ ] No provider-internal channel drain deadlock
- [ ] Committed segment deduplication before public `TranscriptUpdate` emission
- [ ] SDK never synthesizes silence, drops silence, or chooses a client-side VAD policy
- [ ] `AudioChunk.timestamp_ms` preserves caller capture timing; adapters don't fabricate silence
- [ ] `cargo test -p agent-runtime-asr-providers` passes

## 依赖

- 001 Crate scaffold + public types (#109)
- 002 Gateway + router (#110)
