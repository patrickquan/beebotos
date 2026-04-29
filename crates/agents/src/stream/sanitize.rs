//! Message sanitization — strip envelopes, directives, and internal metadata
//!
//! References OpenClaw:
//! - src/shared/chat-envelope.ts (stripEnvelope, stripMessageIdHints)
//! - src/gateway/chat-sanitize.ts (stripEnvelopeFromMessage)
//! - src/auto-reply/tokens.ts (SILENT_REPLY_TOKEN)

use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref ENVELOPE_PREFIX: Regex = Regex::new(r"^\[([^\]]+)\]\s*").unwrap();
    static ref MESSAGE_ID_LINE: Regex = Regex::new(r"^\s*\[message_id:\s*[^\]]+\]\s*$").unwrap();
    static ref DIRECTIVE_TAG: Regex =
        Regex::new(r"\[\[[a-zA-Z_][a-zA-Z0-9_]*(?:\s+[^\]]+)?\]\]").unwrap();
    static ref TOOL_CALL_XML: Regex =
        Regex::new(r"</?(?:tool_call|function_call|tool_calls|function_calls)[^>]*>").unwrap();
    static ref SILENT_TOKEN: Regex = Regex::new(r"(?i)\bNO_REPLY\b|\bno_reply\b").unwrap();
    static ref INTERNAL_RUNTIME_PREFIX: Regex =
        Regex::new(r"^\[internal_runtime_context:[^\]]*\]\s*").unwrap();
    static ref INBOUND_METADATA_PREFIX: Regex =
        Regex::new(r"^\[inbound_metadata:[^\]]*\]\s*").unwrap();
    static ref TIMESTAMP_ISO: Regex = Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}Z\b").unwrap();
    static ref TIMESTAMP_SPACE: Regex = Regex::new(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}\b").unwrap();
}

const ENVELOPE_CHANNELS: &[&str] = &[
    "WebChat",
    "WhatsApp",
    "Telegram",
    "Signal",
    "Slack",
    "Discord",
    "Google Chat",
    "iMessage",
    "Teams",
    "Matrix",
    "Zalo",
    "Zalo Personal",
    "BlueBubbles",
];

/// Strip envelope prefix like "[WebChat 2024-01-15 10:30] text"
/// Reference: openclaw-main/src/shared/chat-envelope.ts::stripEnvelope
pub fn strip_envelope(text: &str) -> &str {
    let Some(captures) = ENVELOPE_PREFIX.captures(text) else {
        return text;
    };
    let header = captures.get(1).map(|m| m.as_str()).unwrap_or("");
    if !looks_like_envelope_header(header) {
        return text;
    }
    &text[captures.get(0).unwrap().end()..]
}

fn looks_like_envelope_header(header: &str) -> bool {
    if TIMESTAMP_ISO.is_match(header) {
        return true;
    }
    if TIMESTAMP_SPACE.is_match(header) {
        return true;
    }
    ENVELOPE_CHANNELS
        .iter()
        .any(|&channel| header.starts_with(channel))
}

/// Strip message_id hint lines
/// Reference: openclaw-main/src/shared/chat-envelope.ts::stripMessageIdHints
pub fn strip_message_id_hints(text: &str) -> String {
    if !text.to_lowercase().contains("[message_id:") {
        return text.to_string();
    }
    text.lines()
        .filter(|line| !MESSAGE_ID_LINE.is_match(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Strip inline directive tags like [[reply_to_123]], [[audio_as_voice]]
/// Reference:
/// openclaw-main/src/utils/directive-tags.
/// ts::stripInlineDirectiveTagsForDisplay
pub fn strip_inline_directive_tags(text: &str) -> String {
    DIRECTIVE_TAG.replace_all(text, "").to_string()
}

/// Strip tool call XML wrappers
pub fn strip_tool_call_xml(text: &str) -> String {
    TOOL_CALL_XML.replace_all(text, "").to_string()
}

/// Strip silent tokens (NO_REPLY / no_reply)
/// Reference: openclaw-main/src/auto-reply/tokens.ts
pub fn strip_silent_tokens(text: &str) -> String {
    SILENT_TOKEN.replace_all(text, "").to_string()
}

/// Strip internal runtime context prefix
/// Reference: openclaw-main/src/agents/internal-runtime-context.ts
pub fn strip_internal_runtime_context(text: &str) -> String {
    INTERNAL_RUNTIME_PREFIX.replace(text, "").to_string()
}

/// Strip inbound metadata prefix
/// Reference: openclaw-main/src/auto-reply/reply/strip-inbound-meta.ts
pub fn strip_inbound_metadata(text: &str) -> String {
    INBOUND_METADATA_PREFIX.replace(text, "").to_string()
}

/// Full sanitization pipeline for display text
/// Reference:
/// openclaw-main/src/gateway/chat-sanitize.ts::stripEnvelopeFromMessage
pub fn sanitize_for_display(text: &str) -> String {
    let mut result = text.to_string();
    result = strip_internal_runtime_context(&result);
    result = strip_inbound_metadata(&result);
    result = strip_envelope(&result).to_string();
    result = strip_message_id_hints(&result);
    result = strip_inline_directive_tags(&result);
    result = strip_tool_call_xml(&result);
    result = strip_silent_tokens(&result);
    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_envelope_with_channel() {
        let text = "[WebChat] Hello world";
        assert_eq!(strip_envelope(text), "Hello world");
    }

    #[test]
    fn test_strip_envelope_with_timestamp() {
        let text = "[2024-01-15T10:30Z] Hello world";
        assert_eq!(strip_envelope(text), "Hello world");
    }

    #[test]
    fn test_strip_envelope_no_match() {
        let text = "Hello world";
        assert_eq!(strip_envelope(text), "Hello world");
    }

    #[test]
    fn test_strip_message_id_hints() {
        let text = "Hello\n[message_id: abc123]\nWorld";
        assert_eq!(strip_message_id_hints(text), "Hello\nWorld");
    }

    #[test]
    fn test_strip_inline_directive_tags() {
        let text = "Hello [[reply_to_123]] world [[audio_as_voice]]";
        assert_eq!(strip_inline_directive_tags(text), "Hello  world ");
    }

    #[test]
    fn test_strip_silent_tokens() {
        let text = "Hello NO_REPLY world no_reply end";
        assert_eq!(strip_silent_tokens(text), "Hello  world  end");
    }

    #[test]
    fn test_sanitize_for_display() {
        let text = "[WebChat] [[reply_to_123]] Hello NO_REPLY world";
        assert_eq!(sanitize_for_display(text), "Hello  world");
    }
}
