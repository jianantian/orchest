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
    assert!(ids.contains(&"anthropic/claude-fable-5"));
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
