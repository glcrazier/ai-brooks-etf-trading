use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("Invalid bar data: {0}")]
    InvalidBar(String),

    #[error("Invalid order: {0}")]
    InvalidOrder(String),

    #[error("Invalid security: {0}")]
    InvalidSecurity(String),

    #[error("Invalid timeframe: {0}")]
    InvalidTimeframe(String),
}
