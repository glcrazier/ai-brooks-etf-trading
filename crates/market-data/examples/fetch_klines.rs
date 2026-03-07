//! Quick shot: connect to FutuOpenD, fetch recent 5-min klines for 510050 (SSE 50 ETF).
//!
//! Run: cargo run -p brooks-market-data --example fetch_klines

use brooks_core::market::{Exchange, SecurityId};
use brooks_core::timeframe::Timeframe;
use brooks_market_data::futu::connection::FutuConnection;
use brooks_market_data::futu::messages::*;
use brooks_market_data::futu::protocol::*;
use prost::Message;
use rust_decimal::prelude::ToPrimitive;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Connect to FutuOpenD
    println!("Connecting to FutuOpenD at 127.0.0.1:11111...");
    let mut conn = FutuConnection::connect("127.0.0.1", 11111).await?;
    println!("Connected!");

    // 2. InitConnect handshake
    let init_req = InitConnectRequestWrapper {
        c2s: InitConnectRequest {
            client_ver: 300,
            client_id: "brooks-fetch-demo".into(),
            recv_notify: Some(true),
        },
    };
    let (_hdr, resp): (_, InitConnectResponseWrapper) =
        conn.request(PROTO_ID_INIT_CONNECT, &init_req).await?;

    if resp.ret_type != 0 {
        eprintln!(
            "InitConnect failed: {}",
            resp.ret_msg.as_deref().unwrap_or("unknown error")
        );
        return Ok(());
    }
    let s2c = resp.s2c.unwrap();
    println!(
        "Handshake OK — conn_id: {}, server_ver: {}",
        s2c.conn_id, s2c.server_ver
    );

    // 3. Subscribe to 5-min klines for SH.510050
    let security = SecurityId::etf("510050", Exchange::SH);
    let futu_sec = id_to_futu_security(&security);

    let sub_req = SubRequestWrapper {
        c2s: SubRequest {
            security_list: vec![futu_sec.clone()],
            sub_type_list: vec![SUB_TYPE_KL_5MIN],
            is_sub_or_un_sub: true,
            is_reg_or_un_reg_push: Some(true),
        },
    };
    let sub_body = sub_req.encode_to_vec();
    conn.send_raw(PROTO_ID_SUB, &sub_body).await?;

    // Read sub response
    let (_hdr, sub_resp_body) = conn.recv_raw().await?;
    let sub_resp = SubResponseWrapper::decode(sub_resp_body.as_slice())?;
    if sub_resp.ret_type != 0 {
        eprintln!(
            "Subscribe failed: {}",
            sub_resp.ret_msg.as_deref().unwrap_or("unknown")
        );
        return Ok(());
    }
    println!("Subscribed to 5-min KL for SH.510050");

    // 4. Request historical klines (last 20 bars)
    let hist_req = RequestHistoryKLRequestWrapper {
        c2s: RequestHistoryKLRequest {
            rehab_type: 1, // Forward adjustment
            kl_type: timeframe_to_sub_type(Timeframe::Minute5),
            security: futu_sec,
            begin_time: "2025-03-05 09:30:00".into(),
            end_time: "2025-03-06 15:00:00".into(),
            max_count: Some(20),
            need_kl_fields_flag: Some(0x1FF),
        },
    };
    let (_hdr, hist_resp): (_, RequestHistoryKLResponseWrapper) =
        conn.request(PROTO_ID_REQUEST_HISTORY_KL, &hist_req).await?;

    if hist_resp.ret_type != 0 {
        eprintln!(
            "HistoryKL failed: {}",
            hist_resp.ret_msg.as_deref().unwrap_or("unknown")
        );
        return Ok(());
    }

    let s2c = hist_resp.s2c.unwrap();
    println!(
        "\nReceived {} klines for SH.510050 (5-min):\n",
        s2c.kl_list.len()
    );
    println!(
        "{:<22} {:>10} {:>10} {:>10} {:>10} {:>12}",
        "Time (CST)", "Open", "High", "Low", "Close", "Volume"
    );
    println!("{}", "-".repeat(80));

    for kl in &s2c.kl_list {
        if kl.is_blank {
            continue;
        }
        // Convert to Bar to get proper Decimal values
        let bar = kline_to_bar(kl, &security, Timeframe::Minute5)?;
        println!(
            "{:<22} {:>10.3} {:>10.3} {:>10.3} {:>10.3} {:>12}",
            kl.time,
            bar.open.to_f64().unwrap_or(0.0),
            bar.high.to_f64().unwrap_or(0.0),
            bar.low.to_f64().unwrap_or(0.0),
            bar.close.to_f64().unwrap_or(0.0),
            bar.volume
        );
    }

    println!("\nDone!");
    Ok(())
}
