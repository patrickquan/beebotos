//! WebSocket message handler (OpenClaw: ws-connection/message-handler.ts)

use std::sync::Arc;

use axum::extract::ws::{Message as WsMessage, WebSocket};
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::websocket::connection::WsConnection;

/// Handle a new WebSocket connection
pub async fn handle_connection(socket: WebSocket, connections: Arc<Mutex<Vec<WsConnection>>>) {
    let conn = WsConnection::new(socket);
    let conn_id = conn.conn_id.clone();

    info!("WS connection opened: {}", conn_id);

    // Send challenge
    if let Err(e) = conn.send_challenge().await {
        warn!("Failed to send challenge to {}: {}", conn_id, e);
        return;
    }

    // Add to connections
    connections.lock().await.push(conn);

    // Handle incoming messages
    let connections_clone = connections.clone();
    tokio::spawn(async move {
        // Find our connection
        loop {
            let msg = {
                let conns = connections_clone.lock().await;
                let Some(conn) = conns.iter().find(|c| c.conn_id == conn_id) else {
                    break;
                };
                let mut socket = conn.socket.lock().await;
                socket.recv().await
            };

            match msg {
                Some(Ok(WsMessage::Text(text))) => {
                    handle_message(&conn_id, &text, connections_clone.clone()).await;
                }
                Some(Ok(WsMessage::Close(_))) | Some(Err(_)) | None => {
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

    let mut conns = connections.lock().await;
    if let Some(conn) = conns.iter_mut().find(|c| c.conn_id == conn_id) {
        if let Some(uid) = user_id {
            conn.authenticated = true;
            conn.user_id = Some(uid);
            let _ = conn.send_auth_success().await;
        }
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
        return;
    }

    let mut conns = connections.lock().await;
    if let Some(conn) = conns.iter_mut().find(|c| c.conn_id == conn_id) {
        if let Some(sk) = session_key {
            conn.subscribed_sessions.push(sk.to_string());
            info!("Conn {} subscribed to {}", conn_id, sk);
        }
    }
}
