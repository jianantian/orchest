mod aliyun;
mod crazyrouter;
mod openrouter;
mod renderful;

pub use aliyun::{AliyunImageAdapter, AliyunImageConfig};
pub use crazyrouter::{CrazyrouterImageAdapter, CrazyrouterImageConfig};
pub use openrouter::{OpenRouterImageAdapter, OpenRouterImageConfig};
pub use renderful::{RenderfulImageAdapter, RenderfulImageConfig};

use std::collections::HashMap;

use serde_json::Value;

use crate::{
    AigcError, CapabilitySource, GenerationExecutionMode, ImageFormat, ImageModelCapabilities,
    ImageOperation, ImageOperationCapability,
};

fn basic_capabilities(
    provider: &str,
    model: &str,
    operations: Vec<ImageOperation>,
) -> ImageModelCapabilities {
    let mut map = HashMap::new();
    for operation in operations {
        map.insert(
            format!("{:?}", operation).to_ascii_lowercase(),
            ImageOperationCapability {
                operation,
                execution_modes: vec![GenerationExecutionMode::Sync],
                max_outputs: Some(4),
                supports_streaming: false,
                supports_transparent_background: true,
                supported_formats: vec![ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::Webp],
            },
        );
    }
    ImageModelCapabilities {
        provider: provider.into(),
        model: model.into(),
        operations: map,
        source: CapabilitySource::Static,
    }
}

fn size_to_string(size: &crate::ImageSize) -> String {
    match size {
        crate::ImageSize::Auto => "auto".into(),
        crate::ImageSize::Pixels { width, height } => format!("{width}x{height}"),
        crate::ImageSize::AspectRatio(value) => value.clone(),
        crate::ImageSize::ResolutionTier(value) => value.clone(),
    }
}

fn ensure_no_unresolved_inputs(
    request: &crate::ImageGenerationRequest,
) -> Result<(), crate::AigcError> {
    for input in &request.inputs {
        match input.asset {
            crate::AssetRef::LocalPath(_) | crate::AssetRef::Stored { .. } => {
                return Err(crate::AigcError::new(
                    "unresolved_input",
                    "adapter received unresolved local or stored input; gateway preprocessing is required",
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

async fn parse_json_response(
    provider: &str,
    response: reqwest::Response,
) -> Result<Value, AigcError> {
    let status = response.status();
    let text = response.text().await.map_err(|err| {
        AigcError::new("provider_response_read_failed", err.to_string()).provider(provider)
    })?;
    if !status.is_success() {
        let upstream_body = serde_json::from_str::<Value>(&text).ok();
        return Err(AigcError {
            code: "provider_http_error".into(),
            message: format!("{provider} returned HTTP {status}"),
            provider: Some(provider.into()),
            status: Some(status.as_u16()),
            upstream_code: upstream_body
                .as_ref()
                .and_then(|body| body.pointer("/error/code").or_else(|| body.get("code")))
                .and_then(|v| v.as_str())
                .map(str::to_string),
            upstream_message: upstream_body
                .as_ref()
                .and_then(|body| {
                    body.pointer("/error/message")
                        .or_else(|| body.get("message"))
                })
                .and_then(|v| v.as_str())
                .map(str::to_string),
            upstream_body,
        });
    }
    serde_json::from_str(&text).map_err(|err| {
        AigcError::new("provider_response_parse_failed", err.to_string()).provider(provider)
    })
}
