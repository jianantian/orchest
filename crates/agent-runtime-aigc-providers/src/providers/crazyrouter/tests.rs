use super::*;
use crate::{
    CompatibilityPolicy, ImageBackground, ImageFormat, ImageGenerationConfig, ImageInput,
    ImageOutputConfig, ImageQuality,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn adapter() -> CrazyrouterImageAdapter {
    CrazyrouterImageAdapter::from_config(CrazyrouterImageConfig {
        model: "gpt-image-1".into(),
        api_key: "key".into(),
        api_url: None,
        timeout: None,
    })
    .unwrap()
}

#[test]
fn generation_maps_to_images_generations() {
    let request = ImageGenerationRequest {
        operation: ImageOperation::TextToImage,
        prompt: "cat".into(),
        negative_prompt: None,
        inputs: vec![],
        generation_config: ImageGenerationConfig::default(),
        execution_config: Default::default(),
        output_config: ImageOutputConfig::default(),
        compatibility_policy: Default::default(),
        provider_options: json!({}),
    };
    let (url, body) = adapter().build_request(&request).unwrap();
    assert!(url.ends_with("/v1/images/generations"));
    assert_eq!(body["prompt"], "cat");
    assert_eq!(body["n"], 1);
}

#[test]
fn response_urls_become_provider_assets() {
    let job = adapter()
        .parse_response(json!({"id": "job", "data": [{"url": "https://provider/image.png"}]}))
        .unwrap();
    assert_eq!(job.assets.len(), 1);
    assert!(matches!(job.assets[0].source, AssetIngestSource::Url(_)));
}

#[test]
fn generation_maps_documented_options_and_rejects_standard_quality() {
    let mut request = ImageGenerationRequest {
        operation: ImageOperation::TextToImage,
        prompt: "cat".into(),
        negative_prompt: None,
        inputs: vec![],
        generation_config: ImageGenerationConfig {
            count: Some(2),
            quality: Some(ImageQuality::Hd),
            format: Some(ImageFormat::Jpeg),
            background: Some(ImageBackground::Opaque),
            safety: Some(crate::SafetyConfig {
                moderation: Some("low".into()),
            }),
            ..Default::default()
        },
        execution_config: crate::GenerationExecutionConfig {
            stream: true,
            partial_image_count: Some(2),
            user: Some("user-1".into()),
            ..Default::default()
        },
        output_config: ImageOutputConfig::default(),
        compatibility_policy: CompatibilityPolicy::Coerce,
        provider_options: json!({ "output_compression": 80 }),
    };
    let (_, body) = adapter().build_request(&request).unwrap();
    assert_eq!(body["quality"], "high");
    assert_eq!(body["output_format"], "jpeg");
    assert_eq!(body["output_compression"], 80);
    assert_eq!(body["background"], "opaque");
    assert_eq!(body["moderation"], "low");
    assert_eq!(body["partial_images"], 2);
    assert_eq!(body["user"], "user-1");

    request.generation_config.quality = Some(ImageQuality::Standard);
    request.compatibility_policy = CompatibilityPolicy::Strict;
    let err = adapter().build_request(&request).unwrap_err();
    assert_eq!(err.code, "unsupported_option");
}

#[tokio::test]
async fn create_generation_posts_to_crazyrouter_api() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
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
        let request = String::from_utf8_lossy(&request);
        let lower_request = request.to_ascii_lowercase();
        assert!(request.starts_with("POST /v1/images/generations "));
        assert!(lower_request.contains("authorization: bearer key"));
        assert!(request.contains("\"prompt\":\"cat\""));
        let body = r#"{"id":"job-real","data":[{"url":"https://provider/image.png"}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let adapter = CrazyrouterImageAdapter::from_config(CrazyrouterImageConfig {
        model: "gpt-image-1".into(),
        api_key: "key".into(),
        api_url: Some(format!("http://{addr}")),
        timeout: None,
    })
    .unwrap();
    let job = adapter
        .create_image_generation(&ImageGenerationRequest {
            operation: ImageOperation::TextToImage,
            prompt: "cat".into(),
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
    assert_eq!(job.id, "job-real");
    assert!(matches!(job.assets[0].source, AssetIngestSource::Url(_)));
}

#[tokio::test]
async fn edit_generation_posts_multipart_to_crazyrouter_api() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
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
        let request = String::from_utf8_lossy(&request);
        let lower_request = request.to_ascii_lowercase();
        assert!(request.starts_with("POST /v1/images/edits "));
        assert!(lower_request.contains("content-type: multipart/form-data"));
        assert!(request.contains("name=\"image[]\""));
        assert!(request.contains("name=\"mask\""));
        assert!(request.contains("source-bytes"));
        assert!(request.contains("mask-bytes"));
        let body = r#"{"id":"edit-real","data":[{"url":"https://provider/edit.png"}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let adapter = CrazyrouterImageAdapter::from_config(CrazyrouterImageConfig {
        model: "gpt-image-2".into(),
        api_key: "key".into(),
        api_url: Some(format!("http://{addr}")),
        timeout: None,
    })
    .unwrap();
    let job = adapter
        .create_image_generation(&ImageGenerationRequest {
            operation: ImageOperation::EditImage,
            prompt: "add rainbow".into(),
            negative_prompt: None,
            inputs: vec![
                ImageInput {
                    role: ImageInputRole::Source,
                    asset: AssetRef::Bytes {
                        bytes: Bytes::from_static(b"source-bytes"),
                        mime_type: "image/png".into(),
                    },
                    mime_type: Some("image/png".into()),
                },
                ImageInput {
                    role: ImageInputRole::Mask,
                    asset: AssetRef::Bytes {
                        bytes: Bytes::from_static(b"mask-bytes"),
                        mime_type: "image/png".into(),
                    },
                    mime_type: Some("image/png".into()),
                },
            ],
            generation_config: ImageGenerationConfig::default(),
            execution_config: Default::default(),
            output_config: ImageOutputConfig::default(),
            compatibility_policy: Default::default(),
            provider_options: json!({}),
        })
        .await
        .unwrap();

    server.await.unwrap();
    assert_eq!(job.id, "edit-real");
    assert!(matches!(job.assets[0].source, AssetIngestSource::Url(_)));
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
