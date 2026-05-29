//! Token counting using tiktoken cl100k_base BPE.

use std::sync::OnceLock;

use serde_json::Value;
use tiktoken_rs::{cl100k_base, CoreBPE};

static TOKENIZER: OnceLock<CoreBPE> = OnceLock::new();

fn tokenizer() -> &'static CoreBPE {
    // INVARIANT: cl100k_base is a baked-in BPE shipped with the tiktoken-rs
    // crate.  Its `get_bpe_from_tokenizer` call can only fail if the crate's
    // embedded data files are corrupt or missing — a fatal linker-level error
    // that warrants an immediate panic rather than silent degradation.
    TOKENIZER.get_or_init(|| cl100k_base().expect("tiktoken cl100k_base init should never fail: data files corrupt"))
}

pub fn count_tokens(text: &str) -> usize {
    tokenizer().encode_ordinary(text).len()
}

pub fn truncate_to_tokens(value: Value, max_tokens: usize) -> Value {
    const SUFFIX: &str = "\n[output truncated]";
    let suffix_tokens = count_tokens(SUFFIX);
    let limit = max_tokens.saturating_sub(suffix_tokens);

    match value {
        Value::String(text) => truncate_string(text, max_tokens, limit),
        other => {
            let serialized = serde_json::to_string(&other).unwrap_or_default();
            let tokens = tokenizer().encode_ordinary(&serialized);
            if tokens.len() <= max_tokens {
                other
            } else {
                Value::String(decode_truncated(tokens, limit))
            }
        }
    }
}

fn truncate_string(text: String, max_tokens: usize, limit: usize) -> Value {
    let tokens = tokenizer().encode_ordinary(&text);
    if tokens.len() <= max_tokens {
        Value::String(text)
    } else {
        Value::String(decode_truncated(tokens, limit))
    }
}

fn decode_truncated(tokens: Vec<u32>, limit: usize) -> String {
    const SUFFIX: &str = "\n[output truncated]";
    let truncated = tokenizer()
        .decode(tokens[..limit.min(tokens.len())].to_vec())
        .unwrap_or_default();
    format!("{truncated}{SUFFIX}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn short_ascii_not_truncated() {
        let value = Value::String("hello world".to_string());
        let result = truncate_to_tokens(value.clone(), 100);
        assert_eq!(result, value);
    }

    #[test]
    fn cjk_truncated_at_token_boundary() {
        let text = "你好".repeat(100);
        let result = truncate_to_tokens(Value::String(text), 10);
        let result = result.as_str().unwrap();
        assert!(result.contains("[output truncated]"));
        assert!(count_tokens(result) <= 10);
    }

    #[test]
    fn exactly_at_limit_not_truncated() {
        let value = Value::String("hello".to_string());
        let result = truncate_to_tokens(value.clone(), 5);
        assert_eq!(result, value);
        assert!(!result.as_str().unwrap().contains("[output truncated]"));
    }

    #[test]
    fn json_object_truncated_when_too_long() {
        let big_obj = json!({"key": "a".repeat(10000)});
        let result = truncate_to_tokens(big_obj, 50);
        assert!(result.is_string());
        assert!(result.as_str().unwrap().contains("[output truncated]"));
    }
}
