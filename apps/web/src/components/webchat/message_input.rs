//! 消息输入组件

use leptos::prelude::*;

use crate::utils::event_target_value;

/// 消息输入组件
#[component]
pub fn MessageInput(
    #[prop(optional)] placeholder: Option<String>,
    #[prop(optional, into)] disabled: Option<Signal<bool>>,
    #[prop(optional)] on_submit: Option<Box<dyn Fn(String)>>,
    #[prop(optional)] on_typing: Option<Box<dyn Fn(String)>>,
) -> impl IntoView {
    let placeholder = placeholder.unwrap_or_else(|| "Type a message...".to_string());
    let disabled = disabled.unwrap_or_else(|| Signal::derive(move || false));
    let textarea_ref = NodeRef::<leptos::html::Textarea>::new();

    // 用 Rc 包装回调，使其可在多个闭包间共享
    let on_submit_rc = on_submit.map(|cb| std::rc::Rc::new(cb) as std::rc::Rc<dyn Fn(String)>);
    let on_typing_rc = on_typing.map(|cb| std::rc::Rc::new(cb) as std::rc::Rc<dyn Fn(String)>);

    let on_keydown = {
        let on_submit = on_submit_rc.clone();
        let textarea_ref = textarea_ref.clone();
        move |ev: leptos::ev::KeyboardEvent| {
            if ev.key() == "Enter" && !ev.shift_key() {
                ev.prevent_default();
                if let Some(textarea) = textarea_ref.get() {
                    let content = textarea.value();
                    if !content.trim().is_empty() {
                        if let Some(ref cb) = on_submit {
                            cb(content.clone());
                        }
                        textarea.set_value("");
                    }
                }
            }
        }
    };

    let on_input = {
        let on_typing = on_typing_rc.clone();
        move |ev: leptos::ev::Event| {
            let content = event_target_value(&ev);
            if let Some(ref cb) = on_typing {
                cb(content);
            }
        }
    };

    let on_click_submit = {
        let on_submit = on_submit_rc;
        let textarea_ref = textarea_ref.clone();
        move |_| {
            let _ = web_sys::console::log_1(&"[MessageInput] send button clicked".into());
            if let Some(textarea) = textarea_ref.get() {
                let content = textarea.value();
                let _ = web_sys::console::log_1(
                    &format!("[MessageInput] content='{}', empty={}", content, content.trim().is_empty()).into(),
                );
                if !content.trim().is_empty() {
                    if let Some(ref cb) = on_submit {
                        let _ = web_sys::console::log_1(
                            &"[MessageInput] calling on_submit callback".into(),
                        );
                        cb(content.clone());
                    } else {
                        let _ = web_sys::console::warn_1(
                            &"[MessageInput] on_submit is None".into(),
                        );
                    }
                    textarea.set_value("");
                } else {
                    let _ = web_sys::console::warn_1(
                        &"[MessageInput] content is empty, not calling callback".into(),
                    );
                }
            } else {
                let _ = web_sys::console::warn_1(
                    &"[MessageInput] textarea ref not available".into(),
                );
            }
        }
    };

    view! {
        <div class="message-input-container">
            <div class="message-input-wrapper">
                <button class="btn btn-icon attachment-btn" disabled=move || disabled.get()>
                    "📎"
                </button>

                <textarea
                    class="message-textarea"
                    placeholder=placeholder
                    disabled=move || disabled.get()
                    node_ref=textarea_ref
                    on:input=on_input
                    on:keydown=on_keydown
                    rows=1
                />

                <button
                    class="btn btn-primary send-btn"
                    disabled=move || disabled.get()
                    on:click=on_click_submit
                >
                    "➤"
                </button>
            </div>

            <div class="input-hints">
                <span>"Press Enter to send, Shift+Enter for new line"</span>
                <span>"Use /btw for side question"</span>
            </div>
        </div>
    }
}
