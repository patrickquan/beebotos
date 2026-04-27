//! WebChat 页面
//!
//! 提供聊天界面、会话管理、侧边提问等功能
//! 已接入 WebChat Channel：通过 WebSocket 接收 Agent 回复，通过 HTTP POST
//! 发送消息

use gloo_storage::{LocalStorage, Storage};
use leptos::prelude::*;
use leptos::view;
use leptos_meta::Title;

use crate::api::{create_client, create_webchat_service};
use crate::components::webchat::{
    MessageInput, MessageList, SessionList, SidePanel, UsagePanelComponent,
};
use crate::gateway::websocket::{WebSocketClient, WsConnectionStatus};
use crate::state::{use_auth_state, use_chat_ui_state, use_webchat_state, WebchatWsHandler};
use crate::webchat::{ChatMessage, MessageRole};

/// 获取或创建持久化的会话 ID（仅作本地缓存，后端为准）
fn get_stored_session_id() -> Option<String> {
    LocalStorage::get("beebotos_webchat_session_id").ok()
}

fn store_session_id(id: &str) {
    let _ = LocalStorage::set("beebotos_webchat_session_id", id);
}

/// WebChat 页面
#[component]
pub fn WebchatPage() -> impl IntoView {
    let chat_state = use_webchat_state();
    let ui_state = use_chat_ui_state();
    let auth_state = use_auth_state();

    // 组件挂载：从后端加载会话列表
    let chat_state_for_load = chat_state.clone();
    let auth_state_for_load = auth_state.clone();
    Effect::new(move |_| {
        let chat_state = chat_state_for_load.clone();
        let auth_state = auth_state_for_load.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let client = create_client();
            client.set_auth_token(auth_state.get_token());
            let service = create_webchat_service(client);
            match service.list_sessions().await {
                Ok(sessions) => {
                    let _ = web_sys::console::log_1(
                        &format!(
                            "[webchat] list_sessions returned {} sessions",
                            sessions.len()
                        )
                        .into(),
                    );
                    // 如果有本地缓存的会话 ID，尝试恢复选中
                    let stored = get_stored_session_id();
                    let has_stored = stored
                        .as_ref()
                        .map(|id| sessions.iter().any(|s| &s.id == id))
                        .unwrap_or(false);
                    let _ = web_sys::console::log_1(
                        &format!(
                            "[webchat] stored_session_id={:?}, has_stored={}",
                            stored, has_stored
                        )
                        .into(),
                    );

                    chat_state.sessions.set(sessions.clone());

                    if has_stored {
                        if let Some(id) = stored {
                            chat_state.current_session_id.set(Some(id.clone()));
                            // 加载该会话的消息
                            match service.get_messages(&id).await {
                                Ok(msgs) => {
                                    let _ = web_sys::console::log_1(
                                        &format!(
                                            "[webchat] loaded {} messages for session {}",
                                            msgs.len(),
                                            id
                                        )
                                        .into(),
                                    );
                                    chat_state.current_messages.set(msgs.clone());
                                    chat_state.message_cache.update(|cache| {
                                        cache.insert(id.clone(), msgs);
                                    });
                                }
                                Err(e) => {
                                    let _ = web_sys::console::error_1(
                                        &format!("[webchat] get_messages failed: {}", e).into(),
                                    );
                                    chat_state.set_error(Some(format!("加载消息失败: {}", e)));
                                }
                            }
                        }
                    } else if let Some(first) = sessions.first() {
                        let id = first.id.clone();
                        chat_state.current_session_id.set(Some(id.clone()));
                        store_session_id(&id);
                        match service.get_messages(&id).await {
                            Ok(msgs) => {
                                let _ = web_sys::console::log_1(
                                    &format!(
                                        "[webchat] loaded {} messages for session {}",
                                        msgs.len(),
                                        id
                                    )
                                    .into(),
                                );
                                chat_state.current_messages.set(msgs.clone());
                                chat_state.message_cache.update(|cache| {
                                    cache.insert(id.clone(), msgs);
                                });
                            }
                            Err(e) => {
                                let _ = web_sys::console::error_1(
                                    &format!("[webchat] get_messages failed: {}", e).into(),
                                );
                                chat_state.set_error(Some(format!("加载消息失败: {}", e)));
                            }
                        }
                    } else {
                        // 没有会话时自动创建一个
                        let _ = web_sys::console::log_1(&"[webchat] creating session".into());
                        match service.create_session("New Chat").await {
                            Ok(session) => {
                                let id = session.id.clone();
                                chat_state.sessions.update(|s| s.push(session));
                                chat_state.current_session_id.set(Some(id.clone()));
                                store_session_id(&id);
                            }
                            Err(e) => {
                                chat_state.set_error(Some(format!("创建会话失败: {}", e)));
                            }
                        }
                    }
                }
                Err(e) => {
                    chat_state.set_error(Some(format!("加载会话失败: {}", e)));
                }
            }
        });
    });

    // WebSocket 连接：使用 OpenClaw 风格协议
    let chat_state_for_ws = chat_state.clone();
    let auth_state_for_ws = auth_state.clone();

    // 创建 WebSocket 客户端
    let ws_client = if let Some(window) = web_sys::window() {
        let location = window.location();
        let protocol = location.protocol().unwrap_or_else(|_| "http:".to_string());
        let hostname = location
            .hostname()
            .unwrap_or_else(|_| "localhost".to_string());
        let port = location.port().unwrap_or_default();
        let ws_protocol = if protocol == "https:" { "wss" } else { "ws" };
        let ws_host = if port == "8090" {
            format!("{}:8000", hostname)
        } else if port.is_empty() {
            hostname
        } else {
            format!("{}:{}", hostname, port)
        };
        let ws_url = format!("{}://{}/ws", ws_protocol, ws_host);

        let ws = WebSocketClient::new(&ws_url);

        // 设置事件处理器
        let handler = WebchatWsHandler::new(chat_state_for_ws.clone());
        ws.set_handler(Box::new(handler));

        // 设置 token
        if let Some(token) = auth_state_for_ws.get_token() {
            ws.set_token(token);
        }

        // 连接
        if let Err(e) = ws.connect() {
            web_sys::console::error_1(&format!("[webchat] WebSocket connect failed: {}", e).into());
        } else {
            web_sys::console::log_1(
                &format!("[webchat] WebSocket connecting to {}", ws_url).into(),
            );
        }

        Some(ws)
    } else {
        None
    };

    // WebSocket 订阅：当连接成功且选中会话变化时订阅
    let ws_client_for_sub = ws_client.clone();
    Effect::new(move |_| {
        let status = chat_state.ws_status.get();
        let session_id = chat_state.current_session_id.get();

        if status == WsConnectionStatus::Connected {
            if let (Some(ws), Some(session_id)) = (ws_client_for_sub.as_ref(), session_id) {
                // 使用 session_id 作为 session_key 订阅
                let session_key = format!("user:{}", session_id);
                if let Err(e) = ws.subscribe(&session_key) {
                    web_sys::console::warn_1(&format!("[webchat] subscribe failed: {}", e).into());
                } else {
                    web_sys::console::log_1(
                        &format!("[webchat] subscribed to {}", session_key).into(),
                    );
                }
            }
        }
    });

    // 提前 clone auth_state，因为 Effect 会 move 它
    let auth_state_for_send = auth_state.clone();
    let auth_state_for_select = auth_state.clone();
    let auth_state_for_new = auth_state.clone();

    // 监听 token 变化并更新 WebSocket token
    let ws_client_for_token = ws_client.clone();
    Effect::new(move |_| {
        if let Some(token) = auth_state.get_token() {
            if let Some(ws) = ws_client_for_token.as_ref() {
                ws.set_token(token);
            }
        }
    });

    // 发送消息处理
    let chat_state_for_send = chat_state.clone();
    let handle_send = move |content: String| {
        if chat_state_for_send.is_sending.get() {
            return;
        }
        let session_id = chat_state_for_send.current_session_id.get();
        if session_id.is_none() {
            return;
        }
        let session_id = session_id.unwrap();

        // 本地添加用户消息
        let user_message = ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::User,
            content: content.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            attachments: vec![],
            metadata: Default::default(),
            token_usage: None,
        };
        chat_state_for_send.add_message(user_message);
        chat_state_for_send.is_sending.set(true);
        chat_state_for_send.set_error(None);

        // 异步发送到后端
        let chat_state_send = chat_state_for_send.clone();
        let auth_state_send = auth_state_for_send.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let client = create_client();
            client.set_auth_token(auth_state_send.get_token());
            let service = create_webchat_service(client);
            let user_id = auth_state_send
                .user
                .get()
                .as_ref()
                .map(|u| u.id.clone())
                .unwrap_or_default();
            match service.send_message(&session_id, &content, &user_id).await {
                Ok(_) => {
                    // HTTP 发送成功，但保持 is_sending=true 等待 WebSocket 回复
                    // 如果 WebSocket 长时间无响应，允许 30 秒后自动解除锁定
                    let chat_state_send = chat_state_send.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        gloo_timers::future::TimeoutFuture::new(30_000).await;
                        if chat_state_send.is_sending.get() {
                            chat_state_send.is_sending.set(false);
                        }
                    });
                }
                Err(e) => {
                    chat_state_send.set_error(Some(format!("Failed to send: {}", e)));
                    chat_state_send.is_sending.set(false);
                }
            }
        });
    };
    let on_submit: Box<dyn Fn(String)> = Box::new(handle_send);
    let on_submit = on_submit as Box<dyn Fn(String)>;

    // 切换会话
    let chat_state_select = chat_state.clone();
    let on_select_session: std::sync::Arc<dyn Fn(String) + Send + Sync> = std::sync::Arc::new({
        let chat_state = chat_state_select.clone();
        move |id: String| {
            let chat_state = chat_state.clone();
            let auth_state = auth_state_for_select.clone();
            store_session_id(&id);
            chat_state.current_session_id.set(Some(id.clone()));

            // Check cache first
            let cached = chat_state
                .message_cache
                .with(|cache| cache.get(&id).cloned());
            if let Some(msgs) = cached {
                chat_state.current_messages.set(msgs);
            } else {
                chat_state.current_messages.set(Vec::new());
                wasm_bindgen_futures::spawn_local(async move {
                    let client = create_client();
                    client.set_auth_token(auth_state.get_token());
                    let service = create_webchat_service(client);
                    match service.get_messages(&id).await {
                        Ok(msgs) => {
                            let _ = web_sys::console::log_1(
                                &format!(
                                    "[webchat] select_session loaded {} messages for session {}",
                                    msgs.len(),
                                    id
                                )
                                .into(),
                            );
                            chat_state.current_messages.set(msgs.clone());
                            chat_state.message_cache.update(|cache| {
                                cache.insert(id.clone(), msgs);
                            });
                        }
                        Err(e) => {
                            let _ = web_sys::console::error_1(
                                &format!("[webchat] select_session get_messages failed: {}", e)
                                    .into(),
                            );
                            chat_state.set_error(Some(format!("加载消息失败: {}", e)));
                        }
                    }
                });
            }
        }
    });

    // 新建会话
    let chat_state_new = chat_state.clone();
    let on_new_session: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new({
        let chat_state = chat_state_new.clone();
        move || {
            let chat_state = chat_state.clone();
            let auth_state = auth_state_for_new.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let client = create_client();
                client.set_auth_token(auth_state.get_token());
                let service = create_webchat_service(client);
                match service.create_session("New Chat").await {
                    Ok(session) => {
                        let id = session.id.clone();
                        chat_state.sessions.update(|s| s.push(session));
                        chat_state.current_session_id.set(Some(id.clone()));
                        chat_state.current_messages.set(Vec::new());
                        store_session_id(&id);
                    }
                    Err(e) => {
                        chat_state.set_error(Some(format!("创建会话失败: {}", e)));
                    }
                }
            });
        }
    });

    // 当前会话标题
    let current_title = Signal::derive({
        let chat_state = chat_state.clone();
        move || {
            let id = chat_state.current_session_id.get();
            chat_state
                .sessions
                .get()
                .into_iter()
                .find(|s| Some(s.id.clone()) == id)
                .map(|s| s.title)
                .unwrap_or_else(|| "Chat Session".to_string())
        }
    });

    let ui_state_sessions = ui_state.clone();
    let ui_state_usage_show = ui_state.clone();
    let ui_state_usage_toggle = ui_state.clone();
    let ui_state_side_show = ui_state.clone();
    let ui_state_side_toggle = ui_state.clone();
    let _ui_state_header = ui_state.clone();

    view! {
        <Title text="Chat - BeeBotOS" />
        <div class="webchat-page">
            <div class="webchat-container">
                {move || {
                    if ui_state_sessions.show_sessions_panel.get() {
                        view! {
                            <SessionsSidebar on_select=on_select_session.clone() on_new=on_new_session.clone() />
                        }.into_any()
                    } else {
                        view! { <div class="sidebar-collapsed" /> }.into_any()
                    }
                }}

                <main class="chat-main">
                    <ChatHeader title=current_title />
                    {move || view! {
                        <MessageList
                            messages=chat_state.current_messages.into()
                            is_streaming=chat_state.is_streaming.get()
                            streaming_content=chat_state.streaming_content.get()
                        />
                    }}
                    <MessageInput
                        placeholder="Type a message... (use /btw for side question)".to_string()
                        disabled=chat_state.is_sending.get()
                        on_submit=on_submit
                    />
                    {move || {
                        if let Some(ref error) = chat_state.error.get() {
                            view! {
                                <div class="chat-error">{error.clone()}</div>
                            }.into_any()
                        } else {
                            view! { <div /> }.into_any()
                        }
                    }}
                </main>

                <Show
                    when=move || ui_state_usage_show.show_usage_panel.get()
                    fallback=|| view! { <div class="side-panel-collapsed" /> }
                >
                    <UsagePanelComponent
                        usage=chat_state.usage.get()
                        is_open=true
                        on_close={
                            let ui_state_usage = ui_state_usage_toggle.clone();
                            Box::new(move || ui_state_usage.toggle_usage_panel())
                        }
                    />
                </Show>

                <Show
                    when=move || ui_state_side_show.show_side_panel.get()
                    fallback=|| view! { <div class="side-panel-collapsed" /> }
                >
                    <SidePanel
                        questions=chat_state.side_questions.get()
                        is_open=true
                        on_close={
                            let ui_state_side = ui_state_side_toggle.clone();
                            Box::new(move || ui_state_side.toggle_side_panel())
                        }
                        on_new_question={
                            let chat_state = chat_state.clone();
                            Box::new(move |q: String| {
                                let session_id = chat_state.current_session_id.get().unwrap_or_default();
                                chat_state.add_side_question(crate::webchat::SideQuestion::new(session_id, q));
                            })
                        }
                    />
                </Show>
            </div>
        </div>
    }
}

