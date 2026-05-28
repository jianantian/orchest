//! Budget tracking and enforcement for agent runs.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::model::TokenUsage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    pub max_tokens: Option<u64>,
    pub max_tool_calls: Option<u32>,
    pub max_duration: Option<Duration>,
    pub max_cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BudgetUsage {
    pub tokens_used: u64,
    pub tool_calls_used: u32,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum BudgetViolation {
    #[error("token limit exceeded")]
    MaxTokensExceeded,
    #[error("tool call limit exceeded")]
    MaxToolCallsExceeded,
    #[error("duration limit exceeded")]
    MaxDurationExceeded,
    #[error("cost limit exceeded")]
    MaxCostExceeded,
}

pub struct BudgetGuard {
    config: BudgetConfig,
    usage: BudgetUsage,
    start: Instant,
}

impl BudgetGuard {
    pub fn new(config: BudgetConfig) -> Self {
        Self {
            config,
            usage: BudgetUsage::default(),
            start: Instant::now(),
        }
    }

    pub fn record_model_call(&mut self, token_usage: &TokenUsage) {
        self.usage.tokens_used += token_usage.input_tokens + token_usage.output_tokens;
        if let Some(cost) = token_usage.cost_usd {
            self.usage.cost_usd += cost;
        }
    }

    pub fn record_tool_call(&mut self) {
        self.usage.tool_calls_used += 1;
    }

    pub fn record_external_usage(&mut self, usage: &BudgetUsage) {
        self.usage.tokens_used += usage.tokens_used;
        self.usage.tool_calls_used += usage.tool_calls_used;
        self.usage.cost_usd += usage.cost_usd;
    }

    pub fn remaining_config(&self) -> BudgetConfig {
        BudgetConfig {
            max_tokens: self
                .config
                .max_tokens
                .map(|max| max.saturating_sub(self.usage.tokens_used)),
            max_tool_calls: self
                .config
                .max_tool_calls
                .map(|max| max.saturating_sub(self.usage.tool_calls_used)),
            max_duration: self
                .config
                .max_duration
                .map(|max| max.saturating_sub(self.start.elapsed())),
            max_cost_usd: self
                .config
                .max_cost_usd
                .map(|max| (max - self.usage.cost_usd).max(0.0)),
        }
    }

    pub fn check(&self) -> Option<BudgetViolation> {
        if let Some(max) = self.config.max_tokens {
            if self.usage.tokens_used > max {
                return Some(BudgetViolation::MaxTokensExceeded);
            }
        }
        if let Some(max) = self.config.max_tool_calls {
            if self.usage.tool_calls_used > max {
                return Some(BudgetViolation::MaxToolCallsExceeded);
            }
        }
        if let Some(max) = self.config.max_duration {
            if self.start.elapsed() > max {
                return Some(BudgetViolation::MaxDurationExceeded);
            }
        }
        if let Some(max) = self.config.max_cost_usd {
            if self.usage.cost_usd > max {
                return Some(BudgetViolation::MaxCostExceeded);
            }
        }
        None
    }

    pub fn usage(&self) -> &BudgetUsage {
        &self.usage
    }

    pub fn config(&self) -> &BudgetConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_limits_never_violates() {
        let guard = BudgetGuard::new(BudgetConfig {
            max_tokens: None,
            max_tool_calls: None,
            max_duration: None,
            max_cost_usd: None,
        });
        assert!(guard.check().is_none());
    }

    #[test]
    fn token_limit_exceeded() {
        let mut guard = BudgetGuard::new(BudgetConfig {
            max_tokens: Some(100),
            max_tool_calls: None,
            max_duration: None,
            max_cost_usd: None,
        });
        guard.record_model_call(&TokenUsage {
            input_tokens: 60,
            output_tokens: 50,
            ..Default::default()
        });
        assert!(matches!(
            guard.check(),
            Some(BudgetViolation::MaxTokensExceeded)
        ));
    }

    #[test]
    fn tool_call_limit_exceeded() {
        let mut guard = BudgetGuard::new(BudgetConfig {
            max_tokens: None,
            max_tool_calls: Some(2),
            max_duration: None,
            max_cost_usd: None,
        });
        guard.record_tool_call();
        guard.record_tool_call();
        assert!(guard.check().is_none());
        guard.record_tool_call();
        assert!(matches!(
            guard.check(),
            Some(BudgetViolation::MaxToolCallsExceeded)
        ));
    }

    #[test]
    fn cost_tracking() {
        let mut guard = BudgetGuard::new(BudgetConfig {
            max_tokens: None,
            max_tool_calls: None,
            max_duration: None,
            max_cost_usd: Some(0.01),
        });
        guard.record_model_call(&TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 0,
            cost_usd: Some(3.0),
            ..Default::default()
        });
        assert!(guard.usage().cost_usd > 2.9);
        assert!(matches!(
            guard.check(),
            Some(BudgetViolation::MaxCostExceeded)
        ));
    }

    #[test]
    fn budget_skips_cost_when_adapter_reports_none() {
        let mut guard = BudgetGuard::new(BudgetConfig {
            max_tokens: None,
            max_tool_calls: None,
            max_duration: None,
            max_cost_usd: Some(1.0),
        });
        guard.record_model_call(&TokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
            cost_usd: None,
            ..Default::default()
        });
        assert_eq!(guard.usage().cost_usd, 0.0);
    }

    #[test]
    fn budget_accumulates_reported_cost() {
        let mut guard = BudgetGuard::new(BudgetConfig {
            max_tokens: None,
            max_tool_calls: None,
            max_duration: None,
            max_cost_usd: None,
        });
        guard.record_model_call(&TokenUsage {
            cost_usd: Some(0.01),
            ..Default::default()
        });
        guard.record_model_call(&TokenUsage {
            cost_usd: Some(0.02),
            ..Default::default()
        });
        assert!((guard.usage().cost_usd - 0.03).abs() < 1e-10);
    }
}
