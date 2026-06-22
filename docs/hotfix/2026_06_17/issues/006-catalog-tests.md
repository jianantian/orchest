# Issue 006:catalog 测试钉住关键事实

## 背景

新字段加完后,需要回归测试钉住关键事实,防止后续修改无意中破坏数据。

## 范围

`crates/agent-runtime-providers/src/catalog/tests.rs` 增加测试。

### 必加测试(至少 5 条)

#### 1. `description_is_non_empty_for_all_models`

```rust
#[test]
fn description_is_non_empty_for_all_models() {
    for entry in list_models() {
        assert!(
            !entry.description.is_empty(),
            "{} description should be filled",
            entry.model_id
        );
    }
}
```

#### 2. `multimodal_models_declare_image_input`

```rust
#[test]
fn multimodal_models_declare_image_input() {
    // Anthropic 全系应支持 Image input;
    // OpenAI gpt-5.4-nano 只支持 Text;
    // Volcengine doubao-seed-2.0 系列支持 Image。
    let must_have_image = [
        "anthropic/claude-opus-4-8",
        "anthropic/claude-sonnet-4-6",
        "anthropic/claude-haiku-4-5",
        "openai/gpt-5.5",
        "openai/gpt-5.4",
        "openai/gpt-5.4-mini",
        "volcengine/doubao-seed-2-0-pro-260215",
    ];
    for model_id in must_have_image {
        let entry = list_models()
            .find(|m| m.model_id == model_id)
            .unwrap_or_else(|| panic!("{model_id} missing"));
        assert!(
            entry.input_modalities.contains(&Modality::Image),
            "{model_id} should support Image input",
        );
    }
}
```

#### 3. `text_only_models_have_only_text_modality`

```rust
#[test]
fn text_only_models_have_only_text_modality() {
    let text_only = [
        "openai/gpt-5.4-nano",
        "deepseek/deepseek-v4-flash",
        "deepseek/deepseek-v4-pro",
        "volcengine/doubao-seed-1-6-flash-250615",
    ];
    for model_id in text_only {
        let entry = list_models()
            .find(|m| m.model_id == model_id)
            .unwrap_or_else(|| panic!("{model_id} missing"));
        assert_eq!(
            entry.input_modalities,
            &[Modality::Text],
            "{model_id} input modalities mismatch",
        );
        assert_eq!(
            entry.output_modalities,
            &[Modality::Text],
            "{model_id} output modalities mismatch",
        );
    }
}
```

#### 4. `reasoning_scene_only_for_top_tier_models`

```rust
#[test]
fn reasoning_scene_marks_top_tier_models() {
    let reasoning_models = [
        "anthropic/claude-opus-4-8",
        "anthropic/claude-opus-4-7",
        "deepseek/deepseek-v4-pro",
        "openai/gpt-5.5",
        "openai/gpt-5.4",
        "volcengine/doubao-seed-2-0-pro-260215",
    ];
    for model_id in reasoning_models {
        let entry = list_models()
            .find(|m| m.model_id == model_id)
            .unwrap_or_else(|| panic!("{model_id} missing"));
        assert!(
            entry.scenes.contains(&ModelScene::Reasoning),
            "{model_id} should be flagged as Reasoning SOTA",
        );
    }
}
```

#### 5. `thinking_support_matches_provider_implementation`

```rust
#[test]
fn thinking_support_matches_provider_implementation() {
    // 这些模型在自己的 capabilities() 里报 reasoning.supported = true
    // catalog thinking 字段应该一致
    let thinking_supported = [
        "anthropic/claude-opus-4-8",
        "anthropic/claude-sonnet-4-6",
        "openai/gpt-5.5",
        "openai/gpt-5.4",
        "deepseek/deepseek-v4-pro",
        "volcengine/doubao-seed-2-0-pro-260215",
        "volcengine/doubao-seed-1-6-flash-250615",
    ];
    for model_id in thinking_supported {
        let entry = list_models()
            .find(|m| m.model_id == model_id)
            .unwrap_or_else(|| panic!("{model_id} missing"));
        assert!(
            entry.thinking.is_some(),
            "{model_id} should declare thinking support",
        );
    }
}
```

#### 6. `max_input_tokens_within_context_window`

```rust
#[test]
fn max_input_tokens_within_context_window() {
    for entry in list_models() {
        if let Some(max_input) = entry.max_input_tokens {
            assert!(
                max_input <= entry.context_window,
                "{} max_input_tokens ({}) exceeds context_window ({})",
                entry.model_id,
                max_input,
                entry.context_window,
            );
        }
    }
}
```

#### 7. `coding_scene_for_dev_oriented_models`(可选)

```rust
#[test]
fn coding_scene_for_dev_oriented_models() {
    let coding_models = [
        "anthropic/claude-sonnet-4-6",
        "anthropic/claude-opus-4-8",
        "openai/gpt-5.4",
        "deepseek/deepseek-v4-flash",
        "deepseek/deepseek-v4-pro",
    ];
    for model_id in coding_models {
        let entry = list_models()
            .find(|m| m.model_id == model_id)
            .unwrap_or_else(|| panic!("{model_id} missing"));
        assert!(
            entry.scenes.contains(&ModelScene::Coding),
            "{model_id} should be flagged for Coding scene",
        );
    }
}
```

## 不在本 issue 范围

- 测试 `usd_model` / `cny_model`(已删除)
- 改动既有 10 条测试

## 验收标准

- [ ] 加至少 5 条新测试(上述 1-5 必加,6-7 可选)
- [ ] 既有 10 条测试全部继续通过
- [ ] `cargo test -p agent-runtime-providers --lib catalog` 全部通过
- [ ] 测试名称遵循 `<aspect>_<expected_behavior>` 模式
- [ ] `cargo clippy -p agent-runtime-providers --tests -- -D warnings` 通过

## 依赖

- 依赖 issue 001-005 全部完成(数据填齐后才有意义钉住)
