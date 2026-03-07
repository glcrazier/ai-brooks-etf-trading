//! High-level Futu OpenD client.
//!
//! Wraps `FutuConnection` with API-level methods: handshake, subscribe,
//! fetch historical klines, and receive push notifications.

use std::sync::Arc;

use brooks_core::market::SecurityId;
use prost::Message;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, trace, warn};

use super::connection::{FutuConnection, FutuReader, FutuWriter};
use super::messages::*;
use super::protocol::*;
use crate::config::FutuConfig;
use crate::error::MarketDataError;

/// Events pushed by Futu OpenD to the client
#[derive(Debug, Clone)]
pub enum FutuPushEvent {
    /// A kline (bar) update was received
    KLineUpdate {
        security: FutuSecurity,
        kl_type: i32,
        klines: Vec<KLine>,
    },
    /// A real-time tick was received
    TickUpdate {
        security: FutuSecurity,
        ticks: Vec<Tick>,
    },
}

/// High-level client for communicating with Futu OpenD.
///
/// Handles the InitConnect handshake, subscription management,
/// historical data requests, and push notification dispatching.
pub struct FutuClient {
    writer: Arc<Mutex<FutuWriter>>,
    #[allow(dead_code)]
    config: FutuConfig,
    conn_id: u64,
    keep_alive_handle: Option<JoinHandle<()>>,
    push_handle: Option<JoinHandle<()>>,
}

impl FutuClient {
    /// Connect to Futu OpenD and perform the InitConnect handshake.
    pub async fn connect(config: FutuConfig) -> Result<Self, MarketDataError> {
        let mut conn = FutuConnection::connect(&config.host, config.port).await?;

        // Perform InitConnect handshake
        let init_req = InitConnectRequestWrapper {
            c2s: InitConnectRequest {
                client_ver: 300,
                client_id: config.client_id.clone(),
                recv_notify: Some(true),
            },
        };

        let (_header, resp): (_, InitConnectResponseWrapper) =
            conn.request(PROTO_ID_INIT_CONNECT, &init_req).await?;

        // Check response status
        if resp.ret_type != 0 {
            return Err(MarketDataError::ApiError {
                code: resp.err_code.unwrap_or(-1),
                message: resp.ret_msg.unwrap_or_default(),
            });
        }

        let s2c = resp
            .s2c
            .ok_or_else(|| MarketDataError::InvalidResponse("InitConnect: missing s2c".into()))?;

        let conn_id = s2c.conn_id;
        let keep_alive_interval = s2c.keep_alive_interval;

        info!(
            conn_id,
            server_ver = s2c.server_ver,
            keep_alive_interval,
            "InitConnect handshake successful"
        );

        let (_reader, writer) = conn.into_split();
        let writer = Arc::new(Mutex::new(writer));

        // Start keep-alive loop
        let ka_writer = Arc::clone(&writer);
        let ka_interval = keep_alive_interval as u64;
        let keep_alive_handle = tokio::spawn(async move {
            Self::keep_alive_loop(ka_writer, ka_interval).await;
        });

        Ok(Self {
            writer,
            config,
            conn_id,
            keep_alive_handle: Some(keep_alive_handle),
            push_handle: None,
        })
    }

    /// Subscribe to data types for the given securities.
    pub async fn subscribe(
        &self,
        securities: &[SecurityId],
        sub_types: &[i32],
    ) -> Result<(), MarketDataError> {
        let security_list: Vec<FutuSecurity> = securities.iter().map(id_to_futu_security).collect();

        let req = SubRequestWrapper {
            c2s: SubRequest {
                security_list,
                sub_type_list: sub_types.to_vec(),
                is_sub_or_un_sub: true,
                is_reg_or_un_reg_push: Some(true),
            },
        };

        let body = req.encode_to_vec();
        let mut writer = self.writer.lock().await;
        writer.send_raw(PROTO_ID_SUB, &body).await?;

        debug!(
            securities = ?securities.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            sub_types = ?sub_types,
            "Subscribed"
        );

        Ok(())
    }

    /// Unsubscribe from data types for the given securities.
    pub async fn unsubscribe(
        &self,
        securities: &[SecurityId],
        sub_types: &[i32],
    ) -> Result<(), MarketDataError> {
        let security_list: Vec<FutuSecurity> = securities.iter().map(id_to_futu_security).collect();

        let req = SubRequestWrapper {
            c2s: SubRequest {
                security_list,
                sub_type_list: sub_types.to_vec(),
                is_sub_or_un_sub: false,
                is_reg_or_un_reg_push: Some(false),
            },
        };

        let body = req.encode_to_vec();
        let mut writer = self.writer.lock().await;
        writer.send_raw(PROTO_ID_SUB, &body).await?;

        debug!("Unsubscribed");
        Ok(())
    }

