//! Message sanitization utilities
//!
//! Provides functions to strip envelope wrappers and message ID hints
//! from chat messages.
//!
//! Reference: openclaw-main/src/shared/chat-envelope.ts

/// Strip envelope markers from a message string
pub fn strip_envelope(input: &str) -> String {
    input
        .trim_start_matches("[[envelope]]")
        .trim_end_matches("[[/envelope]]")
        .trim()
        .to_string()
}

/// Strip message ID hints from a message string
pub fn strip_message_id_hints(input: &str) -> String {
    let re = regex::Regex::new(r"\[msg-id:[^\]]+\]").unwrap();
    re.replace_all(input, "").trim().to_string()
}

/// Sanitize a message by applying all strip operations
pub fn sanitize_message(input: &str) -> String {
    let stripped = strip_envelope(input);
    strip_message_id_hints(&stripped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_envelope() {
        let input = "[[envelope]]Hello world[[/envelope]]";
        assert_eq!(strip_envelope(input), "Hello world");
    }

    #[test]
    fn test_strip_message_id_hints() {
        let input = "Hello [msg-id:abc123] world";
        assert_eq!(strip_message_id_hints(input), "Hello world");
    }

    #[test]
    fn test_sanitize_message() {
        let input = "[[envelope]]Hello [msg-id:abc123] world[[/envelope]]";
        assert_eq!(sanitize_message(input), "Hello world");
    }
}
