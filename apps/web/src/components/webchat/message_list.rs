//! 消息列表组件（全手动 DOM 操作，完全绕过 Leptos 响应式通知）
//!
//! 核心策略：
//! - 所有消息和流式内容都通过 setInterval 轮询 + 直接 DOM 操作
//! - 不使用 For 组件，不调用 RwSignal::set()，避免 RefCell 冲突
//! - 流式内容也在 #messages-container 内，跟随消息一起滚动

use std::sync::{Arc, Mutex};

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::state::StreamSegment;
use crate::webchat::{ChatMessage, MessageRole};

/// 简易 Markdown 转 HTML（仅处理常用语法）
fn markdown_to_html(md: &str) -> String {
    let mut html = String::new();
    let mut in_code_block = false;
    #[allow(unused_assignments)]
    let mut code_lang = String::new();

    for line in md.lines() {
        if line.starts_with("```") {
            if in_code_block {
                html.push_str("</code></pre>\n");
                in_code_block = false;
            } else {
                code_lang = line.trim_start_matches('`').trim().to_string();
                html.push_str(&format!(
                    "<pre><code class=\"language-{}\">",
                    html_escape(&code_lang)
                ));
                in_code_block = true;
            }
            continue;
        }
        if in_code_block {
            html.push_str(&html_escape(line));
            html.push('\n');
            continue;
        }

        if line.starts_with("### ") {
            html.push_str(&format!("<h3>{}</h3>\n", html_escape(&line[4..])));
        } else if line.starts_with("## ") {
            html.push_str(&format!("<h2>{}</h2>\n", html_escape(&line[3..])));
        } else if line.starts_with("# ") {
            html.push_str(&format!("<h1>{}</h1>\n", html_escape(&line[2..])));
        } else if line.starts_with("- ") || line.starts_with("* ") {
            html.push_str(&format!("<li>{}</li>\n", inline_markdown(&line[2..])));
        } else if line.trim().is_empty() {
            html.push('\n');
        } else {
            html.push_str(&format!("<p>{}</p>\n", inline_markdown(line)));
        }
    }
    if in_code_block {
        html.push_str("</code></pre>\n");
    }
    html
}

/// 内联 Markdown（加粗、行内代码、链接）
fn inline_markdown(text: &str) -> String {
    let mut result = html_escape(text);
    result = replace_between(&result, '`', "<code>", "</code>");
    result = replace_between_pairs(&result, "**", "<strong>", "</strong>");
    result
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn replace_between(s: &str, marker: char, open: &str, close: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == marker {
            result.push_str(open);
            while let Some(&next) = chars.peek() {
                if next == marker {
                    chars.next();
                    break;
                }
                result.push(chars.next().unwrap());
            }
            result.push_str(close);
        } else {
            result.push(c);
        }
    }
    result
}

fn replace_between_pairs(s: &str, marker: &str, open: &str, close: &str) -> String {
    let mut result = String::new();
    let mut remaining = s;
    while let Some(pos) = remaining.find(marker) {
        result.push_str(&remaining[..pos]);
        remaining = &remaining[pos + marker.len()..];
        if let Some(end) = remaining.find(marker) {
            result.push_str(open);
            result.push_str(&remaining[..end]);
            result.push_str(close);
            remaining = &remaining[end + marker.len()..];
        } else {
            result.push_str(marker);
        }
    }
    result.push_str(remaining);
    result
}

/// 构建单条消息的 HTML
fn build_message_html(msg: &ChatMessage) -> String {
    let is_user = matches!(msg.role, MessageRole::User);
    let role_class = if is_user { "user" } else { "assistant" };
    let avatar = if is_user { "👤" } else { "🤖" };
    let content = if is_user {
        html_escape(&msg.content)
    } else {
        markdown_to_html(&msg.content)
    };
    let time = format_timestamp(&msg.timestamp);

    format!(
        r#"<div class="message {role_class}">
            <div class="message-avatar">{avatar}</div>
            <div class="message-content-wrapper">
                <div class="message-content markdown-message">{content}</div>
                <div class="message-meta"><span class="message-time">{time}</span></div>
            </div>
        </div>"#
    )
}

/// 构建流式消息 HTML（带闪烁光标）
fn build_streaming_html(content: &str) -> String {
    let escaped = html_escape(content);
    format!(
        r#"<div class="message assistant streaming" id="streaming-message">
            <div class="message-avatar">🤖</div>
            <div class="message-content-wrapper">
                <div class="message-content">{escaped}<span class="streaming-cursor">▋</span></div>
            </div>
        </div>"#
    )
}

