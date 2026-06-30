//! Unified cost accounting (Issue 007 pricing reconciliation).
//!
//! Providers bill in three different shapes: LLMs by **token tier**
//! ([`ModelPricing`]), ASR/TTS by audio **duration**, and image/video/music
//! generation by produced **asset**. This module folds all three onto one
//! surface — a [`Pricing`] model is applied to a matching [`Meter`] to yield a
//! [`Cost`] — so the runtime has a single place to compute spend regardless of
//! modality.

use orchest_protocol::{ModelPricing, TokenUsage};

/// A computed cost in `currency` units.
#[derive(Debug, Clone, PartialEq)]
pub struct Cost {
    pub currency: String,
    pub amount: f64,
}

impl Cost {
    /// The cost rounded to integer **micros** (millionths of a currency unit) —
    /// the integer accounting unit the runtime persists.
    pub fn micros(&self) -> u64 {
        (self.amount * 1_000_000.0).round().max(0.0) as u64
    }
}

/// The three billing models reconciled onto one surface.
#[derive(Debug, Clone)]
pub enum Pricing {
    /// LLM token-tier pricing (text/audio/image/video/cache), delegating to the
    /// protocol's [`ModelPricing::calculate`].
    Tokens(ModelPricing),
    /// Audio **duration** pricing (ASR/TTS): `per_second` of audio.
    Duration { currency: String, per_second: f64 },
    /// Produced-**asset** pricing (image/video/music): `per_asset`.
    Asset { currency: String, per_asset: f64 },
}

/// The metered quantity for one request, paired with a [`Pricing`] model.
#[derive(Debug, Clone, Copy)]
pub enum Meter<'a> {
    /// Token counts (for [`Pricing::Tokens`]).
    Tokens(&'a TokenUsage),
    /// Audio duration in milliseconds (for [`Pricing::Duration`]).
    DurationMs(u64),
    /// Number of produced assets (for [`Pricing::Asset`]).
    Assets(u64),
}

impl Pricing {
    /// The currency this pricing model bills in.
    pub fn currency(&self) -> &str {
        match self {
            Pricing::Tokens(p) => &p.currency,
            Pricing::Duration { currency, .. } | Pricing::Asset { currency, .. } => currency,
        }
    }

    /// Compute the [`Cost`] for `meter` under this pricing model. A `meter` that
    /// does not match the model (e.g. token counts against duration pricing) is a
    /// zero cost — the caller mixed billing shapes — never a panic.
    pub fn cost(&self, meter: Meter<'_>) -> Cost {
        let amount = match (self, meter) {
            (Pricing::Tokens(pricing), Meter::Tokens(usage)) => pricing.calculate(usage),
            (Pricing::Duration { per_second, .. }, Meter::DurationMs(ms)) => {
                (ms as f64 / 1000.0) * per_second
            }
            (Pricing::Asset { per_asset, .. }, Meter::Assets(count)) => (count as f64) * per_asset,
            // Mismatched pricing/meter shapes contribute nothing.
            _ => 0.0,
        };
        Cost {
            currency: self.currency().to_string(),
            amount,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_pricing_delegates_to_model_pricing() {
        let pricing = Pricing::Tokens(ModelPricing::flat_text("USD", 3.0, 15.0));
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            ..Default::default()
        };
        let cost = pricing.cost(Meter::Tokens(&usage));
        assert_eq!(cost.currency, "USD");
        // 1M input @ $3/M + 1M output @ $15/M = $18
        assert!((cost.amount - 18.0).abs() < 1e-9);
        assert_eq!(cost.micros(), 18_000_000);
    }

    #[test]
    fn duration_pricing_bills_per_second() {
        let pricing = Pricing::Duration {
            currency: "USD".to_string(),
            per_second: 0.01,
        };
        // 2500 ms = 2.5 s @ $0.01/s = $0.025
        let cost = pricing.cost(Meter::DurationMs(2500));
        assert!((cost.amount - 0.025).abs() < 1e-9);
        assert_eq!(cost.micros(), 25_000);
    }

    #[test]
    fn asset_pricing_bills_per_asset() {
        let pricing = Pricing::Asset {
            currency: "USD".to_string(),
            per_asset: 0.04,
        };
        let cost = pricing.cost(Meter::Assets(3));
        assert!((cost.amount - 0.12).abs() < 1e-9);
        assert_eq!(cost.currency, "USD");
    }

    #[test]
    fn mismatched_meter_is_zero_not_a_panic() {
        let pricing = Pricing::Asset {
            currency: "USD".to_string(),
            per_asset: 0.04,
        };
        // Duration meter against asset pricing → 0, currency preserved.
        let cost = pricing.cost(Meter::DurationMs(5000));
        assert_eq!(cost.amount, 0.0);
        assert_eq!(cost.currency, "USD");
    }
}
