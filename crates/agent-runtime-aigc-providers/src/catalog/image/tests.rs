use super::*;

#[test]
fn catalog_has_five_providers() {
    assert_eq!(list_providers().len(), 5);
}

#[test]
fn volcengine_seedream_present() {
    let entry = list_models()
        .find(|m| m.model_id == "doubao-seedream-5-0-260128")
        .expect("doubao-seedream-5-0-260128 should be in catalog");
    assert_eq!(entry.provider, "volcengine");
    assert!(entry.operations.contains(&"text_to_image"));
}

#[test]
fn volcengine_seedream_4_5_present() {
    let entry = list_models()
        .find(|m| m.model_id == "doubao-seedream-4-5-251128")
        .expect("doubao-seedream-4-5-251128 should be in catalog");
    assert_eq!(entry.provider, "volcengine");
    assert!(entry.operations.contains(&"image_to_image"));
}

#[test]
fn aliyun_wan_models_present() {
    let ids: Vec<_> = list_models()
        .filter(|m| m.provider == "aliyun")
        .map(|m| m.model_id)
        .collect();
    assert!(
        ids.contains(&"wan2.7-image-pro"),
        "wan2.7 model should be in catalog"
    );
    assert!(ids.contains(&"qwen-image-2.0-pro"));
}

#[test]
fn crazyrouter_has_gpt_image_2() {
    let entry = list_models()
        .find(|m| m.model_id == "gpt-image-2")
        .expect("gpt-image-2 should be in catalog");
    assert_eq!(entry.provider, "crazyrouter");
}

#[test]
fn dynamic_providers_excluded_from_list_models() {
    let providers: Vec<_> = list_models().map(|m| m.provider).collect();
    assert!(!providers.contains(&"openrouter"));
    assert!(!providers.contains(&"renderful"));
}

#[test]
fn openrouter_is_dynamic() {
    let p = list_providers()
        .iter()
        .find(|p| p.provider_id == "openrouter")
        .unwrap();
    assert!(matches!(p.models, ImageModelList::Dynamic { .. }));
}
