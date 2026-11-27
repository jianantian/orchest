# 002 · 实施计划

## 先读

- `crates/orchest/src/{events,capabilities}.rs`、`crates/orchest/src/run/{request,handle}.rs`、现有 SDK 事件转换。
- `scripts/check-ts-event-variants.sh` 与双端 SDK 声明。

## 修改范围

- 两端运行控制与事件转换、公开类型、共享 fixture 和检查脚本。

## 步骤

1. 从现有核心事件和错误建立双端映射表，先固定公开判别字段、载荷和错误类别。
2. 将控制交给各自 Rust 句柄，保持现有 Agent 入口可用。
3. 验证模型流、审批、非法负载、过期 ID、并发隔离与事件变体覆盖。
