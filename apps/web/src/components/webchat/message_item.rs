//! 消息项组件（OpenClaw 流式传输 + Markdown 渲染）

use leptos::prelude::*;

use crate::components::markdown::MarkdownRenderer;
use crate::webchat::ChatMessage;

/// 消息项组件
#[component]
pub fn MessageItem(
    message: ChatMessage,
    #[prop(optional)] is_streaming: Option<bool>,
) -> impl IntoView {
    let is_streaming = is_streaming.unwrap_or(false);
    let is_user = matches!(message.role, crate::webchat::MessageRole::User);

    let class = format!(
        "message {} {}",
        if is_user { "user" } else { "assistant" },
        if is_streaming { "streaming" } else { "" }
    );

    // 用户消息用纯文本，assistant 消息用 Markdown 渲染
    // 流式消息也用纯文本（避免不完整的 Markdown 导致渲染问题）
    let content_view = if is_user || is_streaming {
        view! {
            <div class="message-content">{message.content.clone()}</div>
        }
        .into_any()
    } else {
        let content_signal = Signal::derive(move || message.content.clone());
        view! {
            <div class="message-content markdown-message">
                <MarkdownRenderer content=content_signal />
            </div>
        }
        .into_any()
    };

    view! {
        <div class=class>
            <div class="message-avatar">
                {if is_user {
                    "👤"
                } else {
                    "🤖"
                }}
            </div>
            <div class="message-content-wrapper">
                {content_view}
                <div class="message-meta">
                    <span class="message-time">{format_timestamp(&message.timestamp)}</span>
                    {if let Some(usage) = &message.token_usage {
                        view! {
                            <span class="token-usage">{usage.format()}</span>
                        }.into_any()
                    } else {
                        view! { <div /> }.into_any()
                    }}
                </div>
            </div>
        </div>
    }
}

/// 流式消息项（显示正在生成的内容）
#[component]
pub fn StreamingMessageItem(
    #[prop(into)]
    content: Signal<String>,
) -> impl IntoView {
    view! {
        <div class="message assistant streaming">
            <div class="message-avatar">{"🤖"}</div>
            <div class="message-content-wrapper">
                <div class="message-content">
                    {move || content.get()}
                    <span class="streaming-cursor">"▋"</span>
                </div>
            </div>
        </div>
    }
}

fn format_timestamp(timestamp: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(timestamp) {
        let local = dt.with_timezone(&chrono::Local);
        local.format("%H:%M").to_string()
    } else {
        timestamp.to_string()
    }
}
