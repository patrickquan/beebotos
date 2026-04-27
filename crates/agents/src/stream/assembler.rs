//! Stream assembler
//!
//! Assembles streaming agent events into complete messages.
//!
//! Reference: openclaw-main/src/tui/tui-stream-assembler.ts

use super::types::AgentEventPayload;

/// Stream assembler for agent events
#[derive(Debug, Default)]
pub struct StreamAssembler {
    events: Vec<AgentEventPayload>,
}

impl StreamAssembler {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn push(&mut self, event: AgentEventPayload) {
        self.events.push(event);
    }

    pub fn events(&self) -> &[AgentEventPayload] {
        &self.events
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }
}
