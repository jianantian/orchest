# Hotfix 2026-07-18 PRD:ASR 流式方言 SegmentRef 填充

## 背景

协议脊的 `StreamEvent::Transcript` 带 `segment: Option<SegmentRef>`(`crates/orchest-protocol/src/stream.rs`),设计稿 `docs/archive/iteration/v0_9_12/issues/001-protocol-design/design.md` §1.3 明确规定 `AsrStreamEvent::TranscriptUpdate{text,stability,segment_id,update_kind}` → `Transcript{..., segment: Some(SegmentRef{segment_id, update_kind})}`。但 `orchest-provider-stream` 的五个 WS ASR 方言(volcengine/deepgram/aliyun/soniox/elevenlabs)全部填 `segment: None`,消费端无法做「流式上屏、定稿按段替换」——Provisional 与其 Committed 之间没有可对应的锚点。

此外 volcengine 在 `result_type:"single"` 下,服务端每帧全量重发 utterance 列表,`map_frame` 对每条 utterance 每帧都发事件,同一句的 Committed 会重复出现,消费端被迫自行去重。

本 hotfix 从各 provider 原生协议中提取段标识,填充 `Transcript.segment` 与(有意义的)`EndOfSpeech.segment`,并在 volcengine 侧加状态 diff 消除重发噪音。

## 目标

1. 五个方言发出的 `Transcript` 事件均携带 `Some(SegmentRef)`,段 ID 来源对齐各 provider 原生协议:

   | 方言 | segment_id 来源 | Provisional | Committed | EndOfSpeech.segment |
   |---|---|---|---|---|
   | volcengine | utterance `start_time`(ms),如 `"utt450"`;rolling 兜底段 `"rolling"` | Snapshot | Snapshot | None(is_last 为流级) |
   | deepgram | Results 原生 `start`(秒→ms),如 `"seg1230"` | Snapshot | Snapshot | Some(speech_final 对应段) |
   | aliyun | sentence 原生 `begin_time`(ms);缺省退化计数器 `s{n}` | Snapshot | Snapshot | None(task-finished 为流级) |
   | soniox | 合成两段 `"final"` / `"tail"` | Snapshot(tail) | Append(final) | None(finished 为流级) |
   | elevenlabs | 合成计数器 `s{n}`,Committed 后递增 | Snapshot | Snapshot | 不发 EndOfSpeech |

2. 跨方言不变量写入 `stream.rs` 的 `SegmentRef`/`Transcript` 文档注释:同一 `segment_id` 的 Provisional 均为 Snapshot(后到全量替换);Committed 与其定稿的 Provisional 共享 segment_id;`Append` 仅用于 soniox final token 流这类原生增量协议。
3. volcengine 引入带状态 diff(`start_time → (text, definite)`),只在内容或定稿位变化时发事件。

## 非目标(本 hotfix 明确不做)

- `Done{usage}` / `SessionStarted` / `SessionClosed` 生命周期事件的补齐
- `CapabilityEventExt::Asr` 富详情(confidence、词级时间戳等)
- `format`/`language`/`options` 静默失效的校验与报错
- volcengine/elevenlabs 模型名不上 wire 的修复
- omni(`RealtimeSession`)的 Transcript 事件、`orchest-provider` fakes(streaming 本不支持)
- 为 aliyun/volcengine 新增句子级 EndOfSpeech 发射(保持现有事件基数,Committed 携带 segment_id 即为句边界)

## 成功指标

- 五个方言的 `Transcript` 事件全部带 `Some(SegmentRef)`(volcengine rolling 分支为固定段 `"rolling"`)
- 同一句的 Provisional 与其 Committed 共享 segment_id,逐方言有单元测试固定
- volcengine 对未变化的 utterance 不再重复发事件
- `cargo test --workspace`、`cargo clippy --workspace -- -D warnings`、`cargo fmt --check`、`bash scripts/lint-check.sh` 全过

## Issue 拆分

| Issue | 标题 | GitHub | 依赖 |
|-------|------|--------|------|
| 001 | volcengine SegmentRef + 状态 diff | [#210](https://github.com/jianantian/orchest/issues/210) | 无(先行,立状态型 mapper 范式 + protocol 文档不变量) |
| 002 | deepgram/aliyun 原生时间字段段 ID | [#211](https://github.com/jianantian/orchest/issues/211) | 001(沿用范式) |
| 003 | elevenlabs/soniox 合成段 ID | [#212](https://github.com/jianantian/orchest/issues/212) | 001(沿用范式) |

按 WORKFLOW:实施分支 `hotfix/2026_07_18`,一 issue 一 commit,commit message 带 `closes #N`。文档(prd + specs)先行一个 `docs:` commit。

## 验收标准

- [ ] 001–003 各自 spec 的验收 checklist 全过
- [ ] 四件套(`cargo test --workspace` / `cargo clippy --workspace -- -D warnings` / `cargo fmt --check` / `bash scripts/lint-check.sh`)全过
- [ ] merge 后 roadmap 已完成表加本 hotfix 行,docs 归档至 `docs/archive/hotfix/2026_07_18`

## 依赖

- 无外部依赖;改动集中在 `crates/orchest-provider-stream/src/asr/` 五个方言文件与 `crates/orchest-protocol/src/stream.rs` 文档注释
- live provider 验证受无凭证环境限制不做(沿用既有惯例),单元测试按各家 wire 形状固定协议行为
