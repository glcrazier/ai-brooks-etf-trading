use serde::{Deserialize, Serialize};

/// Configuration for connecting to Futu OpenD
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FutuConfig {
    /// Host address of FutuOpenD (typically localhost)
    pub host: String,
    /// TCP port of FutuOpenD (default: 11111)
    pub port: u16,
    /// Client identifier sent during handshake
    pub client_id: String,
    /// Request timeout in milliseconds
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_timeout_ms() -> u64 {
    5000
}

impl Default for FutuConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 11111,
            client_id: "brooks-trading".into(),
            timeout_ms: default_timeout_ms(),
        }
    }
}

/// Full market data configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MarketDataConfig {
    /// Futu connection settings
    pub futu: FutuConfig,
    /// Default exchange ("SH" or "SZ")
    pub exchange: String,
    /// Security codes to track (e.g., ["510050", "510300"])
    pub securities: Vec<String>,
    /// Primary analysis timeframe (e.g., "5min")
    pub primary_timeframe: String,
    /// Higher-timeframe context (e.g., "daily")
    pub context_timeframe: String,
}

impl Default for MarketDataConfig {
    fn default() -> Self {
        Self {
            futu: FutuConfig::default(),
            exchange: "SH".into(),
            securities: vec!["510050".into(), "510300".into()],
            primary_timeframe: "5min".into(),
            context_timeframe: "daily".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_futu_config_default() {
        let config = FutuConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 11111);
        assert_eq!(config.client_id, "brooks-trading");
        assert_eq!(config.timeout_ms, 5000);
    }

    #[test]
    fn test_market_data_config_default() {
        let config = MarketDataConfig::default();
        assert_eq!(config.exchange, "SH");
        assert_eq!(config.securities.len(), 2);
        assert_eq!(config.primary_timeframe, "5min");
        assert_eq!(config.context_timeframe, "daily");
    }

    #[test]
    fn test_futu_config_serde_roundtrip() {
        let config = FutuConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: FutuConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.host, config.host);
        assert_eq!(deserialized.port, config.port);
    }

    #[test]
    fn test_futu_config_serde_default_timeout() {
        // Omitting timeout_ms should use default
        let json = r#"{"host":"127.0.0.1","port":11111,"client_id":"test"}"#;
        let config: FutuConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.timeout_ms, 5000);
    }
}
