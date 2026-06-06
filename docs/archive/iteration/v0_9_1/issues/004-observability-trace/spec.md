# 004 · Observability + trace

GitHub: [#112](https://github.com/jianantian/orchest/issues/112)

## 背景

为 ASR crate 实现结构化 observability：telemetry 填充、trace ID 传播、tracing spans、metrics。

## 契约

### 输入

- 001 定义的 `AsrTelemetry`, `AsrStreamEvent` types
- 002 的 gateway/router 框架
- 003 的 streaming event 流

### 输出

- `AsrTelemetry` 在所有调用路径填充
- Trace ID 全链路传播
- Tracing spans 和 metrics 记录

## 影响范围

### 修改文件

- `crates/agent-runtime-asr-providers/src/observability.rs` — span/metrics 实现
- `crates/agent-runtime-asr-providers/src/routing.rs` — gateway 层 span 和 telemetry 填充
- `crates/agent-runtime-asr-providers/src/streaming.rs` — stream event telemetry

## 实现要点

### AsrTelemetry 填充

- `trace_id` — gateway 生成或 caller 提供
- `model` — normalized `"provider/model"`
- `language` — detected or requested
- `audio_duration_ms` — 从 usage 或 provider 响应
- `latency_first_update_ms` — 首个 `TranscriptUpdate` 延迟
- `latency_final_ms` — `AsrFinal` 延迟
- `update_rollback_count` — provisional transcript 回退次数
- `confidence_avg` — 平均置信度
- `cost_estimate_micros` — 费用估算
- `network_region` — 路由选择的区域
- `upstream_status` — provider HTTP/WebSocket 状态码
- `option_adjustment_count` — Coerce 模式调整次数

### Trace ID 传播

Gateway 生成或接收 trace_id 后，注入到：
- `RouteSelected { trace_id, ... }`
- `Started { trace_id, ... }`
- 每个 `TranscriptUpdate { trace_id, ... }`
- `EndOfSpeech { trace_id, ... }`
- `AsrFinal { final_output: AsrFinalOutput { trace_id, ... } }`
- `Error { trace_id, ... }`
- `TranscribeResult.telemetry.trace_id`

Provider adapters 不生成无关 trace ID。

### Spans

| Span | Scope |
|------|-------|
| `asr.gateway.transcribe` | Gateway `transcribe()` 全过程 |
| `asr.gateway.stream` | Gateway `start_stream()` 全过程 |
| `asr.provider.request` | Provider `transcribe()` 调用 |
| `asr.provider.stream` | Provider streaming session |
| `asr.router.select` | Router 选择过程 |

### Metrics

| Metric | Description |
|--------|-------------|
| request duration | 请求总时长 |
| first `TranscriptUpdate` latency | 首个 partial 延迟 |
| `AsrFinal` latency | final 结果延迟 |
| audio duration | 音频总时长 |
| upstream error count | 按 stable code/status 分类 |
| transcript update rollback count | partial 回退次数 |
| option adjustment count | 兼容性调整次数 |

### 安全

- 默认不 log raw audio bytes, signed URLs, API keys, full transcripts
- `AsrError` 保留 upstream body，但 redact secrets
- Transcript logging 是应用层策略

## 验收标准

- [ ] `AsrTelemetry` is populated in `TranscribeResult` for all call paths (streaming + reserved transcribe)
- [ ] Telemetry includes `trace_id` and can be correlated with an external Orchest run
- [ ] Trace ID propagates through all `AsrStreamEvent` variants
- [ ] Tracing spans are emitted for gateway, provider, and router operations
- [ ] Metrics are recorded for latency, duration, errors, rollbacks, and adjustments
- [ ] `AsrError` preserves upstream status/code/message/body with secret redaction
- [ ] No raw audio, signed URLs, API keys, or transcripts in default log output
- [ ] Crate does not add ASR-specific variants to core `RuntimeEvent`
- [ ] `cargo test -p agent-runtime-asr-providers` passes

## 依赖

- 001 Crate scaffold + public types (#109)
- 002 Gateway + router (#110)
- 003 Duplex streaming contract (#111)
