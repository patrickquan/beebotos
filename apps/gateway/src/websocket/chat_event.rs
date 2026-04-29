//! Chat event handler — emitChatDelta, flushBufferedChatDeltaIfNeeded,
//! emitChatFinal
//!
//! Reference: openclaw-main/src/gateway/server-chat.ts::createAgentEventHandler

use std::collections::HashMap;
use std::sync::Arc;

use beebotos_agents::stream::sanitize_for_display;
use tokio::sync::Mutex;
use tracing::info;

use crate::websocket::broadcast::BroadcastOptions;
use crate::websocket::state::ChatRunState;
use crate::websocket::types::{AssistantMessage, ChatEvent, ChatEventState, ContentBlock};

/// Delta throttle constant (OpenClaw: 150ms)
const DELTA_THROTTLE_MS: u64 = 150;

/// Chat event handler
pub struct ChatEventHandler {
    run_state: Arc<Mutex<ChatRunState>>,
    agent_run_seq: Arc<Mutex<HashMap<String, u64>>>,
}

impl ChatEventHandler {
    pub fn new(run_state: Arc<Mutex<ChatRunState>>) -> Self {
        Self {
            run_state,
            agent_run_seq: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Emit chat delta (OpenClaw: emitChatDelta)
    pub async fn emit_chat_delta(
        &self,
        session_key: &str,
        client_run_id: &str,
        _source_run_id: &str,
        seq: u64,
        text: &str,
        delta: Option<&str>,
        broadcast_fn: &(dyn Fn(String, serde_json::Value, Option<BroadcastOptions>) + Send + Sync),
    ) {
        let cleaned_text = sanitize_for_display(text);
        let cleaned_delta = delta.map(sanitize_for_display);

        let mut state = self.run_state.lock().await;

        let previous_raw = state
            .raw_buffers
            .get(client_run_id)
            .cloned()
            .unwrap_or_default();
        let merged_raw =
            resolve_merged_assistant_text(&previous_raw, &cleaned_text, cleaned_delta.as_deref());

        if merged_raw.is_empty() {
            info!("[CHAT-EVENT] emit_chat_delta: merged_raw is empty, skipping");
            return;
        }

        state
            .raw_buffers
            .insert(client_run_id.to_string(), merged_raw.clone());
        state
            .buffers
            .insert(client_run_id.to_string(), merged_raw.clone());

        let now = current_timestamp_ms();
        let last = state.delta_sent_at.get(client_run_id).copied().unwrap_or(0);
        if now - last < DELTA_THROTTLE_MS {
            info!("[CHAT-EVENT] emit_chat_delta: throttled ({}ms < {}ms)", now - last, DELTA_THROTTLE_MS);
            return;
        }

        state.delta_sent_at.insert(client_run_id.to_string(), now);
        state
            .delta_last_broadcast_len
            .insert(client_run_id.to_string(), merged_raw.len());

        let payload = serde_json::to_value(&ChatEvent {
            run_id: client_run_id.to_string(),
            session_key: session_key.to_string(),
            seq,
            state: ChatEventState::Delta,
            message: Some(AssistantMessage {
                role: "assistant".to_string(),
                content: vec![ContentBlock::Text { text: merged_raw.clone() }],
                timestamp: now,
                stop_reason: None,
                api: None,
                provider: None,
                model: None,
                usage: None,
            }),
            error_message: None,
            error_kind: None,
            usage: None,
            stop_reason: None,
        })
        .unwrap_or_default();

        info!(
            "[CHAT-EVENT] Broadcasting delta for run_id={}, seq={}, text_len={}",
            client_run_id,
            seq,
            merged_raw.len()
        );
        broadcast_fn(
            "chat".to_string(),
            payload,
            Some(BroadcastOptions { drop_if_slow: true }),
        );
    }

    /// Flush buffered chat delta before tool events (OpenClaw:
    /// flushBufferedChatDeltaIfNeeded)
    pub async fn flush_buffered_chat_delta(
        &self,
        session_key: &str,
        client_run_id: &str,
        _source_run_id: &str,
        seq: u64,
        broadcast_fn: &(dyn Fn(String, serde_json::Value, Option<BroadcastOptions>) + Send + Sync),
    ) {
        let mut state = self.run_state.lock().await;

        let text = state
            .buffers
            .get(client_run_id)
            .cloned()
            .unwrap_or_default();
        let text = text.trim();

        if text.is_empty() {
            return;
        }

        let last_broadcast_len = state
            .delta_last_broadcast_len
            .get(client_run_id)
            .copied()
            .unwrap_or(0);
        if text.len() <= last_broadcast_len {
            return;
        }

        let now = current_timestamp_ms();
        let payload = serde_json::to_value(&ChatEvent {
            run_id: client_run_id.to_string(),
            session_key: session_key.to_string(),
            seq,
            state: ChatEventState::Delta,
            message: Some(AssistantMessage {
                role: "assistant".to_string(),
                content: vec![ContentBlock::Text {
                    text: text.to_string(),
                }],
                timestamp: now,
                stop_reason: None,
                api: None,
                provider: None,
                model: None,
                usage: None,
            }),
            error_message: None,
            error_kind: None,
            usage: None,
            stop_reason: None,
        })
        .unwrap_or_default();

        broadcast_fn(
            "chat".to_string(),
            payload,
            Some(BroadcastOptions { drop_if_slow: true }),
        );
        state
            .delta_last_broadcast_len
            .insert(client_run_id.to_string(), text.len());
        state.delta_sent_at.insert(client_run_id.to_string(), now);
    }

    /// Emit chat final (OpenClaw: emitChatFinal)
    pub async fn emit_chat_final(
        &self,
        session_key: &str,
        client_run_id: &str,
        _source_run_id: &str,
        seq: u64,
        job_state: &str,
        error: Option<&str>,
        stop_reason: Option<&str>,
        _error_kind: Option<&str>,
        broadcast_fn: &(dyn Fn(String, serde_json::Value, Option<BroadcastOptions>) + Send + Sync),
    ) {
        let mut state = self.run_state.lock().await;

        let text = state
            .buffers
            .get(client_run_id)
            .cloned()
            .unwrap_or_default();

        state.delta_last_broadcast_len.remove(client_run_id);
        state.raw_buffers.remove(client_run_id);
        state.buffers.remove(client_run_id);
        state.delta_sent_at.remove(client_run_id);

        let (event_state, error_message) = if job_state == "done" {
            (ChatEventState::Final, None)
        } else {
            (ChatEventState::Error, error.map(|e| e.to_string()))
        };

        let has_message = !text.is_empty() && error_message.is_none();
        info!(
            "[CHAT-EVENT] emit_chat_final: run_id={} job_state={} text_len={} has_message={}",
            client_run_id, job_state, text.len(), has_message
        );

        let payload = serde_json::to_value(&ChatEvent {
            run_id: client_run_id.to_string(),
            session_key: session_key.to_string(),
            seq,
            state: event_state,
            message: if has_message {
                Some(AssistantMessage {
                    role: "assistant".to_string(),
                    content: vec![ContentBlock::Text { text }],
                    timestamp: current_timestamp_ms(),
                    stop_reason: stop_reason.map(|s| s.to_string()),
                    api: None,
                    provider: None,
                    model: None,
                    usage: None,
                })
            } else {
                None
            },
            error_message,
            error_kind: None,
            usage: None,
            stop_reason: stop_reason.map(|s| s.to_string()),
        })
        .unwrap_or_default();

        broadcast_fn("chat".to_string(), payload, None);
    }
}

/// Merge assistant text with deduplication (OpenClaw:
/// resolveMergedAssistantText)
fn resolve_merged_assistant_text(
    previous_text: &str,
    next_text: &str,
    next_delta: Option<&str>,
) -> String {
    if !next_text.is_empty() && !previous_text.is_empty() {
        if next_text.starts_with(previous_text) {
            return next_text.to_string();
        }
        if previous_text.starts_with(next_text) && next_delta.is_none() {
            return previous_text.to_string();
        }
    }

    if let Some(delta) = next_delta {
        append_unique_suffix(previous_text, delta)
    } else if !next_text.is_empty() {
        next_text.to_string()
    } else {
        previous_text.to_string()
    }
}

/// Append suffix avoiding overlap (OpenClaw: appendUniqueSuffix)
fn append_unique_suffix(base: &str, suffix: &str) -> String {
    if suffix.is_empty() {
        return base.to_string();
    }
    if base.is_empty() {
        return suffix.to_string();
    }
    if base.ends_with(suffix) {
        return base.to_string();
    }

    let max_overlap = base.len().min(suffix.len());
    for overlap in (1..=max_overlap).rev() {
        if base.ends_with(&suffix[..overlap]) {
            return format!("{}{}", base, &suffix[overlap..]);
        }
    }

    format!("{}{}", base, suffix)
}

fn current_timestamp_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
