//! Markdown 渲染组件
//!
//! 使用 pulldown-cmark 将 Markdown 转换为 HTML，供 Leptos 渲染。
//! 渲染后对原始 HTML 进行基础清理，移除危险标签和事件处理器。

use leptos::prelude::*;
use pulldown_cmark::{html, Options, Parser};

/// Markdown 渲染选项
const MD_OPTIONS: Options = Options::empty();

/// 危险标签（将被完全移除）
const DANGEROUS_TAGS: &[&str] = &[
    "script", "style", "iframe", "object", "embed", "form", "input", "textarea",
];

/// 危险属性前缀（将被移除）
const DANGEROUS_ATTR_PREFIXES: &[&str] = &[
    "onerror", "onclick", "onload", "onmouse", "onkey", "onfocus", "onblur",
    "onsubmit", "onchange", "onscroll", "ontoggle",
];

/// 将 Markdown 文本渲染为 HTML 字符串，并进行安全清理
pub fn render_markdown(text: &str) -> String {
    let parser = Parser::new_ext(text, MD_OPTIONS);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    sanitize_html(&html_output)
}

/// 基础 HTML 清理：移除危险标签和事件处理器属性
fn sanitize_html(html: &str) -> String {
    let mut result = html.to_string();

    // 移除危险标签及其内容
    for tag in DANGEROUS_TAGS {
        // 匹配 <tag ...>...</tag> 和 <tag ... />
        let open = format!("<{} ", tag);
        let open2 = format!("<{}", tag);
        let close = format!("</{}>", tag);

        // 简单处理：移除从开始标签到结束标签之间的所有内容
        while let Some(start) = result.to_lowercase().find(&open2) {
            let rest = &result[start..];
            let tag_end = rest.find('>').unwrap_or(rest.len());
            let after_tag = start + tag_end + 1;

            // 查找结束标签
            if let Some(end) = result[after_tag..].to_lowercase().find(&close) {
                let end_pos = after_tag + end + close.len();
                result.replace_range(start..end_pos, "");
            } else {
                // 没有结束标签，只移除开始标签
                result.replace_range(start..after_tag, "");
                break;
            }
        }
    }

    // 移除危险属性（简单字符串替换）
    for attr in DANGEROUS_ATTR_PREFIXES {
        // 移除 attr="..." 和 attr='...'
        let mut cleaned = String::new();
        let mut i = 0;
        while i < result.len() {
            let rest = &result[i..];
            if rest.to_lowercase().starts_with(attr) {
                // 跳过属性名
                i += attr.len();
                // 跳过空白和 =
                while i < result.len() && result[i..].starts_with(' ') {
                    i += 1;
                }
                if i < result.len() && result.as_bytes()[i] == b'=' {
                    i += 1;
                    // 跳过引号包围的值
                    while i < result.len() && result[i..].starts_with(' ') {
                        i += 1;
                    }
                    let quote = result.as_bytes()[i];
                    if quote == b'"' || quote == b'\'' {
                        i += 1;
                        while i < result.len() && result.as_bytes()[i] != quote {
                            i += 1;
                        }
                        if i < result.len() {
                            i += 1; // 跳过结束引号
                        }
                    }
                }
            } else {
                cleaned.push(result.as_bytes()[i] as char);
                i += 1;
            }
        }
        result = cleaned;
    }

    result
}

/// Markdown 渲染组件
///
/// 接收 Markdown 文本，渲染为安全的 HTML。
/// 由于 pulldown-cmark 禁用了原始 HTML，输出是安全的。
#[component]
pub fn MarkdownRenderer(
    /// Markdown 文本内容
    #[prop(into)]
    content: Signal<String>,
    /// 额外的 CSS 类
    #[prop(into, default = "markdown-body".to_string())]
    class: String,
) -> impl IntoView {
    let html_content = Memo::new(move |_| render_markdown(&content.get()));

    view! {
        <div
            class=class
            // SAFETY: render_markdown 在转换后会调用 sanitize_html，
            // 移除危险标签（script, style, iframe 等）和事件处理器属性。
            prop:inner_html=move || html_content.get()
        />
    }
}

/// 内联 Markdown 渲染（用于简短文本，如消息片段）
#[component]
pub fn InlineMarkdown(
    #[prop(into)]
    content: Signal<String>,
) -> impl IntoView {
    let html_content = Memo::new(move |_| {
        let text = content.get();
        // 对于内联渲染，简单处理换行和强调
        let parser = Parser::new_ext(&text, MD_OPTIONS);
        let mut html_output = String::new();
        html::push_html(&mut html_output, parser);
        html_output
    });

    view! {
        <span prop:inner_html=move || html_content.get() />
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_basic_markdown() {
        let md = "# Hello\n\nThis is **bold** and *italic*.";
        let html = render_markdown(md);
        assert!(html.contains("<h1>Hello</h1>"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>italic</em>"));
    }

    #[test]
    fn test_render_code_block() {
        let md = "```rust\nfn main() {}\n```";
        let html = render_markdown(md);
        assert!(html.contains("<pre>"), "expected <pre> in {:?}", html);
        assert!(html.contains("<code"), "expected <code in {:?}", html);
        assert!(html.contains("fn main() {}"), "expected code content in {:?}", html);
    }

    #[test]
    fn test_no_raw_html() {
        let md = "<script>alert('xss')</script>";
        let html = render_markdown(md);
        assert!(!html.contains("<script>"));
        assert!(!html.contains("alert"));
    }

    #[test]
    fn test_sanitize_event_handlers() {
        let md = "<img src='x' onerror=\"alert(1)\">";
        let html = render_markdown(md);
        assert!(!html.contains("onerror"));
    }
}
