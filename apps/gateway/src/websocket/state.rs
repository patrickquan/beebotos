//! WebSocket state management — ChatRunRegistry, ChatRunState, subscriber
//! registries
//!
//! Reference: openclaw-main/src/gateway/server-chat.ts

use std::collections::{HashMap, HashSet};

/// Chat run entry (OpenClaw: ChatRunEntry)
#[derive(Debug, Clone)]
pub struct ChatRunEntry {
    pub session_key: String,
    pub client_run_id: String,
}

/// Chat run registry (OpenClaw: ChatRunRegistry)
#[derive(Debug, Default)]
pub struct ChatRunRegistry {
    sessions: HashMap<String, Vec<ChatRunEntry>>,
}

impl ChatRunRegistry {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    pub fn add(&mut self, session_id: &str, entry: ChatRunEntry) {
        self.sessions
            .entry(session_id.to_string())
            .or_default()
            .push(entry);
    }

    pub fn peek(&self, session_id: &str) -> Option<&ChatRunEntry> {
        self.sessions.get(session_id)?.first()
    }

    pub fn shift(&mut self, session_id: &str) -> Option<ChatRunEntry> {
        let queue = self.sessions.get_mut(session_id)?;
        if queue.is_empty() {
            return None;
        }
        let entry = queue.remove(0);
        if queue.is_empty() {
            self.sessions.remove(session_id);
        }
        Some(entry)
    }

    pub fn remove(
        &mut self,
        session_id: &str,
        client_run_id: &str,
        session_key: Option<&str>,
    ) -> Option<ChatRunEntry> {
        let queue = self.sessions.get_mut(session_id)?;
        let idx = queue.iter().position(|entry| {
            entry.client_run_id == client_run_id
                && session_key.map_or(true, |sk| entry.session_key == sk)
        })?;
        let entry = queue.remove(idx);
        if queue.is_empty() {
            self.sessions.remove(session_id);
        }
        Some(entry)
    }

    pub fn clear(&mut self) {
        self.sessions.clear();
    }
}

/// Chat run state (OpenClaw: ChatRunState)
#[derive(Debug, Default)]
pub struct ChatRunState {
    pub registry: ChatRunRegistry,
    pub raw_buffers: HashMap<String, String>,
    pub buffers: HashMap<String, String>,
    pub delta_sent_at: HashMap<String, u64>,
    pub delta_last_broadcast_len: HashMap<String, usize>,
    pub aborted_runs: HashMap<String, u64>,
}

impl ChatRunState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.registry.clear();
        self.raw_buffers.clear();
        self.buffers.clear();
        self.delta_sent_at.clear();
        self.delta_last_broadcast_len.clear();
        self.aborted_runs.clear();
    }

    pub fn clear_buffered_state(&mut self, client_run_id: &str) {
        self.raw_buffers.remove(client_run_id);
        self.buffers.remove(client_run_id);
        self.delta_sent_at.remove(client_run_id);
        self.delta_last_broadcast_len.remove(client_run_id);
    }
}

/// Tool event recipient registry (OpenClaw: ToolEventRecipientRegistry)
#[derive(Debug, Default)]
pub struct ToolEventRecipientRegistry {
    recipients: HashMap<String, ToolRecipientEntry>,
}

#[derive(Debug)]
struct ToolRecipientEntry {
    conn_ids: HashSet<String>,
    updated_at: u64,
    finalized_at: Option<u64>,
}

const TOOL_EVENT_RECIPIENT_TTL_MS: u64 = 10 * 60 * 1000;
const TOOL_EVENT_RECIPIENT_FINAL_GRACE_MS: u64 = 30 * 1000;

impl ToolEventRecipientRegistry {
    pub fn new() -> Self {
        Self {
            recipients: HashMap::new(),
        }
    }

