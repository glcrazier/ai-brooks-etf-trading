use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Which fill model to use for simulating order execution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FillModelType {
    /// Fill at the next bar's open price (+ slippage).
    #[default]
    NextBarOpen,
}

/// Configuration for a backtest run.
/// Deserializes from the `[backtest]` section of config/default.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestConfig {
    /// Start date (YYYY-MM-DD) -- used for display/logging only;
    /// the DataFeed controls which bars are actually replayed.
    pub start_date: String,
    /// End date (YYYY-MM-DD)
    pub end_date: String,
    /// Fill model to use
    #[serde(default)]
    pub fill_model: FillModelType,
    /// Slippage in basis points (e.g., 5 = 0.05%)
    #[serde(default = "default_slippage_bps")]
    pub slippage_bps: u32,
    /// Initial capital for the backtest portfolio
    #[serde(default = "default_initial_capital")]
    pub initial_capital: Decimal,
}

fn default_slippage_bps() -> u32 {
    5
}

fn default_initial_capital() -> Decimal {
    Decimal::new(100_000, 0)
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            start_date: "2024-01-01".to_string(),
            end_date: "2024-12-31".to_string(),
            fill_model: FillModelType::default(),
            slippage_bps: default_slippage_bps(),
            initial_capital: default_initial_capital(),
        }
    }
}

impl BacktestConfig {
    /// Slippage as a Decimal fraction (e.g., 5 bps -> 0.0005).
    pub fn slippage_fraction(&self) -> Decimal {
        Decimal::new(self.slippage_bps as i64, 4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    const TOML_CONFIG: &str = r#"
start_date = "2024-01-01"
end_date = "2024-12-31"
fill_model = "next_bar_open"
slippage_bps = 5
"#;

    #[test]
    fn test_deserialize_from_toml() {
        let config: BacktestConfig = toml::from_str(TOML_CONFIG).unwrap();
        assert_eq!(config.start_date, "2024-01-01");
        assert_eq!(config.end_date, "2024-12-31");
        assert_eq!(config.fill_model, FillModelType::NextBarOpen);
        assert_eq!(config.slippage_bps, 5);
    }

    #[test]
    fn test_default_config() {
        let config = BacktestConfig::default();
        assert_eq!(config.fill_model, FillModelType::NextBarOpen);
        assert_eq!(config.slippage_bps, 5);
        assert_eq!(config.initial_capital, dec!(100000));
    }

    #[test]
    fn test_slippage_fraction() {
        let config = BacktestConfig {
            slippage_bps: 5,
            ..Default::default()
        };
        assert_eq!(config.slippage_fraction(), dec!(0.0005));
    }

    #[test]
    fn test_slippage_fraction_zero() {
        let config = BacktestConfig {
            slippage_bps: 0,
            ..Default::default()
        };
        assert_eq!(config.slippage_fraction(), dec!(0));
    }

    #[test]
    fn test_slippage_fraction_large() {
        let config = BacktestConfig {
            slippage_bps: 100,
            ..Default::default()
        };
        // 100 bps = 1% = 0.01
        assert_eq!(config.slippage_fraction(), dec!(0.0100));
    }

    #[test]
    fn test_defaults_applied_when_missing() {
        let minimal = r#"
start_date = "2024-06-01"
end_date = "2024-06-30"
"#;
        let config: BacktestConfig = toml::from_str(minimal).unwrap();
        assert_eq!(config.fill_model, FillModelType::NextBarOpen);
        assert_eq!(config.slippage_bps, 5);
        assert_eq!(config.initial_capital, dec!(100000));
    }
}
