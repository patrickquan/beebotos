//! WebSocket module for real-time chat streaming
//!
//! Provides WebSocket connection handling, chat event broadcasting,
//! and session subscription management.

pub mod broadcast;
pub mod chat_event;
pub mod connection;
pub mod handler;
pub mod state;
pub mod types;

pub use types::*;
