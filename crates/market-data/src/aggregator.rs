//! Bar aggregation from real-time ticks.
//!
//! Aggregates individual price ticks into OHLCV bars at timeframe boundaries,
//! respecting China market session hours (09:30-11:30, 13:00-15:00 CST).

use brooks_china_market::session::TradingSession;
use brooks_core::bar::Bar;
use brooks_core::market::SecurityId;
use brooks_core::timeframe::Timeframe;
use chrono::{DateTime, NaiveTime, Utc};
use rust_decimal::Decimal;

/// A partially-built bar that accumulates ticks until its timeframe boundary.
#[derive(Debug, Clone)]
pub(crate) struct PartialBar {
    start_time: DateTime<Utc>,
    open: Decimal,
    high: Decimal,
    low: Decimal,
    close: Decimal,
    volume: u64,
}

impl PartialBar {
    fn new(price: Decimal, volume: u64, timestamp: DateTime<Utc>) -> Self {
        Self {
            start_time: timestamp,
            open: price,
            high: price,
            low: price,
            close: price,
            volume,
        }
    }

    fn update(&mut self, price: Decimal, volume: u64) {
        self.high = self.high.max(price);
        self.low = self.low.min(price);
        self.close = price;
        self.volume += volume;
    }

    fn into_bar(self, security: SecurityId, timeframe: Timeframe) -> Bar {
        Bar {
            timestamp: self.start_time,
            open: self.open,
            high: self.high,
            low: self.low,
            close: self.close,
            volume: self.volume,
            timeframe,
            security,
        }
    }
}

/// Aggregates ticks into bars at timeframe boundaries.
///
/// Handles China market specifics:
/// - Flushes bars at 11:30 (morning close) and 15:00 (market close)
/// - Starts fresh bars after 13:00 (afternoon open)
/// - Aligns bar timestamps to timeframe boundaries (e.g., 9:30, 9:35, 9:40 for 5min)
pub struct BarAggregator {
    security: SecurityId,
    timeframe: Timeframe,
    session: TradingSession,
    current_bar: Option<PartialBar>,
    /// CST offset for converting UTC timestamps to local market time
    cst_offset: chrono::FixedOffset,
}

impl BarAggregator {
    /// Create a new aggregator for the given security and timeframe.
    pub fn new(security: SecurityId, timeframe: Timeframe) -> Self {
        Self {
            security,
            timeframe,
            session: TradingSession::china_a_share(),
            current_bar: None,
            cst_offset: chrono::FixedOffset::east_opt(8 * 3600).unwrap(),
        }
    }

    /// Process a single tick. Returns a completed `Bar` if the tick
    /// crosses a timeframe boundary or session boundary.
    pub fn process_tick(
        &mut self,
        price: Decimal,
        volume: u64,
        timestamp: DateTime<Utc>,
    ) -> Option<Bar> {
        let local_time = timestamp.with_timezone(&self.cst_offset).time();

        // Check if we should flush due to session boundary
        if let Some(bar) = self.check_session_boundary(local_time) {
            // Don't start a new bar with this tick — it arrived at a session
            // boundary (11:30 or 15:00). The next tick in a new session
            // will start a fresh bar.
            return Some(bar);
        }

        // Check boundary before borrowing current_bar mutably
        let crosses_boundary = self.is_boundary(timestamp);

        match &mut self.current_bar {
            Some(partial) => {
                if crosses_boundary {
                    let completed = partial
                        .clone()
                        .into_bar(self.security.clone(), self.timeframe);
                    // Start a new bar
                    self.current_bar = Some(PartialBar::new(price, volume, timestamp));
                    Some(completed)
                } else {
                    partial.update(price, volume);
                    None
                }
            }
            None => {
                self.current_bar = Some(PartialBar::new(price, volume, timestamp));
                None
            }
        }
    }

    /// Force-close the current bar and return it (e.g., at end of day).
    pub fn flush(&mut self) -> Option<Bar> {
        self.current_bar
            .take()
            .map(|partial| partial.into_bar(self.security.clone(), self.timeframe))
    }

    /// Check if the given UTC timestamp crosses a timeframe boundary
    /// relative to the current partial bar.
    fn is_boundary(&self, timestamp: DateTime<Utc>) -> bool {
        let Some(ref partial) = self.current_bar else {
            return false;
        };

        let duration_secs = self.timeframe.duration_secs();
        if duration_secs <= 0 {
            return false;
        }

        // Compute the bar slot (floor to timeframe boundary) for both timestamps
        let start_slot = partial.start_time.timestamp() / duration_secs;
        let tick_slot = timestamp.timestamp() / duration_secs;

        tick_slot > start_slot
    }

    /// Check session boundaries and flush if necessary.
    ///
    /// Returns a completed bar if:
    /// - Local time crosses 11:30 (morning close) while bar is open
    /// - Local time crosses 15:00 (market close) while bar is open
    fn check_session_boundary(&mut self, local_time: NaiveTime) -> Option<Bar> {
        let partial = self.current_bar.as_ref()?;

        let partial_local = partial.start_time.with_timezone(&self.cst_offset).time();

        // Morning close at 11:30 — flush if bar started during morning session
        if partial_local < self.session.morning_close && local_time >= self.session.morning_close {
            return self
                .current_bar
                .take()
                .map(|p| p.into_bar(self.security.clone(), self.timeframe));
        }

        // Market close at 15:00 — flush if bar is open
        if partial_local < self.session.afternoon_close
            && local_time >= self.session.afternoon_close
        {
            return self
                .current_bar
                .take()
                .map(|p| p.into_bar(self.security.clone(), self.timeframe));
        }

        None
    }

