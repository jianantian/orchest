# 003 实现路线

## 要读的现有代码

- `crates/agent-runtime-providers/src/lib.rs` — `stream_chat()` 的 `(future, rx)` pair 模式，channel capacity=64
- `study_buddy/server/src/speech/asr.rs` — flush state machine (`FlushState`), flush timeout 处理, `AsrResultMessage` 的 partial/final 分层
- `study_buddy/server/src/speech/protocol.rs` — 理解 `build_audio_frame(data, is_last)` 的 last-frame 语义

## 步骤

### 1. 设计内部 channel 架构

`AsrStream` 需要支持两种使用模式，内部 channel 设计：

```
Caller → [AudioChunk channel] → Adapter task → Provider WebSocket
                                    ↓
Caller ← [AsrStreamEvent channel] ← Adapter task ← Provider WebSocket
```

- Audio input channel: `mpsc::Sender<AudioChunk>` / `mpsc::Receiver<AudioChunk>`，bounded capacity (建议 32，约 3.2s at 100ms chunks)
- Event output channel: `mpsc::Sender<AsrStreamEvent>` / `mpsc::Receiver<AsrStreamEvent>`，bounded capacity 64
- `AsrStream` 持有 sender half (audio) + receiver half (events)
- `split(self)` 把两个 half 分别包装为 `AsrAudioSink` / `AsrEventStream`

### 2. 实现 `AsrAudioSink` (`streaming.rs`)

替换 001 的 `todo!()`:

- `send_audio(data, timestamp_ms)` — construct `AudioChunk { data, timestamp_ms, boundary: None }`, send via channel. Channel closed → `AsrError { code: Cancelled }`
- `flush_segment()` — send `AudioChunk { data: Bytes::new(), timestamp_ms: None, boundary: Flush }`. Channel closed → error
- `end_stream()` — send `AudioChunk { data: Bytes::new(), timestamp_ms: None, boundary: End }`. Channel closed → error
- `Drop` impl: 如果没有 prior `End`，close sender half → adapter task 检测到 sender drop

### 3. 实现 `AsrEventStream` (`streaming.rs`)

- `next()` — `self.inner.recv().await`，None 时返回 None (stream ended)
- `next_final()` — loop `next()` 直到收到 `AsrStreamEvent::AsrFinal` 或 `AsrStreamEvent::Error { fatal: true }`。AsrFinal → extract `AsrFinalOutput`。Fatal error → `AsrError`。Stream ended without final → `AsrError { code: Cancelled }`

### 4. 实现 `AsrStream` convenience methods (`streaming.rs`)

- `split(self) -> (AsrAudioSink, AsrEventStream)` — move out fields
- `flush_and_wait_final(&mut self)`:
  1. `self.input.flush_segment().await?`
  2. `self.events.next_final().await`
- `end_and_wait_final(&mut self)`:
  1. `self.input.end_stream().await?`
  2. `self.events.next_final().await`

### 5. 实现 fake provider adapter task（self-contained，不抽共享 framework）

003 只在 fake provider 中实现 adapter task，不过早抽通用 framework。Volcengine 的二进制帧协议和 Aliyun 的 DashScope JSON 协议差异很大，过早抽象大概率不合适。等 005/006 各自实现后，如果有明确共性再提取。

Fake provider 的 adapter task 直接在 `tests/fake_provider.rs` 中实现，内部管理以下状态：

- `segment_finalized: bool` — 当前 segment 已发 AsrFinal
- `flush_pending: bool`
- `flush_started_at: Option<Instant>`
- `accumulated_text: String` (for FinalResultScope::Stream)
- `committed_segments: HashSet<(i32, i32, u64)>` (dedup key)

### 6. 实现 flush timeout 机制

- Adapter task 的 `tokio::select!` loop:
  ```rust
  tokio::select! {
      chunk = audio_rx.recv() => { /* dispatch chunk */ }
      _ = flush_timeout_future => {
          // emit AsrFinalOutput { reason: Timeout, ... }
          // mark segment_finalized = true
          // allow next segment
      }
  }
  ```
