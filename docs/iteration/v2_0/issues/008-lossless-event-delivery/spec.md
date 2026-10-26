# 008 · canonical 事件零丢失

GitHub issue: #338

## Background

事件投递现在有两档：主接收者 `await` 发送但有 500ms 超时，超时即**丢弃该事件**并记 `record_event_drop("primary", 1)`（`crates/orchest/src/events.rs:273`、`:523-530`）；次要订阅者用 `try_send`，有损但用合并的 `EventsDropped` 告知（`crates/orchest/src/events.rs:285-318`）。

对"用事件流做 canonical 持久化"的宿主（Multivac）来说，主通道丢事件是致命的：一次慢消费就会在持久化的事件流里留下空洞，而 Multivac 的事件流是单调 seq、有序、可重放的（`crates/multivac-core/src/event/mod.rs`）。

对照调研：dsh 的持久化层把"append 接受"与"flush 承诺耐久"分开，次要消费者有损但显式；openhuman 的 run journal + status 双写支持 late-attach 重放；monocode 的 events 表以 `revision` 为主键并只保留最近 N 条。

ADR-0005 §4.1 要求：canonical 路径零丢失、顺序稳定；慢消费者产生背压而不是丢事件。

## Goal

让主事件通道**零丢失**：慢消费者会让 run 产生背压，而不是丢掉事件；同时保留次要订阅者的有损语义（但必须可观测）。

## Acceptance Criteria

- [ ] 主通道发送不再因超时丢弃事件：要么成功送达，要么阻塞等待容量（背压）。
- [ ] 背压有界且可观测：容量上限、等待时长、阻塞次数被记录（telemetry），宿主能看见"事件流跟不上"。
- [ ] 事件顺序在主通道上严格保持（与 seq 一致）。
- [ ] 次要订阅者维持有损语义，但丢失必须以 `EventsDropped` 报告（现有语义保留），且主通道不再收到自己造成的丢失误报。
- [ ] 提供**持久 sink** 接口：宿主可把事件直接写入自己的存储（追加写 + 显式 flush），该路径同样零丢失。
- [ ] 事件通道容量可配，默认值按 008 的压测结果确定并写进文档。
- [ ] 测试：慢消费者下发送 1 万条事件零丢失、顺序不变；容量耗尽时 run 表现为背压而非静默丢弃；持久 sink 路径在崩溃后内容完整。
- [ ] 现有 `EVENT_DELIVERY_TIMEOUT` 语义变更在 `CHANGELOG.md` 记录（公开面行为变更，ADR-0003 归为 major）。

## Blocked by

001（日志与事件面的边界；持久 sink 与执行态日志要分清职责）。

## Notes

- 执行态真相是**会话日志**（001），事件流是**观察流**；两者都零丢失但不是同一份东西，持久 sink 写的是观察流。
- 背压意味着慢消费者会拖慢 run：这是有意的取舍，宿主若要"不阻塞"，应改为次要订阅者（可丢）而不是主通道。
- 与 001 的 `flush` 区分：那是执行态日志的耐久屏障，这是观察流的投递保证。
