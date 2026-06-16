use super::*;
use crate::{
    AssetScope, InMemoryAssetRegistry, LocalAssetStore, ProviderGenerationStatus, ProviderVideoJob,
    VideoContentItem, VideoExecutionConfig, VideoGenerationConfig, VideoTaskListQuery,
};
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};

struct MockVideoProvider {
    video_url: Option<String>,
    last_frame_url: Option<String>,
    create_status: ProviderGenerationStatus,
    terminal_status: ProviderGenerationStatus,
    polls_until_terminal: usize,
    polls: AtomicUsize,
}

#[async_trait]
impl VideoProvider for MockVideoProvider {
    fn provider_name(&self) -> &str {
        "mock"
    }

    fn model_name(&self) -> &str {
        "mock-video"
    }

    async fn create_video_generation(
        &self,
        _request: &VideoGenerationRequest,
    ) -> Result<ProviderVideoJob, AigcError> {
        Ok(ProviderVideoJob {
            id: "job-1".into(),
            status: self.create_status.clone(),
            raw_status: None,
            video_url: None,
            last_frame_url: None,
            error: None,
            metadata: serde_json::json!({}),
        })
    }

    async fn get_video_generation(&self, _job_id: &str) -> Result<ProviderVideoJob, AigcError> {
        let n = self.polls.fetch_add(1, Ordering::SeqCst) + 1;
        let status = if n >= self.polls_until_terminal {
            self.terminal_status.clone()
        } else {
            ProviderGenerationStatus::Running
        };
        let is_terminal_success = status == ProviderGenerationStatus::Completed;
        Ok(ProviderVideoJob {
            id: "job-1".into(),
            status,
            raw_status: None,
            video_url: if is_terminal_success {
                self.video_url.clone()
            } else {
                None
            },
            last_frame_url: if is_terminal_success {
                self.last_frame_url.clone()
            } else {
                None
            },
            error: None,
            metadata: serde_json::json!({}),
        })
    }

    async fn cancel_video_generation(&self, _job_id: &str) -> Result<(), AigcError> {
        Ok(())
    }

    async fn list_video_generations(
        &self,
        _query: &VideoTaskListQuery,
    ) -> Result<(Vec<ProviderVideoJob>, u64), AigcError> {
        Ok((vec![], 0))
    }
}

fn video_request() -> VideoGenerationRequest {
    VideoGenerationRequest {
        content: vec![VideoContentItem::Text {
            text: "a cat playing piano".into(),
        }],
        generation_config: VideoGenerationConfig::default(),
        execution_config: VideoExecutionConfig {
            poll_interval: Some(Duration::from_millis(1)),
            timeout: Some(Duration::from_secs(5)),
        },
        provider_options: serde_json::json!({}),
    }
}

#[tokio::test]
async fn video_gateway_persists_provider_url_and_returns_controlled_url() {
    let dir = tempfile::tempdir().unwrap();
    let provider_video = dir.path().join("provider-video.mp4");
    std::fs::write(&provider_video, b"fake-mp4-bytes").unwrap();
    let provider_video_url = format!("file://{}", provider_video.display());

    let store_dir = tempfile::tempdir().unwrap();
    let gateway = VideoGateway::new(
        Arc::new(MockVideoProvider {
            video_url: Some(provider_video_url.clone()),
            last_frame_url: None,
            create_status: ProviderGenerationStatus::Running,
            terminal_status: ProviderGenerationStatus::Completed,
            polls_until_terminal: 2,
            polls: AtomicUsize::new(0),
        }),
        Arc::new(LocalAssetStore::new(
            store_dir.path(),
            Some("http://localhost/assets".into()),
        )),
        Arc::new(InMemoryAssetRegistry::default()),
        VideoGatewayConfig {
            scope: AssetScope::test(),
            signed_url_ttl: None,
        },
    );

    let response = gateway.generate(video_request()).await.unwrap();

    assert_eq!(response.status, GenerationStatus::Completed);
    let video = response.video.expect("expected a video asset");
    assert!(!video.asset_id.is_empty());
    assert!(
        video.url.starts_with("http://localhost/assets/"),
        "expected our own controlled URL, got: {}",
        video.url
    );
    assert_ne!(video.url, provider_video_url);
}

#[tokio::test]
async fn video_gateway_persists_last_frame_when_present() {
    let dir = tempfile::tempdir().unwrap();
    let provider_video = dir.path().join("provider-video.mp4");
    let provider_frame = dir.path().join("provider-frame.png");
    std::fs::write(&provider_video, b"fake-mp4-bytes").unwrap();
    std::fs::write(&provider_frame, b"fake-png-bytes").unwrap();

    let store_dir = tempfile::tempdir().unwrap();
    let gateway = VideoGateway::new(
        Arc::new(MockVideoProvider {
            video_url: Some(format!("file://{}", provider_video.display())),
            last_frame_url: Some(format!("file://{}", provider_frame.display())),
            create_status: ProviderGenerationStatus::Running,
            terminal_status: ProviderGenerationStatus::Completed,
            polls_until_terminal: 1,
            polls: AtomicUsize::new(0),
        }),
        Arc::new(LocalAssetStore::new(store_dir.path(), None)),
        Arc::new(InMemoryAssetRegistry::default()),
        VideoGatewayConfig {
            scope: AssetScope::test(),
            signed_url_ttl: None,
        },
    );

    let response = gateway.generate(video_request()).await.unwrap();

    assert!(response.video.is_some());
    assert!(response.last_frame.is_some());
}

#[tokio::test]
async fn video_gateway_propagates_provider_failure() {
    let dir = tempfile::tempdir().unwrap();
    let gateway = VideoGateway::new(
        Arc::new(MockVideoProvider {
            video_url: None,
            last_frame_url: None,
            create_status: ProviderGenerationStatus::Failed,
            terminal_status: ProviderGenerationStatus::Failed,
            polls_until_terminal: 0,
            polls: AtomicUsize::new(0),
        }),
        Arc::new(LocalAssetStore::new(dir.path(), None)),
        Arc::new(InMemoryAssetRegistry::default()),
        VideoGatewayConfig {
            scope: AssetScope::test(),
            signed_url_ttl: None,
        },
    );

    let err = gateway.generate(video_request()).await.unwrap_err();

    assert_eq!(err.code, "provider_generation_failed");
}

#[tokio::test]
async fn video_gateway_times_out_when_never_terminal() {
    let dir = tempfile::tempdir().unwrap();
    let gateway = VideoGateway::new(
        Arc::new(MockVideoProvider {
            video_url: Some("https://provider.example/video.mp4".into()),
            last_frame_url: None,
            create_status: ProviderGenerationStatus::Running,
            terminal_status: ProviderGenerationStatus::Running,
            polls_until_terminal: usize::MAX,
            polls: AtomicUsize::new(0),
        }),
        Arc::new(LocalAssetStore::new(dir.path(), None)),
        Arc::new(InMemoryAssetRegistry::default()),
        VideoGatewayConfig {
            scope: AssetScope::test(),
            signed_url_ttl: None,
        },
    );
    let mut request = video_request();
    request.execution_config.timeout = Some(Duration::from_millis(10));
    request.execution_config.poll_interval = Some(Duration::from_millis(1));

    let err = gateway.generate(request).await.unwrap_err();

    assert_eq!(err.code, "provider_generation_timed_out");
}
