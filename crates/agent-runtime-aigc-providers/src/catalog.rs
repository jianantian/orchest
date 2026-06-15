//! Static AIGC (image generation) model catalog for discovery — no credentials needed.
//!
//! Use `list_providers()` to see all supported providers and their model lists,
//! or `list_models()` to iterate all individually enumerable models.
//!
//! Gateway providers (OpenRouter, Renderful) appear as `ImageModelList::Dynamic`.

use std::sync::LazyLock;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// An individually enumerable image generation model.
#[derive(Debug, Clone)]
pub struct ImageModelEntry {
    /// Full model ID to put in `ImageGenerationConfig { model: "..." }`.
    pub model_id: &'static str,
    pub provider: &'static str,
    pub display_name: &'static str,
    /// Supported operations: "text_to_image", "image_to_image", "edit_image", etc.
    pub operations: &'static [&'static str],
}

/// How a provider exposes its model list.
#[derive(Debug, Clone)]
pub enum ImageModelList {
    /// The provider has a fixed, known set of models.
    Known(Vec<ImageModelEntry>),
    /// The provider is a dynamic gateway. See `description` for usage guidance.
    Dynamic {
        description: &'static str,
        model_id_format: &'static str,
        model_id_example: &'static str,
    },
}

/// Top-level metadata about an image generation provider.
#[derive(Debug, Clone)]
pub struct ImageProviderInfo {
    pub provider_id: &'static str,
    pub display_name: &'static str,
    pub models: ImageModelList,
}

// ---------------------------------------------------------------------------
// Static catalog
// ---------------------------------------------------------------------------

static IMAGE_PROVIDERS: LazyLock<Vec<ImageProviderInfo>> = LazyLock::new(build_catalog);

fn aliyun_models() -> ImageProviderInfo {
    // Source: docs/external/aliyun/image-generation.md
    let models = vec![
        // --- Qwen-Image series ---
        ImageModelEntry {
            model_id: "qwen-image-2.0-pro",
            provider: "aliyun",
            display_name: "Aliyun Qwen-Image 2.0 Pro",
            operations: &["text_to_image", "image_to_image", "edit_image"],
        },
        ImageModelEntry {
            model_id: "qwen-image-2.0",
            provider: "aliyun",
            display_name: "Aliyun Qwen-Image 2.0",
            operations: &["text_to_image", "image_to_image", "edit_image"],
        },
        ImageModelEntry {
            model_id: "qwen-image-max",
            provider: "aliyun",
            display_name: "Aliyun Qwen-Image Max",
            operations: &["text_to_image"],
        },
        ImageModelEntry {
            model_id: "qwen-image-plus",
            provider: "aliyun",
            display_name: "Aliyun Qwen-Image Plus",
            operations: &["text_to_image"],
        },
        // --- Wan (万相) series ---
        ImageModelEntry {
            model_id: "wan2.7-image-pro",
            provider: "aliyun",
            display_name: "Aliyun Wan 2.7 Image Pro",
            operations: &["text_to_image", "image_to_image"],
        },
        ImageModelEntry {
            model_id: "wan2.6-t2i",
            provider: "aliyun",
            display_name: "Aliyun Wan 2.6 Text-to-Image",
            operations: &["text_to_image"],
        },
    ];

    ImageProviderInfo {
        provider_id: "aliyun",
        display_name: "Aliyun (DashScope)",
        models: ImageModelList::Known(models),
    }
}

fn crazyrouter_models() -> ImageProviderInfo {
    let models = vec![ImageModelEntry {
        model_id: "gpt-image-2",
        provider: "crazyrouter",
        display_name: "GPT Image 2 (via CrazyRouter)",
        operations: &["text_to_image", "image_to_image", "edit_image"],
    }];

    ImageProviderInfo {
        provider_id: "crazyrouter",
        display_name: "CrazyRouter",
        models: ImageModelList::Known(models),
    }
}

fn openrouter_provider() -> ImageProviderInfo {
    ImageProviderInfo {
        provider_id: "openrouter",
        display_name: "OpenRouter",
        models: ImageModelList::Dynamic {
            description: "OpenRouter is a gateway to many image models. \
                Pass any image-capable model available on openrouter.ai.",
            model_id_format: "<upstream-provider>/<model-name>",
            model_id_example: "openai/gpt-image-1",
        },
    }
}

fn renderful_provider() -> ImageProviderInfo {
    ImageProviderInfo {
        provider_id: "renderful",
        display_name: "Renderful",
        models: ImageModelList::Dynamic {
            description:
                "Renderful is a managed image generation gateway. \
                Available models depend on your plan; query the Renderful API for the current list.",
            model_id_format: "<model-name>",
            model_id_example: "flux-pro",
        },
    }
}

fn build_catalog() -> Vec<ImageProviderInfo> {
    vec![
        aliyun_models(),
        crazyrouter_models(),
        openrouter_provider(),
        renderful_provider(),
    ]
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Returns metadata for all known image generation providers.
pub fn list_providers() -> &'static [ImageProviderInfo] {
    &IMAGE_PROVIDERS
}

/// Returns all individually enumerable image generation models.
/// Dynamic providers are excluded; use `list_providers()` to see them.
pub fn list_models() -> impl Iterator<Item = &'static ImageModelEntry> {
    IMAGE_PROVIDERS.iter().flat_map(|p| match &p.models {
        ImageModelList::Known(models) => models.as_slice(),
        ImageModelList::Dynamic { .. } => &[],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_four_providers() {
        assert_eq!(list_providers().len(), 4);
    }

    #[test]
    fn aliyun_wan_models_present() {
        let ids: Vec<_> = list_models()
            .filter(|m| m.provider == "aliyun")
            .map(|m| m.model_id)
            .collect();
        assert!(
            ids.contains(&"wan2.7-image-pro"),
            "wan2.7 model should be in catalog"
        );
        assert!(ids.contains(&"qwen-image-2.0-pro"));
    }

    #[test]
    fn crazyrouter_has_gpt_image_2() {
        let entry = list_models()
            .find(|m| m.model_id == "gpt-image-2")
            .expect("gpt-image-2 should be in catalog");
        assert_eq!(entry.provider, "crazyrouter");
    }

    #[test]
    fn dynamic_providers_excluded_from_list_models() {
        let providers: Vec<_> = list_models().map(|m| m.provider).collect();
        assert!(!providers.contains(&"openrouter"));
        assert!(!providers.contains(&"renderful"));
    }

    #[test]
    fn openrouter_is_dynamic() {
        let p = list_providers()
            .iter()
            .find(|p| p.provider_id == "openrouter")
            .unwrap();
        assert!(matches!(p.models, ImageModelList::Dynamic { .. }));
    }
}
