# Issue 003:elevenlabs/soniox 合成段 ID

GitHub: [#212](https://github.com/jianantian/orchest/issues/212) · 依赖 001(沿用状态型 mapper 范式)

## 现状

两家协议都没有原生句子/段标识:

- `elevenlabs.rs`:`map_message`(约 L75-93)对 `partial_transcript`/`committed_transcript[_with_timestamps]` 发 `Transcript{segment: None}`;wire 无任何段 ID。
- `soniox.rs`:`map_message`(约 L83-118)把每条消息的 final token 拼成 Committed、non-final 尾拼成 Provisional,均 `segment: None`;Soniox 协议无句子结构,只有 final/non-final token 二分。

## 方向

- **elevenlabs**:引入 `ElevenLabsMapper { current: u64 }`:Partial/Committed 共享 segment_id `s{current}`(`Snapshot`),Committed 发出后 `current += 1`;`run_elevenlabs_stream` 持有 mapper 实例。不发 EndOfSpeech 的现状不变。
- **soniox**:保持纯函数,合成两段模型——Committed(final token 增量)段 `"final"`、`Append`;Provisional(可修订尾部快照)段 `"tail"`、`Snapshot`;`finished` 的 `EndOfSpeech` 保持 `segment: None`。
- 两家均不新增事件种类/数量,只填 segment 字段。

soniox 的「一个永恒 final 段」是刻意简化:Soniox 的 endpoint 检测(`<end>` token)目前未启用也未解析,待后续需要时再按 endpoint 切分 final 段(属另一 issue)。

## 落地与测试

- `elevenlabs.rs` 单测:partial → `s0` Provisional;committed → 同 `s0` Committed;下一条 partial → `s1`
- `soniox.rs` 单测:一条混合 token 消息 → Committed{segment: final/Append} + Provisional{segment: tail/Snapshot};`finished` → `EndOfSpeech{segment: None}`
- 现有 map 测试相应更新
- `cargo test -p orchest-provider-stream` 全绿

## 验收标准

- [ ] elevenlabs Transcript 携带计数器 segment_id,同句 Partial/Committed 同 id,Committed 后换新 id
- [ ] soniox Committed 为 `Append` + 段 `"final"`,Provisional 为 `Snapshot` + 段 `"tail"`
- [ ] 不新增事件种类;两家现有测试更新后全绿
- [ ] `cargo test -p orchest-provider-stream` 通过
