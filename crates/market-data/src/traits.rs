use async_trait::async_trait;
use brooks_core::bar::Bar;
use brooks_core::event::MarketEvent;
use brooks_core::market::SecurityId;
use brooks_core::timeframe::Timeframe;
use chrono::{DateTime, Utc};
use tokio::sync::mpsc;

use crate::error::MarketDataError;

/// Async provider for market data — historical fetch and real-time subscription.
///
/// Implemented by `FutuMarketDataProvider` for live/paper trading,
/// and by `MockMarketDataProvider` for testing.
#[async_trait]
pub trait MarketDataProvider: Send + Sync {
    /// Fetch historical bars for a security within a time range.
    ///
    /// Returns bars sorted by timestamp ascending.
    /// `limit` caps the number of bars returned (None = no limit).
    async fn fetch_historical_bars(
        &self,
        security: &SecurityId,
        timeframe: Timeframe,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        limit: Option<usize>,
    ) -> Result<Vec<Bar>, MarketDataError>;

    /// Fetch the most recent N bars for a security.
    ///
    /// Returns bars sorted by timestamp ascending (oldest first).
    async fn fetch_latest_bars(
        &self,
        security: &SecurityId,
        timeframe: Timeframe,
        count: usize,
    ) -> Result<Vec<Bar>, MarketDataError>;

    /// Subscribe to real-time updates for the given securities and timeframes.
    ///
    /// Returns a channel receiver that will emit `MarketEvent` values
    /// (BarUpdate, TickUpdate, session events) as they occur.
    async fn subscribe(
        &self,
        securities: &[SecurityId],
        timeframes: &[Timeframe],
    ) -> Result<mpsc::Receiver<MarketEvent>, MarketDataError>;

    /// Unsubscribe from real-time updates.
    async fn unsubscribe(
        &self,
        securities: &[SecurityId],
        timeframes: &[Timeframe],
    ) -> Result<(), MarketDataError>;

    /// Whether the provider is currently connected to its data source.
    fn is_connected(&self) -> bool;
}

/// Synchronous bar-by-bar feed for backtesting.
///
/// Provides bars one at a time, supporting peek, reset, and length queries.
/// Implemented by `VecDataFeed` (in-memory) and `HistoricalDataFeed` (loaded from provider).
pub trait DataFeed: Send {
    /// Get the next bar, advancing the internal cursor.
    fn next_bar(&mut self) -> Option<Bar>;

    /// Peek at the next bar without consuming it.
    fn peek(&self) -> Option<&Bar>;

    /// Reset the feed to the beginning.
    fn reset(&mut self);

    /// Total number of bars in the feed.
    fn len(&self) -> usize;

    /// Whether the feed contains no bars.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brooks_core::market::Exchange;
    use rust_decimal_macros::dec;

    /// A minimal DataFeed implementation for testing the trait contract
    struct TestFeed {
        bars: Vec<Bar>,
        index: usize,
    }

    impl DataFeed for TestFeed {
        fn next_bar(&mut self) -> Option<Bar> {
            if self.index < self.bars.len() {
                let bar = self.bars[self.index].clone();
                self.index += 1;
                Some(bar)
            } else {
                None
            }
        }

        fn peek(&self) -> Option<&Bar> {
            self.bars.get(self.index)
        }

        fn reset(&mut self) {
            self.index = 0;
        }

        fn len(&self) -> usize {
            self.bars.len()
        }
    }

    fn make_bar(close: rust_decimal::Decimal) -> Bar {
        Bar {
            timestamp: Utc::now(),
            open: close,
            high: close + dec!(0.05),
            low: close - dec!(0.05),
            close,
            volume: 1000,
            timeframe: Timeframe::Minute5,
            security: SecurityId::etf("510050", Exchange::SH),
        }
    }

    #[test]
    fn test_data_feed_iteration() {
        let bars = vec![make_bar(dec!(3.00)), make_bar(dec!(3.05))];
        let mut feed = TestFeed { bars, index: 0 };

        assert_eq!(feed.len(), 2);
        assert!(!feed.is_empty());
        assert!(feed.peek().is_some());

        let bar1 = feed.next_bar().unwrap();
        assert_eq!(bar1.close, dec!(3.00));

        let bar2 = feed.next_bar().unwrap();
        assert_eq!(bar2.close, dec!(3.05));

        assert!(feed.next_bar().is_none());
    }

    #[test]
    fn test_data_feed_reset() {
        let bars = vec![make_bar(dec!(3.00))];
        let mut feed = TestFeed { bars, index: 0 };

        feed.next_bar();
        assert!(feed.next_bar().is_none());

        feed.reset();
        assert!(feed.next_bar().is_some());
    }

    #[test]
    fn test_data_feed_empty() {
        let feed = TestFeed {
            bars: vec![],
            index: 0,
        };
        assert!(feed.is_empty());
        assert_eq!(feed.len(), 0);
    }
}
