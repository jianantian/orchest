# Issue 001:volcengine SegmentRef + 状态 diff

GitHub: [#210](https://github.com/jianantian/orchest/issues/210) · 无依赖(先行,立状态型 mapper 范式)

## 现状

`crates/orchest-provider-stream/src/asr/volcengine.rs` 的 `map_frame`(约 L223-273)是纯函数,对每帧里的**每条** utterance 都发一个 `Transcript{segment: None}`。两个问题:

1. 协议 `result_type:"single"` 下服务端每帧全量重发 utterance 列表,未变化的 definite utterance 的 Committed 被反复发出,消费端无法不去重;
2. wire 上 `VolcengineUtterance` 自带 `start_time`/`end_time`(ms,会话内按 utterance 唯一),是天然段标识,但被丢弃。

## 方向

- 引入 `VolcengineMapper { emitted: HashMap<i32, (String, bool)> }`(key = utterance `start_time`,value = 上次发出的 `(text, definite)`),`map(&mut self, frame) -> Vec<StreamEvent>`;只在 utterance 的 text 或 definite 发生变化时发事件。
- segment_id 取 `format!("utt{}", start_time)`;Provisional/Committed 均为 `Snapshot`。
- 无 utterances 的 rolling `result.text` 兜底分支:固定段 `"rolling"`,`Snapshot`。
- `is_last` 的 `EndOfSpeech` 保持 `segment: None`(流级信号,不指向具体段)。
- `run_asr_stream` 持有一个 `VolcengineMapper` 实例替代直接调 `map_frame`;`ErrorResponse` 路径行为不变。
- `crates/orchest-protocol/src/stream.rs`:给 `SegmentRef` 与 `StreamEvent::Transcript` 的文档注释补跨方言不变量(同 id Provisional 为 Snapshot 全量替换;Committed 与其 Provisional 共享 id;`Append` 仅用于原生增量 token 流)。

不选「换 `result_type:"incremental"`」路线:改请求参数属于行为面变更,且 single 模式 + diff 在 SDK 侧即可达到同等降噪效果,不动 wire 请求最稳。

## 落地与测试

- `volcengine.rs`:`VolcengineMapper` + 单测
  - 同一 utterance 连续两帧 text/definite 不变 → 第二帧零 Transcript 事件
  - definite 翻转 → 发出与之前 Provisional 相同 segment_id 的 Committed
  - 一帧多 utterance → 各自 segment_id,互不影响
  - rolling 分支 → `Some(SegmentRef{ segment_id: Some("rolling"), Snapshot })`
  - is_last → 仍发 `EndOfSpeech{segment: None}`
- 现有 `map_frame` 相关测试迁移到 mapper 形态
- `cargo test -p orchest-provider-stream` 全绿

## 验收标准

- [x] volcengine 发出的 Transcript 全部携带 `Some(SegmentRef)`,segment_id 来自 utterance `start_time`(rolling 分支为 `"rolling"`)
- [x] 未变化 utterance 不重复发事件(单测固定)
- [x] 同句 Provisional→Committed 共享 segment_id(单测固定)
- [x] `stream.rs` 文档注释写入跨方言不变量
- [x] `cargo test -p orchest-provider-stream` 通过

## 实现记录

- `map_frame` 纯函数 → `VolcengineMapper`(`volcengine.rs`):`emitted: HashMap<i32,(String,bool)>` 按 `start_time` diff,只在 text/definite 变化时发事件;rolling 分支以 `Option<(String,bool)>` 跟踪 `(text, committed)`,同文本末帧仍能补 Committed 稳定性跃迁
- segment_id = `utt{start_time}`,Provisional/Committed 均 `Snapshot`;rolling 分支固定段 `"rolling"`;`is_last` 的 `EndOfSpeech` 保持 `segment: None`(流级信号)
- `crates/orchest-protocol/src/stream.rs`:`SegmentRef` 与 `StreamEvent::Transcript` 文档注释写入跨方言契约(Snapshot 全量替换 / Committed 同 id / Append 仅原生增量流)
- 测试:utterance/rolling 两个 fixture helper + 5 个 mapper 测试(原生 id、重发抑制、id 共享、rolling 固定段与末帧提交、fatal error);`run_asr_stream` 端到端测试不动
- clippy 一处 `collapsible_match`(rolling 分支 if 并入 match guard)已修;`cargo test -p orchest-provider-stream` 68 通过,clippy/fmt 净
