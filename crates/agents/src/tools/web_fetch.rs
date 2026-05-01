//! Tool: web_fetch
//!
//! Fetches web page content (text extraction).

use crate::llm::types::{FunctionDefinition, Tool};
use crate::llm::ToolHandler;

/// Web fetch tool
pub struct WebFetchTool;

impl WebFetchTool {
    /// Create new web fetch tool
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ToolHandler for WebFetchTool {
    fn definition(&self) -> Tool {
        Tool {
            r#type: "function".to_string(),
            function: FunctionDefinition {
                name: "web_fetch".to_string(),
                description: Some("获取网页内容".to_string()),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "网页 URL"
                        }
                    },
                    "required": ["url"]
                }),
            },
        }
    }

    async fn execute(&self, arguments: &str) -> Result<String, String> {
        let args: serde_json::Value =
            serde_json::from_str(arguments).map_err(|e| format!("Invalid arguments: {}", e))?;

        let url = args["url"].as_str().ok_or("Missing url")?;

        let response = reqwest::get(url)
            .await
            .map_err(|e| format!("Failed to fetch URL: {}", e))?;

        let text = response
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        // Simple HTML to text extraction
        let plain_text = html_to_text(&text);

        // Truncate if too long (max 32KB)
        const MAX_LEN: usize = 32 * 1024;
        if plain_text.len() > MAX_LEN {
            let end = plain_text[..MAX_LEN]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(MAX_LEN);
            Ok(format!(
                "{}\n\n[... {} more characters truncated ...]",
                &plain_text[..end],
                plain_text.len() - end
            ))
        } else {
            Ok(plain_text)
        }
    }
}

/// Very basic HTML tag stripping
/// 使用字节级遍历，完全避免 UTF-8 切片越界问题
fn html_to_text(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script = false;
    let mut prev_char = ' ';
    let bytes = html.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // 安全获取当前位置的字符及其字节长度
        let ch = match html[i..].chars().next() {
            Some(c) => c,
            None => break,
        };
        let ch_len = ch.len_utf8();

        if in_script {
            if ch == '<' && i + 9 <= bytes.len() && &bytes[i..i + 9] == b"</script>" {
                in_script = false;
            }
            i += ch_len;
            continue;
        }

        if ch == '<' {
            in_tag = true;
            // 字节级比较，避免任何字符串切片
            if i + 7 <= bytes.len() {
                let tag = &bytes[i..i + 7];
                if tag.eq_ignore_ascii_case(b"<script") {
                    in_script = true;
                }
            }
            i += ch_len;
            continue;
        }

        if ch == '>' && in_tag {
            in_tag = false;
            i += ch_len;
            continue;
        }

        if !in_tag {
            if ch.is_whitespace() {
                if prev_char != ' ' {
                    result.push(' ');
                    prev_char = ' ';
                }
            } else {
                result.push(ch);
                prev_char = ch;
            }
        }
        i += ch_len;
    }

    result.trim().to_string()
}
