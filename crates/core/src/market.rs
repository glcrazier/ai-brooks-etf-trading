use serde::{Deserialize, Serialize};
use std::fmt;

/// Stock exchange identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Exchange {
    /// Shanghai Stock Exchange
    SH,
    /// Shenzhen Stock Exchange
    SZ,
}

impl fmt::Display for Exchange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Exchange::SH => write!(f, "SH"),
            Exchange::SZ => write!(f, "SZ"),
        }
    }
}

/// Type of security
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SecurityType {
    ETF,
    Stock,
}

/// Unique identifier for a security
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SecurityId {
    /// Security code, e.g., "510050" for SSE 50 ETF
    pub code: String,
    /// Exchange the security is listed on
    pub exchange: Exchange,
    /// Type of security
    pub security_type: SecurityType,
}

impl SecurityId {
    pub fn new(code: impl Into<String>, exchange: Exchange, security_type: SecurityType) -> Self {
        Self {
            code: code.into(),
            exchange,
            security_type,
        }
    }

    /// Create an ETF security ID
    pub fn etf(code: impl Into<String>, exchange: Exchange) -> Self {
        Self::new(code, exchange, SecurityType::ETF)
    }

    /// Create a stock security ID
    pub fn stock(code: impl Into<String>, exchange: Exchange) -> Self {
        Self::new(code, exchange, SecurityType::Stock)
    }
}

impl fmt::Display for SecurityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.code, self.exchange)
    }
}

/// Trade direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Direction {
    Long,
    Short,
}

impl Direction {
    pub fn opposite(&self) -> Self {
        match self {
            Direction::Long => Direction::Short,
            Direction::Short => Direction::Long,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_id_display() {
        let sec = SecurityId::etf("510050", Exchange::SH);
        assert_eq!(sec.to_string(), "510050.SH");
    }

    #[test]
    fn test_security_id_etf() {
        let sec = SecurityId::etf("510050", Exchange::SH);
        assert_eq!(sec.security_type, SecurityType::ETF);
        assert_eq!(sec.exchange, Exchange::SH);
        assert_eq!(sec.code, "510050");
    }

    #[test]
    fn test_direction_opposite() {
        assert_eq!(Direction::Long.opposite(), Direction::Short);
        assert_eq!(Direction::Short.opposite(), Direction::Long);
    }
}
