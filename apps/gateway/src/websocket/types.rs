//! WebSocket types — ChatEvent, ContentBlock, and related structures
//!
//! Reference: openclaw-main/src/gateway/protocol/schema/logs-chat.ts

use serde::{Deserialize, Serialize};

/// WebSocket top-level message envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub event: String,
    pub payload: serde_json::Value,
}

/// Chat event state (OpenClaw: ChatEventSchema.state)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChatEventState {
    Delta,
    Final,
    Aborted,
    Error,
}

/// Chat error kind (OpenClaw: ChatEventSchema.errorKind)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChatErrorKind {
    Refusal,
    Timeout,
    RateLimit,
    ContextLength,
    Unknown,
}

/// Token usage (OpenClaw: Usage)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    #[serde(rename = "cache_read", skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<u64>,
    #[serde(rename = "cache_write", skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<u64>,
    #[serde(rename = "total_tokens")]
    pub total_tokens: u64,
}

/// Content block (OpenClaw: AssistantMessage.content)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
    },
    ImageUrl {
        #[serde(rename = "image_url")]
        image_url: ImageUrlContent,
    },
    File {
        name: String,
        source: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrlContent {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Assistant message (OpenClaw: AssistantMessage)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub role: String,
    pub content: Vec<ContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
    pub timestamp: u64,
}

/// Chat event (OpenClaw: ChatEventSchema)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatEvent {
    pub run_id: String,
    pub session_key: String,
    pub seq: u64,
    pub state: ChatEventState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<AssistantMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<ChatErrorKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
}

/// Tool event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEventData {
    pub phase: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_result: Option<serde_json::Value>,
}

/// Connect challenge payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectChallengePayload {
    pub nonce: String,
    pub ts: u64,
}

/// Auth request payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthRequestPayload {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub token: String,
}

/// Subscribe request payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribePayload {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub channel: String,
    pub session_key: String,
}