/// 会话侧边栏
#[component]
fn SessionsSidebar(
    #[prop(into)] on_select: std::sync::Arc<dyn Fn(String) + Send + Sync>,
    #[prop(into)] on_new: std::sync::Arc<dyn Fn() + Send + Sync>,
) -> impl IntoView {
    let ui_state = use_chat_ui_state();
    let chat_state = use_webchat_state();

    let on_new_chat = {
        let on_new = on_new.clone();
        move |_| {
            on_new();
        }
    };

    view! {
        <aside class="sessions-sidebar">
            <div class="sidebar-header">
                <h3>"Sessions"</h3>
                <button class="btn btn-icon" on:click=move |_| ui_state.toggle_sessions_panel()>
                    "◀"
                </button>
            </div>

            <div class="sidebar-actions">
                <button class="btn btn-primary btn-block" on:click=on_new_chat>
                    "+ New Chat"
                </button>
            </div>

            <div class="search-box">
                <input
                    type="text"
                    placeholder="Search sessions..."
                />
            </div>

            <SessionList
                sessions=chat_state.sessions.into()
                selected_id=Signal::derive(move || chat_state.current_session_id.get().unwrap_or_default())
                on_select=on_select.clone()
                on_new=on_new.clone()
            />
        </aside>
    }
}

/// 聊天头部
#[component]
fn ChatHeader(title: Signal<String>) -> impl IntoView {
    let ui_state = use_chat_ui_state();

    view! {
        <header class="chat-header">
            <div class="header-left">
                <h2>{move || title.get()}</h2>
            </div>

            <div class="header-actions">
                <button class="btn btn-icon" title="New Chat" on:click={
                    let ui_state = ui_state.clone();
                    move |_| {
                        ui_state.toggle_sessions_panel();
                    }
                }>
                    "+"
                </button>
                <button class="btn btn-icon" title="Usage" on:click={
                    let ui_state = ui_state.clone();
                    move |_| ui_state.toggle_usage_panel()
                }>
                    "📊"
                </button>
                <button class="btn btn-icon" title="Side Questions" on:click={
                    let ui_state = ui_state.clone();
                    move |_| ui_state.toggle_side_panel()
                }>
                    "💬"
                </button>
            </div>
        </header>
    }
}
