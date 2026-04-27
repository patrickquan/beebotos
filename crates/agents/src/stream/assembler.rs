//! Stream assembler — compose thinking + content, handle boundary drop
//!
//! Reference: openclaw-main/src/tui/tui-stream-assembler.ts (TuiStreamAssembler)

use serde_json::Value;
use std::collections::HashMap;

/// Per-run stream state (OpenClaw: RunStreamState)
#[derive(Debug, Clone, Default)]
pub struct RunStreamState {
    pub thinking_text: String,
    pub content_text: String,
    pub content_blocks: Vec<String>,
    pub saw_non_text_content_blocks: bool,
    pub display_text: String,
}

/// Stream assembler (OpenClaw: TuiStreamAssembler)
#[derive(Debug, Default)]
pub struct StreamAssembler {
    runs: HashMap<String, RunStreamState>,
}

impl StreamAssembler {
    pub fn new() -> Self {
        Self {
            runs: HashMap::new(),
        }
    }

    /// Ingest a delta message, return new display text if changed
    /// Reference: openclaw-main/src/tui/tui-stream-assembler.ts::ingestDelta
    pub fn ingest_delta(
        &mut self,
        run_id: &str,
        message: &Value,
        show_thinking: bool,
    ) -> Option<String> {
        let state = self.get_or_create_run(run_id);
        let previous_display = state.display_text.clone();

        let thinking_text = extract_thinking_from_message(message);
        let content_text = extract_content_from_message(message);
        let (text_blocks, saw_non_text) = extract_text_blocks_and_signals(message);

        if let Some(thinking) = thinking_text {
            state.thinking_text = thinking;
        }

        if let Some(content) = content_text {
            let next_blocks = if text_blocks.is_empty() {
                vec![content.clone()]
            } else {
                text_blocks
            };

            let should_keep = should_preserve_boundary_dropped_text(
                &state.content_blocks,
                &next_blocks,
                state.saw_non_text_content_blocks,
                saw_non_text,
            );

            if !should_keep {
                state.content_text = content;
                state.content_blocks = next_blocks;
            }
        }

        if saw_non_text {
            state.saw_non_text_content_blocks = true;
        }

        let display = compose_thinking_and_content(
            &state.thinking_text,
            &state.content_text,
            show_thinking,
        );

        state.display_text = display.clone();

        if display.is_empty() || display == previous_display {
            None
        } else {
            Some(display)
        }
    }

    /// Finalize a run, return the final display text
    /// Reference: openclaw-main/src/tui/tui-stream-assembler.ts::finalize
    pub fn finalize(
        &mut self,
        run_id: &str,
        message: &Value,
        show_thinking: bool,
        error_message: Option<&str>,
    ) -> String {
        let state = self.runs.remove(run_id).unwrap_or_default();
        let streamed_display = state.display_text;
        let streamed_blocks = state.content_blocks;
        let streamed_saw_non_text = state.saw_non_text_content_blocks;

        // Re-compute final state
        let mut temp = Self::new();
        temp.ingest_delta(run_id, message, show_thinking);
        let final_state = temp.runs.get(run_id).cloned().unwrap_or_default();
        let final_composed = final_state.display_text;

        let should_keep_streamed = streamed_saw_non_text
            && is_dropped_boundary_text_block_subset(&streamed_blocks,
                &final_state.content_blocks,
            );

        let final_text = if should_keep_streamed {
            streamed_display.clone()
        } else {
            final_composed
        };

        resolve_final_assistant_text(Some(&final_text), Some(&streamed_display), error_message)
    }

    /// Drop a run state
    pub fn drop_run(&mut self, run_id: &str) {
        self.runs.remove(run_id);
    }

    fn get_or_create_run(&mut self, run_id: &str) -> &mut RunStreamState {
        self.runs.entry(run_id.to_string()).or_default()
    }
}

