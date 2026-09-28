# 004 实现路线

## 要读的现有代码和资料

- `docs/external/aliyun/tts-api-doc.md` — Aliyun voice and instruction behavior
- `docs/external/aliyun/tts-guideline.md` — custom voice/design behavior
- `docs/external/volceengine/tts_bidirection.md` — Volcengine voice and control behavior
- `crates/agent-runtime-tts-providers/src/routing.rs` — compatibility hooks from 002

## 步骤

### 1. Centralize voice catalog helpers

- 在 `voices.rs` 添加 `VoiceInfo` filter helpers
- 按 provider/model/voice_id 做 deterministic sorting
- 添加 `VoiceSelection.kind` resolution helper

### 2. 实现 speech control validators

- speed/pitch/volume range validator
- Strict path 返回 `TtsErrorCode::InvalidRequest`
- Coerce path clamp 并记录 `OptionAdjustment`

### 3. 实现 semantic control validators

- 根据 `TtsModelCapabilities` 校验 instruction、emotion、style 和 SSML
- drop/convert semantic controls 必须检查 `allow_semantic_coercions`
- 测试 unsupported controls 不会 silent disappear

### 4. 接入 router/gateway

- Gateway route filtering 尽可能使用 resolved voice kind
- `list_voices()` 复用 002 的 provider/model route/aggregation 行为，再做 language/kind/include_custom filtering
- Batch/stream routing 复用同一组 voice/control validation

### 5. 测试

- Static voice catalogs
- Custom voice inclusion/exclusion
- Unknown voice kind in strict/coerce
- Numeric range strict/coerce
- Instruction/SSML support differences

## 验证

```bash
cargo test -p agent-runtime-tts-providers --no-default-features voice
cargo test -p agent-runtime-tts-providers --no-default-features controls
cargo test -p agent-runtime-tts-providers
cargo clippy -p agent-runtime-tts-providers -- -D warnings
cargo fmt --check
```

## 关键决策

- Voice creation and governance remain out of scope; only synthesis with existing voice ids is supported
- Static catalogs are allowed only when source is explicit
- Provider-native controls outside portable ranges must not be forced into portable typed fields
- Provider/model routing remains owned by 002; this issue only layers voice/control semantics on top
