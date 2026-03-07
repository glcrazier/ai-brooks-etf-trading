use brooks_core::timeframe::Timeframe;
use brooks_pa_engine::analyzer::PAConfig;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Top-level strategy configuration. Deserializes from the `[strategy]` section
/// of `config/default.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    /// Strategy name for logging and identification
    pub name: String,
    /// Number of bars to process before generating signals
    pub warm_up_bars: u64,
    /// Risk management parameters
    pub risk: RiskConfig,
    /// Price action analysis parameters
    pub pa: PAStrategyConfig,
    /// Primary analysis timeframe (default: 5min)
    #[serde(default = "default_primary_tf")]
    pub primary_timeframe: Timeframe,
    /// Context (higher) timeframe (default: Daily)
    #[serde(default = "default_context_tf")]
    pub context_timeframe: Timeframe,
}

/// Risk management configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    /// Maximum percentage of capital to risk per trade (e.g. 1.0 = 1%)
    pub max_risk_per_trade_pct: Decimal,
    /// Maximum daily loss as percentage of starting capital (e.g. 3.0 = 3%)
    pub max_daily_loss_pct: Decimal,
    /// Maximum number of simultaneously open positions
    pub max_open_positions: usize,
    /// Starting capital for position sizing
    pub initial_capital: Decimal,
    /// Minimum reward:risk ratio to accept a trade (default: 1.5)
    #[serde(default = "default_min_rr")]
    pub min_reward_risk_ratio: Decimal,
}

/// Price action analysis parameters that map to `PAConfig`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PAStrategyConfig {
    /// EMA period for trend reference
    pub ema_period: usize,
    /// Lookback for swing point detection
    pub swing_lookback: usize,
    /// Body/range ratio threshold for doji classification
    pub doji_threshold: Decimal,
    /// Price cluster tolerance for S/R level grouping
    pub sr_cluster_tolerance: Decimal,
}

impl PAStrategyConfig {
    /// Convert to the PA engine's `PAConfig`, using defaults for fields
    /// not exposed in the strategy configuration.
    pub fn to_pa_config(&self) -> PAConfig {
        PAConfig {
            ema_period: self.ema_period,
            swing_lookback: self.swing_lookback,
            doji_threshold: self.doji_threshold,
            sr_cluster_tolerance: self.sr_cluster_tolerance,
            ..Default::default()
        }
    }
}

fn default_primary_tf() -> Timeframe {
    Timeframe::Minute5
}

fn default_context_tf() -> Timeframe {
    Timeframe::Daily
}

fn default_min_rr() -> Decimal {
    Decimal::new(15, 1) // 1.5
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            name: "brooks_pa".to_string(),
            warm_up_bars: 200,
            risk: RiskConfig::default(),
            pa: PAStrategyConfig::default(),
            primary_timeframe: default_primary_tf(),
            context_timeframe: default_context_tf(),
        }
    }
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_risk_per_trade_pct: Decimal::ONE, // 1%
            max_daily_loss_pct: Decimal::new(3, 0), // 3%
            max_open_positions: 3,
            initial_capital: Decimal::new(100_000, 0),
            min_reward_risk_ratio: default_min_rr(),
        }
    }
}

impl Default for PAStrategyConfig {
    fn default() -> Self {
        Self {
            ema_period: 20,
            swing_lookback: 10,
            doji_threshold: Decimal::new(2, 1), // 0.2
            sr_cluster_tolerance: Decimal::new(5, 3), // 0.005
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    const TOML_CONFIG: &str = r#"
name = "brooks_pa"
warm_up_bars = 200

[risk]
max_risk_per_trade_pct = 1.0
max_daily_loss_pct = 3.0
max_open_positions = 3
initial_capital = 100000.0

[pa]
ema_period = 20
swing_lookback = 10
doji_threshold = 0.2
sr_cluster_tolerance = 0.005
"#;

    #[test]
    fn test_deserialize_from_toml() {
        let config: StrategyConfig = toml::from_str(TOML_CONFIG).unwrap();
        assert_eq!(config.name, "brooks_pa");
        assert_eq!(config.warm_up_bars, 200);
        assert_eq!(config.risk.max_risk_per_trade_pct, dec!(1.0));
        assert_eq!(config.risk.max_daily_loss_pct, dec!(3.0));
        assert_eq!(config.risk.max_open_positions, 3);
        assert_eq!(config.risk.initial_capital, dec!(100000.0));
        assert_eq!(config.pa.ema_period, 20);
        assert_eq!(config.pa.swing_lookback, 10);
        assert_eq!(config.pa.doji_threshold, dec!(0.2));
        assert_eq!(config.pa.sr_cluster_tolerance, dec!(0.005));
    }

    #[test]
    fn test_default_timeframes() {
        let config: StrategyConfig = toml::from_str(TOML_CONFIG).unwrap();
        assert_eq!(config.primary_timeframe, Timeframe::Minute5);
        assert_eq!(config.context_timeframe, Timeframe::Daily);
    }

    #[test]
    fn test_default_min_reward_risk() {
        let config: StrategyConfig = toml::from_str(TOML_CONFIG).unwrap();
        assert_eq!(config.risk.min_reward_risk_ratio, dec!(1.5));
    }

    #[test]
    fn test_strategy_config_default() {
        let config = StrategyConfig::default();
        assert_eq!(config.name, "brooks_pa");
        assert_eq!(config.warm_up_bars, 200);
        assert_eq!(config.primary_timeframe, Timeframe::Minute5);
        assert_eq!(config.context_timeframe, Timeframe::Daily);
    }

    #[test]
    fn test_risk_config_default() {
        let config = RiskConfig::default();
        assert_eq!(config.max_risk_per_trade_pct, dec!(1));
        assert_eq!(config.max_daily_loss_pct, dec!(3));
        assert_eq!(config.max_open_positions, 3);
        assert_eq!(config.initial_capital, dec!(100000));
        assert_eq!(config.min_reward_risk_ratio, dec!(1.5));
    }

    #[test]
    fn test_pa_strategy_config_default() {
        let config = PAStrategyConfig::default();
        assert_eq!(config.ema_period, 20);
        assert_eq!(config.swing_lookback, 10);
        assert_eq!(config.doji_threshold, dec!(0.2));
        assert_eq!(config.sr_cluster_tolerance, dec!(0.005));
    }

    #[test]
    fn test_to_pa_config_conversion() {
        let config = PAStrategyConfig {
            ema_period: 25,
            swing_lookback: 8,
            doji_threshold: dec!(0.15),
            sr_cluster_tolerance: dec!(0.01),
        };
        let pa_config = config.to_pa_config();
        assert_eq!(pa_config.ema_period, 25);
        assert_eq!(pa_config.swing_lookback, 8);
        assert_eq!(pa_config.doji_threshold, dec!(0.15));
        assert_eq!(pa_config.sr_cluster_tolerance, dec!(0.01));
        // Verify defaults are used for fields not in PAStrategyConfig
        assert_eq!(pa_config.max_recent_classifications, 50);
        assert_eq!(pa_config.climax_threshold, 8);
        assert_eq!(pa_config.min_range_bars, 20);
    }

    #[test]
    fn test_roundtrip_serialize_deserialize() {
        let original = StrategyConfig::default();
        let toml_str = toml::to_string(&original).unwrap();
        let deserialized: StrategyConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(deserialized.name, original.name);
        assert_eq!(deserialized.warm_up_bars, original.warm_up_bars);
        assert_eq!(
            deserialized.risk.initial_capital,
            original.risk.initial_capital
        );
    }
}
