# 006 — 实施计划

## 要读的文件

- `crates/orchest/src/tool/mod.rs`(ToolContext 定义 :68-84、Tool trait)
- `examples/demo/music-gift/src/tools/countdown.rs`(`tool_context()` :286 与调用点)
- `examples/demo/music-gift/src/tools/collect_info.rs`(测试模块中复制的校验逻辑)
- 其他手造 ToolContext 的点(rg `ToolContext {` 找齐,评估受益面)

## 要改的文件

- `crates/orchest/src/tool/mod.rs`(`oneshot()` + 可选 `call_oneshot`)
- `examples/demo/music-gift/src/tools/countdown.rs`、`collect_info.rs`
- 测试

## 步骤

1. `ToolContext::oneshot()` 实现 + doc;视手感加 `Tool::call_oneshot`。
2. SDK 侧单测:oneshot 驱动真实 tool execute。
3. demo:countdown 改 oneshot 删本地 helper;collect_info 测试改真实 callback、删复制逻辑。
4. 五项检查。
