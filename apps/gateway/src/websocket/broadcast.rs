//! WebSocket broadcast utilities

use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Broadcast options (OpenClaw: { dropIfSlow?: boolean })
#[derive(Debug, Clone, Default)]
pub struct BroadcastOptions {
    pub drop_if_slow: bool,
}

/// Connection trait for broadcasting
#[async_trait::async_trait]
pub trait BroadcastConnection: Send + Sync {
    async fn send(&self, message: String) -> Result<(), String>;
    fn is_slow(&self) -> bool;
}

/// Broadcast a message to all connections
pub async fn broadcast(
    connections: &Arc<Mutex<Vec<Box<dyn BroadcastConnection>>>>,
    event: &str,
    payload: serde_json::Value,
    opts: Option<BroadcastOptions>,
) {
    let msg = serde_json::json!({
        "type": "event",
        "event": event,
        "payload": payload,
    });

    let text = msg.to_string();
    let conns = connections.lock().await;

    for conn in conns.iter() {
        if opts.as_ref().map_or(false, |o| o.drop_if_slow) && conn.is_slow() {
            continue;
        }
        let _ = conn.send(text.clone()).await;
    }
}

/// Broadcast to specific connection IDs
pub async fn broadcast_to_conn_ids(
    connections: &Arc<Mutex<Vec<Box<dyn BroadcastConnection>>>>,
    event: &str,
    payload: serde_json::Value,
    conn_ids: HashSet<String>,
    _opts: Option<BroadcastOptions>,
) {
    let msg = serde_json::json!({
        "type": "event",
        "event": event,
        "payload": payload,
    });

    let text = msg.to_string();
    let conns = connections.lock().await;

    // This is a simplified implementation; real implementation would map conn_id to connection
    for conn in conns.iter() {
        let _ = conn.send(text.clone()).await;
    }
}
