# 006 实现路线

## 步骤

1. **添加依赖**
   - 在 `crates/agent-runtime-core/Cargo.toml` 的 `[dependencies]` 下添加 `tiktoken-rs = "0.6"`
   - 运行 `cargo fetch` 确认可以下载（第一次可能需要几秒钟）

2. **创建 `tokenizer.rs` 模块**
   - 新建 `crates/agent-runtime-core/src/tokenizer.rs`
   - 实现 `count_tokens(text: &str) -> usize` 和 `truncate_to_tokens(value: Value, max_tokens: usize) -> Value`（完整实现见 spec）
   - 用 `OnceLock<CoreBPE>` 做 lazy singleton，避免每次调用都重新加载词表
   - 在文件底部加 spec 要求的三个 `#[cfg(test)]` 测试

3. **注册模块**
   - 在 `crates/agent-runtime-core/src/lib.rs` 添加 `pub mod tokenizer;`
   - 运行 `cargo build -p agent-runtime-core` 确认编译通过

4. **替换 `truncate_output`**
   - 找到 `run/helpers.rs` 中的 `truncate_output` 函数
   - 改为调用 `crate::tokenizer::truncate_to_tokens(value, max_tokens as usize)`
   - 删除 `truncate_str_utf8_safe` 函数（先 grep 确认无其他调用方）

5. **运行测试**
   - `cargo test -p agent-runtime-core tokenizer` — 三个测试全过
   - 注意：`cjk_truncated_at_token_boundary` 测试依赖 cl100k_base 对 "你好" 的编码行为，首次运行会下载 BPE 词表文件（约 1.7MB），后续 cached

6. **验收**
   - `grep -n "max_bytes\|\* 4" crates/agent-runtime-core/src/run/helpers.rs` — 无输出
   - `grep -n "truncate_str_utf8_safe" crates/agent-runtime-core/src/` — 无输出
   - `cargo test --workspace` 全绿
   - `cargo clippy --workspace -- -D warnings` 全绿

## 要读的现有代码

- `crates/agent-runtime-core/src/run/helpers.rs`（001 拆分后）— `truncate_output` 和 `truncate_str_utf8_safe` 的现有实现
- `crates/agent-runtime-core/src/lib.rs` — 确认 `pub mod` 的添加位置

## 关键决策

- **词表选择 `cl100k_base` vs `o200k_base`**：Claude 和 GPT-4 都使用 cl100k_base，GPT-4o 使用 o200k_base。对于 token 计数用于截断的场景，使用哪个词表对结果影响不大，cl100k_base 更普遍支持，用它
- **初始化开销**：`OnceLock` 第一次调用有约 20ms 的词表加载时间（解压 + 构建 BPE 哈希表）。这发生在第一次截断操作时，不影响启动时间，可接受
- **`truncate_to_tokens` 对非 String Value 的处理**：先序列化为 JSON 字符串再计 token，截断后返回 `Value::String`（注意：不再是原始的 JSON 类型）。这与旧行为一致（旧实现也把非 String 序列化后截断），不是新引入的变化
- **测试中的精确 token 数**：cl100k_base 对 "你好" 的编码是 2 tokens（每个汉字 1 token），这个数字在 cl100k_base 版本间是稳定的，测试断言 `count_tokens(result) <= 10` 而不是 `== 某个精确值`，避免因词表更新而失败
