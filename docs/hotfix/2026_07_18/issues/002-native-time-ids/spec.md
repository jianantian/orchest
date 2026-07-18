# Issue 002:deepgram/aliyun 原生时间字段段 ID

GitHub: [#211](https://github.com/jianantian/orchest/issues/211) · 依赖 001(沿用状态型 mapper 范式)

## 现状

- `deepgram.rs`:`map_result`(约 L94-113)发 `Transcript{segment: None}`;wire 的 `Results` 消息原生带 `start`(秒偏移,同一句话的 interim 序列共享同一 `start`),`ResultsMessage` 目前未解析该字段;`speech_final` 已发 `EndOfSpeech{segment: None}`。
- `aliyun.rs`:`map_event`(约 L135-171)发 `Transcript{segment: None}`;DashScope paraformer-realtime 的 sentence 对象原生带 `begin_time`/`end_time`(ms),`Sentence`(约 L67-72)目前只解析 `text`/`sentence_end`。

## 方向

- **deepgram**:`ResultsMessage` 增加 `start: f64`(秒,缺省 0.0);segment_id 取 `format!("seg{}", (start * 1000.0) as u64)`;Provisional/Committed 均 `Snapshot` 且共享该 id;`speech_final` 的 `EndOfSpeech` 填 `Some(同一 SegmentRef)`。保持纯函数形态(无状态)。
- **aliyun**:`Sentence` 增加 `begin_time: Option<i64>`、`end_time: Option<i64>`;引入 `AliyunMapper { fallback_counter: u64 }`:有 `begin_time` 时 segment_id = `format!("seg{}", begin_time)`,否则用 `s{fallback_counter}`、`sentence_end` 后递增;Provisional/Committed 均 `Snapshot` 共享 id;`task-finished` 的 `EndOfSpeech` 保持 `segment: None`(流级);`run_aliyun_stream` 持有 mapper 实例。
- 两家均不新增事件种类/数量,只填 segment 字段。

deepgram 的「同一句话 interim 共享同一 start」与 aliyun 的 `begin_time` 语义按 wire 形状写单测固定;live 验证受无凭证环境限制不做(沿用仓库惯例,见 prd)。

## 落地与测试

- `deepgram.rs` 单测:同 `start` 的 interim 序列 → 相同 segment_id;`is_final=true` → 同 id Committed;`speech_final` → `EndOfSpeech{segment: Some(同 id)}`;缺 `start` 字段 → `seg0` 不 panic
- `aliyun.rs` 单测:带 `begin_time` 的句子序列 → id 来自 begin_time 且 Provisional/Committed 同 id;缺 `begin_time` → 计数器 id 且 sentence_end 后递增;`task-finished` → `EndOfSpeech{segment: None}`
- 现有 map 测试相应更新
- `cargo test -p orchest-provider-stream` 全绿

## 验收标准

- [ ] deepgram Transcript 携带来自 `start` 的 segment_id,Provisional/Committed 同 id;`speech_final` 的 EndOfSpeech 带该段
- [ ] aliyun Transcript 携带来自 `begin_time` 的 segment_id(缺省计数器兜底),Provisional/Committed 同 id
- [ ] 不新增事件种类;两家现有测试更新后全绿
- [ ] `cargo test -p orchest-provider-stream` 通过
