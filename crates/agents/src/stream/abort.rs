//! Stream abort utilities
//!
//! Provides mechanisms for aborting active streams.

use std::sync::Arc;

use tokio::sync::Notify;

/// Abort handle for agent streams
#[derive(Debug, Clone)]
pub struct StreamAbortHandle {
    notify: Arc<Notify>,
}

impl StreamAbortHandle {
    pub fn new() -> Self {
        Self {
            notify: Arc::new(Notify::new()),
        }
    }

    pub fn abort(&self) {
        self.notify.notify_waiters();
    }

    pub async fn wait_for_abort(&self) {
        self.notify.notified().await;
    }
}

impl Default for StreamAbortHandle {
    fn default() -> Self {
        Self::new()
    }
}
