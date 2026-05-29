# 006 · tiktoken-rs 集成

## 背景

`run/helpers.rs`（拆分前为 `run.rs:201`）中的 `truncate_output` 使用字节估算代替真实 token 计数：

```rust
let max_bytes = max_tokens as usize * 4;
```

问题：
- 对 CJK 字符（每字符 1 token，但 3-4 bytes），估算偏保守——实际上一个 token 可能对应 4 个 bytes，会截断过多
- 对纯 ASCII 代码（1 token ≈ 4 chars ≈ 4 bytes），估算凑巧准确，但这是巧合
- 对混合语言内容（常见于多语言 agent），估算误差无规律

正确做法是实际数 token。

## 变更

### 依赖

文件：`crates/agent-runtime-core/Cargo.toml`

```toml
tiktoken-rs = "0.6"
```

tiktoken-rs 使用 `cl100k_base` BPE 词表（Claude 和 GPT-4 共用的分词器，对本 runtime 的截断场景足够精确）。

### 新模块

文件：`crates/agent-runtime-core/src/tokenizer.rs`

```rust
//! Token counting using tiktoken cl100k_base BPE.

use std::sync::OnceLock;
use serde_json::Value;
use tiktoken_rs::{cl100k_base, CoreBPE};

static TOKENIZER: OnceLock<CoreBPE> = OnceLock::new();

fn tokenizer() -> &'static CoreBPE {
    TOKENIZER.get_or_init(|| cl100k_base().expect("tiktoken cl100k_base init failed"))
}

/// Count tokens in a UTF-8 string.
pub fn count_tokens(text: &str) -> usize {
    tokenizer().encode_ordinary(text).len()
}

/// Truncate a JSON value so it fits within `max_tokens` tokens when serialized.
/// String values are truncated at token boundary; non-string values are serialized
/// first and then truncated as string if too long.
pub fn truncate_to_tokens(value: Value, max_tokens: usize) -> Value {
    const SUFFIX: &str = "\n[output truncated]";
    // Reserve tokens for the suffix
    let limit = max_tokens.saturating_sub(count_tokens(SUFFIX));

    match value {
        Value::String(s) => {
            let tokens = tokenizer().encode_ordinary(&s);
            if tokens.len() <= max_tokens {
                return Value::String(s);
            }
            let truncated = tokenizer()
                .decode(tokens[..limit].to_vec())
                .unwrap_or_default();
            Value::String(format!("{truncated}{SUFFIX}"))
        }
        other => {
            let serialized = serde_json::to_string(&other).unwrap_or_default();
            let tokens = tokenizer().encode_ordinary(&serialized);
            if tokens.len() <= max_tokens {
                return other;
            }
            let truncated = tokenizer()
                .decode(tokens[..limit].to_vec())
                .unwrap_or_default();
            Value::String(format!("{truncated}{SUFFIX}"))
        }
    }
}
```

### 替换 `truncate_output`

文件：`run/helpers.rs`

旧实现：

```rust
fn truncate_output(value: Value, max_tokens: u64) -> Value {
    let max_bytes = max_tokens as usize * 4;   // ← 字节估算，删除
    // ...
}
```

新实现：

```rust
// run/helpers.rs
use crate::tokenizer::truncate_to_tokens;

pub(crate) fn truncate_output(value: Value, max_tokens: u64) -> Value {
    truncate_to_tokens(value, max_tokens as usize)
}
```

同时删除 `truncate_str_utf8_safe` 函数（已无调用方）。

### lib.rs 注册模块

文件：`crates/agent-runtime-core/src/lib.rs`

```rust
pub mod tokenizer;
```

## 验收标准

### 实现

- [ ] `crates/agent-runtime-core/Cargo.toml` 包含 `tiktoken-rs = "0.6"`
- [ ] `crates/agent-runtime-core/src/tokenizer.rs` 文件存在
- [ ] `count_tokens(text: &str) -> usize` 函数 pub 可用
- [ ] `truncate_to_tokens(value: Value, max_tokens: usize) -> Value` 函数 pub 可用
- [ ] `run/helpers.rs` 中不含 `* 4` 字节估算逻辑（grep `max_bytes` 应无匹配）
- [ ] `truncate_str_utf8_safe` 函数已删除

### 单元测试（`tokenizer.rs` 内嵌）

- [ ] ASCII 文本不被错误截断：
  ```rust
  #[test]
  fn short_ascii_not_truncated() {
      let v = Value::String("hello world".to_string());
      let result = truncate_to_tokens(v.clone(), 100);
      assert_eq!(result, v);
  }
  ```
- [ ] CJK 字符按 token 截断（不按字节）：
  ```rust
  #[test]
  fn cjk_truncated_at_token_boundary() {
      // "你好" = 2 tokens in cl100k_base
      let text = "你好".repeat(100);
      let v = Value::String(text);
      let result = truncate_to_tokens(v, 10);
      // 结果应包含 "[output truncated]" 后缀
      assert!(result.as_str().unwrap().contains("[output truncated]"));
      // 截断后 token 数不超过 10
      assert!(count_tokens(result.as_str().unwrap()) <= 10);
  }
  ```
- [ ] 刚好在限制内的文本不被截断：
  ```rust
  #[test]
  fn exactly_at_limit_not_truncated() {
      // "hello" ≈ 1 token
      let v = Value::String("hello".to_string());
      let result = truncate_to_tokens(v.clone(), 5);
      assert!(!result.as_str().unwrap().contains("[output truncated]"));
  }
  ```
- [ ] 非 String Value（JSON 对象）在超长时被序列化后截断：
  ```rust
  #[test]
  fn json_object_truncated_when_too_long() {
      let big_obj = json!({"key": "a".repeat(10000)});
      let result = truncate_to_tokens(big_obj, 50);
      assert!(result.is_string());
      assert!(result.as_str().unwrap().contains("[output truncated]"));
  }
  ```

### 正确性

- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
- [ ] 初始化开销（`OnceLock` 第一次 init）只发生一次，后续 `count_tokens` 调用无 lock 竞争