/// 构建"思考中"指示器 HTML
fn build_reading_indicator_html() -> String {
    r#"<div class="message assistant" id="streaming-message">
        <div class="message-avatar">🤖</div>
        <div class="message-content-wrapper">
            <div class="reading-indicator">
                <div class="reading-indicator__dots"><span></span><span></span><span></span></div>
                <span class="reading-indicator__label">思考中</span>
            </div>
        </div>
    </div>"#
    .to_string()
}

/// 检查用户是否在底部附近（距离底部 < 150px）
fn is_near_bottom() -> bool {
    if let Some(mc) = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .query_selector("#messages-container")
        .ok()
        .flatten()
        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let scroll_top = mc.scroll_top() as f64;
        let scroll_height = mc.scroll_height() as f64;
        let client_height = mc.client_height() as f64;
        (scroll_height - scroll_top - client_height) < 150.0
    } else {
        false
    }
}

/// 滚动容器到底部（仅在用户已在底部附近时）
fn scroll_to_bottom() {
    if !is_near_bottom() {
        return;
    }
    if let Some(mc) = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .query_selector("#messages-container")
        .ok()
        .flatten()
        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
    {
        mc.set_scroll_top(mc.scroll_height());
    }
}

/// 消息列表组件
#[component]
pub fn MessageList(
    message_data: Arc<Mutex<Vec<ChatMessage>>>,
    #[allow(unused_variables)]
    #[prop(optional)]
    message_version: RwSignal<u64>,
    #[prop(optional)] is_streaming: Option<Signal<bool>>,
    #[prop(optional)] stream_segments: Option<Signal<Vec<StreamSegment>>>,
    #[prop(optional)] stream_buffer: Option<Signal<String>>,
    #[prop(optional)] on_scroll: Option<Box<dyn Fn(web_sys::Event)>>,
) -> impl IntoView {
    // 已渲染消息计数（非信号，避免触发响应式通知）
    let rendered_count: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));

    // 初始渲染：在 Effect 中加载已有消息到 DOM（仅执行一次）
    let init_data = message_data.clone();
    let init_count = rendered_count.clone();
    Effect::new(move |_| {
        let container = web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .query_selector("#messages-container")
            .ok()
            .flatten();
        if let Some(container) = container {
            let msgs = init_data.lock().unwrap();
            let mut count = init_count.lock().unwrap();
            if msgs.len() > *count {
                web_sys::console::log_1(
                    &format!("[MessageList] initial render: {} messages", msgs.len()).into(),
                );
                for msg in msgs.iter().skip(*count) {
                    let html = build_message_html(msg);
                    container.insert_adjacent_html("beforeend", &html).unwrap();
                }
                *count = msgs.len();
                scroll_to_bottom();
            }
        }
    });

    // 主轮询：新消息 + 流式内容，全部手动 DOM 操作
    let poll_data = message_data.clone();
    let poll_count = rendered_count.clone();
    // 克隆信号到闭包中（Signal 是 Copy 的）
    let poll_is_streaming = is_streaming;
    let poll_segments = stream_segments;
    let poll_buffer = stream_buffer;
    // 流式内容缓存（避免无变化时重复更新 DOM 导致闪烁）
    let last_streaming_content: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let last_streaming_state: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
    // 用户是否主动往上滚动（禁用自动跟随）
    let user_scrolled_up: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
    let last_scroll_top: Arc<Mutex<f64>> = Arc::new(Mutex::new(-1.0));

    let window = web_sys::window().unwrap();
    let poll_closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
        let container = web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .query_selector("#messages-container")
            .ok()
            .flatten();
        let Some(container) = container else { return };

        // 0. 追踪用户滚动方向（检测是否主动往上滚）
        {
            let el = container.clone().dyn_into::<web_sys::HtmlElement>().ok();
            if let Some(el) = el {
                let scroll_top = el.scroll_top() as f64;
                let scroll_height = el.scroll_height() as f64;
                let client_height = el.client_height() as f64;
                let distance_from_bottom = scroll_height - scroll_top - client_height;
                let mut last_top = last_scroll_top.lock().unwrap();

                if *last_top < 0.0 {
                    // 首次运行，仅记录位置
                    *last_top = scroll_top;
                } else if scroll_top < *last_top - 2.0 {
                    // 用户往上滚了（超过 2px 容差）
                    *user_scrolled_up.lock().unwrap() = true;
                } else if distance_from_bottom < 50.0 {
                    // 用户回到接近底部（50px 内）
                    *user_scrolled_up.lock().unwrap() = false;
                }
                *last_top = scroll_top;
            }
        }

        // 通用 DOM 查询（提前，供后续多处使用）
        let doc = web_sys::window().unwrap().document().unwrap();

        // 1. 追加新消息
        let data_len = poll_data.lock().unwrap().len();
        let local_len = *poll_count.lock().unwrap();
        if data_len > local_len {
            let msgs = poll_data.lock().unwrap();
            let mut count = poll_count.lock().unwrap();
            for msg in msgs.iter().skip(*count) {
                let html = build_message_html(msg);
                container.insert_adjacent_html("beforeend", &html).unwrap();
            }
            web_sys::console::log_1(
                &format!(
                    "[poll] rendered {} -> {}, appended {} messages",
                    local_len,
                    msgs.len(),
                    msgs.len() - local_len
                )
                .into(),
            );
            *count = msgs.len();
            // 新消息到达时，移除"思考中"提示（非流式 Final 的情况）
            if let Some(thinking) = doc.query_selector("#thinking-message").ok().flatten() {
                thinking.remove();
            }
            // 仅在用户未主动上滚时强制滚动到底部
            if !*user_scrolled_up.lock().unwrap() {
                if let Some(el) = container.clone().dyn_into::<web_sys::HtmlElement>().ok() {
                    el.set_scroll_top(el.scroll_height());
                }
            }
        }

        // 2. 流式内容（手动 DOM，与消息在同一个容器内，一起滚动）
        let streaming = poll_is_streaming
            .as_ref()
            .map(|s| s.get_untracked())
            .unwrap_or(false);

        let existing = doc.query_selector("#streaming-message").ok().flatten();

        // 仅在流式内容实际到达时移除"思考中"提示（保留到有真实内容）
        if streaming {
            if let Some(thinking) = doc.query_selector("#thinking-message").ok().flatten() {
                thinking.remove();
            }
        }

        // 读取流式内容（使用 get_untracked 避免响应式追踪）
        let mut content = String::new();
        if streaming {
            if let Some(segs) = poll_segments.as_ref() {
                for seg in segs.get_untracked() {
                    content.push_str(&seg.text);
                }
            }
            if let Some(buf) = poll_buffer.as_ref() {
                content.push_str(&buf.get_untracked());
            }
        }

        let mut last_state = last_streaming_state.lock().unwrap();
        let mut last_content = last_streaming_content.lock().unwrap();

        if streaming {
            let content_changed = content != *last_content;
            let state_changed = streaming != *last_state;

            // 读取用户滚动状态
            let scrolled_up = *user_scrolled_up.lock().unwrap();

            if state_changed {
                // 状态变化（非流式 → 流式）：创建流式元素
                let html = if content.is_empty() {
                    build_reading_indicator_html()
                } else {
                    build_streaming_html(&content)
                };
                container.insert_adjacent_html("beforeend", &html).unwrap();
                // 仅在用户未主动上滚时自动跟随
                if !scrolled_up {
                    scroll_to_bottom();
                }
            } else if content_changed {
                // 内容变化：只更新文本部分，不重建整个元素
                if let Some(el) = existing.as_ref() {
                    if let Some(text_el) = el
                        .query_selector(".message-content")
                        .ok()
                        .flatten()
                    {
                        if content.is_empty() {
                            text_el.set_inner_html(
                                "<div class=\"reading-indicator\">\
                                 <div class=\"reading-indicator__dots\"><span></span><span></span><span></span></div>\
                                 <span class=\"reading-indicator__label\">思考中</span>\
                                 </div>",
                            );
                        } else {
                            text_el.set_inner_html(
                                &format!("{}<span class=\"streaming-cursor\">▋</span>",
                                    html_escape(&content)),
                            );
                        }
                        // 仅在用户未主动上滚时自动跟随
                        if !scrolled_up {
                            scroll_to_bottom();
                        }
                    }
                }
            }
            // 内容没变时什么都不做，避免闪烁
            *last_content = content;
        } else if *last_state {
            // 流式结束：移除流式元素
            if let Some(existing) = existing {
                existing.remove();
            }
            last_content.clear();
        }
        *last_state = streaming;
    }) as Box<dyn Fn()>);
    window
        .set_interval_with_callback_and_timeout_and_arguments_0(
            poll_closure.as_ref().unchecked_ref(),
            50,
        )
        .unwrap();
    poll_closure.forget();

    view! {
        <div class="message-list">
            // 所有内容（消息 + 流式）都在这个容器内，统一滚动
            <div id="messages-container" on:scroll=move |ev| {
                if let Some(ref handler) = on_scroll {
                    handler(ev);
                }
            }></div>
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