- `flush_timeout_future`: 当 `flush_pending` 时设为 `tokio::time::sleep(flush_timeout)`，否则 `futures::future::pending()`
- Late provider final: 如果 `segment_finalized == true`，忽略 provider 的 final response for this segment

### 7. 实现 sink-drop cancellation

- Adapter task 检测 `audio_rx.recv()` 返回 `None` → sender dropped
- 检查是否有 pending flush:
  - 有 pending flush → 等待 flush_timeout 后 emit Timeout final，然后 cancel
  - 无 pending flush（直接 drop without End）→ emit `Error { code: Cancelled, fatal: true }`
- Close event channel

### 8. 实现 provider endpointing 处理

- Adapter 收到 provider-side endpoint signal → emit `EndOfSpeech { trace_id, segment_id }`
- 如果 provider 同时提供 final text/metadata → emit `AsrFinal { reason: ProviderEndpoint }`
- 如果 caller 随后 Flush 同一 segment → `segment_finalized` 为 true → 不再发第二个 AsrFinal
- `flush_and_wait_final()` 收到 ProviderEndpoint final → 直接返回（不等 CallerFlush final）

### 9. 实现 FinalResultScope

- `Segment` mode: `AsrFinalOutput.result.text` = 当前 segment text
- `Stream` mode: 每次 segment final 后 append text 到 `accumulated_text`，`AsrFinalOutput.result.text` = accumulated_text

### 10. 扩展 fake provider for streaming tests

`FakeAsrProvider::start_stream()` 返回真正可用的 `AsrStream`:
- 内部 spawn adapter task
- 支持可配置行为：normal flow, timeout scenario, provider endpointing, duplicate definite utterances
- 至少 4 个 fake 场景:
  1. Normal flush → final
  2. Flush timeout → Timeout final
  3. Provider endpoint → ProviderEndpoint final
  4. Sink drop without End → Cancelled error

### 11. 写 streaming 测试

所有测试使用 fake provider，不需要网络:

- **Event ordering**: RouteSelected → Started → TranscriptUpdate* → AsrFinal
- **flush_and_wait_final()**: 发 audio → flush → 收到 AsrFinalOutput with CallerFlush
- **end_and_wait_final()**: 发 audio → end → 收到 AsrFinalOutput with CallerEnd
- **split() full-duplex**: spawn 两个 task 各自操作 sink/events
- **Flush timeout**: fake provider 不响应 → 收到 Timeout final
- **Late provider final**: timeout 后 fake provider 发 final → 不收到 duplicate AsrFinal
- **Provider endpointing**: fake provider 发 EndOfSpeech + ProviderEndpoint final → 收到
- **Provider endpoint + caller Flush**: fake provider 先发 ProviderEndpoint final → flush_and_wait_final() 返回该 final → 无 duplicate
- **Sink drop cancellation**: drop sink → events 收到 Cancelled error
- **FinalResultScope::Stream**: 多次 flush → 每次 final 的 text 累积
- **Committed dedup**: fake provider 发重复 committed TranscriptUpdate → 只看到一次
- **split() consumed**: 编译时确认 split 后不能调用 flush_and_wait_final (这个是类型系统保证，不需要 runtime test)

### 12. 验证

```bash
cargo test -p agent-runtime-asr-providers
cargo clippy -p agent-runtime-asr-providers -- -D warnings
cargo fmt --check
```

## 关键决策

- Channel capacity: audio channel 32 (约 3.2s buffer at 100ms chunks), event channel 64。太小会 backpressure block caller，太大会 buffer 过多音频浪费内存
- **不在 003 中抽通用 adapter task framework**。Fake provider 的 adapter task 是 self-contained 实现。005/006 各自实现 provider-specific adapter task，之后如果有明确共性再提取到 `streaming.rs`。过早抽象会被两个 provider 截然不同的协议（Volcengine 二进制帧 vs Aliyun DashScope JSON）打脸
- `AsrStream` 的 `flush_and_wait_final(&mut self)` 需要同时操作 input 和 events — 直接用 `&mut self.input` 和 `&mut self.events` 即可，因为 `send` 只需 `&self` (mpsc::Sender)，`recv` 需要 `&mut self` (mpsc::Receiver)，不冲突
