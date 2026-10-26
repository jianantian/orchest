# 008 implementation plan

## Files to read

- `crates/orchest/src/events.rs`（`EVENT_DELIVERY_TIMEOUT`、`deliver_to_subscribers`、`EventSink`、`offer_secondary`、`PendingLoss`）
- `crates/orchest/src/telemetry.rs`（`record_event_drop` 等指标）
- `crates/orchest/src/run/actor.rs` 与 `run/mod.rs`（事件通道创建处，容量常量 `EVENT_CHANNEL_CAPACITY`）
- `crates/orchest/src/run/watcher.rs`（次要订阅者与 watcher 通道）
- `crates/orchest/src/session/log.rs`（与持久 sink 的职责边界）

## Files to change

- `crates/orchest/src/events.rs`：主通道改为阻塞投递（带容量上限与背压观测）、次要订阅者语义保持、持久 sink 接口
- `crates/orchest/src/telemetry.rs`：背压指标（等待次数、最长等待、通道占用）
- `crates/orchest/src/run/{actor.rs,mod.rs}`：通道容量可配
- `crates/orchest/src/bindings.rs`、`orchest-py`、`orchest-node`：若新增 sink 公开面则同步

## Steps

1. 把主通道投递从"超时即丢"改为"阻塞等待容量"；删除 `EVENT_DELIVERY_TIMEOUT` 的丢弃语义（公开面行为变更）。
2. 保留次要订阅者的 `try_send` + 合并 `EventsDropped`；修正主通道镜像通知的误报路径。
3. 背压可观测：记录等待次数、最长等待时长、通道占用水位；宿主能据此判断"事件流跟不上"。
4. 通道容量改为可配（默认按压测结果定），写进配置与文档。
5. 持久 sink：定义宿主可注入的事件写入接口（追加 + 显式 flush），该路径零丢失；与执行态日志（001）职责分离。
6. 压测与测试：慢消费者 + 1 万条事件 → 零丢失、顺序稳定；容量耗尽 → 背压可观测；持久 sink 崩溃后内容完整。
7. 跑 `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --check`；`CHANGELOG.md` 记行为变更。
