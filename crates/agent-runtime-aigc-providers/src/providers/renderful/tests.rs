use super::*;
use crate::{AssetRef, ImageGenerationConfig, ImageInput, ImageInputRole, ImageOutputConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[test]
fn rejects_unresolved_stored_inputs() {
    let adapter = RenderfulImageAdapter::from_config(RenderfulImageConfig {
        model: "flux".into(),
        api_key: "key".into(),
        api_url: None,
        timeout: None,
        webhook: None,
    })
    .unwrap();
    let err = adapter
        .build_request(&ImageGenerationRequest {
            operation: ImageOperation::ImageToImage,
            prompt: "edit".into(),
            negative_prompt: None,
            inputs: vec![ImageInput {
                role: ImageInputRole::Source,
                asset: AssetRef::Stored {
                    asset_id: "asset".into(),
                },
                mime_type: None,
            }],
            generation_config: ImageGenerationConfig::default(),
            execution_config: Default::default(),
            output_config: ImageOutputConfig::default(),
            compatibility_policy: Default::default(),
            provider_options: json!({}),
        })
        .unwrap_err();
    assert_eq!(err.code, "unresolved_input");
}

#[test]
fn image_to_image_maps_resolved_url_input() {
    let adapter = RenderfulImageAdapter::from_config(RenderfulImageConfig {
        model: "grok-imagine-image-i2i".into(),
        api_key: "key".into(),
        api_url: None,
        timeout: None,
        webhook: None,
    })
    .unwrap();
    let body = adapter
        .build_request(&ImageGenerationRequest {
            operation: ImageOperation::ImageToImage,
            prompt: "restyle".into(),
            negative_prompt: None,
            inputs: vec![ImageInput {
                role: ImageInputRole::Source,
                asset: AssetRef::Url("https://assets.example/input.png".into()),
                mime_type: None,
            }],
            generation_config: ImageGenerationConfig::default(),
            execution_config: Default::default(),
            output_config: ImageOutputConfig::default(),
            compatibility_policy: Default::default(),
            provider_options: json!({}),
        })
        .unwrap();

    assert_eq!(body["type"], "image-to-image");
    assert_eq!(body["image_url"], "https://assets.example/input.png");
}

#[test]
fn model_metadata_parser_records_renderful_capabilities() {
    let caps = RenderfulImageAdapter::parse_models_metadata(
        "flux-dev",
        "text-to-image",
        json!({
            "models": [{
                "id": "flux-dev",
                "max_outputs": 4,
                "aspect_ratios": ["1:1", "16:9"],
                "resolutions": ["1024x1024"],
                "cost": {"min": 0.01, "max": 0.04},
                "supports_webhook": true
            }]
        }),
    )
    .unwrap();
    let op = caps.operations.get("texttoimage").unwrap();
    assert_eq!(caps.source, crate::CapabilitySource::ProviderMetadata);
    assert_eq!(op.max_outputs, Some(4));
    assert_eq!(op.metadata["aspect_ratios"], json!(["1:1", "16:9"]));
    assert_eq!(op.metadata["supports_webhook"], true);
}

#[test]
fn failed_poll_response_preserves_provider_error_details() {
    let adapter = RenderfulImageAdapter::from_config(RenderfulImageConfig {
        model: "flux".into(),
        api_key: "key".into(),
        api_url: None,
        timeout: None,
        webhook: None,
    })
    .unwrap();
    let job = adapter.parse_poll_response(json!({
        "id": "gen_failed",
        "status": "failed",
        "error": {"code": "bad_prompt", "message": "prompt rejected"}
    }));
    assert_eq!(job.status, ProviderGenerationStatus::Failed);
    assert_eq!(job.metadata["error"]["code"], "bad_prompt");
}

#[tokio::test]
async fn create_and_get_generation_use_renderful_api() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut create_socket, _) = listener.accept().await.unwrap();
        let create_request = read_http_request(&mut create_socket).await;
        let lower_create_request = create_request.to_ascii_lowercase();
        assert!(create_request.starts_with("POST /api/v1/generations "));
        assert!(lower_create_request.contains("authorization: bearer key"));
        assert!(create_request.contains("\"type\":\"text-to-image\""));
        let create_body = r#"{"id":"gen_1","status":"processing","outputs":[]}"#;
        let create_response = format!(
                "HTTP/1.1 200 OK\r\nconnection: close\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                create_body.len(),
                create_body
            );
        create_socket
            .write_all(create_response.as_bytes())
            .await
            .unwrap();

        let (mut get_socket, _) = listener.accept().await.unwrap();
        let get_request = read_http_request(&mut get_socket).await;
        let lower_get_request = get_request.to_ascii_lowercase();
        assert!(get_request.starts_with("GET /api/v1/generations/gen_1 "));
        assert!(lower_get_request.contains("authorization: bearer key"));
        let get_body =
            r#"{"id":"gen_1","status":"completed","outputs":["https://provider/image.png"]}"#;
        let get_response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            get_body.len(),
            get_body
        );
        get_socket.write_all(get_response.as_bytes()).await.unwrap();
    });

    let adapter = RenderfulImageAdapter::from_config(RenderfulImageConfig {
        model: "flux-dev".into(),
        api_key: "key".into(),
        api_url: Some(format!("http://{addr}/api/v1")),
        timeout: None,
        webhook: None,
    })
    .unwrap();
    let created = adapter
        .create_image_generation(&ImageGenerationRequest {
            operation: ImageOperation::TextToImage,
            prompt: "sunset".into(),
            negative_prompt: None,
            inputs: vec![],
            generation_config: ImageGenerationConfig::default(),
            execution_config: Default::default(),
            output_config: ImageOutputConfig::default(),
            compatibility_policy: Default::default(),
            provider_options: json!({}),
        })
        .await
        .unwrap();
    let completed = adapter.get_image_generation(&created.id).await.unwrap();

    server.await.unwrap();
    assert_eq!(created.status, ProviderGenerationStatus::Running);
    assert_eq!(completed.status, ProviderGenerationStatus::Completed);
    assert!(matches!(
        completed.assets[0].source,
        AssetIngestSource::Url(_)
    ));
}

async fn read_http_request(socket: &mut tokio::net::TcpStream) -> String {
    let mut request = Vec::new();
    let mut buf = [0; 1024];
    loop {
        let n = socket.read(&mut buf).await.unwrap();
        if n == 0 {
            break;
        }
        request.extend_from_slice(&buf[..n]);
        if request_is_complete(&request) {
            break;
        }
    }
    String::from_utf8(request).unwrap()
}

fn request_is_complete(request: &[u8]) -> bool {
    let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let headers = String::from_utf8_lossy(&request[..header_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0);
    request.len() >= header_end + 4 + content_length
}
