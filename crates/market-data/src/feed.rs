//! DataFeed implementations for bar-by-bar iteration.
//!
//! - `VecDataFeed` — Simple in-memory feed from a `Vec<Bar>`
//! - `HistoricalDataFeed` — Loads bars from a `MarketDataProvider` then iterates

use brooks_core::bar::Bar;
use brooks_core::market::SecurityId;
use brooks_core::timeframe::Timeframe;
use chrono::{DateTime, Utc};

use crate::error::MarketDataError;
use crate::traits::{DataFeed, MarketDataProvider};

/// In-memory bar feed backed by a `Vec<Bar>`.
///
/// Primary use: backtesting with pre-loaded data.
#[derive(Debug, Clone)]
pub struct VecDataFeed {
    bars: Vec<Bar>,
    index: usize,
}

impl VecDataFeed {
    /// Create a feed from a vector of bars.
    pub fn new(bars: Vec<Bar>) -> Self {
        Self { bars, index: 0 }
    }

    /// Create a feed from any iterable of bars.
    pub fn from_bars(bars: impl IntoIterator<Item = Bar>) -> Self {
        Self::new(bars.into_iter().collect())
    }

    /// How many bars remain (not yet consumed).
    pub fn remaining(&self) -> usize {
        self.bars.len().saturating_sub(self.index)
    }

    /// Current position in the feed (0-based).
    pub fn position(&self) -> usize {
        self.index
    }
}

impl DataFeed for VecDataFeed {
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

/// A data feed that loads historical bars from a `MarketDataProvider`.
///
/// Wraps a `VecDataFeed` after fetching data asynchronously.
pub struct HistoricalDataFeed {
    inner: VecDataFeed,
}

impl HistoricalDataFeed {
    /// Load bars from a provider for the given security, timeframe, and date range.
    pub async fn load(
        provider: &dyn MarketDataProvider,
        security: &SecurityId,
        timeframe: Timeframe,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Self, MarketDataError> {
        let bars = provider
            .fetch_historical_bars(security, timeframe, start, end, None)
            .await?;
        Ok(Self {
            inner: VecDataFeed::new(bars),
        })
    }

    /// Create from pre-fetched bars (useful for testing).
    pub fn from_bars(bars: Vec<Bar>) -> Self {
        Self {
            inner: VecDataFeed::new(bars),
        }
    }
}

impl DataFeed for HistoricalDataFeed {
    fn next_bar(&mut self) -> Option<Bar> {
        self.inner.next_bar()
    }

    fn peek(&self) -> Option<&Bar> {
        self.inner.peek()
    }

    fn reset(&mut self) {
        self.inner.reset();
    }

    fn len(&self) -> usize {
        self.inner.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brooks_core::market::Exchange;
    use rust_decimal_macros::dec;

    fn make_bar(close: rust_decimal::Decimal, idx: u32) -> Bar {
        use chrono::Duration;
        Bar {
            timestamp: Utc::now() + Duration::minutes(idx as i64 * 5),
            open: close - dec!(0.01),
            high: close + dec!(0.05),
            low: close - dec!(0.05),
            close,
            volume: 1000 + idx as u64 * 100,
            timeframe: Timeframe::Minute5,
            security: SecurityId::etf("510050", Exchange::SH),
        }
    }

    #[test]
    fn test_vec_feed_basic_iteration() {
        let bars = vec![make_bar(dec!(3.00), 0), make_bar(dec!(3.05), 1)];
        let mut feed = VecDataFeed::new(bars);

        assert_eq!(feed.len(), 2);
        assert!(!feed.is_empty());
        assert_eq!(feed.remaining(), 2);
        assert_eq!(feed.position(), 0);

        let bar1 = feed.next_bar().unwrap();
        assert_eq!(bar1.close, dec!(3.00));
        assert_eq!(feed.remaining(), 1);
        assert_eq!(feed.position(), 1);

        let bar2 = feed.next_bar().unwrap();
        assert_eq!(bar2.close, dec!(3.05));

        assert!(feed.next_bar().is_none());
        assert_eq!(feed.remaining(), 0);
    }

    #[test]
    fn test_vec_feed_peek() {
        let bars = vec![make_bar(dec!(3.00), 0), make_bar(dec!(3.05), 1)];
        let mut feed = VecDataFeed::new(bars);

        // Peek does not advance
        let peeked = feed.peek().unwrap();
        assert_eq!(peeked.close, dec!(3.00));
        let peeked2 = feed.peek().unwrap();
        assert_eq!(peeked2.close, dec!(3.00));

        // Consume then peek next
        feed.next_bar();
        let peeked3 = feed.peek().unwrap();
        assert_eq!(peeked3.close, dec!(3.05));
    }

    #[test]
    fn test_vec_feed_reset() {
        let bars = vec![make_bar(dec!(3.00), 0)];
        let mut feed = VecDataFeed::new(bars);

        assert!(feed.next_bar().is_some());
        assert!(feed.next_bar().is_none());

        feed.reset();
        assert_eq!(feed.position(), 0);
        assert!(feed.next_bar().is_some());
    }

    #[test]
    fn test_vec_feed_empty() {
        let feed = VecDataFeed::new(vec![]);
        assert!(feed.is_empty());
        assert_eq!(feed.len(), 0);
        assert_eq!(feed.remaining(), 0);
        assert!(feed.peek().is_none());
    }

    #[test]
    fn test_vec_feed_from_bars() {
        let bars = vec![make_bar(dec!(3.00), 0), make_bar(dec!(3.05), 1)];
        let feed = VecDataFeed::from_bars(bars);
        assert_eq!(feed.len(), 2);
    }

    #[test]
    fn test_historical_feed_from_bars() {
        let bars = vec![make_bar(dec!(3.00), 0)];
        let mut feed = HistoricalDataFeed::from_bars(bars);
        assert_eq!(feed.len(), 1);
        assert!(feed.next_bar().is_some());
        assert!(feed.next_bar().is_none());
        feed.reset();
        assert!(feed.next_bar().is_some());
    }
}
