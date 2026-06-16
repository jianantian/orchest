//! `AssetRegistry` implementations: in-memory (tests/ephemeral use) and a
//! JSON-file-backed one for simple persistent deployments.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use async_trait::async_trait;

use super::{AssetRegistry, AssetScope, AssetStoreError, StoredAsset};

#[derive(Default)]
pub struct InMemoryAssetRegistry {
    assets: Mutex<HashMap<(AssetScope, String), StoredAsset>>,
}

#[async_trait]
impl AssetRegistry for InMemoryAssetRegistry {
    async fn save(&self, scope: &AssetScope, asset: StoredAsset) -> Result<(), AssetStoreError> {
        self.assets
            .lock()
            .map_err(|_| AssetStoreError::new("registry_poisoned", "asset registry lock poisoned"))?
            .insert((scope.clone(), asset.asset_id.clone()), asset);
        Ok(())
    }

    async fn get(
        &self,
        scope: &AssetScope,
        asset_id: &str,
    ) -> Result<StoredAsset, AssetStoreError> {
        self.assets
            .lock()
            .map_err(|_| AssetStoreError::new("registry_poisoned", "asset registry lock poisoned"))?
            .get(&(scope.clone(), asset_id.to_string()))
            .cloned()
            .ok_or_else(|| {
                AssetStoreError::new("asset_not_found", "asset not found in requested scope")
            })
    }
}

pub struct FileAssetRegistry {
    path: PathBuf,
    memory: InMemoryAssetRegistry,
}

impl FileAssetRegistry {
    pub async fn open(path: impl Into<PathBuf>) -> Result<Self, AssetStoreError> {
        let path = path.into();
        let memory = InMemoryAssetRegistry::default();
        if tokio::fs::try_exists(&path).await.unwrap_or(false) {
            let bytes = tokio::fs::read(&path)
                .await
                .map_err(|err| AssetStoreError::new("registry_read_failed", err.to_string()))?;
            let entries: Vec<(AssetScope, StoredAsset)> = serde_json::from_slice(&bytes)
                .map_err(|err| AssetStoreError::new("registry_parse_failed", err.to_string()))?;
            for (scope, asset) in entries {
                memory.save(&scope, asset).await?;
            }
        }
        Ok(Self { path, memory })
    }

    async fn flush(&self) -> Result<(), AssetStoreError> {
        let entries = self
            .memory
            .assets
            .lock()
            .map_err(|_| AssetStoreError::new("registry_poisoned", "asset registry lock poisoned"))?
            .iter()
            .map(|((scope, _), asset)| (scope.clone(), asset.clone()))
            .collect::<Vec<_>>();
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|err| AssetStoreError::new("registry_write_failed", err.to_string()))?;
        }
        let bytes = serde_json::to_vec_pretty(&entries)
            .map_err(|err| AssetStoreError::new("registry_serialize_failed", err.to_string()))?;
        tokio::fs::write(&self.path, bytes)
            .await
            .map_err(|err| AssetStoreError::new("registry_write_failed", err.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl AssetRegistry for FileAssetRegistry {
    async fn save(&self, scope: &AssetScope, asset: StoredAsset) -> Result<(), AssetStoreError> {
        self.memory.save(scope, asset).await?;
        self.flush().await
    }

    async fn get(
        &self,
        scope: &AssetScope,
        asset_id: &str,
    ) -> Result<StoredAsset, AssetStoreError> {
        self.memory.get(scope, asset_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::StorageLocation;
    use chrono::Utc;

    #[tokio::test]
    async fn in_memory_registry_scopes_assets() {
        let registry = InMemoryAssetRegistry::default();
        let asset = StoredAsset {
            asset_id: "asset-1".into(),
            location: StorageLocation::Noop,
            content_type: "image/png".into(),
            sha256: "abc".into(),
            byte_count: 1,
            created_at: Utc::now(),
            expires_at: None,
        };
        let scope = AssetScope::test();
        registry.save(&scope, asset.clone()).await.unwrap();
        assert_eq!(
            registry.get(&scope, "asset-1").await.unwrap().asset_id,
            "asset-1"
        );
        let wrong = AssetScope {
            namespace: "wrong".into(),
            ..scope
        };
        assert_eq!(
            registry.get(&wrong, "asset-1").await.unwrap_err().code,
            "asset_not_found"
        );
    }

    #[tokio::test]
    async fn file_registry_persists_across_instances() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("registry.json");
        let scope = AssetScope::test();
        let asset = StoredAsset {
            asset_id: "asset-2".into(),
            location: StorageLocation::Noop,
            content_type: "image/png".into(),
            sha256: "abc".into(),
            byte_count: 1,
            created_at: Utc::now(),
            expires_at: None,
        };
        let registry = FileAssetRegistry::open(&path).await.unwrap();
        registry.save(&scope, asset).await.unwrap();
        let reopened = FileAssetRegistry::open(&path).await.unwrap();
        assert_eq!(
            reopened.get(&scope, "asset-2").await.unwrap().asset_id,
            "asset-2"
        );
    }
}
