use rust_decimal::Decimal;

/// Price type alias for clarity throughout the codebase.
/// All financial calculations MUST use Decimal, never f64.
pub type Price = Decimal;
