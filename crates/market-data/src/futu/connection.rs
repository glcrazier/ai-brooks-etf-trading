//! TCP connection to Futu OpenD with binary framing.
//!
//! Handles raw send/receive of Futu-framed messages over a TCP socket.
//! Higher-level request/response logic lives in `client.rs`.

use std::sync::atomic::{AtomicU32, Ordering};

use prost::Message;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tracing::{debug, trace};

use super::protocol::{FutuHeader, HEADER_SIZE};
use crate::error::MarketDataError;

/// A raw TCP connection to Futu OpenD with frame-level send/receive.
pub struct FutuConnection {
    reader: OwnedReadHalf,
    writer: OwnedWriteHalf,
    serial_no: AtomicU32,
}

impl FutuConnection {
    /// Connect to Futu OpenD at the given host and port.
    pub async fn connect(host: &str, port: u16) -> Result<Self, MarketDataError> {
        let addr = format!("{}:{}", host, port);
        debug!(addr = %addr, "Connecting to FutuOpenD");

        let stream = TcpStream::connect(&addr).await.map_err(|e| {
            MarketDataError::ConnectionFailed(format!("TCP connect to {} failed: {}", addr, e))
        })?;

        // Disable Nagle's algorithm for low-latency messaging
        stream.set_nodelay(true).ok();

        let (reader, writer) = stream.into_split();

        debug!("Connected to FutuOpenD at {}", addr);
        Ok(Self {
            reader,
            writer,
            serial_no: AtomicU32::new(1),
        })
    }

    /// Get the next serial number for a request.
    pub fn next_serial(&self) -> u32 {
        self.serial_no.fetch_add(1, Ordering::SeqCst)
    }

    /// Send a raw Futu-framed message (header + body bytes).
    pub async fn send_raw(&mut self, proto_id: u32, body: &[u8]) -> Result<u32, MarketDataError> {
        let serial = self.next_serial();
        let header = FutuHeader::new_with_sha1(proto_id, serial, body);
        let header_bytes = header.encode();

        trace!(proto_id, serial, body_len = body.len(), "Sending frame");

        self.writer.write_all(&header_bytes).await?;
        if !body.is_empty() {
            self.writer.write_all(body).await?;
        }
        self.writer.flush().await?;

        Ok(serial)
    }

    /// Receive a single Futu-framed message (header + body bytes).
    pub async fn recv_raw(&mut self) -> Result<(FutuHeader, Vec<u8>), MarketDataError> {
        // Read header
        let mut header_buf = [0u8; HEADER_SIZE];
        self.reader.read_exact(&mut header_buf).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                MarketDataError::ConnectionClosed
            } else {
                MarketDataError::Io(e)
            }
        })?;

        let header = FutuHeader::decode(&header_buf)?;

        // Read body
        let mut body = vec![0u8; header.body_len as usize];
        if header.body_len > 0 {
            self.reader.read_exact(&mut body).await.map_err(|e| {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    MarketDataError::ConnectionClosed
                } else {
                    MarketDataError::Io(e)
                }
            })?;
        }

        trace!(
            proto_id = header.proto_id,
            serial = header.serial_no,
            body_len = header.body_len,
            "Received frame"
        );

        Ok((header, body))
    }

    /// Send a protobuf request and receive a protobuf response.
    ///
    /// Encodes `req` to bytes, sends it with the given `proto_id`, then
    /// reads the response and decodes it into type `Resp`.
    pub async fn request<Req, Resp>(
        &mut self,
        proto_id: u32,
        req: &Req,
    ) -> Result<(FutuHeader, Resp), MarketDataError>
    where
        Req: Message,
        Resp: Message + Default,
    {
        let body = req.encode_to_vec();
        let _serial = self.send_raw(proto_id, &body).await?;

        let (header, resp_body) = self.recv_raw().await?;
        let resp = Resp::decode(resp_body.as_slice())?;

        Ok((header, resp))
    }

    /// Split this connection into separate read and write halves.
    ///
    /// Used when we need to read push notifications on one task
    /// while sending requests on another.
    pub fn into_split(self) -> (FutuReader, FutuWriter) {
        (
            FutuReader {
                reader: self.reader,
            },
            FutuWriter {
                writer: self.writer,
                serial_no: self.serial_no,
            },
        )
    }
}

/// Read half of a Futu connection — receives frames.
pub struct FutuReader {
    reader: OwnedReadHalf,
}

impl FutuReader {
    /// Receive a single frame (header + body).
    pub async fn recv_raw(&mut self) -> Result<(FutuHeader, Vec<u8>), MarketDataError> {
        let mut header_buf = [0u8; HEADER_SIZE];
        self.reader.read_exact(&mut header_buf).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                MarketDataError::ConnectionClosed
            } else {
                MarketDataError::Io(e)
            }
        })?;

        let header = FutuHeader::decode(&header_buf)?;
        let mut body = vec![0u8; header.body_len as usize];
        if header.body_len > 0 {
            self.reader.read_exact(&mut body).await?;
        }

        Ok((header, body))
    }
}

/// Write half of a Futu connection — sends frames.
pub struct FutuWriter {
    writer: OwnedWriteHalf,
    serial_no: AtomicU32,
}

impl FutuWriter {
    /// Send a raw frame.
    pub async fn send_raw(&mut self, proto_id: u32, body: &[u8]) -> Result<u32, MarketDataError> {
        let serial = self.serial_no.fetch_add(1, Ordering::SeqCst);
        let header = FutuHeader::new_with_sha1(proto_id, serial, body);
        let header_bytes = header.encode();

        self.writer.write_all(&header_bytes).await?;
        if !body.is_empty() {
            self.writer.write_all(body).await?;
        }
        self.writer.flush().await?;

        Ok(serial)
    }

    /// Send a protobuf-encoded request.
    pub async fn send<Req: Message>(
        &mut self,
        proto_id: u32,
        req: &Req,
    ) -> Result<u32, MarketDataError> {
        let body = req.encode_to_vec();
        self.send_raw(proto_id, &body).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serial_number_increments() {
        let serial = AtomicU32::new(1);
        assert_eq!(serial.fetch_add(1, Ordering::SeqCst), 1);
        assert_eq!(serial.fetch_add(1, Ordering::SeqCst), 2);
        assert_eq!(serial.fetch_add(1, Ordering::SeqCst), 3);
    }

    // Integration tests with actual TCP would require a running FutuOpenD.
    // Those go in the integration test module, gated behind #[cfg(feature = "integration")].
}
