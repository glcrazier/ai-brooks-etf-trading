//! Minimal hand-written protobuf message types for Futu OpenD.
//!
//! Rather than compiling all ~100 Futu proto files with `prost-build`, we define
//! only the message types we actually need. The field numbers and types match the
//! Futu OpenAPI protobuf schema exactly.
//!
//! **IMPORTANT**: Futu returns prices as `f64` in protobuf. We convert to
//! `rust_decimal::Decimal` immediately at the protocol boundary. The conversion
//! helpers are at the bottom of this file.
//!
//! All Response wrappers follow the same pattern:
//!   tag 1: retType (int32, required)
//!   tag 2: retMsg  (string, optional)
//!   tag 3: errCode (int32, optional)
//!   tag 4: s2c     (message, optional)

use brooks_core::bar::Bar;
use brooks_core::market::{Exchange, SecurityId};
use brooks_core::timeframe::Timeframe;
use chrono::{DateTime, NaiveDateTime, Utc};
use prost::Message;
use rust_decimal::Decimal;

// ============================================================================
// Common types
// ============================================================================

/// Futu security identifier (market + code)
#[derive(Clone, PartialEq, Message)]
pub struct FutuSecurity {
    /// Market: 1 = SH (Shanghai), 2 = SZ (Shenzhen), 11 = HK
    #[prost(int32, required, tag = "1")]
    pub market: i32,
    /// Security code, e.g. "510050"
    #[prost(string, required, tag = "2")]
    pub code: String,
}

// ============================================================================
// InitConnect (proto_id = 1001)
// ============================================================================

/// C2S: Initial handshake request
#[derive(Clone, PartialEq, Message)]
pub struct InitConnectRequest {
    #[prost(int32, required, tag = "1")]
    pub client_ver: i32,
    #[prost(string, required, tag = "2")]
    pub client_id: String,
    #[prost(bool, optional, tag = "3")]
    pub recv_notify: Option<bool>,
}

/// S2C: Handshake response
#[derive(Clone, PartialEq, Message)]
pub struct InitConnectResponse {
    #[prost(int32, required, tag = "1")]
    pub server_ver: i32,
    #[prost(uint64, required, tag = "2")]
    pub login_user_id: u64,
    #[prost(uint64, required, tag = "3")]
    pub conn_id: u64,
    #[prost(string, required, tag = "4")]
    pub conn_aes_key: String,
    #[prost(int32, required, tag = "5")]
    pub keep_alive_interval: i32,
}

/// Wrapper for InitConnect request
#[derive(Clone, PartialEq, Message)]
pub struct InitConnectRequestWrapper {
    #[prost(message, required, tag = "1")]
    pub c2s: InitConnectRequest,
}

/// Wrapper for InitConnect response
#[derive(Clone, PartialEq, Message)]
pub struct InitConnectResponseWrapper {
    #[prost(int32, required, tag = "1")]
    pub ret_type: i32,
    #[prost(string, optional, tag = "2")]
    pub ret_msg: Option<String>,
    #[prost(int32, optional, tag = "3")]
    pub err_code: Option<i32>,
    #[prost(message, optional, tag = "4")]
    pub s2c: Option<InitConnectResponse>,
}

// ============================================================================
// KeepAlive (proto_id = 1004)
// ============================================================================

/// C2S: Keep-alive heartbeat
#[derive(Clone, PartialEq, Message)]
pub struct KeepAliveRequest {
    #[prost(int64, required, tag = "1")]
    pub time: i64,
}

/// Wrapper for KeepAlive request
#[derive(Clone, PartialEq, Message)]
pub struct KeepAliveRequestWrapper {
    #[prost(message, required, tag = "1")]
    pub c2s: KeepAliveRequest,
}

/// Wrapper for KeepAlive response
#[derive(Clone, PartialEq, Message)]
pub struct KeepAliveResponseWrapper {
    #[prost(int32, required, tag = "1")]
    pub ret_type: i32,
    #[prost(string, optional, tag = "2")]
    pub ret_msg: Option<String>,
    #[prost(int32, optional, tag = "3")]
    pub err_code: Option<i32>,
}

// ============================================================================
// Sub / Unsub (proto_id = 3001 / 3002)
// ============================================================================

