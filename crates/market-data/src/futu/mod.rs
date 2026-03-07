//! Futu OpenD integration layer.
//!
//! This module provides the TCP/protobuf client for communicating with
//! FutuOpenD, a local gateway to Futu Securities' market data and trading APIs.
//!
//! # Submodules
//!
//! - `protocol` — Binary framing (44-byte header) and proto_id constants
//! - `messages` — Hand-written protobuf message types (minimal subset)
//! - `connection` — Raw TCP connection with frame-level send/receive
//! - `client` — High-level API client (handshake, subscribe, fetch, push)

pub mod client;
pub mod connection;
pub mod messages;
pub mod protocol;

// Re-export key types
pub use client::{FutuClient, FutuPushEvent};
pub use connection::FutuConnection;
pub use messages::{
    f64_to_decimal, futu_security_to_id, id_to_futu_security, kline_to_bar, parse_futu_timestamp,
    timeframe_to_sub_type, FutuSecurity, KLine,
};
pub use protocol::{FutuHeader, HEADER_SIZE};
