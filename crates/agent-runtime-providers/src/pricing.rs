//! Centralized model pricing constants.

use agent_runtime_model::ModelPricing;

fn usd(
    input: f64,
    output: f64,
    cache_read: Option<f64>,
    cache_write: Option<f64>,
) -> ModelPricing {
    ModelPricing {
        currency: "USD".into(),
        input_per_million: input,
        output_per_million: output,
        cache_read_per_million: cache_read,
        cache_write_per_million: cache_write,
    }
}

fn cny(input: f64, output: f64) -> ModelPricing {
    ModelPricing {
        currency: "CNY".into(),
        input_per_million: input,
        output_per_million: output,
        cache_read_per_million: None,
        cache_write_per_million: None,
    }
}

/// Pricing for Anthropic Claude models.
///
/// Source: docs/external/anthropic/models.md
pub fn anthropic_pricing(model: &str) -> ModelPricing {
    match model {
        // Fable 5 — $10 / $50
        m if m.starts_with("claude-fable-5") || m.starts_with("claude-mythos-5") => {
            usd(10.0, 50.0, Some(1.0), Some(12.5))
        }
        // Opus 4.8 / 4.7 / 4.6 / 4.5 — $5 / $25
        m if m.starts_with("claude-opus-4") => usd(5.0, 25.0, Some(0.5), Some(6.25)),
        // Sonnet 4.6 / 4.5 — $3 / $15
        m if m.starts_with("claude-sonnet-4") => usd(3.0, 15.0, Some(0.3), Some(3.75)),
        // Haiku 4.5 — $1 / $5
        m if m.starts_with("claude-haiku-4") => usd(1.0, 5.0, Some(0.1), Some(1.25)),
        _ => usd(3.0, 15.0, None, None),
    }
}

/// Pricing for OpenAI models.
///
/// Source: https://developers.openai.com/api/docs/models/all
pub fn openai_pricing(model: &str) -> ModelPricing {
    match model {
        // gpt-5.5 — $5 / $30
        m if m.starts_with("gpt-5.5") => usd(5.0, 30.0, None, None),
        // gpt-5.4-mini — $0.75 / $4.50 (must precede gpt-5.4 base)
        m if m.starts_with("gpt-5.4-mini") => usd(0.75, 4.5, None, None),
        // gpt-5.4-nano — $0.20 / $1.25 (must precede gpt-5.4 base)
        m if m.starts_with("gpt-5.4-nano") => usd(0.20, 1.25, None, None),
        // gpt-5.4 (base) — $2.50 / $15
        m if m.starts_with("gpt-5.4") => usd(2.5, 15.0, None, None),
        _ => usd(2.5, 15.0, None, None),
    }
}

/// Pricing for DeepSeek models.
///
/// Source: https://api-docs.deepseek.com/zh-cn/quick_start/pricing
pub fn deepseek_pricing(model: &str) -> ModelPricing {
    match model {
        // v4-flash (non-thinking): ¥1 / ¥2 per MTok
        // v4-flash (thinking):     ¥4 / ¥16 per MTok  — reported at runtime, not here
        "deepseek-v4-flash" | "deepseek-chat" => cny(1.0, 2.0),
        // v4-pro: ¥3 / ¥6 per MTok
        "deepseek-v4-pro" | "deepseek-reasoner" => cny(3.0, 6.0),
        _ => cny(1.0, 2.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fable5_pricing() {
        let p = anthropic_pricing("claude-fable-5");
        assert_eq!(p.input_per_million, 10.0);
        assert_eq!(p.output_per_million, 50.0);
        assert_eq!(p.currency, "USD");
    }

    #[test]
    fn opus4_pricing() {
        let p = anthropic_pricing("claude-opus-4-8");
        assert_eq!(p.input_per_million, 5.0);
        assert_eq!(p.output_per_million, 25.0);
    }

    #[test]
    fn sonnet4_pricing() {
        let p = anthropic_pricing("claude-sonnet-4-6");
        assert_eq!(p.input_per_million, 3.0);
        assert_eq!(p.output_per_million, 15.0);
    }

    #[test]
    fn haiku4_pricing() {
        let p = anthropic_pricing("claude-haiku-4-5-20251001");
        assert_eq!(p.input_per_million, 1.0);
        assert_eq!(p.output_per_million, 5.0);
    }

    #[test]
    fn gpt54_mini_pricing() {
        let p = openai_pricing("gpt-5.4-mini");
        assert_eq!(p.input_per_million, 0.75);
        assert_eq!(p.currency, "USD");
    }

    #[test]
    fn deepseek_v4_flash_pricing() {
        let p = deepseek_pricing("deepseek-v4-flash");
        assert_eq!(p.input_per_million, 1.0);
        assert_eq!(p.currency, "CNY");
    }

    #[test]
    fn deepseek_v4_pro_pricing() {
        let p = deepseek_pricing("deepseek-v4-pro");
        assert_eq!(p.input_per_million, 3.0);
    }
}
