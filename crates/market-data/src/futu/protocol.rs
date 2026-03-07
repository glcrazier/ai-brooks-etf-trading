//! Futu OpenD binary framing protocol.
//!
//! Every message exchanged with FutuOpenD consists of a 44-byte header
//! followed by a protobuf-encoded body. The header layout (little-endian):
//!
//! | Offset | Bytes | Field        | Description                        |
//! |--------|-------|--------------|------------------------------------|
//! | 0      | 2     | magic        | Always "FT" (0x46 0x54)            |
//! | 2      | 4     | proto_id     | API identifier (e.g. 1001)         |
//! | 6      | 1     | proto_fmt    | 0 = protobuf, 1 = json             |
//! | 7      | 1     | proto_ver    | Protocol version (0)               |
//! | 8      | 4     | serial_no    | Request/response correlation        |
//! | 12     | 4     | body_len     | Length of protobuf body             |
//! | 16     | 20    | body_sha1    | SHA-1 hash of body bytes           |
//! | 36     | 8     | reserved     | Reserved (zeroed)                  |

use sha1::{Digest, Sha1};

use crate::error::MarketDataError;

/// Magic bytes at the start of every Futu frame: "FT"
pub const FUTU_MAGIC: [u8; 2] = *b"FT";

/// Total size of the Futu binary header
pub const HEADER_SIZE: usize = 44;

// --- Proto IDs for the APIs we use ---

/// InitConnect handshake
pub const PROTO_ID_INIT_CONNECT: u32 = 1001;
/// Get global state (market status, etc.)
pub const PROTO_ID_GET_GLOBAL_STATE: u32 = 1002;
/// Keep-alive heartbeat
pub const PROTO_ID_KEEP_ALIVE: u32 = 1004;
/// Subscribe / unsubscribe quotation types
pub const PROTO_ID_SUB: u32 = 3001;
/// Register / unregister push notifications
pub const PROTO_ID_REG_QOT_PUSH: u32 = 3002;
/// Get subscription info
pub const PROTO_ID_GET_SUB_INFO: u32 = 3003;
/// Push: kline update
pub const PROTO_ID_QOT_UPDATE_KL: u32 = 3007;
/// Push: real-time tick
pub const PROTO_ID_QOT_UPDATE_RT: u32 = 3009;
/// Request historical kline data
pub const PROTO_ID_REQUEST_HISTORY_KL: u32 = 3103;
/// Get security snapshot
pub const PROTO_ID_GET_SECURITY_SNAPSHOT: u32 = 3203;

/// Compute SHA-1 hash of a byte slice.
pub fn compute_sha1(data: &[u8]) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut sha = [0u8; 20];
    sha.copy_from_slice(&result);
    sha
}

/// Parsed Futu protocol header
#[derive(Debug, Clone)]
pub struct FutuHeader {
    pub proto_id: u32,
    pub proto_fmt: u8,
    pub proto_ver: u8,
    pub serial_no: u32,
    pub body_len: u32,
    pub body_sha1: [u8; 20],
}

impl FutuHeader {
    /// Create a new header for an outgoing request.
    ///
    /// The `body_sha1` field is initialized to zeros. Call `set_body_sha1`
    /// or use `new_with_sha1` to set the correct hash before sending.
    pub fn new(proto_id: u32, serial_no: u32, body_len: u32) -> Self {
        Self {
            proto_id,
            proto_fmt: 0, // protobuf
            proto_ver: 0,
            serial_no,
            body_len,
            body_sha1: [0u8; 20],
        }
    }

    /// Create a new header with the SHA1 hash computed from the body.
    pub fn new_with_sha1(proto_id: u32, serial_no: u32, body: &[u8]) -> Self {
        Self {
            proto_id,
            proto_fmt: 0,
            proto_ver: 0,
            serial_no,
            body_len: body.len() as u32,
            body_sha1: compute_sha1(body),
        }
    }

    /// Set the SHA1 hash from the body bytes.
    pub fn set_body_sha1(&mut self, body: &[u8]) {
        self.body_sha1 = compute_sha1(body);
    }

