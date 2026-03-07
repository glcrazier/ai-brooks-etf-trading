use brooks_core::market::{Exchange, SecurityId};
use serde::{Deserialize, Serialize};

/// Additional metadata about a Chinese security
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityInfo {
    pub security: SecurityId,
    /// Human-readable name
    pub name: String,
    /// Board the security is listed on
    pub board: Board,
}

/// Listing board, which affects trading rules
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Board {
    /// Main board (上交所主板 / 深交所主板)
    MainBoard,
    /// ChiNext (创业板) - 20% price limit
    ChiNext,
    /// STAR Market (科创板) - 20% price limit
    Star,
    /// Beijing Stock Exchange (北交所) - 30% price limit
    BSE,
}

/// Well-known Chinese ETFs for initial development and testing
pub fn popular_etfs() -> Vec<SecurityInfo> {
    vec![
        SecurityInfo {
            security: SecurityId::etf("510050", Exchange::SH),
            name: "SSE 50 ETF (华夏上证50ETF)".into(),
            board: Board::MainBoard,
        },
        SecurityInfo {
            security: SecurityId::etf("510300", Exchange::SH),
            name: "CSI 300 ETF (华泰柏瑞沪深300ETF)".into(),
            board: Board::MainBoard,
        },
        SecurityInfo {
            security: SecurityId::etf("510500", Exchange::SH),
            name: "CSI 500 ETF (南方中证500ETF)".into(),
            board: Board::MainBoard,
        },
        SecurityInfo {
            security: SecurityId::etf("159915", Exchange::SZ),
            name: "ChiNext ETF (易方达创业板ETF)".into(),
            board: Board::MainBoard,
        },
        SecurityInfo {
            security: SecurityId::etf("159919", Exchange::SZ),
            name: "CSI 300 ETF (嘉实沪深300ETF)".into(),
            board: Board::MainBoard,
        },
        SecurityInfo {
            security: SecurityId::etf("512880", Exchange::SH),
            name: "Securities ETF (国泰中证全指证券公司ETF)".into(),
            board: Board::MainBoard,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_popular_etfs_not_empty() {
        let etfs = popular_etfs();
        assert!(!etfs.is_empty());
    }

    #[test]
    fn test_popular_etfs_are_etfs() {
        use brooks_core::market::SecurityType;
        for info in popular_etfs() {
            assert_eq!(info.security.security_type, SecurityType::ETF);
        }
    }
}
