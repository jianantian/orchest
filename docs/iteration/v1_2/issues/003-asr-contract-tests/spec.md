# 003 · 方言无关的 ASR 契约测试

GitHub issue: #344

## Background

方言测试各写各的，没有一套断言保证它们对外行为一致。语音 V3 一接聚合 provider，不一致会被放大，所以契约要先用测试钉住。

## Goal

一套方言无关的断言，用录制的 WebSocket fixture 对五个流式 ASR 方言各跑一遍，离线运行。

## Acceptance Criteria

- [ ] 断言集：同段 Draft 全是 Snapshot；Stable 之后不再有同段 Draft；每个 Turn 指向一个已出现的 Stable；Finalize 之后恰好一个结果：一个 Turn，或一个 `operation = "finalize"` 的非致命错误，且会话仍可继续收发；`client_finalize = false` 的方言只能是 `UnsupportedOption`，`true` 的方言在正常 fixture 下必须是 Turn。
- [ ] 五个方言各有至少一份录制 fixture（同一段录音），测试在无网络下通过。
- [ ] 有录制脚本：给定 key 与录音，重新生成 fixture；fixture 中不含 key 与可识别的个人信息。
- [ ] CI 跑契约测试。

## Blocked by

002。

## Notes

- 录 fixture 需要 owner 提供五个方言的 key；缺 key 的方言先用手写 fixture，并在 PRD 风险里标注。
