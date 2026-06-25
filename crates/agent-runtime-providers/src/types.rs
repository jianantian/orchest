//! Re-exported model types from `agent-runtime-model`.
// Re-exported here for backward compatibility.
pub use agent_runtime_model::{
    CacheCapability, CachePolicy, CapabilitySource, CompatibilityPolicy, ContentBlock, JsonSchema,
    MediaSource, Message, ModelAdapter, ModelCapabilities, ModelError, ModelPricing, ModelResponse,
    ModelSpec, OptionAdjustment, ProviderRuntimeConfig, ReasoningCapability, RequestOptions, Role,
    StopReason, StreamEvent, ThinkingLevel, TokenUsage, ToolDef, UpstreamErrorDetail,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinking_level_default_is_medium() {
        assert_eq!(ThinkingLevel::default(), ThinkingLevel::Medium);
    }

    #[test]
    fn cache_policy_default_is_auto() {
        assert_eq!(CachePolicy::default(), CachePolicy::Auto);
    }

    #[test]
    fn compatibility_policy_default_is_coerce() {
        assert_eq!(CompatibilityPolicy::default(), CompatibilityPolicy::Coerce);
    }

    #[test]
    fn request_options_default() {
        let opts = RequestOptions::default();
        assert_eq!(opts.thinking, ThinkingLevel::Medium);
        assert!(opts.include_thinking);
        assert_eq!(opts.compatibility_policy, CompatibilityPolicy::Coerce);
        assert_eq!(opts.cache_policy, CachePolicy::Auto);
        assert!(opts.thinking_budget_tokens.is_none());
        assert!(opts.max_tokens.is_none());
        assert!(opts.temperature.is_none());
        assert!(opts.top_p.is_none());
    }

    #[test]
    fn model_error_internal_constructor() {
        let err = ModelError::internal("something failed", "test_error");
        assert_eq!(err.message, "something failed");
        assert_eq!(err.code.as_deref(), Some("test_error"));
        assert!(err.provider.is_none());
        assert!(err.status.is_none());
        assert!(err.upstream.is_none());
    }

    #[test]
    fn model_error_display() {
        let err = ModelError::internal("something failed", "test_error");
        assert_eq!(format!("{err}"), "something failed");
    }

    #[test]
    fn token_usage_default() {
        let usage = TokenUsage::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.reasoning_tokens, 0);
        assert_eq!(usage.cache_read_tokens, 0);
        assert_eq!(usage.cache_write_tokens, 0);
        assert!(usage.details.is_empty());
    }

    #[test]
    fn model_pricing_calculate_sonnet() {
        let pricing = ModelPricing::flat_text("USD", 3.0, 15.0);
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            ..Default::default()
        };
        let cost = pricing.calculate(&usage);
        assert!((cost - 18.0).abs() < 1e-10);
    }

    #[test]
    fn model_pricing_calculate_with_cache() {
        use agent_runtime_model::PricingRates;
        let pricing = ModelPricing::single_tier(
            "USD",
            PricingRates::text(3.0, 15.0).with_cache(Some(0.3), Some(3.75)),
        );
        let usage = TokenUsage {
            input_tokens: 500_000,
            output_tokens: 100_000,
            cache_read_tokens: 200_000,
            cache_write_tokens: 50_000,
            ..Default::default()
        };
        let cost = pricing.calculate(&usage);
        let expected = 500_000.0 * 3.0 / 1_000_000.0
            + 100_000.0 * 15.0 / 1_000_000.0
            + 200_000.0 * 0.3 / 1_000_000.0
            + 50_000.0 * 3.75 / 1_000_000.0;
        assert!((cost - expected).abs() < 1e-10);
    }

    #[test]
    fn model_pricing_calculate_tiered_with_audio() {
        use agent_runtime_model::{PricingRates, PricingTier};
        // doubao-seed-2-0-mini-260428 三档 (audio surcharge) — verify
        // calculate() picks the right tier and applies audio rate.
        let pricing = ModelPricing {
            currency: "CNY".into(),
            tiers: vec![
                PricingTier {
                    max_input_tokens: Some(32_000),
                    rates: PricingRates::text(0.2, 2.0).with_audio_input(3.0),
                },
                PricingTier {
                    max_input_tokens: Some(128_000),
                    rates: PricingRates::text(0.4, 4.0).with_audio_input(6.0),
                },
                PricingTier {
                    max_input_tokens: None,
                    rates: PricingRates::text(0.8, 8.0).with_audio_input(12.0),
                },
            ],
        };
        // Mid tier: 50K text input + 10K audio input + 1K output
        let usage = TokenUsage {
            input_tokens: 50_000,
            output_tokens: 1_000,
            audio_input_tokens: 10_000,
            ..Default::default()
        };
        let cost = pricing.calculate(&usage);
        // 50_000 * 0.4 + 1_000 * 4 + 10_000 * 6, all / 1_000_000.
        let expected = (50_000.0 * 0.4 + 1_000.0 * 4.0 + 10_000.0 * 6.0) / 1_000_000.0;
        assert!(
            (cost - expected).abs() < 1e-10,
            "got {cost}, expected {expected}"
        );
        // High tier: 200K text input falls into the catch-all
        let usage_high = TokenUsage {
            input_tokens: 200_000,
            output_tokens: 1_000,
            ..Default::default()
        };
        let cost_high = pricing.calculate(&usage_high);
        let expected_high = (200_000.0 * 0.8 + 1_000.0 * 8.0) / 1_000_000.0;
        assert!((cost_high - expected_high).abs() < 1e-10);
    }

    #[test]
    fn content_block_thinking_serde_roundtrip() {
        let block = ContentBlock::Thinking {
            text: Some("let me think...".into()),
            signature: Some("sig123".into()),
            provider_details: Some(serde_json::json!({"key": "value"})),
        };
        let json = serde_json::to_string(&block).unwrap();
        let restored: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(block, restored);
    }

    #[test]
    fn message_serde_roundtrip() {
        let msg = Message {
            role: Role::User,
            content: vec![ContentBlock::Text("hello".into())],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let restored: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, restored);
    }

    #[test]
    fn stream_event_done_serde_roundtrip() {
        let event = StreamEvent::Done {
            usage: TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
                ..Default::default()
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        let restored: StreamEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, restored);
    }

    #[test]
    fn stream_event_thinking_end_with_signature_serde_roundtrip() {
        let event = StreamEvent::ThinkingEnd {
            signature: Some("opaque-sig-abc".into()),
            provider_details: Some(serde_json::json!({"reasoning_id": "r_123"})),
        };
        let json = serde_json::to_string(&event).unwrap();
        let restored: StreamEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, restored);
    }

    #[test]
    fn model_response_empty_adjustments_omitted_from_json() {
        let resp = ModelResponse {
            content: vec![ContentBlock::Text("hi".into())],
            usage: TokenUsage::default(),
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![],
        };
        let json_val: serde_json::Value = serde_json::to_value(&resp).unwrap();
        assert!(json_val.get("option_adjustments").is_none());
    }

    #[test]
    fn model_response_with_adjustments_includes_field() {
        let resp = ModelResponse {
            content: vec![ContentBlock::Text("hi".into())],
            usage: TokenUsage::default(),
            stop_reason: StopReason::EndTurn,
            option_adjustments: vec![OptionAdjustment {
                option: "thinking".into(),
                requested: serde_json::json!("High"),
                applied: serde_json::json!("Medium"),
                reason: "unsupported_effort".into(),
            }],
        };
        let json_val: serde_json::Value = serde_json::to_value(&resp).unwrap();
        assert!(json_val.get("option_adjustments").is_some());
    }

    #[test]
    fn stop_reason_other_serde_preserves_string() {
        let reason = StopReason::Other("custom_reason".into());
        let json = serde_json::to_string(&reason).unwrap();
        let restored: StopReason = serde_json::from_str(&json).unwrap();
        assert_eq!(reason, restored);
        if let StopReason::Other(s) = restored {
            assert_eq!(s, "custom_reason");
        } else {
            panic!("expected Other variant");
        }
    }

    #[test]
    fn model_capabilities_default() {
        let caps = ModelCapabilities::default();
        assert!(!caps.streaming);
        assert!(!caps.tool_use);
        assert!(!caps.parallel_tool_use);
        assert!(caps.max_output_tokens.is_none());
        assert!(caps.context_window_size.is_none());
    }

    #[test]
    fn tool_def_serde_roundtrip() {
        let tool = ToolDef {
            name: "read_file".into(),
            description: "Read a file".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                }
            }),
        };
        let json = serde_json::to_string(&tool).unwrap();
        let restored: ToolDef = serde_json::from_str(&json).unwrap();
        assert_eq!(tool, restored);
    }

    #[test]
    fn model_spec_optional_fields_deserialize() {
        let json = r#"{"provider":"anthropic","model":"claude-sonnet-4"}"#;
        let spec: ModelSpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.provider, "anthropic");
        assert_eq!(spec.model, "claude-sonnet-4");
        assert!(spec.api_key_env.is_none());
        assert!(spec.api_url.is_none());
        assert!(spec.max_tokens.is_none());
        assert!(spec.context_window_size.is_none());
    }
}
