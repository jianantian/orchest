use super::*;
use crate::VideoGenerationConfig;
use serde_json::json;

fn adapter() -> VolcengineVideoAdapter {
    VolcengineVideoAdapter::from_config(VolcengineVideoConfig {
        model: "doubao-seedance-1-0-pro-250528".into(),
        api_key: "test-key".into(),
        api_url: None,
        timeout: None,
    })
    .unwrap()
}

#[test]
fn build_create_request_text_only() {
    let request = VideoGenerationRequest {
        content: vec![VideoContentItem::Text {
            text: "a cat playing piano".into(),
        }],
        generation_config: VideoGenerationConfig {
            resolution: Some("480p".into()),
            duration_secs: Some(4),
            ..Default::default()
        },
        execution_config: Default::default(),
        provider_options: json!({}),
    };
    let body = adapter().build_create_request(&request).unwrap();
    assert_eq!(body["model"], "doubao-seedance-1-0-pro-250528");
    assert_eq!(body["content"][0]["type"], "text");
    assert_eq!(body["content"][0]["text"], "a cat playing piano");
    assert_eq!(body["resolution"], "480p");
    assert_eq!(body["duration"], 4);
}

#[test]
fn build_create_request_with_first_frame_image() {
    let request = VideoGenerationRequest {
        content: vec![
            VideoContentItem::Text {
                text: "zoom in slowly".into(),
            },
            VideoContentItem::Image {
                asset: AssetRef::Url("https://example.com/frame.png".into()),
                role: VideoImageRole::FirstFrame,
            },
        ],
        generation_config: Default::default(),
        execution_config: Default::default(),
        provider_options: json!({}),
    };
    let body = adapter().build_create_request(&request).unwrap();
    assert_eq!(body["content"][1]["type"], "image_url");
    assert_eq!(
        body["content"][1]["image_url"]["url"],
        "https://example.com/frame.png"
    );
    assert_eq!(body["content"][1]["role"], "first_frame");
}

#[test]
fn rejects_empty_content() {
    let request = VideoGenerationRequest {
        content: vec![],
        generation_config: Default::default(),
        execution_config: Default::default(),
        provider_options: json!({}),
    };
    let err = adapter().build_create_request(&request).unwrap_err();
    assert_eq!(err.code, "missing_input");
}

#[test]
fn parse_job_create_response_has_no_status_yet() {
    let job = adapter().parse_job(json!({"id": "cgt-123"}));
    assert_eq!(job.id, "cgt-123");
    assert_eq!(job.status, ProviderGenerationStatus::Queued);
    assert!(job.raw_status.is_none());
}

#[test]
fn parse_job_maps_succeeded_to_completed_with_video_url() {
    let job = adapter().parse_job(json!({
        "id": "cgt-123",
        "status": "succeeded",
        "content": {"video_url": "https://example.com/out.mp4"}
    }));
    assert_eq!(job.status, ProviderGenerationStatus::Completed);
    assert_eq!(job.raw_status, Some("succeeded".into()));
    assert_eq!(job.video_url, Some("https://example.com/out.mp4".into()));
}

#[test]
fn parse_job_maps_running_and_queued() {
    let running = adapter().parse_job(json!({"id": "a", "status": "running"}));
    assert_eq!(running.status, ProviderGenerationStatus::Running);
    let queued = adapter().parse_job(json!({"id": "a", "status": "queued"}));
    assert_eq!(queued.status, ProviderGenerationStatus::Queued);
}

#[test]
fn parse_job_maps_failed_and_cancelled_to_failed() {
    let failed = adapter().parse_job(json!({
        "id": "a", "status": "failed", "error": {"message": "boom"}
    }));
    assert_eq!(failed.status, ProviderGenerationStatus::Failed);
    assert_eq!(failed.error, Some("boom".into()));

    let cancelled = adapter().parse_job(json!({"id": "a", "status": "cancelled"}));
    assert_eq!(cancelled.status, ProviderGenerationStatus::Failed);
    assert_eq!(cancelled.raw_status, Some("cancelled".into()));
}

#[test]
fn parse_job_maps_expired_to_timed_out() {
    let job = adapter().parse_job(json!({"id": "a", "status": "expired"}));
    assert_eq!(job.status, ProviderGenerationStatus::TimedOut);
}
