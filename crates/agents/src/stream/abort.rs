//! Chat abort controller — manage abort signals per run
//!
//! Reference: openclaw-main/src/gateway/chat-abort.ts

use std::collections::HashMap;

/// Abort kind (OpenClaw: ChatAbortControllerEntry["kind"])
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AbortKind {
    ChatSend,
    Agent,
}

/// Abort controller entry (OpenClaw: ChatAbortControllerEntry)
#[derive(Debug, Clone)]
pub struct AbortControllerEntry {
    pub tx: tokio::sync::broadcast::Sender<()>,
    pub session_id: String,
    pub session_key: String,
    pub started_at_ms: u64,
    pub expires_at_ms: u64,
    pub owner_conn_id: Option<String>,
    pub kind: AbortKind,
}

/// Chat abort controller (OpenClaw: chatAbortControllers Map)
#[derive(Debug, Default)]
pub struct ChatAbortController {
    controllers: HashMap<String, AbortControllerEntry>,
}

/// Abort result
#[derive(Debug)]
pub struct AbortResult {
    pub aborted: bool,
}

impl ChatAbortController {
    const DEFAULT_GRACE_MS: u64 = 60_000;
    const MIN_ABORT_MS: u64 = 2 * 60_000;
    const MAX_ABORT_MS: u64 = 24 * 60 * 60 * 1000;

    pub fn new() -> Self {
        Self {
            controllers: HashMap::new(),
        }
    }

    /// Register an abort controller for a run
    /// Reference: openclaw-main/src/gateway/chat-abort.ts::registerChatAbortController
    pub fn register(
        &mut self,
        run_id: &str,
        session_id: &str,
        session_key: &str,
        timeout_ms: u64,
        owner_conn_id: Option<String>,
        kind: AbortKind,
    ) -> bool {
        if self.controllers.contains_key(run_id) {
            return false;
        }

        let now = current_timestamp_ms();
        let expires_at = Self::resolve_expires_at(now, timeout_ms);
        let (tx, _) = tokio::sync::broadcast::channel(1);

        self.controllers.insert(
            run_id.to_string(),
            AbortControllerEntry {
                tx,
                session_id: session_id.to_string(),
                session_key: session_key.to_string(),
                started_at_ms: now,
                expires_at_ms: expires_at,
                owner_conn_id,
                kind,
            },
        );

        true
    }

    /// Abort a specific run
    /// Reference: openclaw-main/src/gateway/chat-abort.ts::abortChatRunById
    pub fn abort(
        &mut self,
        run_id: &str,
        session_key: &str,
    ) -> AbortResult {
        let Some(entry) = self.controllers.get(run_id) else {
            return AbortResult { aborted: false };
        };

        if entry.session_key != session_key {
            return AbortResult { aborted: false };
        }

        let _ = entry.tx.send(());
        self.controllers.remove(run_id);

        AbortResult { aborted: true }
    }

    /// Abort all runs for a session
    /// Reference: openclaw-main/src/gateway/chat-abort.ts::abortChatRunsForSessionKey
    pub fn abort_by_session(
        &mut self,
        session_key: &str,
    ) -> Vec<String> {
        let run_ids: Vec<String> = self
            .controllers
            .iter()
            .filter(|(_, entry)| entry.session_key == session_key)
            .map(|(id, _)| id.clone())
            .collect();

        for run_id in &run_ids {
            if let Some(entry) = self.controllers.get(run_id) {
                let _ = entry.tx.send(());
            }
        }

        self.controllers
            .retain(|_, entry| entry.session_key != session_key);

        run_ids
    }

    /// Check if a run has been aborted
    pub fn is_aborted(&self, run_id: &str) -> bool {
        !self.controllers.contains_key(run_id)
    }

    /// Get a receiver for abort signals
    pub fn get_receiver(
        &self,
        run_id: &str,
    ) -> Option<tokio::sync::broadcast::Receiver<()>> {
        self.controllers.get(run_id).map(|e| e.tx.subscribe())
    }

    /// Clean up expired controllers
    pub fn sweep_expired(&mut self) {
        let now = current_timestamp_ms();
        self.controllers
            .retain(|_, entry| entry.expires_at_ms > now);
    }

    fn resolve_expires_at(now: u64, timeout_ms: u64) -> u64 {
        let bounded_timeout = timeout_ms;
        let target = now + bounded_timeout + Self::DEFAULT_GRACE_MS;
        let min = now + Self::MIN_ABORT_MS;
        let max = now + Self::MAX_ABORT_MS;
        target.clamp(min, max)
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
    fn test_register_and_abort() {
        let mut controller = ChatAbortController::new();
        assert!(controller.register("run_1", "sess_1", "user:1", 30_000, None, AbortKind::ChatSend));
        assert!(!controller.is_aborted("run_1"));

        let result = controller.abort("run_1", "user:1");
        assert!(result.aborted);
        assert!(controller.is_aborted("run_1"));
    }

    #[test]
    fn test_abort_wrong_session() {
        let mut controller = ChatAbortController::new();
        controller.register("run_1", "sess_1", "user:1", 30_000, None, AbortKind::ChatSend);

        let result = controller.abort("run_1", "user:2");
        assert!(!result.aborted);
    }

    #[test]
    fn test_abort_by_session() {
        let mut controller = ChatAbortController::new();
        controller.register("run_1", "sess_1", "user:1", 30_000, None, AbortKind::ChatSend);
        controller.register("run_2", "sess_1", "user:1", 30_000, None, AbortKind::ChatSend);
        controller.register("run_3", "sess_2", "user:2", 30_000, None, AbortKind::ChatSend);

        let aborted = controller.abort_by_session("user:1");
        assert_eq!(aborted.len(), 2);
        assert!(controller.is_aborted("run_1"));
        assert!(controller.is_aborted("run_2"));
        assert!(!controller.is_aborted("run_3"));
    }
}
