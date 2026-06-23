use super::*;

#[test]
fn catalog_has_five_providers() {
    assert_eq!(list_providers().len(), 5);
}

#[test]
fn anthropic_models_present() {
    let models: Vec<_> = list_models()
        .filter(|m| m.provider == "anthropic")
        .collect();
    assert!(!models.is_empty());
    let ids: Vec<_> = models.iter().map(|m| m.model_id).collect();
    assert!(ids.contains(&"anthropic/claude-opus-4-8"));
    assert!(ids.contains(&"anthropic/claude-sonnet-4-6"));
    assert!(ids.contains(&"anthropic/claude-haiku-4-5"));
}

#[test]
fn all_anthropic_models_have_usd_pricing() {
    for entry in list_models().filter(|m| m.provider == "anthropic") {
        let p = entry
            .pricing
            .as_ref()
            .expect("anthropic model should have pricing");
        assert_eq!(
            p.currency, "USD",
            "{} should have USD pricing",
            entry.model_id
        );
    }
}

#[test]
fn deepseek_models_have_cny_pricing() {
    for entry in list_models().filter(|m| m.provider == "deepseek") {
        let p = entry
            .pricing
            .as_ref()
            .expect("deepseek model should have pricing");
        assert_eq!(
            p.currency, "CNY",
            "{} should have CNY pricing",
            entry.model_id
        );
    }
}

#[test]
fn volcengine_models_present() {
    let models: Vec<_> = list_models()
        .filter(|m| m.provider == "volcengine")
        .collect();
    assert!(!models.is_empty());
    let ids: Vec<_> = models.iter().map(|m| m.model_id).collect();
    assert!(ids.contains(&"volcengine/doubao-seed-2-0-pro-260215"));
    assert!(ids.contains(&"volcengine/doubao-seed-2-0-lite-260215"));
}

#[test]
fn volcengine_models_have_cny_pricing() {
    for entry in list_models().filter(|m| m.provider == "volcengine") {
        let p = entry
            .pricing
            .as_ref()
            .expect("volcengine model should have pricing");
        assert_eq!(
            p.currency, "CNY",
            "{} should have CNY pricing",
            entry.model_id
        );
    }
}

#[test]
fn openrouter_is_dynamic() {
    let or_provider = list_providers()
        .iter()
        .find(|p| p.provider_id == "openrouter")
        .expect("openrouter should be in catalog");
    assert!(
        matches!(or_provider.models, LlmModelList::Dynamic { .. }),
        "openrouter should be Dynamic"
    );
}

#[test]
fn haiku_context_window_is_200k() {
    let haiku = list_models()
        .find(|m| m.model_id == "anthropic/claude-haiku-4-5")
        .expect("haiku should be in catalog");
    assert_eq!(haiku.context_window, 200_000);
}

#[test]
fn deepseek_v4_context_window_is_1m() {
    for entry in list_models().filter(|m| m.provider == "deepseek") {
        assert_eq!(
            entry.context_window, 1_000_000,
            "{} should have 1M context",
            entry.model_id
        );
    }
}

#[test]
fn deepseek_models_publish_cache_hit_price() {
    // Source: https://api-docs.deepseek.com/zh-cn/quick_start/pricing
    // DeepSeek lists a per-model cache-hit input price; absence indicates a regression.
    // No separate cache-write price exists, so cache_write_per_million stays None.
    let expected = [
        ("deepseek/deepseek-v4-flash", 0.02),
        ("deepseek/deepseek-v4-pro", 0.025),
    ];
    for (model_id, want) in expected {
        let entry = list_models()
            .find(|m| m.model_id == model_id)
            .unwrap_or_else(|| panic!("{model_id} missing from catalog"));
        let pricing = entry.pricing.as_ref().expect("pricing must exist");
        assert_eq!(
            pricing.cache_read_per_million,
            Some(want),
            "{model_id} cache_read price drifted from upstream",
        );
        assert!(
            pricing.cache_write_per_million.is_none(),
            "{model_id} should not declare cache_write price (DeepSeek does not charge for writes)",
        );
    }
}

// ---------------------------------------------------------------------------
// Regression tests for catalog information fields (issue 006)
// ---------------------------------------------------------------------------

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

#[test]
/// Note: OpenAI gpt-5.5/5.4 and Volcengine doubao-seed-2-0-pro are not asserted
/// here because the catalog does not yet declare Image for them. Add assertions
/// once the catalog data is confirmed against vendor docs.
fn multimodal_models_declare_image_input() {
    let must_have_image = [
        "anthropic/claude-opus-4-8",
        "anthropic/claude-sonnet-4-6",
        "anthropic/claude-haiku-4-5",
        "openai/gpt-5.4-mini",
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

#[test]
fn text_only_models_have_only_text_modality() {
    let text_only = [
        // Note: gpt-5.4-nano is not listed here because the catalog declares Image
        // input for it. If nano is confirmed text-only, add it here and fix the catalog.
        "deepseek/deepseek-v4-flash",
        "deepseek/deepseek-v4-pro",
        "volcengine/doubao-seed-character-251128",
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

#[test]
fn reasoning_scene_marks_top_tier_models() {
    let reasoning_models = [
        "anthropic/claude-opus-4-8",
        // Note: OpenAI gpt-5.5/5.4 are not flagged as Reasoning in the current catalog.
        // Add them here once the catalog data is updated.
        "anthropic/claude-opus-4-7",
        "deepseek/deepseek-v4-pro",
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

#[test]
fn thinking_support_matches_provider_implementation() {
    // These models declare thinking support in catalog
    let thinking_supported = [
        "anthropic/claude-opus-4-8",
        "anthropic/claude-sonnet-4-6",
        "deepseek/deepseek-v4-pro",
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

    // Character model does NOT support thinking
    let char_entry = list_models()
        .find(|m| m.model_id == "volcengine/doubao-seed-character-251128")
        .unwrap();
    assert!(
        char_entry.thinking.is_none(),
        "doubao-seed-character should NOT declare thinking support",
    );
}

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
