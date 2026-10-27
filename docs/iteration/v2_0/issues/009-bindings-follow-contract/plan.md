# 009 implementation plan

## Files to read

- `crates/orchest-py/src/lib.rs`、`crates/orchest-node/src/lib.rs`（`Agent`、`respond_approval`、事件转换）
- `crates/orchest/src/bindings.rs`（共享的绑定辅助）
- 001、004、006、007 合入后的公开面

## Files to change

- `crates/orchest-py/src/lib.rs`、`crates/orchest-node/src/lib.rs`
- `crates/orchest/src/bindings.rs`
- `python/orchest/__init__.py`、`__init__.pyi`；`js/index.js`、`js/types.d.ts`、`js/native.d.ts`
- `python/tests/`、`js/tests/`、`CHANGELOG-SDK.md`

## Steps

1. 修编译：按新的 `SessionStore`、请求通道、事件类型调整绑定。
2. 加 `abort()`，接到 006 的取消令牌。
3. 把 `respond_approval` 换成 `respond(request_id, payload)`。
4. 同步类型声明与事件变体脚本。
5. 补测试，跑全部 SDK 测试与 workspace 检查。