    /// Get a reference to the current partial bar, if any.
    #[cfg(test)]
    pub(crate) fn current_partial(&self) -> Option<&PartialBar> {
        self.current_bar.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brooks_core::market::Exchange;
    use chrono::TimeZone;
    use rust_decimal_macros::dec;

    fn cst(hour: u32, min: u32) -> DateTime<Utc> {
        let cst = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
        cst.with_ymd_and_hms(2025, 1, 15, hour, min, 0)
            .single()
            .unwrap()
            .with_timezone(&Utc)
    }

    fn make_aggregator() -> BarAggregator {
        let security = SecurityId::etf("510050", Exchange::SH);
        BarAggregator::new(security, Timeframe::Minute5)
    }

    #[test]
    fn test_first_tick_creates_partial() {
        let mut agg = make_aggregator();
        let result = agg.process_tick(dec!(3.10), 1000, cst(9, 30));
        assert!(result.is_none()); // First tick doesn't complete a bar
        assert!(agg.current_partial().is_some());
    }

    #[test]
    fn test_ticks_within_same_bar() {
        let mut agg = make_aggregator();
        agg.process_tick(dec!(3.10), 1000, cst(9, 30));
        agg.process_tick(dec!(3.15), 2000, cst(9, 31));
        agg.process_tick(dec!(3.08), 500, cst(9, 33));

        let partial = agg.current_partial().unwrap();
        assert_eq!(partial.open, dec!(3.10));
        assert_eq!(partial.high, dec!(3.15));
        assert_eq!(partial.low, dec!(3.08));
        assert_eq!(partial.close, dec!(3.08));
        assert_eq!(partial.volume, 3500);
    }

    #[test]
    fn test_boundary_crossing_completes_bar() {
        let mut agg = make_aggregator();

        // Ticks in the 9:30-9:35 bar
        agg.process_tick(dec!(3.10), 1000, cst(9, 30));
        agg.process_tick(dec!(3.12), 2000, cst(9, 32));

        // Tick at 9:35 crosses the 5-min boundary
        let bar = agg.process_tick(dec!(3.14), 500, cst(9, 35));
        assert!(bar.is_some());

        let bar = bar.unwrap();
        assert_eq!(bar.open, dec!(3.10));
        assert_eq!(bar.high, dec!(3.12));
        assert_eq!(bar.close, dec!(3.12));
        assert_eq!(bar.volume, 3000);
        assert_eq!(bar.timeframe, Timeframe::Minute5);
    }

    #[test]
    fn test_flush_returns_partial_bar() {
        let mut agg = make_aggregator();
        agg.process_tick(dec!(3.10), 1000, cst(9, 30));
        agg.process_tick(dec!(3.12), 500, cst(9, 32));

        let bar = agg.flush();
        assert!(bar.is_some());
        let bar = bar.unwrap();
        assert_eq!(bar.open, dec!(3.10));
        assert_eq!(bar.close, dec!(3.12));
        assert_eq!(bar.volume, 1500);

        // After flush, no partial bar
        assert!(agg.current_partial().is_none());
        assert!(agg.flush().is_none());
    }

    #[test]
    fn test_morning_close_flushes_bar() {
        let mut agg = make_aggregator();

        // Bar starts at 11:25
        agg.process_tick(dec!(3.10), 1000, cst(11, 25));
        agg.process_tick(dec!(3.12), 500, cst(11, 28));

        // Tick at 11:30 triggers morning close flush
        let bar = agg.process_tick(dec!(3.11), 200, cst(11, 30));
        assert!(bar.is_some());
        let bar = bar.unwrap();
        assert_eq!(bar.open, dec!(3.10));
        assert_eq!(bar.volume, 1500); // Only the pre-flush ticks
    }

    #[test]
    fn test_afternoon_close_flushes_bar() {
        let mut agg = make_aggregator();

        // Bar starts at 14:55
        agg.process_tick(dec!(3.20), 1000, cst(14, 55));

        // Tick at 15:00 triggers market close flush
        let bar = agg.process_tick(dec!(3.22), 500, cst(15, 0));
        assert!(bar.is_some());
        let bar = bar.unwrap();
        assert_eq!(bar.open, dec!(3.20));
        assert_eq!(bar.volume, 1000);
    }

    #[test]
    fn test_after_lunch_starts_fresh_bar() {
        let mut agg = make_aggregator();

        // Morning bar
        agg.process_tick(dec!(3.10), 1000, cst(11, 25));
        // 11:30 flush
        let bar = agg.process_tick(dec!(3.12), 500, cst(11, 30));
        assert!(bar.is_some());

        // Afternoon tick starts fresh
        let result = agg.process_tick(dec!(3.15), 800, cst(13, 0));
        // Should not produce a bar (just starting)
        assert!(result.is_none());
        let partial = agg.current_partial().unwrap();
        assert_eq!(partial.open, dec!(3.15));
        assert_eq!(partial.volume, 800);
    }
}