/// C2S: Subscribe or unsubscribe from data types
#[derive(Clone, PartialEq, Message)]
pub struct SubRequest {
    /// Securities to subscribe to
    #[prost(message, repeated, tag = "1")]
    pub security_list: Vec<FutuSecurity>,
    /// Subscription types: 1=Quote, 4=KL_1Min, 6=KL_5Min, etc.
    #[prost(int32, repeated, packed = "false", tag = "2")]
    pub sub_type_list: Vec<i32>,
    /// true = subscribe, false = unsubscribe
    #[prost(bool, required, tag = "3")]
    pub is_sub_or_un_sub: bool,
    /// Also register/unregister for push notifications
    #[prost(bool, optional, tag = "4")]
    pub is_reg_or_un_reg_push: Option<bool>,
}

/// Wrapper for Sub request
#[derive(Clone, PartialEq, Message)]
pub struct SubRequestWrapper {
    #[prost(message, required, tag = "1")]
    pub c2s: SubRequest,
}

/// Wrapper for Sub response
#[derive(Clone, PartialEq, Message)]
pub struct SubResponseWrapper {
    #[prost(int32, required, tag = "1")]
    pub ret_type: i32,
    #[prost(string, optional, tag = "2")]
    pub ret_msg: Option<String>,
    #[prost(int32, optional, tag = "3")]
    pub err_code: Option<i32>,
}

// ============================================================================
// RequestHistoryKL (proto_id = 3100)
// ============================================================================

/// C2S: Request historical kline (candlestick) data
#[derive(Clone, PartialEq, Message)]
pub struct RequestHistoryKLRequest {
    /// Rehabilitation type: 0=None, 1=Forward, 2=Backward
    #[prost(int32, required, tag = "1")]
    pub rehab_type: i32,
    /// KLine type: matches Sub type constants
    #[prost(int32, required, tag = "2")]
    pub kl_type: i32,
    /// Target security
    #[prost(message, required, tag = "3")]
    pub security: FutuSecurity,
    /// Start time "yyyy-MM-dd HH:mm:ss"
    #[prost(string, required, tag = "4")]
    pub begin_time: String,
    /// End time "yyyy-MM-dd HH:mm:ss"
    #[prost(string, required, tag = "5")]
    pub end_time: String,
    /// Maximum number of klines to return
    #[prost(int32, optional, tag = "6")]
    pub max_count: Option<i32>,
    /// Bitmap of fields to return (set all bits for full data)
    #[prost(int64, optional, tag = "7")]
    pub need_kl_fields_flag: Option<i64>,
}

/// A single kline (candlestick bar) from Futu
///
/// Field tags match Qot_Common.KLine exactly.
#[derive(Clone, PartialEq, Message)]
pub struct KLine {
    /// Timestamp string "yyyy-MM-dd HH:mm:ss"
    #[prost(string, required, tag = "1")]
    pub time: String,
    /// Whether this is a blank/placeholder bar
    #[prost(bool, required, tag = "2")]
    pub is_blank: bool,
    /// Highest price (f64 — convert to Decimal immediately)
    #[prost(double, optional, tag = "3")]
    pub high_price: Option<f64>,
    /// Open price
    #[prost(double, optional, tag = "4")]
    pub open_price: Option<f64>,
    /// Lowest price
    #[prost(double, optional, tag = "5")]
    pub low_price: Option<f64>,
    /// Close price
    #[prost(double, optional, tag = "6")]
    pub close_price: Option<f64>,
    /// Previous close price
    #[prost(double, optional, tag = "7")]
    pub last_close_price: Option<f64>,
    /// Trading volume
    #[prost(int64, optional, tag = "8")]
    pub volume: Option<i64>,
    /// Trading turnover (amount)
    #[prost(double, optional, tag = "9")]
    pub turnover: Option<f64>,
}

/// S2C: Historical kline response
#[derive(Clone, PartialEq, Message)]
pub struct RequestHistoryKLResponse {
    #[prost(message, optional, tag = "1")]
    pub security: Option<FutuSecurity>,
    #[prost(message, repeated, tag = "2")]
    pub kl_list: Vec<KLine>,
    #[prost(bytes, optional, tag = "3")]
    pub next_req_key: Option<Vec<u8>>,
    #[prost(string, optional, tag = "4")]
    pub name: Option<String>,
}

/// Wrapper for RequestHistoryKL request
#[derive(Clone, PartialEq, Message)]
pub struct RequestHistoryKLRequestWrapper {
    #[prost(message, required, tag = "1")]
    pub c2s: RequestHistoryKLRequest,
}

/// Wrapper for RequestHistoryKL response
#[derive(Clone, PartialEq, Message)]
pub struct RequestHistoryKLResponseWrapper {
    #[prost(int32, required, tag = "1")]
    pub ret_type: i32,
    #[prost(string, optional, tag = "2")]
    pub ret_msg: Option<String>,
    #[prost(int32, optional, tag = "3")]
    pub err_code: Option<i32>,
    #[prost(message, optional, tag = "4")]
    pub s2c: Option<RequestHistoryKLResponse>,
}

