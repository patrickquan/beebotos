//! WebSocket Handlers
//!
//! Real-time communication with agents using gateway-lib's WebSocketManager.
//!
//! The `/ws` endpoint now uses our OpenClaw-style WebSocket handler for
//! chat streaming. The legacy gateway-lib WebSocketManager is still
//! available for status queries and admin broadcasts.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use gateway::middleware::AuthUser;
use tracing::info;

use crate::AppState;

/// WebSocket upgrade handler using OpenClaw-style protocol
///
/// This handler upgrades the connection and delegates to our
/// chat-streaming WebSocket handler with challenge/auth/subscribe
/// lifecycle.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
) -> axum::response::Response {
    info!(
        "WebSocket upgrade request from {} (user: {:?})",
        addr,
        user.as_ref().map(|u| &u.user_id)
    );

    // Use our new OpenClaw-style WebSocket handler for chat streaming
    let connections = state.ws_connections.clone();
    ws.on_upgrade(move |socket| async move {
        crate::websocket::handler::handle_connection(socket, connections).await;
    })
}

/// WebSocket status endpoint
///
/// Returns information about the WebSocket server status
pub async fn ws_status_handler(
    State(state): State<Arc<AppState>>,
) -> axum::Json<serde_json::Value> {
    let status = match &state.ws_manager {
        Some(ws_manager) => {
            let connections = ws_manager.connection_count();
            let all_connections = ws_manager.list_connections().await;

            serde_json::json!({
                "enabled": true,
                "connections": connections,
                "connection_details": all_connections.iter().map(|c| {
                    serde_json::json!({
                        "id": c.id,
                        "addr": c.addr.to_string(),
                        "user_id": c.user_id,
                        "channels": c.channels,
                        "connected_at": c.connected_at.elapsed().as_secs(),
                    })
                }).collect::<Vec<_>>(),
            })
        }
        None => {
            serde_json::json!({
                "enabled": false,
                "connections": 0,
            })
        }
    };

    axum::Json(status)
}

/// Broadcast message to all connected WebSocket clients
///
/// Admin-only endpoint for system-wide notifications
pub async fn ws_broadcast_handler(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    axum::Json(payload): axum::Json<serde_json::Value>,
) -> Result<axum::Json<serde_json::Value>, gateway::error::GatewayError> {
    // Only admins can broadcast
    if !user.is_admin() {
        return Err(gateway::error::GatewayError::forbidden(
            "Only admins can broadcast WebSocket messages",
        ));
    }

    match &state.ws_manager {
        Some(ws_manager) => {
            // Broadcast to all connections
            let message = gateway::websocket::WsMessage::Notification {
                title: payload
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("System Notification")
                    .to_string(),
                body: payload
                    .get("body")
                    .cloned()
                    .unwrap_or(serde_json::json!({})),
            };

            ws_manager.broadcast_all(message).await.map_err(|e| {
                gateway::error::GatewayError::internal(format!("Broadcast failed: {}", e))
            })?;

            Ok(axum::Json(serde_json::json!({
                "success": true,
                "message": "Broadcast sent"
            })))
        }
        None => Err(gateway::error::GatewayError::service_unavailable(
            "websocket",
            "WebSocket is not enabled",
        )),
    }
}
