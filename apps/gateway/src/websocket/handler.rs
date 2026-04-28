//! WebSocket message handler (OpenClaw: ws-connection/message-handler.ts)

use std::sync::Arc;

use axum::extract::ws::{Message as WsMessage, WebSocket};
use futures::{SinkExt, StreamExt};
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::websocket::connection::WsConnection;

/// Handle a new WebSocket connection
pub async fn handle_connection(socket: WebSocket, connections: Arc<Mutex<Vec<WsConnection>>>) {
    // 拆分 WebSocket 为发送端和接收端，彻底消除 socket 锁竞争
    let (mut sender, mut receiver) = socket.split();
    let (send_tx, mut send_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    let conn = WsConnection::new(send_tx);
    let conn_id = conn.conn_id.clone();

    info!("WS connection opened: {}", conn_id);

    // 启动独立发送任务，持有 sender
    let conn_id_send = conn_id.clone();
    tokio::spawn(async move {
        while let Some(text) = send_rx.recv().await {
            if sender
                .send(WsMessage::Text(text))
                .await
                .is_err()
            {
                warn!("WS send task failed for {}", conn_id_send);
                break;
            }
        }
        info!("WS send task closed: {}", conn_id_send);
    });

    // Send challenge
    if let Err(e) = conn.send_challenge().await {
        warn!("Failed to send challenge to {}: {}", conn_id, e);
        return;
    }

    // Add to connections
    connections.lock().await.push(conn);

    // Handle incoming messages using receiver (无锁)
    let connections_clone = connections.clone();
    tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(WsMessage::Text(text)) => {
                    handle_message(&conn_id, &text, connections_clone.clone()).await;
                }
                Ok(WsMessage::Close(_)) | Err(_) => {
                    info!("WS connection closed: {}", conn_id);
                    break;
                }
                _ => {}
            }
        }

        // Remove connection
        let mut conns = connections_clone.lock().await;
        conns.retain(|c| c.conn_id != conn_id);
    });
}

async fn handle_message(conn_id: &str, text: &str, connections: Arc<Mutex<Vec<WsConnection>>>) {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };

    let msg_type = json.get("type").and_then(|v| v.as_str());
    let event = json.get("event").and_then(|v| v.as_str());

    match (msg_type, event) {
        (Some("auth"), _) => {
            handle_auth(conn_id, &json, connections).await;
        }
        (Some("subscribe"), _) => {
            handle_subscribe(conn_id, &json, connections).await;
        }
        _ => {}
    }
}

async fn handle_auth(
    conn_id: &str,
    json: &serde_json::Value,
    connections: Arc<Mutex<Vec<WsConnection>>>,
) {
    let token = json.get("token").and_then(|v| v.as_str());

    // TODO: Validate JWT token
    let user_id = token.map(|_| "user_id".to_string());

    // 设置认证状态
    let send_tx = {
        let mut conns = connections.lock().await;
        if let Some(conn) = conns.iter_mut().find(|c| c.conn_id == conn_id) {
            if let Some(uid) = user_id {
                conn.authenticated = true;
                conn.user_id = Some(uid);
            }
            // 克隆 send_tx 以便在锁外使用
            Some(conn.send_tx.clone())
        } else {
            None
        }
    };

    // 发送 auth success 在锁外
    if let Some(send_tx) = send_tx {
        let msg = serde_json::json!({
            "type": "event",
            "event": "auth.success",
            "payload": { "status": "authenticated" },
        });
        let _ = send_tx.send(msg.to_string());
    }
}

async fn handle_subscribe(
    conn_id: &str,
    json: &serde_json::Value,
    connections: Arc<Mutex<Vec<WsConnection>>>,
) {
    let channel = json.get("channel").and_then(|v| v.as_str());
    let session_key = json.get("session_key").and_then(|v| v.as_str());

    if channel != Some("webchat") {
        info!("Conn {} subscribe rejected: channel={:?} != webchat", conn_id, channel);
        return;
    }

    let mut conns = connections.lock().await;
    if let Some(conn) = conns.iter_mut().find(|c| c.conn_id == conn_id) {
        if let Some(sk) = session_key {
            conn.subscribed_sessions.push(sk.to_string());
            info!("Conn {} subscribed to {}. All sessions: {:?}", conn_id, sk, conn.subscribed_sessions);
        } else {
            info!("Conn {} subscribe missing session_key", conn_id);
        }
    } else {
        info!("Conn {} subscribe: connection not found", conn_id);
    }
}
