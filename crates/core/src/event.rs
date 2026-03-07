use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::bar::Bar;
use crate::market::{Exchange, SecurityId};
use crate::order::Order;
use crate::position::Position;
use crate::signal::Signal;
use crate::timeframe::Timeframe;

/// Events related to market data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketEvent {
    /// A new bar has been completed
    BarUpdate {
        security: SecurityId,
        bar: Bar,
        timeframe: Timeframe,
    },
    /// A tick update (real-time price change)
    TickUpdate {
        security: SecurityId,
        price: Decimal,
        volume: u64,
        timestamp: DateTime<Utc>,
    },
    /// Trading session has opened
    SessionOpen { exchange: Exchange },
    /// Trading session has closed
    SessionClose { exchange: Exchange },
    /// Lunch break started
    SessionBreakStart { exchange: Exchange },
    /// Lunch break ended
    SessionBreakEnd { exchange: Exchange },
}

/// Events related to trading operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TradingEvent {
    /// A new trading signal was generated
    SignalGenerated(Signal),
    /// An order was submitted
    OrderSubmitted(Order),
    /// An order was filled
    OrderFilled(Order),
    /// An order was cancelled
    OrderCancelled(Order),
    /// A new position was opened
    PositionOpened(Position),
    /// A position was closed
    PositionClosed {
        position: Position,
        realized_pnl: Decimal,
    },
}