// ============================================================================
// Push: KLine update (proto_id = 3007)
// ============================================================================

/// S2C push: Real-time kline update
#[derive(Clone, PartialEq, Message)]
pub struct QotUpdateKLResponse {
    /// Rehabilitation type
    #[prost(int32, required, tag = "1")]
    pub rehab_type: i32,
    /// KLine type
    #[prost(int32, required, tag = "2")]
    pub kl_type: i32,
    #[prost(message, required, tag = "3")]
    pub security: FutuSecurity,
    #[prost(message, repeated, tag = "4")]
    pub kl_list: Vec<KLine>,
}

/// Wrapper for KLine push
#[derive(Clone, PartialEq, Message)]
pub struct QotUpdateKLResponseWrapper {
    #[prost(int32, required, tag = "1")]
    pub ret_type: i32,
    #[prost(string, optional, tag = "2")]
    pub ret_msg: Option<String>,
    #[prost(int32, optional, tag = "3")]
    pub err_code: Option<i32>,
    #[prost(message, optional, tag = "4")]
    pub s2c: Option<QotUpdateKLResponse>,
}

// ============================================================================
// Push: Real-time tick / TimeShare (proto_id = 3009)
// ============================================================================

/// A single real-time time-share point (Qot_Common.TimeShare)
#[derive(Clone, PartialEq, Message)]
pub struct Tick {
    /// Timestamp string
    #[prost(string, required, tag = "1")]
    pub time: String,
    /// Minutes elapsed since midnight
    #[prost(int32, required, tag = "2")]
    pub minute: i32,
    /// Whether this is a blank point
    #[prost(bool, required, tag = "3")]
    pub is_blank: bool,
    /// Current price (f64 — convert to Decimal immediately)
    #[prost(double, optional, tag = "4")]
    pub price: Option<f64>,
    /// Volume of this point
    #[prost(int64, optional, tag = "7")]
    pub volume: Option<i64>,
}

/// S2C push: Real-time tick update
#[derive(Clone, PartialEq, Message)]
pub struct QotUpdateRTResponse {
    #[prost(message, required, tag = "1")]
    pub security: FutuSecurity,
    #[prost(message, repeated, tag = "2")]
    pub rt_list: Vec<Tick>,
}

/// Wrapper for tick push
#[derive(Clone, PartialEq, Message)]
pub struct QotUpdateRTResponseWrapper {
    #[prost(int32, required, tag = "1")]
    pub ret_type: i32,
    #[prost(string, optional, tag = "2")]
    pub ret_msg: Option<String>,
    #[prost(int32, optional, tag = "3")]
    pub err_code: Option<i32>,
    #[prost(message, optional, tag = "4")]
    pub s2c: Option<QotUpdateRTResponse>,
}

// ============================================================================
// Conversion helpers
// ============================================================================

/// Convert an f64 price from Futu to Decimal.
///
/// Uses `Decimal::from_f64_retain` which preserves all digits of the f64
/// representation. Falls back to `Decimal::ZERO` for NaN/Inf.
pub fn f64_to_decimal(val: f64) -> Decimal {
    Decimal::from_f64_retain(val).unwrap_or(Decimal::ZERO)
}

/// Convert a `FutuSecurity` to a `SecurityId`.
///
/// Market mapping: 21 = SH (Shanghai), 22 = SZ (Shenzhen).
pub fn futu_security_to_id(
    sec: &FutuSecurity,
) -> Result<SecurityId, crate::error::MarketDataError> {
    let exchange = match sec.market {
        21 => Exchange::SH,
        22 => Exchange::SZ,
        other => {
            return Err(crate::error::MarketDataError::InvalidSecurity(format!(
                "Unknown Futu market: {}",
                other
            )))
        }
    };
    // Default to ETF — caller can adjust if needed
    Ok(SecurityId::etf(&sec.code, exchange))
}

/// Convert a `SecurityId` to a `FutuSecurity`.
pub fn id_to_futu_security(id: &SecurityId) -> FutuSecurity {
    let market = match id.exchange {
        Exchange::SH => 21,
        Exchange::SZ => 22,
    };
    FutuSecurity {
        market,
        code: id.code.clone(),
    }
}

