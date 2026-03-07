use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Timeframe for price bars / candlesticks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum Timeframe {
    Minute1,
    Minute5,
    Minute15,
    Minute30,
    Minute60,
    Daily,
    Weekly,
}

impl Timeframe {
    /// Duration of the timeframe in seconds
    pub fn duration_secs(&self) -> i64 {
        match self {
            Timeframe::Minute1 => 60,
            Timeframe::Minute5 => 300,
            Timeframe::Minute15 => 900,
            Timeframe::Minute30 => 1800,
            Timeframe::Minute60 => 3600,
            Timeframe::Daily => 86400,
            Timeframe::Weekly => 604800,
        }
    }

    /// Map to Futu OpenAPI KLType enum values
    pub fn as_futu_kl_type(&self) -> i32 {
        match self {
            Timeframe::Minute1 => 1,
            Timeframe::Minute5 => 6,
            Timeframe::Minute15 => 7,
            Timeframe::Minute30 => 8,
            Timeframe::Minute60 => 9,
            Timeframe::Daily => 2,
            Timeframe::Weekly => 3,
        }
    }

    /// Whether this is an intraday timeframe
    pub fn is_intraday(&self) -> bool {
        matches!(
            self,
            Timeframe::Minute1
                | Timeframe::Minute5
                | Timeframe::Minute15
                | Timeframe::Minute30
                | Timeframe::Minute60
        )
    }
}

impl fmt::Display for Timeframe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Timeframe::Minute1 => write!(f, "1min"),
            Timeframe::Minute5 => write!(f, "5min"),
            Timeframe::Minute15 => write!(f, "15min"),
            Timeframe::Minute30 => write!(f, "30min"),
            Timeframe::Minute60 => write!(f, "60min"),
            Timeframe::Daily => write!(f, "daily"),
            Timeframe::Weekly => write!(f, "weekly"),
        }
    }
}

impl FromStr for Timeframe {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "1min" | "1m" => Ok(Timeframe::Minute1),
            "5min" | "5m" => Ok(Timeframe::Minute5),
            "15min" | "15m" => Ok(Timeframe::Minute15),
            "30min" | "30m" => Ok(Timeframe::Minute30),
            "60min" | "60m" | "1h" => Ok(Timeframe::Minute60),
            "daily" | "1d" | "day" => Ok(Timeframe::Daily),
            "weekly" | "1w" | "week" => Ok(Timeframe::Weekly),
            _ => Err(format!(
                "invalid timeframe '{}': expected one of 1min, 5min, 15min, 30min, 60min, daily, weekly",
                s
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeframe_duration() {
        assert_eq!(Timeframe::Minute1.duration_secs(), 60);
        assert_eq!(Timeframe::Minute5.duration_secs(), 300);
        assert_eq!(Timeframe::Daily.duration_secs(), 86400);
    }

    #[test]
    fn test_timeframe_display() {
        assert_eq!(Timeframe::Minute5.to_string(), "5min");
        assert_eq!(Timeframe::Daily.to_string(), "daily");
    }

    #[test]
    fn test_timeframe_is_intraday() {
        assert!(Timeframe::Minute5.is_intraday());
        assert!(Timeframe::Minute60.is_intraday());
        assert!(!Timeframe::Daily.is_intraday());
        assert!(!Timeframe::Weekly.is_intraday());
    }

    #[test]
    fn test_timeframe_ordering() {
        assert!(Timeframe::Minute1 < Timeframe::Minute5);
        assert!(Timeframe::Minute5 < Timeframe::Daily);
        assert!(Timeframe::Daily < Timeframe::Weekly);
    }
}
