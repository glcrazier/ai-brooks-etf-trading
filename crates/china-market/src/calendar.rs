use chrono::{Datelike, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Trading calendar that tracks holidays and trading days
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingCalendar {
    holidays: HashSet<NaiveDate>,
    /// Some weekends are designated as trading days in China (makeup days)
    makeup_trading_days: HashSet<NaiveDate>,
}

impl TradingCalendar {
    /// Create a new empty trading calendar
    pub fn new() -> Self {
        Self {
            holidays: HashSet::new(),
            makeup_trading_days: HashSet::new(),
        }
    }

    /// Create a calendar with China's 2025 public holidays
    /// Note: Actual holiday dates should be updated from official announcements.
    /// These are approximate based on typical patterns.
    pub fn china_2025() -> Self {
        let mut cal = Self::new();

        // New Year's Day
        cal.add_holiday("2025-01-01");

        // Chinese New Year (Spring Festival) - Jan 28 to Feb 4
        for day in 28..=31 {
            cal.add_holiday(&format!("2025-01-{day:02}"));
        }
        for day in 1..=4 {
            cal.add_holiday(&format!("2025-02-{day:02}"));
        }

        // Tomb Sweeping Day (Qingming) - Apr 4-6
        for day in 4..=6 {
            cal.add_holiday(&format!("2025-04-{day:02}"));
        }

        // Labor Day - May 1-5
        for day in 1..=5 {
            cal.add_holiday(&format!("2025-05-{day:02}"));
        }

        // Dragon Boat Festival - May 31, Jun 1-2
        cal.add_holiday("2025-05-31");
        cal.add_holiday("2025-06-01");
        cal.add_holiday("2025-06-02");

        // Mid-Autumn Festival - Oct 6 (combined with National Day)
        // National Day - Oct 1-8
        for day in 1..=8 {
            cal.add_holiday(&format!("2025-10-{day:02}"));
        }

        // Makeup trading days (working weekends)
        cal.add_makeup_day("2025-01-26"); // Sun -> makeup for Spring Festival
        cal.add_makeup_day("2025-02-08"); // Sat -> makeup for Spring Festival
        cal.add_makeup_day("2025-04-27"); // Sun -> makeup for Labor Day
        cal.add_makeup_day("2025-09-28"); // Sun -> makeup for National Day
        cal.add_makeup_day("2025-10-11"); // Sat -> makeup for National Day

        cal
    }

    /// Add a holiday date (format: "YYYY-MM-DD")
    pub fn add_holiday(&mut self, date_str: &str) {
        if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
            self.holidays.insert(date);
        }
    }

    /// Add a makeup trading day (weekend that is a trading day)
    pub fn add_makeup_day(&mut self, date_str: &str) {
        if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
            self.makeup_trading_days.insert(date);
        }
    }

    /// Check if a given date is a trading day
    pub fn is_trading_day(&self, date: NaiveDate) -> bool {
        // Makeup trading days override weekend check
        if self.makeup_trading_days.contains(&date) {
            return !self.holidays.contains(&date);
        }

        let weekday = date.weekday();
        weekday != Weekday::Sat && weekday != Weekday::Sun && !self.holidays.contains(&date)
    }

    /// Find the next trading day from (but not including) the given date
    pub fn next_trading_day(&self, from: NaiveDate) -> NaiveDate {
        let mut date = from.succ_opt().expect("date overflow");
        while !self.is_trading_day(date) {
            date = date.succ_opt().expect("date overflow");
        }
        date
    }

    /// Find the previous trading day before (but not including) the given date
    pub fn prev_trading_day(&self, from: NaiveDate) -> NaiveDate {
        let mut date = from.pred_opt().expect("date underflow");
        while !self.is_trading_day(date) {
            date = date.pred_opt().expect("date underflow");
        }
        date
    }

    /// Count trading days between two dates (exclusive of both endpoints)
    pub fn trading_days_between(&self, start: NaiveDate, end: NaiveDate) -> u32 {
        let mut count = 0;
        let mut date = start.succ_opt().expect("date overflow");
        while date < end {
            if self.is_trading_day(date) {
                count += 1;
            }
            date = date.succ_opt().expect("date overflow");
        }
        count
    }
}

impl Default for TradingCalendar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cal() -> TradingCalendar {
        TradingCalendar::china_2025()
    }

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn test_regular_weekday_is_trading_day() {
        // 2025-03-03 is Monday
        assert!(cal().is_trading_day(date("2025-03-03")));
    }

    #[test]
    fn test_weekend_not_trading_day() {
        // 2025-03-01 is Saturday
        assert!(!cal().is_trading_day(date("2025-03-01")));
        // 2025-03-02 is Sunday
        assert!(!cal().is_trading_day(date("2025-03-02")));
    }

    #[test]
    fn test_holiday_not_trading_day() {
        // New Year's Day
        assert!(!cal().is_trading_day(date("2025-01-01")));
        // Spring Festival
        assert!(!cal().is_trading_day(date("2025-01-29")));
    }

    #[test]
    fn test_makeup_day_is_trading_day() {
        // 2025-01-26 is Sunday but a makeup trading day
        assert!(cal().is_trading_day(date("2025-01-26")));
    }

    #[test]
    fn test_next_trading_day_regular() {
        // Friday -> Monday
        assert_eq!(
            cal().next_trading_day(date("2025-03-07")),
            date("2025-03-10")
        );
    }

    #[test]
    fn test_next_trading_day_over_holiday() {
        // Dec 31, 2024 (Wed) -> Jan 2, 2025 (Thu) skipping Jan 1
        let mut c = TradingCalendar::new();
        c.add_holiday("2025-01-01");
        assert_eq!(c.next_trading_day(date("2024-12-31")), date("2025-01-02"));
    }

    #[test]
    fn test_prev_trading_day() {
        // Monday -> Friday
        assert_eq!(
            cal().prev_trading_day(date("2025-03-10")),
            date("2025-03-07")
        );
    }

    #[test]
    fn test_trading_days_between() {
        // Between Mon Mar 3 and Fri Mar 7 (exclusive): Tue, Wed, Thu = 3
        assert_eq!(
            cal().trading_days_between(date("2025-03-03"), date("2025-03-07")),
            3
        );
    }
}
