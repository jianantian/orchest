# 001 · 实施计划

## 先读

- `crates/orchest/src/run/{mod,handle,delivery}.rs` 与两端 `src/lib.rs`。
- `python/orchest/__init__.pyi`、`js/native.d.ts`、`js/tests/`、`python/tests/`。

## 修改范围

- `crates/orchest-py/`、`crates/orchest-node/`、公开导出与类型声明、双端测试。

## 步骤

1. 先写两端调用示例，覆盖消费、取得结果、过早等待和提前退出，再定公开签名。
2. 复用核心有界事件接收器，实现按需唤醒的拉取和统一的运行占用。对齐过早等待、取消及退出清理的顺序。
3. 用可暂停模型 fixture 验证非阻塞、积压、异常退出和旧入口回归。在相同负载下测量 Rust 与两端 SDK，定位并优化主要新增开销。
