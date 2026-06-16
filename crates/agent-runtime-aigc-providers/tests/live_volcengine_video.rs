//! Live acceptance tests against the real Volcengine Ark video generation API.
//!
//! Requires `ARK_API_KEY` (see `.env` at the repo root — `VOLCENGINE_API_KEY`
//! is the same Ark key and works interchangeably). The gateway test also
//! requires `AIGC_OSS_*` credentials (same as `live_image_gateway.rs`). Run
//! with:
//!
//!   cargo test -p agent-runtime-aigc-providers --test live_volcengine_video -- --ignored --nocapture
//!
//! These tests are slow (video generation takes tens of seconds to a few
//! minutes) and cost real money — run deliberately, not in CI.

use std::env;
use std::sync::Arc;
use std::time::Duration;

use agent_runtime_aigc_providers::{
    create_video_provider_from_config, AigcProviderRuntimeConfig, AssetScope,
    InMemoryAssetRegistry, OssAssetStore, OssStorageConfig, VideoContentItem, VideoGateway,
    VideoGatewayConfig, VideoGenerationConfig, VideoGenerationRequest, VideoTaskListQuery,
};

fn load_dotenv_if_present() {
    let mut path = std::env::current_dir().ok();
    let mut contents = None;
    while let Some(dir) = path {
        let candidate = dir.join(".env");
        if let Ok(value) = std::fs::read_to_string(&candidate) {
            contents = Some(value);
            break;
        }
        path = dir.parent().map(|parent| parent.to_path_buf());
    }
    let Some(contents) = contents else {
        return;
    };
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key
            .trim()
            .strip_prefix("export ")
            .unwrap_or(key.trim())
            .trim();
        if env::var_os(key).is_some() {
            continue;
        }
        env::set_var(key, value.trim().trim_matches('"'));
    }
}

fn provider_config(model: &str) -> AigcProviderRuntimeConfig {
    load_dotenv_if_present();
    let api_key = env::var("ARK_API_KEY").expect("ARK_API_KEY must be set for live test");
    AigcProviderRuntimeConfig {
        provider: "volcengine".into(),
        model: model.into(),
        api_key: Some(api_key),
        timeout: Some(Duration::from_secs(60)),
        ..Default::default()
    }
}

/// End-to-end: create a real video, poll Volcengine until it's done, and
/// verify the gateway downloads the (24h-expiry) provider URL and re-uploads
/// it to our own OSS bucket — the response only ever contains a URL we
/// control, not Volcengine's.
#[tokio::test]
#[ignore = "requires real ARK_API_KEY and AIGC_OSS_* credentials; slow and costs real money"]
async fn live_video_gateway_persists_to_oss_and_returns_controlled_url() {
    let provider = Arc::from(
        create_video_provider_from_config(provider_config("doubao-seedance-1-0-pro-250528"))
            .unwrap(),
    );
    let gateway = VideoGateway::new(
        provider,
        Arc::new(OssAssetStore::new(
            OssStorageConfig::from_env().expect("AIGC_OSS_* config should be set"),
        )),
        Arc::new(InMemoryAssetRegistry::default()),
        VideoGatewayConfig {
            scope: AssetScope {
                tenant: env::var("AIGC_LIVE_TENANT").unwrap_or_else(|_| "live".into()),
                workspace: env::var("AIGC_LIVE_WORKSPACE").unwrap_or_else(|_| "default".into()),
                app: env::var("AIGC_LIVE_APP").unwrap_or_else(|_| "aigc-gateway".into()),
                namespace: "video-smoke".into(),
            },
            signed_url_ttl: Some(Duration::from_secs(900)),
        },
    );

    let response = gateway
        .generate(VideoGenerationRequest {
            content: vec![VideoContentItem::Text {
                text: "A small blue ball bouncing on a white floor".into(),
            }],
            generation_config: VideoGenerationConfig {
                resolution: Some("480p".into()),
                duration_secs: Some(4),
                ..Default::default()
            },
            execution_config: Default::default(),
            provider_options: serde_json::json!({}),
        })
        .await
        .expect("live video gateway generation should succeed");

    let video = response
        .video
        .expect("response should include a video asset");
    assert!(!video.asset_id.is_empty());
    assert!(
        !video.url.starts_with("https://ark-content-generation"),
        "video URL should be our own OSS bucket, not Volcengine's TOS host: {}",
        video.url
    );
    println!("controlled video URL: {}", video.url);
}

#[tokio::test]
#[ignore = "requires real ARK_API_KEY"]
async fn live_create_then_cancel_queued_task() {
    let provider =
        create_video_provider_from_config(provider_config("doubao-seedance-1-0-pro-250528"))
            .unwrap();

    let created = provider
        .create_video_generation(&VideoGenerationRequest {
            content: vec![VideoContentItem::Text {
                text: "A sunset over the ocean".into(),
            }],
            generation_config: VideoGenerationConfig {
                resolution: Some("480p".into()),
                duration_secs: Some(4),
                ..Default::default()
            },
            execution_config: Default::default(),
            provider_options: serde_json::json!({}),
        })
        .await
        .expect("create_video_generation should succeed");

    // Cancel immediately while it should still be queued. If the task already
    // started running, Volcengine rejects cancellation — that's still a valid
    // observed outcome for this smoke test, so don't hard-fail on it.
    let cancel_result = provider.cancel_video_generation(&created.id).await;
    println!("cancel result: {cancel_result:?}");
}

#[tokio::test]
#[ignore = "requires real ARK_API_KEY"]
async fn live_list_video_generations() {
    let provider =
        create_video_provider_from_config(provider_config("doubao-seedance-1-0-pro-250528"))
            .unwrap();

    let (items, total) = provider
        .list_video_generations(&VideoTaskListQuery {
            page_num: Some(1),
            page_size: Some(5),
            ..Default::default()
        })
        .await
        .expect("list_video_generations should succeed");

    println!("total tasks: {total}, returned {} items", items.len());
    assert!(total >= items.len() as u64);
}
