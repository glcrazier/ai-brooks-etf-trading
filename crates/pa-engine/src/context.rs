use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use crate::bar_analysis::BarClassification;
use crate::breakout::BreakoutEvent;
use crate::channel::Channel;
use crate::pattern::PricePattern;
use crate::support_resistance::SRLevel;
use crate::trading_range::TradingRange;
use crate::trend::TrendState;

/// Complete snapshot of the current market context after price action analysis.
/// This is the main output of the PA engine, consumed by the strategy layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketContext {
    /// Current trend state
    pub trend: TrendState,
    /// Trend strength from 0.0 to 1.0
    pub trend_strength: f64,
    /// Whether we're currently in a trading range
    pub in_trading_range: bool,
    /// Current trading range, if any
    pub current_range: Option<TradingRange>,
    /// Active channel pattern, if any
    pub active_channel: Option<Channel>,
    /// Nearest support level below current price
    pub nearest_support: Option<SRLevel>,
    /// Nearest resistance level above current price
    pub nearest_resistance: Option<SRLevel>,
    /// Active (pending or confirmed) breakouts
    pub active_breakouts: Vec<BreakoutEvent>,
    /// Recently detected price patterns
    pub recent_patterns: Vec<PricePattern>,
    /// Number of consecutive bull bars
    pub consecutive_bull_bars: u32,
    /// Number of consecutive bear bars
    pub consecutive_bear_bars: u32,
    /// Whether we're in a potential climax
    pub is_climax: bool,
    /// Recent bar classifications (most recent last)
    pub bar_classifications: VecDeque<BarClassification>,
    /// Current 20-period EMA value
    pub ema: Option<Decimal>,
    /// Current price (last bar's close)
    pub current_price: Decimal,
    /// Total bars processed
    pub bar_count: u64,
}

impl Default for MarketContext {
    fn default() -> Self {
        Self {
            trend: TrendState::TradingRange,
            trend_strength: 0.0,
            in_trading_range: false,
            current_range: None,
            active_channel: None,
            nearest_support: None,
            nearest_resistance: None,
            active_breakouts: Vec::new(),
            recent_patterns: Vec::new(),
            consecutive_bull_bars: 0,
            consecutive_bear_bars: 0,
            is_climax: false,
            bar_classifications: VecDeque::new(),
            ema: None,
            current_price: Decimal::ZERO,
            bar_count: 0,
        }
    }
}

impl MarketContext {
    /// Whether conditions favor a long entry
    pub fn favors_long(&self) -> bool {
        self.trend.is_bull() && !self.is_climax
    }

    /// Whether conditions favor a short entry
    pub fn favors_short(&self) -> bool {
        self.trend.is_bear() && !self.is_climax
    }

    /// Whether we're near support (within the nearest S/R range)
    pub fn near_support(&self) -> bool {
        self.nearest_support.is_some()
    }

    /// Whether we're near resistance
    pub fn near_resistance(&self) -> bool {
        self.nearest_resistance.is_some()
    }
}