/// Parse a Futu timestamp string ("yyyy-MM-dd HH:mm:ss") to `DateTime<Utc>`.
///
/// Futu timestamps are in China Standard Time (UTC+8). We convert to UTC.
pub fn parse_futu_timestamp(s: &str) -> Result<DateTime<Utc>, crate::error::MarketDataError> {
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").map_err(|e| {
        crate::error::MarketDataError::InvalidResponse(format!("Bad timestamp '{}': {}", s, e))
    })?;
    // Futu timestamps are CST (UTC+8)
    let cst_offset = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
    let cst_dt = naive
        .and_local_timezone(cst_offset)
        .single()
        .ok_or_else(|| {
            crate::error::MarketDataError::InvalidResponse(format!("Ambiguous timestamp: {}", s))
        })?;
    Ok(cst_dt.with_timezone(&Utc))
}

/// Convert a `KLine` from Futu into a `Bar`.
pub fn kline_to_bar(
    kl: &KLine,
    security: &SecurityId,
    timeframe: Timeframe,
) -> Result<Bar, crate::error::MarketDataError> {
    if kl.is_blank {
        return Err(crate::error::MarketDataError::InvalidResponse(
            "Blank kline".into(),
        ));
    }

    let timestamp = parse_futu_timestamp(&kl.time)?;

    Ok(Bar {
        timestamp,
        open: f64_to_decimal(kl.open_price.unwrap_or(0.0)),
        high: f64_to_decimal(kl.high_price.unwrap_or(0.0)),
        low: f64_to_decimal(kl.low_price.unwrap_or(0.0)),
        close: f64_to_decimal(kl.close_price.unwrap_or(0.0)),
        volume: kl.volume.unwrap_or(0) as u64,
        timeframe,
        security: security.clone(),
    })
}

// ============================================================================
// Futu subscription type constants
// ============================================================================

/// Subscription type for basic quote data
pub const SUB_TYPE_QUOTE: i32 = 1;
/// Subscription type for 1-minute klines
pub const SUB_TYPE_KL_1MIN: i32 = 4;
/// Subscription type for daily klines
pub const SUB_TYPE_KL_DAY: i32 = 5;
/// Subscription type for 5-minute klines
pub const SUB_TYPE_KL_5MIN: i32 = 6;
/// Subscription type for 15-minute klines
pub const SUB_TYPE_KL_15MIN: i32 = 7;
/// Subscription type for 30-minute klines
pub const SUB_TYPE_KL_30MIN: i32 = 8;
/// Subscription type for 60-minute klines
pub const SUB_TYPE_KL_60MIN: i32 = 9;
/// Subscription type for real-time ticks
pub const SUB_TYPE_RT: i32 = 10;

/// Map a `Timeframe` to the Futu subscription type constant.
pub fn timeframe_to_sub_type(tf: Timeframe) -> i32 {
    match tf {
        Timeframe::Minute1 => SUB_TYPE_KL_1MIN,
        Timeframe::Minute5 => SUB_TYPE_KL_5MIN,
        Timeframe::Minute15 => SUB_TYPE_KL_15MIN,
        Timeframe::Minute30 => SUB_TYPE_KL_30MIN,
        Timeframe::Minute60 => SUB_TYPE_KL_60MIN,
        Timeframe::Daily => SUB_TYPE_KL_DAY,
        Timeframe::Weekly => SUB_TYPE_KL_DAY, // Weekly uses daily sub
    }
}

