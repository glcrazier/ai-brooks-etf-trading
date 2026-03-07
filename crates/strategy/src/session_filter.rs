use brooks_china_market::session::TradingSession;
use chrono::NaiveTime;

/// Decides whether trading is allowed based on session timing and warm-up state.
pub struct SessionFilter {
    session: TradingSession,
    warm_up_bars: u64,
    bars_processed: u64,
    /// Don't open new positions within this many minutes of session close
    no_entry_minutes_before_close: i64,
    /// Don't open new positions within this many minutes after session open
    no_entry_minutes_after_open: i64,
    /// Force close positions this many minutes before session close
    force_close_minutes_before_close: i64,
}

impl SessionFilter {
    pub fn new(session: TradingSession, warm_up_bars: u64) -> Self {
        Self {
            session,
            warm_up_bars,
            bars_processed: 0,
            no_entry_minutes_before_close: 15,
            no_entry_minutes_after_open: 5,
            force_close_minutes_before_close: 5,
        }
    }

    /// Record a bar processed (for warm-up tracking).
    pub fn on_bar_processed(&mut self) {
        self.bars_processed += 1;
    }

    /// Whether the warm-up period is complete.
    pub fn is_warmed_up(&self) -> bool {
        self.bars_processed >= self.warm_up_bars
    }

    /// Whether new entries are allowed at the given time.
    ///
    /// Returns `false` during:
    /// - Warm-up period
    /// - Lunch break
    /// - Near session close (within `no_entry_minutes_before_close`)
    /// - Near session open (within `no_entry_minutes_after_open`)
    /// - Market not open
    pub fn allows_new_entry(&self, time: NaiveTime) -> bool {
        if !self.is_warmed_up() {
            return false;
        }

        if !self.session.is_market_open(time) {
            return false;
        }

        if self.session.is_lunch_break(time) {
            return false;
        }

        // Near open: block first N minutes after morning or afternoon open
        if self.is_near_open(time) {
            return false;
        }

        // Near close: block last N minutes before morning close or afternoon close
        if self.is_near_close(time) {
            return false;
        }

        true
    }

    /// Whether the strategy should force-close all positions (end of day).
    ///
    /// Returns `true` when within `force_close_minutes_before_close` of the
    /// final session close (afternoon close).
    pub fn should_force_close(&self, time: NaiveTime) -> bool {
        // Only force close near the final close (afternoon)
        let close = self.session.afternoon_close;
        let threshold = close
            - chrono::Duration::minutes(self.force_close_minutes_before_close);
        time >= threshold && time <= close
    }

    /// Reset for new trading day.
    pub fn reset_daily(&mut self) {
        self.bars_processed = 0;
    }

    pub fn bars_processed(&self) -> u64 {
        self.bars_processed
    }

    /// Whether we are within the first N minutes after a session open.
    fn is_near_open(&self, time: NaiveTime) -> bool {
        let morning_threshold = self.session.morning_open
            + chrono::Duration::minutes(self.no_entry_minutes_after_open);
        if time >= self.session.morning_open && time < morning_threshold {
            return true;
        }

        let afternoon_threshold = self.session.afternoon_open
            + chrono::Duration::minutes(self.no_entry_minutes_after_open);
        if time >= self.session.afternoon_open && time < afternoon_threshold {
            return true;
        }

        false
    }

