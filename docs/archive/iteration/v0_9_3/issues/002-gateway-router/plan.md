# 002 实现路线

## 要读的现有代码

- `crates/agent-runtime-asr-providers/src/routing.rs` — gateway/router style
- `crates/agent-runtime-asr-providers/tests/router.rs` — fake provider routing tests
- `crates/agent-runtime-providers/src/lib.rs` — model normalization behavior reference
- `docs/archive/iteration/v0_9_3/prd.md` — routing order and compatibility contract

## 步骤

### 1. 建 fake provider test harness

- 新增 fake provider，支持可配置 capabilities、voices、batch result 和 stream events
- Fake provider 记录收到的 requests，方便断言 router 是否选择正确 provider/model
- Fake provider 不依赖 provider feature flags

### 2. 实现 router registry

- `TtsRouter` 保存 normalized provider/model -> provider
- route registration 时校验 route model 能 normalize
- route 和 provider capabilities mismatch 在 router selection 时返回稳定错误

### 3. 实现 route-level compatibility validators

- 按 operation 分 batch/single-stream/duplex validation
- 校验 input kind、operation support、language、explicit `VoiceSelection.kind` route constraint 和 output format
- Batch output format 使用 `batch_output_formats`
- Single-stream / duplex output format 使用 `stream_output_formats`
- 不在本 issue 实现 speech numeric range validation 或 semantic coercion gate；这些留给 004

### 4. 实现 gateway helpers

- `synthesize()` 选择 route 后调用 provider
- `stream_synthesize()` 选择 route 后包装 provider stream，插入 `RouteSelected`
- `start_duplex_stream()` 选择 route 后包装 provider duplex stream，插入 `RouteSelected`
- `list_voices()` 支持 explicit model 和 route-filtered aggregation

### 5. 测试

- 路由优先级和 tie-break
- explicit selector
- operation-specific format validation
- route-level strict/coerce compatibility
- no fake duplex downgrade
- voice-list route selection and deterministic aggregation

## 验证

```bash
cargo test -p agent-runtime-tts-providers --no-default-features
cargo test -p agent-runtime-tts-providers
cargo clippy -p agent-runtime-tts-providers -- -D warnings
cargo fmt --check
```

## 关键决策

- Route-level compatibility validation should be pure functions where practical, so provider adapters can reuse the operation/format checks without needing `TtsGateway`
- Route selection must sort candidate keys before tie-break to avoid HashMap nondeterminism
- `request.compatibility` wins at runtime; gateway config only supplies defaults for caller-side builders/examples
- Voice catalog filtering and speech/semantic controls are implemented in 004, not in this gateway/router issue