/// Map a Futu kl_type int to a `Timeframe`.
pub fn kl_type_to_timeframe(kl_type: i32) -> Option<Timeframe> {
    match kl_type {
        1 => Some(Timeframe::Minute1),  // KLType_1Min
        6 => Some(Timeframe::Minute5),  // KLType_5Min
        7 => Some(Timeframe::Minute15), // KLType_15Min
        8 => Some(Timeframe::Minute30), // KLType_30Min
        9 => Some(Timeframe::Minute60), // KLType_60Min
        5 => Some(Timeframe::Daily),    // KLType_Day
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;
    use rust_decimal_macros::dec;

    #[test]
    fn test_f64_to_decimal_normal() {
        let d = f64_to_decimal(3.145);
        assert!(d > dec!(3.14) && d < dec!(3.15));
    }

    #[test]
    fn test_f64_to_decimal_nan() {
        let d = f64_to_decimal(f64::NAN);
        assert_eq!(d, Decimal::ZERO);
    }

    #[test]
    fn test_f64_to_decimal_infinity() {
        let d = f64_to_decimal(f64::INFINITY);
        assert_eq!(d, Decimal::ZERO);
    }

    #[test]
    fn test_futu_security_to_id_sh() {
        let sec = FutuSecurity {
            market: 21,
            code: "510050".into(),
        };
        let id = futu_security_to_id(&sec).unwrap();
        assert_eq!(id.code, "510050");
        assert_eq!(id.exchange, Exchange::SH);
    }

    #[test]
    fn test_futu_security_to_id_sz() {
        let sec = FutuSecurity {
            market: 22,
            code: "159915".into(),
        };
        let id = futu_security_to_id(&sec).unwrap();
        assert_eq!(id.code, "159915");
        assert_eq!(id.exchange, Exchange::SZ);
    }

    #[test]
    fn test_futu_security_to_id_unknown_market() {
        let sec = FutuSecurity {
            market: 99,
            code: "TEST".into(),
        };
        assert!(futu_security_to_id(&sec).is_err());
    }

    #[test]
    fn test_id_to_futu_security() {
        let id = SecurityId::etf("510050", Exchange::SH);
        let sec = id_to_futu_security(&id);
        assert_eq!(sec.market, 21);
        assert_eq!(sec.code, "510050");
    }

    #[test]
    fn test_parse_futu_timestamp() {
        let ts = parse_futu_timestamp("2025-01-15 09:35:00").unwrap();
        // CST 09:35 = UTC 01:35
        assert_eq!(ts.hour(), 1);
        assert_eq!(ts.minute(), 35);
    }

    #[test]
    fn test_parse_futu_timestamp_invalid() {
        assert!(parse_futu_timestamp("not-a-date").is_err());
    }

    #[test]
    fn test_kline_to_bar() {
        let kl = KLine {
            time: "2025-01-15 09:35:00".into(),
            is_blank: false,
            open_price: Some(3.10),
            high_price: Some(3.15),
            low_price: Some(3.08),
            close_price: Some(3.12),
            last_close_price: Some(3.09),
            volume: Some(50000),
            turnover: Some(156000.0),
        };
        let security = SecurityId::etf("510050", Exchange::SH);
        let bar = kline_to_bar(&kl, &security, Timeframe::Minute5).unwrap();

        assert_eq!(bar.security.code, "510050");
        assert_eq!(bar.timeframe, Timeframe::Minute5);
        assert_eq!(bar.volume, 50000);
        assert!(bar.open > dec!(3.09) && bar.open < dec!(3.11));
        assert!(bar.high > dec!(3.14) && bar.high < dec!(3.16));
    }

    #[test]
    fn test_kline_to_bar_blank() {
        let kl = KLine {
            time: "2025-01-15 09:35:00".into(),
            is_blank: true,
            open_price: None,
            high_price: None,
            low_price: None,
            close_price: None,
            last_close_price: None,
            volume: None,
            turnover: None,
        };
        let security = SecurityId::etf("510050", Exchange::SH);
        assert!(kline_to_bar(&kl, &security, Timeframe::Minute5).is_err());
    }

    #[test]
    fn test_init_connect_request_encode_decode() {
        let req = InitConnectRequestWrapper {
            c2s: InitConnectRequest {
                client_ver: 300,
                client_id: "test".into(),
                recv_notify: Some(true),
            },
        };
        let bytes = req.encode_to_vec();
        let decoded = InitConnectRequestWrapper::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded.c2s.client_ver, 300);
        assert_eq!(decoded.c2s.client_id, "test");
    }

    #[test]
    fn test_sub_request_encode_decode() {
        let req = SubRequestWrapper {
            c2s: SubRequest {
                security_list: vec![FutuSecurity {
                    market: 1,
                    code: "510050".into(),
                }],
                sub_type_list: vec![SUB_TYPE_KL_5MIN],
                is_sub_or_un_sub: true,
                is_reg_or_un_reg_push: Some(true),
            },
        };
        let bytes = req.encode_to_vec();
        let decoded = SubRequestWrapper::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded.c2s.security_list.len(), 1);
        assert!(decoded.c2s.is_sub_or_un_sub);
    }

    #[test]
    fn test_timeframe_to_sub_type() {
        assert_eq!(timeframe_to_sub_type(Timeframe::Minute1), SUB_TYPE_KL_1MIN);
        assert_eq!(timeframe_to_sub_type(Timeframe::Minute5), SUB_TYPE_KL_5MIN);
        assert_eq!(timeframe_to_sub_type(Timeframe::Daily), SUB_TYPE_KL_DAY);
    }

    #[test]
    fn test_kl_type_to_timeframe() {
        assert_eq!(kl_type_to_timeframe(1), Some(Timeframe::Minute1));
        assert_eq!(kl_type_to_timeframe(6), Some(Timeframe::Minute5));
        assert_eq!(kl_type_to_timeframe(5), Some(Timeframe::Daily));
        assert_eq!(kl_type_to_timeframe(999), None);
    }

    use chrono::Timelike;
}
