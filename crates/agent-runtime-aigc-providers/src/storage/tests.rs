use super::*;

#[tokio::test]
async fn resolve_asset_url_returns_public_output_only() {
    let registry = InMemoryAssetRegistry::default();
    let dir = tempfile::tempdir().unwrap();
    let store = LocalAssetStore::new(dir.path(), Some("http://localhost/assets".into()));
    let scope = AssetScope::test();
    let asset = store
        .put_stream(
            AssetIngestSource::Bytes {
                bytes: Bytes::from_static(b"png"),
                mime_type: "image/png".into(),
            },
            PutAssetOptions::default(),
        )
        .await
        .unwrap();
    let asset_id = asset.asset_id.clone();
    registry.save(&scope, asset).await.unwrap();
    let output = resolve_asset_url(&registry, &store, &scope, &asset_id, None)
        .await
        .unwrap();
    assert!(output.url.starts_with("http://localhost/assets/"));
    let serialized = serde_json::to_string(&output).unwrap();
    assert!(!serialized.contains("bucket"));
    assert!(!serialized.contains("object_key"));
}