    /// Request historical kline data from Futu OpenD.
    ///
    /// Note: This is a simplified version that sends the request but
    /// receives the response through the push channel. For a synchronous
    /// request/response pattern, the caller should coordinate with the
    /// push receiver to match serial numbers.
    pub async fn request_history_kl(
        &self,
        security: &SecurityId,
        kl_type: i32,
        begin: &str,
        end: &str,
        max_count: Option<i32>,
    ) -> Result<(), MarketDataError> {
        let req = RequestHistoryKLRequestWrapper {
            c2s: RequestHistoryKLRequest {
                rehab_type: 1, // Forward adjustment
                kl_type,
                security: id_to_futu_security(security),
                begin_time: begin.to_string(),
                end_time: end.to_string(),
                max_count,
                need_kl_fields_flag: Some(0x1FF), // All fields
            },
        };

        let body = req.encode_to_vec();
        let mut writer = self.writer.lock().await;
        writer.send_raw(PROTO_ID_REQUEST_HISTORY_KL, &body).await?;

        debug!(
            security = %security,
            kl_type,
            begin,
            "Requested historical klines"
        );

        Ok(())
    }

    /// Start the push notification receiver.
    ///
    /// Spawns a background task that reads frames from the Futu connection
    /// and dispatches `FutuPushEvent` values to the returned channel.
    pub fn start_push_receiver(&mut self, reader: FutuReader) -> mpsc::Receiver<FutuPushEvent> {
        let (tx, rx) = mpsc::channel(256);

        let handle = tokio::spawn(async move {
            Self::push_receiver_loop(reader, tx).await;
        });

        self.push_handle = Some(handle);
        rx
    }

    /// Internal loop that reads push notifications from Futu.
    async fn push_receiver_loop(mut reader: FutuReader, tx: mpsc::Sender<FutuPushEvent>) {
        loop {
            match reader.recv_raw().await {
                Ok((header, body)) => {
                    match header.proto_id {
                        PROTO_ID_QOT_UPDATE_KL => {
                            if let Ok(wrapper) = QotUpdateKLResponseWrapper::decode(body.as_slice())
                            {
                                if let Some(s2c) = wrapper.s2c {
                                    let event = FutuPushEvent::KLineUpdate {
                                        security: s2c.security,
                                        kl_type: s2c.kl_type,
                                        klines: s2c.kl_list,
                                    };
                                    if tx.send(event).await.is_err() {
                                        debug!("Push channel closed, stopping receiver");
                                        break;
                                    }
                                }
                            }
                        }
                        PROTO_ID_QOT_UPDATE_RT => {
                            if let Ok(wrapper) = QotUpdateRTResponseWrapper::decode(body.as_slice())
                            {
                                if let Some(s2c) = wrapper.s2c {
                                    let event = FutuPushEvent::TickUpdate {
                                        security: s2c.security,
                                        ticks: s2c.rt_list,
                                    };
                                    if tx.send(event).await.is_err() {
                                        debug!("Push channel closed, stopping receiver");
                                        break;
                                    }
                                }
                            }
                        }
                        PROTO_ID_KEEP_ALIVE => {
                            // Keep-alive response — ignore
                            trace!("Keep-alive response received");
                        }
                        PROTO_ID_REQUEST_HISTORY_KL => {
                            // Historical kline response — pass through as KLineUpdate
                            if let Ok(wrapper) =
                                RequestHistoryKLResponseWrapper::decode(body.as_slice())
                            {
                                if let Some(s2c) = wrapper.s2c {
                                    if let Some(security) = s2c.security {
                                        let event = FutuPushEvent::KLineUpdate {
                                            security,
                                            kl_type: 0, // caller must track
                                            klines: s2c.kl_list,
                                        };
                                        if tx.send(event).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        other => {
                            debug!(proto_id = other, "Ignoring unhandled push message");
                        }
                    }
                }
                Err(MarketDataError::ConnectionClosed) => {
                    warn!("FutuOpenD connection closed");
                    break;
                }
                Err(e) => {
                    error!(error = %e, "Error reading from FutuOpenD");
                    break;
                }
            }
        }
    }

    /// Background keep-alive loop.
    async fn keep_alive_loop(writer: Arc<Mutex<FutuWriter>>, interval_secs: u64) {
        let interval = std::time::Duration::from_secs(interval_secs.max(5));
        loop {
            tokio::time::sleep(interval).await;
            let req = KeepAliveRequestWrapper {
                c2s: KeepAliveRequest {
                    time: chrono::Utc::now().timestamp(),
                },
            };
            let body = req.encode_to_vec();
            let mut w = writer.lock().await;
            if let Err(e) = w.send_raw(PROTO_ID_KEEP_ALIVE, &body).await {
                warn!(error = %e, "Keep-alive send failed");
                break;
            }
        }
    }

    /// Get the connection ID assigned by Futu OpenD.
    pub fn conn_id(&self) -> u64 {
        self.conn_id
    }
}

impl Drop for FutuClient {
    fn drop(&mut self) {
        if let Some(handle) = self.keep_alive_handle.take() {
            handle.abort();
        }
        if let Some(handle) = self.push_handle.take() {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_futu_push_event_variants() {
        // Verify the enum can be constructed
        let kl_event = FutuPushEvent::KLineUpdate {
            security: FutuSecurity {
                market: 1,
                code: "510050".into(),
            },
            kl_type: 6,
            klines: vec![],
        };
        assert!(matches!(kl_event, FutuPushEvent::KLineUpdate { .. }));

        let tick_event = FutuPushEvent::TickUpdate {
            security: FutuSecurity {
                market: 1,
                code: "510050".into(),
            },
            ticks: vec![],
        };
        assert!(matches!(tick_event, FutuPushEvent::TickUpdate { .. }));
    }
}