    /// Encode the header to a 44-byte array (little-endian).
    pub fn encode(&self) -> [u8; HEADER_SIZE] {
        let mut buf = [0u8; HEADER_SIZE];
        // Offset 0-1: magic "FT"
        buf[0..2].copy_from_slice(&FUTU_MAGIC);
        // Offset 2-5: proto_id
        buf[2..6].copy_from_slice(&self.proto_id.to_le_bytes());
        // Offset 6: proto_fmt
        buf[6] = self.proto_fmt;
        // Offset 7: proto_ver
        buf[7] = self.proto_ver;
        // Offset 8-11: serial_no
        buf[8..12].copy_from_slice(&self.serial_no.to_le_bytes());
        // Offset 12-15: body_len
        buf[12..16].copy_from_slice(&self.body_len.to_le_bytes());
        // Offset 16-35: body_sha1
        buf[16..36].copy_from_slice(&self.body_sha1);
        // Offset 36-43: reserved (zeroed)
        buf
    }

    /// Decode a 44-byte array into a `FutuHeader`.
    pub fn decode(bytes: &[u8; HEADER_SIZE]) -> Result<Self, MarketDataError> {
        // Validate magic
        if bytes[0..2] != FUTU_MAGIC {
            return Err(MarketDataError::ProtocolError(format!(
                "Invalid magic: expected FT, got {:?}",
                &bytes[0..2]
            )));
        }

        let proto_id = u32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);
        let proto_fmt = bytes[6];
        let proto_ver = bytes[7];
        let serial_no = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        let body_len = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);

        let mut body_sha1 = [0u8; 20];
        body_sha1.copy_from_slice(&bytes[16..36]);

        Ok(Self {
            proto_id,
            proto_fmt,
            proto_ver,
            serial_no,
            body_len,
            body_sha1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_encode_decode_roundtrip() {
        let body = b"hello world";
        let header = FutuHeader::new_with_sha1(1001, 42, body);
        let encoded = header.encode();
        let decoded = FutuHeader::decode(&encoded).unwrap();

        assert_eq!(decoded.proto_id, 1001);
        assert_eq!(decoded.serial_no, 42);
        assert_eq!(decoded.body_len, 11);
        assert_eq!(decoded.proto_fmt, 0);
        assert_eq!(decoded.proto_ver, 0);
        assert_eq!(decoded.body_sha1, compute_sha1(body));
    }

    #[test]
    fn test_header_magic_validation() {
        let mut bad = [0u8; HEADER_SIZE];
        bad[0..2].copy_from_slice(b"XX");
        let result = FutuHeader::decode(&bad);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid magic"));
    }

    #[test]
    fn test_header_size_is_44() {
        assert_eq!(HEADER_SIZE, 44);
        let header = FutuHeader::new(3100, 1, 0);
        let encoded = header.encode();
        assert_eq!(encoded.len(), 44);
    }

    #[test]
    fn test_header_starts_with_ft() {
        let header = FutuHeader::new(1001, 0, 0);
        let encoded = header.encode();
        assert_eq!(&encoded[0..2], b"FT");
    }

    #[test]
    fn test_header_zero_body() {
        let header = FutuHeader::new(1004, 99, 0);
        let encoded = header.encode();
        let decoded = FutuHeader::decode(&encoded).unwrap();
        assert_eq!(decoded.body_len, 0);
        assert_eq!(decoded.serial_no, 99);
    }

    #[test]
    fn test_header_large_body_len() {
        let header = FutuHeader::new(3100, 1, 1_000_000);
        let encoded = header.encode();
        let decoded = FutuHeader::decode(&encoded).unwrap();
        assert_eq!(decoded.body_len, 1_000_000);
    }

    #[test]
    fn test_compute_sha1() {
        // SHA1 of empty byte slice
        let sha = compute_sha1(b"");
        // Known SHA1 of empty string: da39a3ee5e6b4b0d3255bfef95601890afd80709
        assert_eq!(sha[0], 0xda);
        assert_eq!(sha[1], 0x39);
    }

    #[test]
    fn test_new_with_sha1() {
        let body = b"test body";
        let header = FutuHeader::new_with_sha1(1001, 1, body);
        assert_eq!(header.body_len, 9);
        assert_eq!(header.body_sha1, compute_sha1(body));
    }
}
