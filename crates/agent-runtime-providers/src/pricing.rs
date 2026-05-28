//! Centralized model pricing constants.
//!
//! Each provider references these tables instead of inlining dollar amounts,
//! making price updates a single-file change.

use agent_runtime_model::ModelPricing;

/// Pricing for Anthropic Claude models.
pub fn anthropic_pricing(model: &str) -> ModelPricing {
    match model {
        m if m.contains("claude-opus-4") => ModelPricing {
            input_per_million_usd: 15.0,
            output_per_million_usd: 75.0,
            cache_read_per_million_usd: Some(1.5),
            cache_write_per_million_usd: Some(18.75),
        },
        m if m.contains("claude-sonnet-4") => ModelPricing {
            input_per_million_usd: 3.0,
            output_per_million_usd: 15.0,
            cache_read_per_million_usd: Some(0.3),
            cache_write_per_million_usd: Some(3.75),
        },
        m if m.contains("claude-haiku-4") => ModelPricing {
            input_per_million_usd: 0.8,
            output_per_million_usd: 4.0,
            cache_read_per_million_usd: Some(0.08),
            cache_write_per_million_usd: Some(1.0),
        },
        _ => ModelPricing {
            input_per_million_usd: 3.0,
            output_per_million_usd: 15.0,
            cache_read_per_million_usd: None,
            cache_write_per_million_usd: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opus_pricing() {
        let p = anthropic_pricing("claude-opus-4-20250514");
        assert_eq!(p.input_per_million_usd, 15.0);
        assert_eq!(p.output_per_million_usd, 75.0);
    }

    #[test]
    fn sonnet_pricing() {
        let p = anthropic_pricing("claude-sonnet-4-20250514");
        assert_eq!(p.input_per_million_usd, 3.0);
    }

    #[test]
    fn haiku_pricing() {
        let p = anthropic_pricing("claude-haiku-4-20250514");
        assert_eq!(p.input_per_million_usd, 0.8);
    }

    #[test]
    fn unknown_model_defaults_to_sonnet() {
        let p = anthropic_pricing("claude-unknown-99");
        assert_eq!(p.input_per_million_usd, 3.0);
        assert!(p.cache_read_per_million_usd.is_none());
    }
}
