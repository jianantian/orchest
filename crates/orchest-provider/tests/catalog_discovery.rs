#![cfg(feature = "http")]

use orchest_protocol::Capability;
use orchest_provider::catalog::{find_model, find_model_for, list_models, ModelFilter};

#[test]
fn projected_chat_rows_have_non_empty_descriptions() {
    let rows: Vec<_> = list_models(ModelFilter {
        capability: Some(Capability::Chat),
        include_deprecated: true,
        ..Default::default()
    })
    .collect();
    assert!(!rows.is_empty());
    for r in rows {
        assert!(
            !r.description.trim().is_empty(),
            "empty description for {}",
            r.id
        );
    }
}

#[test]
fn chat_ids_are_unique_by_capability_provider_model() {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    for r in list_models(ModelFilter {
        include_deprecated: true,
        ..Default::default()
    }) {
        assert!(
            seen.insert((r.capability, r.provider, r.model)),
            "duplicate {:?}",
            (r.capability, r.provider, r.model)
        );
    }
}

#[test]
fn find_model_peels_openai_responses_protocol() {
    let hit = find_model("openai/responses/gpt-5.4")
        .expect("protocol-explicit id should resolve via ADR-0002");
    assert_eq!(hit.provider, "openai");
    assert_eq!(hit.model, "gpt-5.4");
    assert_eq!(hit.capability, Capability::Chat);
}

#[test]
fn find_model_for_disambiguates_capability() {
    // After Batch 1 Asr rows land this matters more; for Batch 0 assert Chat path.
    let hit = find_model_for("openai/gpt-5.4", Capability::Chat).expect("chat row");
    assert_eq!(hit.capability, Capability::Chat);
}
