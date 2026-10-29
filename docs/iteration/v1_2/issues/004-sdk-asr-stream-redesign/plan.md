# 004 implementation plan

## Files to read

- `crates/orchest-py/src/{asr.rs,asr_stream.rs}`、`crates/orchest-node/src/{asr.rs,asr_stream.rs}`
- `python/orchest/__init__.py`、`__init__.pyi`；`js/index.js`、`js/types.d.ts`、`js/native.d.ts`
- `crates/orchest-provider/src/fakes.rs`
- `docs/guide/sdk-python.md`、`docs/guide/sdk-typescript.md`（若存在）

## Files to change

- 绑定 crate 的 `asr.rs` / `asr_stream.rs`：参数拆分、`finalize()`、事件转换
- Python 与 JS 的包装层与类型声明
- `python/tests/`、`js/tests/`：新增 ASR 流用例
- SDK 指南

## Steps

1. 在绑定层把 `provider` / `model` 拆开构造 `ProviderConfig`；删除合写解析。
2. 流启动时保存 `descriptor()`；加 `finalize()`：`client_finalize` 为 `false` 时同步抛 `UnsupportedOption`，否则发送 `SessionInput::Finalize`，通道关闭时抛异常；非致命错误事件转换时保留 `operation` 与 `fatal`。
3. 重写 `StreamEvent` → SDK 事件的转换，产出带 `kind` 的结构。
4. 更新 `.pyi` / `.d.ts`，跑类型检查与事件变体脚本。
5. 用 `testing` fake 写双端测试；更新指南；跑全部 SDK 测试与 workspace 检查。
