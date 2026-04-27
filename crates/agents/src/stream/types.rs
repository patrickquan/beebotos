//! Stream types — Agent event payload and related structures
//!
//! Reference: openclaw-main/src/infra/agent-events.ts

use serde::{Deserialize, Serialize};

/// Agent event stream type (OpenClaw: AgentEventStream)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentEventStream {
    Lifecycle,
    Tool,
    Assistant,
    Error,
    Item,
    Plan,
    Approval,
    CommandOutput,
    Patch,
    Compaction,
    Thinking,
}

/// Agent event payload (OpenClaw: AgentEventPayload)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEventPayload {
    pub run_id: String,
    pub seq: u64,
    pub stream: AgentEventStream,
    pub ts: u64,
    pub data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
}

/// Agent item event phase (OpenClaw: AgentItemEventPhase)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentItemEventPhase {
    #[default]
    Start,
    Update,
    End,
}

/// Agent item event kind (OpenClaw: AgentItemEventKind)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentItemEventKind {
    #[default]
    Tool,
    Command,
    Patch,
    Search,
    Analysis,
}

/// Agent item event status (OpenClaw: AgentItemEventStatus)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentItemEventStatus {
    #[default]
    Running,
    Completed,
    Failed,
    Blocked,
}

/// Agent item event data (OpenClaw: AgentItemEventData)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentItemEventData {
    pub item_id: String,
    pub phase: AgentItemEventPhase,
    pub kind: AgentItemEventKind,
    pub title: String,
    pub status: AgentItemEventStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_text: Option<String>,
}

impl AgentEventPayload {
    pub fn new(
        run_id: impl Into<String>,
        stream: AgentEventStream,
        data: serde_json::Value,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            seq: 0,
            stream,
            ts: current_timestamp_ms(),
            data,
            session_key: None,
        }
    }

    pub fn with_session_key(mut self, key: impl Into<String>) -> Self {
        self.session_key = Some(key.into());
        self
    }

    pub fn with_seq(mut self, seq: u64) -> Self {
        self.seq = seq;
        self
    }
}

fn current_timestamp_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
