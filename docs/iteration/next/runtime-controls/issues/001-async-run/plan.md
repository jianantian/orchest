# 001 · 实施计划

## 先读

- `crates/orchest/src/run/{mod,handle,delivery}.rs` 与两端 `src/lib.rs`。
- `python/orchest/__init__.pyi`、`js/native.d.ts`、`js/tests/`、`python/tests/`。

## 修改范围

- `crates/orchest-py/`、`crates/orchest-node/`、公开导出与类型声明、双端测试。

## 步骤

1. 先确定两端异步入口、单次事件迭代、等候结果和退出契约。约定提前退出如何取消并释放运行，明确等候结果和消费事件的前置关系。
2. 复用核心有界事件接收器，为绑定实现可唤醒的拉取及运行占用。校验启动冲突和等待清理。
3. 用可暂停模型 fixture 验证非阻塞、慢消费、退出与旧入口回归。
