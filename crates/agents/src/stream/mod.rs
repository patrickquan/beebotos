//! Stream processing module
//!
//! Provides types and utilities for streaming agent responses,
//! message sanitization, and stream assembly.
//!
//! References OpenClaw:
//! - src/infra/agent-events.ts (AgentEventPayload, AgentEventStream)
//! - src/shared/chat-envelope.ts (stripEnvelope, stripMessageIdHints)
//! - src/tui/tui-stream-assembler.ts (TuiStreamAssembler)

pub mod abort;
pub mod assembler;
pub mod sanitize;
pub mod types;

pub use abort::*;
pub use assembler::*;
pub use sanitize::*;
pub use types::*;
