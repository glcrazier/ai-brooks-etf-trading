use thiserror::Error;

/// Errors that can occur in the market data subsystem
#[derive(Debug, Error)]
pub enum MarketDataError {
    /// Failed to establish connection to data source
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// Connection was unexpectedly closed
    #[error("Connection closed")]
    ConnectionClosed,

    /// Wire protocol error (bad magic, malformed header, etc.)
    #[error("Protocol error: {0}")]
    ProtocolError(String),

    /// Response could not be parsed or was unexpected
    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    /// Request timed out waiting for a response
    #[error("Request timeout")]
    Timeout,

    /// API-level error returned by the data source
    #[error("API error (code={code}): {message}")]
    ApiError { code: i32, message: String },

    /// Security identifier is invalid or not recognized
    #[error("Invalid security: {0}")]
    InvalidSecurity(String),

    /// No data available for the requested security/timeframe
    #[error("No data available for {security} {timeframe}")]
    NoData { security: String, timeframe: String },

    /// Underlying I/O error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Protobuf encoding/decoding error
    #[error("Protobuf error: {0}")]
    Protobuf(#[from] prost::DecodeError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = MarketDataError::ConnectionFailed("refused".into());
        assert_eq!(err.to_string(), "Connection failed: refused");
    }

    #[test]
    fn test_api_error_display() {
        let err = MarketDataError::ApiError {
            code: -1,
            message: "rate limited".into(),
        };
        assert_eq!(err.to_string(), "API error (code=-1): rate limited");
    }

    #[test]
    fn test_no_data_display() {
        let err = MarketDataError::NoData {
            security: "510050.SH".into(),
            timeframe: "5min".into(),
        };
        assert_eq!(err.to_string(), "No data available for 510050.SH 5min");
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        let err: MarketDataError = io_err.into();
        assert!(matches!(err, MarketDataError::Io(_)));
    }
}
