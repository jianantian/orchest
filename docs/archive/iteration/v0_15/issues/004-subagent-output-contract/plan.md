# 004 — 实施计划

## 要读的文件

- `crates/orchest/src/tool/agent_as_tool.rs`(003 落地后的 execute/SubAgentBuilder 全貌)
- `crates/orchest/src/run/mod.rs`(`resume_with_input` 签名与语义)
- `examples/demo/music-gift/src/tools/countdown.rs`(`strip_code_fences` :276 与调用点 `config.rs:176`)
- `docs/archive/iteration/` v0.13(`start_with_messages`/`resume_with_input` 用法参照)

## 要改的文件

- `crates/orchest/src/tool/agent_as_tool.rs`(契约类型 + builder 方法 + 提取/纠正逻辑)
- `examples/demo/music-gift/src/tools/countdown.rs`、`src/config.rs`(删 strip_code_fences,改契约)
- 测试

## 步骤

1. 输出契约 enum + `SubAgentBuilder` 配置方法(默认 None,零行为变化)。
2. 提取器:fenced(首个匹配 fence 块,容忍包裹文字)/json(parse 校验);单测覆盖边界。
3. 纠正轮次:提取失败 → resume_with_input 纠正提示(限 1 次)→ 再提取;仍失败 → Err(003 语义)。
4. demo:countdown 声明契约,删 `strip_code_fences` 与 `config.rs:176` 调用,链路测试更新。
5. 五项检查。
