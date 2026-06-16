use super::*;
use crate::{ImageGenerationConfig, ImageOutputConfig, ImageSize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[test]
fn request_uses_chat_completions_image_config() {
    let adapter = OpenRouterImageAdapter::from_config(OpenRouterImageConfig {
        model: "google/gemini".into(),
        api_key: "key".into(),
        api_url: None,
        timeout: None,
        app_title: None,
        site_url: None,
    })
    .unwrap();
    let body = adapter
        .build_request(&ImageGenerationRequest {
            operation: ImageOperation::TextToImage,
            prompt: "poster".into(),
            negative_prompt: None,
            inputs: vec![],
            generation_config: ImageGenerationConfig {
                size: ImageSize::AspectRatio("16:9".into()),
                ..Default::default()
            },
            execution_config: Default::default(),
            output_config: ImageOutputConfig::default(),
            compatibility_policy: Default::default(),
            provider_options: json!({}),
        })
        .unwrap();
    assert_eq!(body["modalities"][0], "image");
    assert_eq!(body["image_config"]["aspect_ratio"], "16:9");
}

#[test]
fn rejects_unverified_image_to_image_mapping() {
    let adapter = OpenRouterImageAdapter::from_config(OpenRouterImageConfig {
        model: "google/gemini".into(),
        api_key: "key".into(),
        api_url: None,
        timeout: None,
        app_title: None,
        site_url: None,
    })
    .unwrap();
    let err = adapter
        .build_request(&ImageGenerationRequest {
            operation: ImageOperation::ImageToImage,
            prompt: "restyle".into(),
            negative_prompt: None,
            inputs: vec![],
            generation_config: ImageGenerationConfig::default(),
            execution_config: Default::default(),
            output_config: ImageOutputConfig::default(),
            compatibility_policy: Default::default(),
            provider_options: json!({"model_specific": true}),
        })
        .unwrap_err();

    assert_eq!(err.code, "unsupported_operation");
}

#[test]
fn image_config_maps_model_specific_provider_options_and_stream_deltas() {
    let adapter = OpenRouterImageAdapter::from_config(OpenRouterImageConfig {
        model: "recraft/recraft-v3".into(),
        api_key: "key".into(),
        api_url: None,
        timeout: None,
        app_title: None,
        site_url: None,
    })
    .unwrap();
    let body = adapter
        .build_request(&ImageGenerationRequest {
            operation: ImageOperation::TextToImage,
            prompt: "poster".into(),
            negative_prompt: None,
            inputs: vec![],
            generation_config: ImageGenerationConfig {
                size: ImageSize::ResolutionTier("4K".into()),
                style: Some(crate::ImageStyleConfig {
                    style: Some("Photorealism".into()),
                    colors: vec!["#ff0000".into(), "#008000".into()],
                }),
                ..Default::default()
            },
            execution_config: Default::default(),
            output_config: ImageOutputConfig::default(),
            compatibility_policy: Default::default(),
            provider_options: json!({
                "include_text": true,
                "strength": 0.7,
                "text_layout": [{"text": "SALE", "bbox": [0, 0, 100, 100]}]
            }),
        })
        .unwrap();
    assert_eq!(body["modalities"], json!(["image", "text"]));
    assert_eq!(body["image_config"]["image_size"], "4K");
    assert_eq!(body["image_config"]["strength"], 0.7);
    assert_eq!(body["image_config"]["style"], "Photorealism");
    assert_eq!(
        body["image_config"]["rgb_colors"],
        json!([[255, 0, 0], [0, 128, 0]])
    );
    assert_eq!(body["image_config"]["text_layout"][0]["text"], "SALE");

    let events = adapter.parse_stream_delta(json!({
        "choices": [{"delta": {"images": [{"image_url": {"url": "data:image/png;base64,cG5n"}}]}}]
    }));
    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0],
        crate::ProviderImageEvent::PartialAsset { .. }
    ));
}

#[test]
fn model_metadata_parser_records_image_modalities() {
    let caps = OpenRouterImageAdapter::parse_model_metadata(json!({
        "id": "google/gemini-2.5-flash-image",
        "architecture": {
            "input_modalities": ["text"],
            "output_modalities": ["image", "text"]
        },
        "supported_parameters": ["image_config"]
    }))
    .unwrap();
    let op = caps.operations.get("texttoimage").unwrap();
    assert_eq!(caps.source, crate::CapabilitySource::ProviderMetadata);
    assert_eq!(op.metadata["output_modalities"], json!(["image", "text"]));
}

#[tokio::test]
async fn create_generation_posts_to_openrouter_api() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut socket).await;
        let lower_request = request.to_ascii_lowercase();
        assert!(request.starts_with("POST /api/v1/chat/completions "));
        assert!(lower_request.contains("authorization: bearer key"));
        assert!(request.contains("\"modalities\":[\"image\",\"text\"]"));
        let body = r#"{"id":"or-1","choices":[{"message":{"images":[{"image_url":{"url":"data:image/png;base64,cG5n"}}]}}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let adapter = OpenRouterImageAdapter::from_config(OpenRouterImageConfig {
        model: "google/gemini-2.5-flash-image".into(),
        api_key: "key".into(),
        api_url: Some(format!("http://{addr}/api/v1")),
        timeout: None,
        app_title: None,
        site_url: None,
    })
    .unwrap();
    let job = adapter
        .create_image_generation(&ImageGenerationRequest {
            operation: ImageOperation::TextToImage,
            prompt: "poster".into(),
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

    server.await.unwrap();
    assert_eq!(job.id, "or-1");
    assert!(matches!(
        job.assets[0].source,
        AssetIngestSource::DataUrl(_)
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
