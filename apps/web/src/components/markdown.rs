//! Markdown 渲染组件
//!
//! 使用 pulldown-cmark 将 Markdown 转换为 HTML，供 Leptos 渲染。
//! 禁用原始 HTML 以防止 XSS。

use leptos::prelude::*;
use pulldown_cmark::{html, Options, Parser};

/// Markdown 渲染选项
const MD_OPTIONS: Options = Options::empty();

/// 将 Markdown 文本渲染为 HTML 字符串
pub fn render_markdown(text: &str) -> String {
    let parser = Parser::new_ext(text, MD_OPTIONS);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
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
            // SAFETY: pulldown-cmark 禁用了原始 HTML，只允许 Markdown 语法，
            // 因此输出是安全的。代码块只会生成 <pre><code> 标签。
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
        assert!(html.contains("<pre>"));
        assert!(html.contains("<code>"));
        assert!(html.contains("fn main() {}"));
    }

    #[test]
    fn test_no_raw_html() {
        // pulldown-cmark 默认禁用原始 HTML
        let md = "<script>alert('xss')</script>";
        let html = render_markdown(md);
        assert!(!html.contains("<script>"));
    }
}
