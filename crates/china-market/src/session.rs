use chrono::NaiveTime;
use serde::{Deserialize, Serialize};

/// Trading session hours for an exchange
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingSession {
    pub morning_open: NaiveTime,
    pub morning_close: NaiveTime,
    pub afternoon_open: NaiveTime,
    pub afternoon_close: NaiveTime,
}

impl TradingSession {
    /// Standard China A-share / ETF trading session
    /// Morning:   09:30 - 11:30 CST
    /// Afternoon: 13:00 - 15:00 CST
    pub fn china_a_share() -> Self {
        Self {
            morning_open: NaiveTime::from_hms_opt(9, 30, 0).unwrap(),
            morning_close: NaiveTime::from_hms_opt(11, 30, 0).unwrap(),
            afternoon_open: NaiveTime::from_hms_opt(13, 0, 0).unwrap(),
            afternoon_close: NaiveTime::from_hms_opt(15, 0, 0).unwrap(),
        }
    }

    /// Whether the market is open at the given time (CST / Asia/Shanghai)
    pub fn is_market_open(&self, time: NaiveTime) -> bool {
        (time >= self.morning_open && time <= self.morning_close)
            || (time >= self.afternoon_open && time <= self.afternoon_close)
    }

    /// Whether the given time falls within the lunch break
    pub fn is_lunch_break(&self, time: NaiveTime) -> bool {
        time > self.morning_close && time < self.afternoon_open
    }

    /// Whether the given time is before market open
    pub fn is_pre_market(&self, time: NaiveTime) -> bool {
        time < self.morning_open
    }

    /// Whether the given time is after market close
    pub fn is_after_hours(&self, time: NaiveTime) -> bool {
        time > self.afternoon_close
    }

    /// Total trading minutes in a session day (excluding lunch break)
    pub fn total_trading_minutes(&self) -> i64 {
        let morning = (self.morning_close - self.morning_open).num_minutes();
        let afternoon = (self.afternoon_close - self.afternoon_open).num_minutes();
        morning + afternoon
    }

    /// Minutes remaining until the next session boundary.
    /// Returns None if outside all relevant windows.
    pub fn minutes_until_next_boundary(&self, time: NaiveTime) -> Option<i64> {
        if time < self.morning_open {
            Some((self.morning_open - time).num_minutes())
        } else if time <= self.morning_close {
            Some((self.morning_close - time).num_minutes())
        } else if time < self.afternoon_open {
            Some((self.afternoon_open - time).num_minutes())
        } else if time <= self.afternoon_close {
            Some((self.afternoon_close - time).num_minutes())
        } else {
            None // after market close
        }
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

    #[test]
    fn test_market_open_morning() {
        let s = session();
        assert!(s.is_market_open(time(9, 30)));
        assert!(s.is_market_open(time(10, 15)));
        assert!(s.is_market_open(time(11, 30)));
    }

    #[test]
    fn test_market_open_afternoon() {
        let s = session();
        assert!(s.is_market_open(time(13, 0)));
        assert!(s.is_market_open(time(14, 30)));
        assert!(s.is_market_open(time(15, 0)));
    }

    #[test]
    fn test_market_closed() {
        let s = session();
        assert!(!s.is_market_open(time(9, 0)));
        assert!(!s.is_market_open(time(12, 0)));
        assert!(!s.is_market_open(time(15, 30)));
    }

    #[test]
    fn test_lunch_break() {
        let s = session();
        assert!(s.is_lunch_break(time(12, 0)));
        assert!(s.is_lunch_break(time(12, 30)));
        assert!(!s.is_lunch_break(time(11, 30)));
        assert!(!s.is_lunch_break(time(13, 0)));
    }

    #[test]
    fn test_pre_market() {
        let s = session();
        assert!(s.is_pre_market(time(8, 0)));
        assert!(s.is_pre_market(time(9, 29)));
        assert!(!s.is_pre_market(time(9, 30)));
    }

    #[test]
    fn test_after_hours() {
        let s = session();
        assert!(s.is_after_hours(time(15, 1)));
        assert!(!s.is_after_hours(time(15, 0)));
        assert!(!s.is_after_hours(time(14, 0)));
    }

    #[test]
    fn test_total_trading_minutes() {
        let s = session();
        // Morning: 120 min, Afternoon: 120 min = 240 min total
        assert_eq!(s.total_trading_minutes(), 240);
    }

    #[test]
    fn test_minutes_until_boundary_pre_market() {
        let s = session();
        assert_eq!(s.minutes_until_next_boundary(time(9, 0)), Some(30));
    }

    #[test]
    fn test_minutes_until_boundary_morning() {
        let s = session();
        assert_eq!(s.minutes_until_next_boundary(time(10, 30)), Some(60));
    }

    #[test]
    fn test_minutes_until_boundary_lunch() {
        let s = session();
        assert_eq!(s.minutes_until_next_boundary(time(12, 0)), Some(60));
    }

    #[test]
    fn test_minutes_until_boundary_afternoon() {
        let s = session();
        assert_eq!(s.minutes_until_next_boundary(time(14, 0)), Some(60));
    }

    #[test]
    fn test_minutes_until_boundary_after_close() {
        let s = session();
        assert_eq!(s.minutes_until_next_boundary(time(16, 0)), None);
    }
}