    /// Whether we are within the last N minutes before a session close.
    fn is_near_close(&self, time: NaiveTime) -> bool {
        let morning_threshold = self.session.morning_close
            - chrono::Duration::minutes(self.no_entry_minutes_before_close);
        if time >= morning_threshold && time <= self.session.morning_close {
            return true;
        }

        let afternoon_threshold = self.session.afternoon_close
            - chrono::Duration::minutes(self.no_entry_minutes_before_close);
        if time >= afternoon_threshold && time <= self.session.afternoon_close {
            return true;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> TradingSession {
        TradingSession::china_a_share()
    }

    fn time(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    fn warmed_up_filter() -> SessionFilter {
        let mut filter = SessionFilter::new(session(), 20);
        for _ in 0..20 {
            filter.on_bar_processed();
        }
        filter
    }

    #[test]
    fn test_not_warmed_up_blocks_entry() {
        let filter = SessionFilter::new(session(), 20);
        assert!(!filter.is_warmed_up());
        assert!(!filter.allows_new_entry(time(10, 30)));
    }

    #[test]
    fn test_warmed_up_after_enough_bars() {
        let mut filter = SessionFilter::new(session(), 5);
        for _ in 0..5 {
            filter.on_bar_processed();
        }
        assert!(filter.is_warmed_up());
    }

    #[test]
    fn test_allows_entry_normal_session() {
        let filter = warmed_up_filter();
        // 10:30 is well within morning session, past open buffer
        assert!(filter.allows_new_entry(time(10, 30)));
    }

    #[test]
    fn test_blocks_entry_during_lunch() {
        let filter = warmed_up_filter();
        assert!(!filter.allows_new_entry(time(12, 0)));
        assert!(!filter.allows_new_entry(time(12, 30)));
    }

    #[test]
    fn test_blocks_entry_near_morning_close() {
        let filter = warmed_up_filter();
        // morning_close = 11:30, threshold = 11:15
        assert!(!filter.allows_new_entry(time(11, 15)));
        assert!(!filter.allows_new_entry(time(11, 25)));
    }

    #[test]
    fn test_blocks_entry_near_afternoon_close() {
        let filter = warmed_up_filter();
        // afternoon_close = 15:00, threshold = 14:45
        assert!(!filter.allows_new_entry(time(14, 50)));
    }

    #[test]
    fn test_blocks_entry_near_morning_open() {
        let filter = warmed_up_filter();
        // morning_open = 09:30, threshold = 09:35
        assert!(!filter.allows_new_entry(time(9, 30)));
        assert!(!filter.allows_new_entry(time(9, 34)));
    }

    #[test]
    fn test_blocks_entry_near_afternoon_open() {
        let filter = warmed_up_filter();
        // afternoon_open = 13:00, threshold = 13:05
        assert!(!filter.allows_new_entry(time(13, 0)));
        assert!(!filter.allows_new_entry(time(13, 4)));
    }

    #[test]
    fn test_allows_entry_after_open_buffer() {
        let filter = warmed_up_filter();
        assert!(filter.allows_new_entry(time(9, 35)));
        assert!(filter.allows_new_entry(time(13, 5)));
    }

    #[test]
    fn test_blocks_entry_outside_market() {
        let filter = warmed_up_filter();
        assert!(!filter.allows_new_entry(time(8, 0)));
        assert!(!filter.allows_new_entry(time(16, 0)));
    }

    #[test]
    fn test_should_force_close_near_end() {
        let filter = SessionFilter::new(session(), 0);
        // force_close_minutes = 5 before 15:00 = 14:55
        assert!(filter.should_force_close(time(14, 55)));
        assert!(filter.should_force_close(time(14, 58)));
        assert!(filter.should_force_close(time(15, 0)));
    }

    #[test]
    fn test_should_not_force_close_early() {
        let filter = SessionFilter::new(session(), 0);
        assert!(!filter.should_force_close(time(14, 30)));
        assert!(!filter.should_force_close(time(14, 54)));
    }

    #[test]
    fn test_reset_daily_clears_bars() {
        let mut filter = warmed_up_filter();
        assert!(filter.is_warmed_up());
        filter.reset_daily();
        assert!(!filter.is_warmed_up());
        assert_eq!(filter.bars_processed(), 0);
    }

    #[test]
    fn test_allows_entry_afternoon_normal() {
        let filter = warmed_up_filter();
        assert!(filter.allows_new_entry(time(13, 30)));
        assert!(filter.allows_new_entry(time(14, 0)));
    }
}
