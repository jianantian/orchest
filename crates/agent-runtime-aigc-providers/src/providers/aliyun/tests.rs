use super::*;
use crate::{
    ImageEditConfig, ImageGenerationConfig, ImageOutputConfig, ImageRegion, ImageSize,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[test]
fn qwen_request_uses_multimodal_shape() {
    let adapter = AliyunImageAdapter::from_config(AliyunImageConfig {
        model: "qwen-image".into(),
        api_key: "key".into(),
        region: None,
        api_url: None,
        timeout: None,
    })
    .unwrap();
    let body = adapter
        .build_request(&ImageGenerationRequest {
            operation: ImageOperation::TextToImage,
            prompt: "mountain".into(),
            negative_prompt: Some("fog".into()),
            inputs: vec![],
            generation_config: ImageGenerationConfig::default(),
            execution_config: Default::default(),
            output_config: ImageOutputConfig::default(),
            compatibility_policy: Default::default(),
            provider_options: json!({}),
        })
        .unwrap();
    assert_eq!(
        body["input"]["messages"][0]["content"][0]["text"],
        "mountain"
    );
    assert_eq!(body["parameters"]["negative_prompt"], "fog");
}

#[test]
fn request_maps_prompt_extend_watermark_wan_options_and_bbox() {
    let adapter = AliyunImageAdapter::from_config(AliyunImageConfig {
        model: "wan2.7-image-pro".into(),
        api_key: "key".into(),
        region: None,
        api_url: None,
        timeout: None,
    })
    .unwrap();
    let body = adapter
        .build_request(&ImageGenerationRequest {
            operation: ImageOperation::EditImage,
            prompt: "replace object".into(),
            negative_prompt: None,
            inputs: vec![],
            generation_config: ImageGenerationConfig {
                size: ImageSize::ResolutionTier("2K".into()),
                edit: Some(ImageEditConfig {
                    regions: vec![ImageRegion::BoundingBox {
                        x: 10,
                        y: 20,
                        width: 30,
                        height: 40,
                    }],
                }),
                ..Default::default()
            },
            execution_config: Default::default(),
            output_config: ImageOutputConfig::default(),
            compatibility_policy: Default::default(),
            provider_options: json!({
                "prompt_extend": true,
                "watermark": false,
                "enable_sequential": true,
                "thinking_mode": false,
                "color_palette": [{"color": "#ff0000", "ratio": 100.0}]
            }),
        })
        .unwrap();
    assert_eq!(body["parameters"]["size"], "2K");
    assert_eq!(body["parameters"]["prompt_extend"], true);
    assert_eq!(body["parameters"]["watermark"], false);
    assert_eq!(body["parameters"]["enable_sequential"], true);
    assert_eq!(body["parameters"]["thinking_mode"], false);
    assert_eq!(body["parameters"]["color_palette"][0]["color"], "#ff0000");
    assert_eq!(body["parameters"]["bbox_list"], json!([[10, 20, 40, 60]]));
}

#[test]
fn wanx_request_uses_prompt_field() {
    let adapter = AliyunImageAdapter::from_config(AliyunImageConfig {
        model: "wanx2.1-t2i-turbo".into(),
        api_key: "key".into(),
        region: None,
        api_url: None,
        timeout: None,
    })
    .unwrap();
    let body = adapter
        .build_request(&ImageGenerationRequest {
            operation: ImageOperation::TextToImage,
            prompt: "mountain".into(),
            negative_prompt: None,
            inputs: vec![],
            generation_config: ImageGenerationConfig::default(),
            execution_config: Default::default(),
            output_config: ImageOutputConfig::default(),
            compatibility_policy: Default::default(),
            provider_options: json!({}),
        })
        .unwrap();
    assert_eq!(body["input"]["prompt"], "mountain");
    assert!(body["input"].get("messages").is_none());
}

#[tokio::test]
async fn wanx_model_posts_to_text2image_endpoint() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut socket).await;
        assert!(request.starts_with("POST /api/v1/services/aigc/text2image/image-synthesis "));
        assert!(request
            .to_ascii_lowercase()
            .contains("x-dashscope-async: enable"));
        assert!(request.contains("\"model\":\"wanx2.1-t2i-turbo\""));
        let body = r#"{"request_id":"req-wanx","output":{"task_id":"task-123","task_status":"PENDING"}}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let adapter = AliyunImageAdapter::from_config(AliyunImageConfig {
        model: "wanx2.1-t2i-turbo".into(),
        api_key: "key".into(),
        region: None,
        api_url: Some(format!("http://{addr}/api/v1")),
        timeout: None,
    })
    .unwrap();
    let job = adapter
        .create_image_generation(&ImageGenerationRequest {
            operation: ImageOperation::TextToImage,
            prompt: "mountain".into(),
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
    assert_eq!(job.id, "task-123");
    assert!(matches!(job.status, ProviderGenerationStatus::Queued));
}

#[tokio::test]
async fn create_generation_posts_to_dashscope_api() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut socket).await;
        let lower_request = request.to_ascii_lowercase();
        assert!(
            request.starts_with("POST /api/v1/services/aigc/multimodal-generation/generation ")
        );
        assert!(lower_request.contains("authorization: bearer key"));
        assert!(request.contains("\"model\":\"qwen-image\""));
        let body = r#"{"request_id":"req-1","output":{"choices":[{"message":{"content":[{"image":"https://dashscope-result/image.png"}]}}]}}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let adapter = AliyunImageAdapter::from_config(AliyunImageConfig {
        model: "qwen-image".into(),
        api_key: "key".into(),
        region: None,
        api_url: Some(format!("http://{addr}/api/v1")),
        timeout: None,
    })
    .unwrap();
    let job = adapter
        .create_image_generation(&ImageGenerationRequest {
            operation: ImageOperation::TextToImage,
            prompt: "mountain".into(),
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
    assert_eq!(job.id, "req-1");
    assert!(matches!(job.assets[0].source, AssetIngestSource::Url(_)));
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