/// Extract thinking blocks from message content
/// Reference: openclaw-main/src/tui/tui-formatters.ts::extractThinkingFromMessage
fn extract_thinking_from_message(message: &Value) -> Option<String> {
    let content = message.get("content")?;

    if let Some(text) = content.as_str() {
        return if text.is_empty() { None } else { Some(text.to_string()) };
    }

    let blocks = content.as_array()?;
    let mut parts = Vec::new();

    for block in blocks {
        if block.get("type")?.as_str()? == "thinking" {
            if let Some(text) = block.get("thinking").and_then(|v| v.as_str()) {
                parts.push(text.to_string());
            }
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

/// Extract text content blocks from message (excludes thinking)
/// Reference: openclaw-main/src/tui/tui-formatters.ts::extractContentFromMessage
fn extract_content_from_message(message: &Value) -> Option<String> {
    let content = message.get("content")?;

    if let Some(text) = content.as_str() {
        return if text.trim().is_empty() { None } else { Some(text.trim().to_string()) };
    }

    let blocks = content.as_array()?;
    let mut parts = Vec::new();

    for block in blocks {
        if block.get("type")?.as_str()? == "text" {
            if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                if !text.trim().is_empty() {
                    parts.push(text.trim().to_string());
                }
            }
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

/// Extract text blocks and detect non-text content
fn extract_text_blocks_and_signals(message: &Value) -> (Vec<String>, bool) {
    let Some(content) = message.get("content") else {
        return (vec![], false);
    };

    if let Some(text) = content.as_str() {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return (vec![], false);
        }
        return (vec![trimmed.to_string()], false);
    }

    let Some(blocks) = content.as_array() else {
        return (vec![], false);
    };

    let mut text_blocks = Vec::new();
    let mut saw_non_text = false;

    for block in blocks {
        let Some(block_type) = block.get("type").and_then(|v| v.as_str()) else {
            continue;
        };

        if block_type == "text" {
            if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    text_blocks.push(trimmed.to_string());
                }
            }
        } else if block_type != "thinking" {
            saw_non_text = true;
        }
    }

    (text_blocks, saw_non_text)
}

/// Check if final blocks are a prefix/suffix subset of streamed blocks
/// Reference: openclaw-main/src/tui/tui-stream-assembler.ts::isDroppedBoundaryTextBlockSubset
fn is_dropped_boundary_text_block_subset(streamed: &[String], final_blocks: &[String]) -> bool {
    if final_blocks.is_empty() || final_blocks.len() >= streamed.len() {
        return false;
    }

    // Check prefix match
    let prefix_matches = final_blocks
        .iter()
        .enumerate()
        .all(|(i, block)| streamed.get(i) == Some(block));
    if prefix_matches {
        return true;
    }

    // Check suffix match
    let suffix_start = streamed.len() - final_blocks.len();
    final_blocks
        .iter()
        .enumerate()
        .all(|(i, block)| streamed.get(suffix_start + i) == Some(block))
}

/// Determine if boundary dropped text should be preserved
fn should_preserve_boundary_dropped_text(
    streamed_blocks: &[String],
    final_blocks: &[String],
    streamed_saw_non_text: bool,
    incoming_saw_non_text: bool,
) -> bool {
    if !streamed_saw_non_text && !incoming_saw_non_text {
        return false;
    }
    is_dropped_boundary_text_block_subset(streamed_blocks, final_blocks)
}

/// Compose thinking + content text
/// Reference: openclaw-main/src/tui/tui-formatters.ts::composeThinkingAndContent
fn compose_thinking_and_content(thinking: &str, content: &str, show_thinking: bool) -> String {
    let mut parts = Vec::new();

    if show_thinking && !thinking.is_empty() {
        parts.push(format!("[thinking]\n{thinking}"));
    }

    if !content.is_empty() {
        parts.push(content.to_string());
    }

    parts.join("\n\n").trim().to_string()
}

/// Resolve final assistant text with fallback chain
/// Reference: openclaw-main/src/tui/tui-formatters.ts::resolveFinalAssistantText
fn resolve_final_assistant_text(
    final_text: Option<&str>,
    streamed_text: Option<&str>,
    error_message: Option<&str>,
) -> String {
    if let Some(text) = final_text {
        if !text.trim().is_empty() {
            return text.to_string();
        }
    }

    if let Some(text) = streamed_text {
        if !text.trim().is_empty() {
            return text.to_string();
        }
    }

    if let Some(err) = error_message {
        if !err.trim().is_empty() {
            return format!("Error: {err}");
        }
    }

    "(no output)".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_ingest_delta_simple_text() {
        let mut assembler = StreamAssembler::new();
        let msg = json!({
            "role": "assistant",
            "content": [{"type": "text", "text": "Hello world"}]
        });

        let result = assembler.ingest_delta("run_1", &msg, false);
        assert_eq!(result, Some("Hello world".to_string()));
    }

    #[test]
    fn test_ingest_delta_with_thinking() {
        let mut assembler = StreamAssembler::new();
        let msg = json!({
            "role": "assistant",
            "content": [
                {"type": "thinking", "thinking": "Let me think..."},
                {"type": "text", "text": "The answer is 42"}
            ]
        });

        let result = assembler.ingest_delta("run_1", &msg, true);
        let text = result.unwrap();
        assert!(text.contains("[thinking]"));
        assert!(text.contains("The answer is 42"));
    }

    #[test]
    fn test_boundary_drop_prefix() {
        let streamed = vec!["block1".to_string(), "block2".to_string(), "block3".to_string()];
        let final_blocks = vec!["block1".to_string(), "block2".to_string()];

        assert!(is_dropped_boundary_text_block_subset(&streamed, &final_blocks));
    }

    #[test]
    fn test_boundary_drop_suffix() {
        let streamed = vec!["block1".to_string(), "block2".to_string(), "block3".to_string()];
        let final_blocks = vec!["block2".to_string(), "block3".to_string()];

        assert!(is_dropped_boundary_text_block_subset(&streamed, &final_blocks));
    }

    #[test]
    fn test_ingest_delta_multiple_deltas() {
        let mut assembler = StreamAssembler::new();

        let delta1 = json!({
            "role": "assistant",
            "content": [{"type": "text", "text": "Hello"}]
        });
        let result1 = assembler.ingest_delta("run_1", &delta1, false);
        assert_eq!(result1, Some("Hello".to_string()));

        let delta2 = json!({
            "role": "assistant",
            "content": [{"type": "text", "text": "Hello world"}]
        });
        let result2 = assembler.ingest_delta("run_1", &delta2, false);
        assert_eq!(result2, Some("Hello world".to_string()));

        let delta3 = json!({
            "role": "assistant",
            "content": [{"type": "text", "text": "Hello world!"}]
        });
        let result3 = assembler.ingest_delta("run_1", &delta3, false);
        assert_eq!(result3, Some("Hello world!".to_string()));
    }

    #[test]
    fn test_finalize_with_boundary_drop() {
        let mut assembler = StreamAssembler::new();

        // Simulate streaming with a non-text block (e.g., image_url) that gets dropped in final
        let delta = json!({
            "role": "assistant",
            "content": [
                {"type": "text", "text": "Here's an image:"},
                {"type": "image_url", "image_url": {"url": "http://example.com/img.png"}},
                {"type": "text", "text": "And some text after."}
            ]
        });
        assembler.ingest_delta("run_1", &delta, false);

        // Final message has fewer blocks (image_url dropped by provider)
        let final_msg = json!({
            "role": "assistant",
            "content": [
                {"type": "text", "text": "Here's an image:"},
                {"type": "text", "text": "And some text after."}
            ]
        });
        let result = assembler.finalize("run_1", &final_msg, false, None);
        // Should keep streamed display since saw_non_text and boundary dropped
        assert!(result.contains("Here's an image:"));
        assert!(result.contains("And some text after."));
    }

    #[test]
    fn test_finalize_with_error() {
        let mut assembler = StreamAssembler::new();
        let delta = json!({
            "role": "assistant",
            "content": [{"type": "text", "text": "Partial output"}]
        });
        assembler.ingest_delta("run_1", &delta, false);

        let final_msg = json!({
            "role": "assistant",
            "content": []
        });
        let result = assembler.finalize("run_1", &final_msg, false, Some("Timeout"));
        assert_eq!(result, "Partial output");
    }
}
