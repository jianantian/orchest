# 005 · Node 绑定 unsafe 审计

## 背景

`agent-runtime-node/src/lib.rs:129-130` 有 `unsafe impl Send/Sync for JsTool`，soundness 取决于 napi-rs 的 `ThreadsafeFunction` 是否实际实现了 `Send + Sync`。

## 问题

```rust
// Safety: ThreadsafeFunction is designed to be Send + Sync
unsafe impl Send for JsTool {}
unsafe impl Sync for JsTool {}
```

注释声称 `ThreadsafeFunction` 设计为 `Send + Sync`，但未验证当前 napi-rs 版本中的实际 trait bound。如果 `ThreadsafeFunction` 在某些版本中不是 `Send`，此 `unsafe impl` 构成未定义行为。

## 修复

添加编译期静态断言：

```rust
const _: () = {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    fn check() {
        assert_send::<napi::threadsafe_function::ThreadsafeFunction<
            napi::JsUnknown,
            napi::threadsafe_function::ErrorStrategy::Fatal,
        >>();
        assert_sync::<napi::threadsafe_function::ThreadsafeFunction<
            napi::JsUnknown,
            napi::threadsafe_function::ErrorStrategy::Fatal,
        >>();
    }
};
```

如果编译通过：`unsafe impl` 是 sound 的，保留并更新注释引用断言。

如果编译失败：说明 `ThreadsafeFunction` 确实不是 `Send/Sync`，需要改为 `Arc<std::sync::Mutex<ThreadsafeFunction<...>>>` 包装，并移除 `unsafe impl`。

## 验收标准

- [ ] 编译期静态断言存在且编译通过，或 `unsafe impl` 已替换为安全实现
- [ ] `unsafe impl` 的注释更新为引用静态断言
- [ ] `cargo build -p agent-runtime-node` 成功