    pub fn add(&mut self, run_id: &str, conn_id: &str) {
        if run_id.is_empty() || conn_id.is_empty() {
            return;
        }
        let now = current_timestamp_ms();
        let entry = self
            .recipients
            .entry(run_id.to_string())
            .or_insert_with(|| ToolRecipientEntry {
                conn_ids: HashSet::new(),
                updated_at: now,
                finalized_at: None,
            });
        entry.conn_ids.insert(conn_id.to_string());
        entry.updated_at = now;
        self.prune();
    }

    pub fn get(&mut self, run_id: &str) -> Option<&HashSet<String>> {
        let entry = self.recipients.get(run_id)?;
        Some(&entry.conn_ids)
    }

    pub fn mark_final(&mut self, run_id: &str) {
        if let Some(entry) = self.recipients.get_mut(run_id) {
            entry.finalized_at = Some(current_timestamp_ms());
        }
        self.prune();
    }

    fn prune(&mut self) {
        let now = current_timestamp_ms();
        self.recipients.retain(|_, entry| {
            let cutoff = entry
                .finalized_at
                .map(|t| t + TOOL_EVENT_RECIPIENT_FINAL_GRACE_MS)
                .unwrap_or(entry.updated_at + TOOL_EVENT_RECIPIENT_TTL_MS);
            now < cutoff
        });
    }
}

/// Session message subscriber registry (OpenClaw:
/// SessionMessageSubscriberRegistry)
#[derive(Debug, Default)]
pub struct SessionMessageSubscriberRegistry {
    session_to_conn: HashMap<String, HashSet<String>>,
    conn_to_sessions: HashMap<String, HashSet<String>>,
}

impl SessionMessageSubscriberRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe(&mut self, conn_id: &str, session_key: &str) {
        let conn_id = conn_id.trim();
        let session_key = session_key.trim();
        if conn_id.is_empty() || session_key.is_empty() {
            return;
        }

        self.session_to_conn
            .entry(session_key.to_string())
            .or_default()
            .insert(conn_id.to_string());
        self.conn_to_sessions
            .entry(conn_id.to_string())
            .or_default()
            .insert(session_key.to_string());
    }

    pub fn unsubscribe(&mut self, conn_id: &str, session_key: &str) {
        let conn_id = conn_id.trim();
        let session_key = session_key.trim();

        if let Some(conns) = self.session_to_conn.get_mut(session_key) {
            conns.remove(conn_id);
            if conns.is_empty() {
                self.session_to_conn.remove(session_key);
            }
        }

        if let Some(sessions) = self.conn_to_sessions.get_mut(conn_id) {
            sessions.remove(session_key);
            if sessions.is_empty() {
                self.conn_to_sessions.remove(conn_id);
            }
        }
    }

    pub fn unsubscribe_all(&mut self, conn_id: &str) {
        let conn_id = conn_id.trim();
        let sessions: Vec<String> = self
            .conn_to_sessions
            .get(conn_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect();

        for session_key in sessions {
            self.unsubscribe(conn_id, &session_key);
        }
    }

    pub fn get(&self, session_key: &str) -> HashSet<String> {
        self.session_to_conn
            .get(session_key.trim())
            .cloned()
            .unwrap_or_default()
    }
}

fn current_timestamp_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_run_registry() {
        let mut reg = ChatRunRegistry::new();
        reg.add(
            "sess_1",
            ChatRunEntry {
                session_key: "user:1".to_string(),
                client_run_id: "run_1".to_string(),
            },
        );

        assert!(reg.peek("sess_1").is_some());
        assert_eq!(reg.shift("sess_1").unwrap().client_run_id, "run_1");
        assert!(reg.peek("sess_1").is_none());
    }

    #[test]
    fn test_session_subscriber_registry() {
        let mut reg = SessionMessageSubscriberRegistry::new();
        reg.subscribe("conn_1", "user:1");
        reg.subscribe("conn_2", "user:1");
        reg.subscribe("conn_1", "user:2");

        let subs = reg.get("user:1");
        assert_eq!(subs.len(), 2);

        reg.unsubscribe("conn_1", "user:1");
        let subs = reg.get("user:1");
        assert_eq!(subs.len(), 1);
    }
}
