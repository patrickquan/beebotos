//! WebSocket connection lifecycle (OpenClaw: ws-connection.ts)

use axum::extract::ws::{Message as WsMessage, WebSocket};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};
use uuid::Uuid;

use crate::websocket::types::ConnectChallengePayload;

/// WebSocket client connection
pub struct WsConnection {
    pub conn_id: String,
    pub socket: Arc<Mutex<WebSocket>>,
    pub authenticated: bool,
    pub user_id: Option<String>,
    pub subscribed_sessions: Vec<String>,
    pub created_at: std::time::Instant,
}

impl WsConnection {
    pub fn new(socket: WebSocket) -> Self {
        Self {
            conn_id: Uuid::new_v4().to_string(),
            socket: Arc::new(Mutex::new(socket)),
            authenticated: false,
            user_id: None,
            subscribed_sessions: Vec::new(),
            created_at: std::time::Instant::now(),
        }
    }

    pub async fn send_challenge(&self) -> Result<(), String> {
        let challenge = serde_json::json!({
            "type": "event",
            "event": "connect.challenge",
            "payload": ConnectChallengePayload {
                nonce: Uuid::new_v4().to_string(),
                ts: current_timestamp_ms(),
            },
        });

        self.send_raw(challenge.to_string()).await
    }

    pub async fn send_auth_success(&self) -> Result<(), String> {
        let msg = serde_json::json!({
            "type": "event",
            "event": "auth.success",
            "payload": { "status": "authenticated" },
        });

        self.send_raw(msg.to_string()).await
    }

    pub async fn send_raw(&self, text: String) -> Result<(), String> {
        let mut socket = self.socket.lock().await;
        socket
            .send(WsMessage::Text(text))
            .await
            .map_err(|e| format!("Failed to send: {}", e))
    }
}

fn current_timestamp_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
