use super::*;

#[test]
fn catalog_has_six_providers() {
    assert_eq!(list_providers().len(), 6);
}

#[test]
fn minimax_models_present() {
    let models: Vec<_> = list_models().filter(|m| m.provider == "minimax").collect();
    assert_eq!(
        models.len(),
        8,
        "minimax catalog should have 8 non-Her models (M3 + 7 M2.x)"
    );
    let ids: Vec<_> = models.iter().map(|m| m.model_id).collect();
    for expected in [
        "minimax/MiniMax-M3",
        "minimax/MiniMax-M2.7",
        "minimax/MiniMax-M2.7-highspeed",
        "minimax/MiniMax-M2.5",
        "minimax/MiniMax-M2.5-highspeed",
        "minimax/MiniMax-M2.1",
        "minimax/MiniMax-M2.1-highspeed",
        "minimax/MiniMax-M2",
    ] {
        assert!(ids.contains(&expected), "missing {expected}: {ids:?}");
    }
    // Her must NOT be in catalog — non-Anthropic protocol, PRD non-goal.
    assert!(!ids.iter().any(|m| m.contains("her") || m.contains("Her")));
}

#[test]
fn minimax_m3_is_multimodal() {
    let m3 = list_models()
        .find(|m| m.model_id == "minimax/MiniMax-M3")
        .expect("M3 catalog entry");
    assert!(m3.input_modalities.contains(&Modality::Image));
    assert_eq!(m3.context_window, 1_000_000);
}

#[test]
fn minimax_m2_series_is_text_only() {
    for model_id in ["minimax/MiniMax-M2.7", "minimax/MiniMax-M2"] {
        let entry = list_models()
            .find(|m| m.model_id == model_id)
            .unwrap_or_else(|| panic!("{model_id} catalog entry"));
        assert_eq!(entry.input_modalities, &[Modality::Text]);
        assert_eq!(entry.context_window, 204_800);
    }
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
    assert!(ids.contains(&"volcengine/doubao-seed-2-1-pro-260628"));
    assert!(ids.contains(&"volcengine/doubao-seed-2-0-lite-260428"));
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
        let rates = &pricing.tiers.first().expect("at least one tier").rates;
        assert_eq!(
            rates.cache_read_per_million,
            Some(want),
            "{model_id} cache_read price drifted from upstream",
        );
        assert!(
            rates.cache_write_per_million.is_none(),
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
        // doubao-seed-character-260628 is intentionally absent: the 2.1 refresh added
        // image + audio input for multi-role companion scenarios.
        "deepseek/deepseek-v4-flash",
        "deepseek/deepseek-v4-pro",
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
        "volcengine/doubao-seed-2-1-turbo-260628",
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
        .find(|m| m.model_id == "volcengine/doubao-seed-character-260628")
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
