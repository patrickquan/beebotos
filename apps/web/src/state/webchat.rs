//! WebChat 状态管理（OpenClaw 流式传输集成）

use std::sync::{Arc, Mutex};

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::gateway::websocket::{
    ChatEventPayload, ChatEventType, WsConnectionStatus, WsEventHandler,
};
use crate::webchat::{ChatMessage, ChatSession, MessageRole, SideQuestion, TokenUsage, UsagePanel};

/// 流式文本片段（OpenClaw 风格双缓冲）
#[derive(Clone, Debug)]
pub struct StreamSegment {
    pub text: String,
    pub timestamp: u64,
}

/// WebChat 状态
#[derive(Clone)]
pub struct WebchatState {
    /// 当前选中的会话 ID
    pub current_session_id: RwSignal<Option<String>>,
    /// 所有会话
    pub sessions: RwSignal<Vec<ChatSession>>,
    /// 消息数据（Arc<Mutex> 避免 RwSignal 通知时的 RefCell 双重借用）
    pub message_data: Arc<Mutex<Vec<ChatMessage>>>,
    /// 消息版本号（用于触发响应式更新，替代直接读取 current_messages）
    pub message_version: RwSignal<u64>,
    /// 输入框内容
    pub input_content: RwSignal<String>,
    /// 是否正在发送
    pub is_sending: RwSignal<bool>,
    /// 是否正在流式接收
    pub is_streaming: RwSignal<bool>,
    /// 流式内容缓冲区（已确认的片段数组）
    pub stream_segments: RwSignal<Vec<StreamSegment>>,
    /// 当前正在接收的实时流式文本
    pub stream_buffer: RwSignal<String>,
    /// 当前流式运行的 run_id
    pub current_run_id: RwSignal<Option<String>>,
    /// WebSocket 连接状态
    pub ws_status: RwSignal<WsConnectionStatus>,
    /// 用量统计
    pub usage: RwSignal<UsagePanel>,
    /// 侧边提问列表
    pub side_questions: RwSignal<Vec<SideQuestion>>,
    /// 消息缓存（按会话 ID）
    pub message_cache: RwSignal<std::collections::HashMap<String, Vec<ChatMessage>>>,
    /// 当前错误
    pub error: RwSignal<Option<String>>,
}

impl WebchatState {
    pub fn new() -> Self {
        Self {
            current_session_id: RwSignal::new(None),
            sessions: RwSignal::new(Vec::new()),
            message_data: Arc::new(Mutex::new(Vec::new())),
            message_version: RwSignal::new(0),
            input_content: RwSignal::new(String::new()),
            is_sending: RwSignal::new(false),
            is_streaming: RwSignal::new(false),
            stream_segments: RwSignal::new(Vec::new()),
            stream_buffer: RwSignal::new(String::new()),
            current_run_id: RwSignal::new(None),
            ws_status: RwSignal::new(WsConnectionStatus::Disconnected),
            usage: RwSignal::new(UsagePanel {
                session_usage: TokenUsage::new("default"),
                daily_usage: TokenUsage::new("default"),
                monthly_usage: TokenUsage::new("default"),
                limit_status: Default::default(),
            }),
            side_questions: RwSignal::new(Vec::new()),
            message_cache: RwSignal::new(std::collections::HashMap::new()),
            error: RwSignal::new(None),
        }
    }

    /// 选中会话
    pub fn select_session(&self, id: impl Into<String>) {
        self.current_session_id.set(Some(id.into()));
        self.message_data.lock().unwrap().clear();
        self.message_version.update(|v| *v += 1);
    }

    /// 清除选中的会话
    pub fn clear_session(&self) {
        self.current_session_id.set(None);
        self.message_data.lock().unwrap().clear();
        self.message_version.update(|v| *v += 1);
    }

    /// 设置输入内容
    pub fn set_input(&self, content: impl Into<String>) {
        self.input_content.set(content.into());
    }

    /// 清空输入
    pub fn clear_input(&self) {
        self.input_content.set(String::new());
    }

    /// 添加消息（仅写入 Mutex，setInterval 轮询会自动检测变化并更新信号）
    pub fn add_message(&self, message: ChatMessage) {
        let session_id = self.current_session_id.get_untracked();
        let msg_id = message.id.clone();
        web_sys::console::log_1(
            &format!("[add_message] id={}", msg_id).into(),
        );
        // 数据写入 Mutex（不触发任何信号通知）
        self.message_data.lock().unwrap().push(message.clone());
        // 缓存更新（使用 get_untracked 避免订阅）
        if let Some(ref sid) = session_id {
            let mut cache = self.message_cache.get_untracked();
            cache.entry(sid.clone()).or_default().push(message);
            self.message_cache.set(cache);
        }
    }

    /// 开始流式接收
    pub fn start_streaming(&self, run_id: impl Into<String>) {
        self.is_streaming.set(true);
        self.stream_segments.set(Vec::new());
        self.stream_buffer.set(String::new());
        self.current_run_id.set(Some(run_id.into()));
    }

    /// 追加流式内容到实时缓冲区
    pub fn append_streaming_content(&self, chunk: impl Into<String>) {
        let mut buf = self.stream_buffer.get_untracked();
        buf.push_str(&chunk.into());
        self.stream_buffer.set(buf);
    }

    /// 将当前实时缓冲区截断为已确认片段（遇到 tool call 等中断时调用）
    pub fn truncate_stream_to_segments(&self) {
        let buffer = self.stream_buffer.get_untracked();
        if !buffer.is_empty() {
            let segment = StreamSegment {
                text: buffer,
                timestamp: chrono::Utc::now().timestamp_millis() as u64,
            };
            let mut segs = self.stream_segments.get_untracked();
            segs.push(segment);
            self.stream_segments.set(segs);
            self.stream_buffer.set(String::new());
        }
    }

    /// 结束流式接收，将缓冲内容转为正式消息
    pub fn finish_streaming(&self) {
        // 合并已确认片段和实时缓冲区
        let mut content = String::new();
        for seg in self.stream_segments.get_untracked() {
            content.push_str(&seg.text);
        }
        content.push_str(&self.stream_buffer.get_untracked());

        web_sys::console::log_1(
            &format!(
                "[finish_streaming] content_len={}, segments={}",
                content.len(),
                self.stream_segments.get_untracked().len()
            )
            .into(),
        );

        self.is_streaming.set(false);
        if !content.is_empty() {
            let message = ChatMessage {
                id: uuid::Uuid::new_v4().to_string(),
                role: MessageRole::Assistant,
                content,
                timestamp: chrono::Utc::now().to_rfc3339(),
                attachments: vec![],
                metadata: Default::default(),
                token_usage: None,
            };
            self.add_message(message);
        }
        self.stream_segments.update(|segs| segs.clear());
        self.stream_buffer.set(String::new());
        self.current_run_id.set(None);
    }

    /// 处理 WebSocket chat 事件
    pub fn handle_chat_event(&self, event: ChatEventPayload) {
        web_sys::console::log_1(
            &format!(
                "[handle_chat_event] state={:?}, run_id={}, seq={}, has_message={}",
                event.state,
                event.run_id,
                event.seq,
                event.message.is_some()
            )
            .into(),
        );
        match event.state {
            ChatEventType::Delta => {
                // 检查是否新的运行
                let current = self.current_run_id.get_untracked();
                if current.as_ref() != Some(&event.run_id) {
                    // 如果有之前的流式内容，先结束它
                    if self.is_streaming.get_untracked() {
                        self.finish_streaming();
                    }
                    self.start_streaming(&event.run_id);
                }

                // Gateway 发送的是全量累积文本，直接替换（仿 OpenClaw）
                if let Some(message) = event.message {
                    for block in message.content {
                        match block {
                            crate::gateway::websocket::ContentBlock::Text { text } => {
                                self.stream_buffer.set(text);
                            }
                            _ => {}
                        }
                    }
                }
            }
            ChatEventType::Final => {
                // 优先从 event.message 提取最终文本（仿 OpenClaw）
                // Gateway 的 Final 事件携带完整 message，应优先使用
                let final_text = event.message.as_ref().and_then(|msg| {
                    let mut text = String::new();
                    for block in &msg.content {
                        if let crate::gateway::websocket::ContentBlock::Text {
                            text: t,
                        } = block
                        {
                            text.push_str(t);
                        }
                    }
                    if text.is_empty() {
                        None
                    } else {
                        Some(text)
                    }
                });

                if let Some(text) = final_text {
                    // Final 事件携带了消息内容，直接创建消息
                    web_sys::console::log_1(
                        &format!("[Final] creating message, text_len={}", text.len()).into(),
                    );
                    // 1. 停止流式
                    self.is_streaming.set(false);
                    // 2. 写入消息数据（仅写入 Mutex，不触发信号通知）
                    let message = ChatMessage {
                        id: uuid::Uuid::new_v4().to_string(),
                        role: MessageRole::Assistant,
                        content: text,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        attachments: vec![],
                        metadata: Default::default(),
                        token_usage: None,
                    };
                    self.message_data.lock().unwrap().push(message.clone());
                    if let Some(ref sid) = self.current_session_id.get_untracked() {
                        let mut cache = self.message_cache.get_untracked();
                        cache.entry(sid.clone()).or_default().push(message);
                        self.message_cache.set(cache);
                    }
                    // 3. 清理流式状态
                    self.stream_segments.update(|segs| segs.clear());
                    self.stream_buffer.set(String::new());
                    self.current_run_id.set(None);
                    // 4. 结束发送状态
                    self.is_sending.set(false);
                } else {
                    // Final 事件没有消息内容，fallback 到 stream_buffer
                    if self.is_streaming.get_untracked() {
                        self.finish_streaming();
                    }
                    self.is_sending.set(false);
                }
            }
            ChatEventType::Aborted => {
                self.is_streaming.set(false);
                self.stream_segments.set(Vec::new());
                self.stream_buffer.set(String::new());
                self.current_run_id.set(None);
                self.is_sending.set(false);
            }
            ChatEventType::Error => {
                self.is_streaming.set(false);
                self.stream_segments.set(Vec::new());
                self.stream_buffer.set(String::new());
                self.current_run_id.set(None);
                self.is_sending.set(false);
                self.set_error(event.error_message);
            }
        }
    }

    /// 设置错误
    pub fn set_error(&self, error: Option<String>) {
        self.error.set(error);
    }

    /// 添加侧边提问
    pub fn add_side_question(&self, question: SideQuestion) {
        let mut qs = self.side_questions.get_untracked();
        qs.push(question);
        self.side_questions.set(qs);
    }

    /// 更新侧边提问响应
    pub fn update_side_question(&self, id: &str, response: impl Into<String>) {
        let response = response.into();
        let mut qs = self.side_questions.get_untracked();
        for q in qs.iter_mut() {
            if q.id == id {
                q.set_response(response.clone());
                break;
            }
        }
        self.side_questions.set(qs);
    }
}

impl Default for WebchatState {
    fn default() -> Self {
        Self::new()
    }
}

/// WebChat UI 状态
#[derive(Clone, Debug)]
pub struct ChatUIState {
    /// 是否显示会话列表面板
    pub show_sessions_panel: RwSignal<bool>,
    /// 是否显示用量面板
    pub show_usage_panel: RwSignal<bool>,
    /// 是否显示侧边提问面板
    pub show_side_panel: RwSignal<bool>,
    /// 是否显示新建会话弹窗
    pub show_new_session_modal: RwSignal<bool>,
    /// 是否显示设置弹窗
    pub show_settings_modal: RwSignal<bool>,
    /// 搜索查询
    pub search_query: RwSignal<String>,
    /// 用户是否在底部附近（OpenClaw: chatUserNearBottom）
    pub user_near_bottom: RwSignal<bool>,
    /// 有新消息在下方（OpenClaw: chatNewMessagesBelow）
    pub new_messages_below: RwSignal<bool>,
    /// 是否已完成首次自动滚动（OpenClaw: chatHasAutoScrolled）
    pub has_auto_scrolled: RwSignal<bool>,
}

impl ChatUIState {
    pub fn new() -> Self {
        Self {
            show_sessions_panel: RwSignal::new(true),
            show_usage_panel: RwSignal::new(false),
            show_side_panel: RwSignal::new(false),
            show_new_session_modal: RwSignal::new(false),
            show_settings_modal: RwSignal::new(false),
            search_query: RwSignal::new(String::new()),
            user_near_bottom: RwSignal::new(true),
            new_messages_below: RwSignal::new(false),
            has_auto_scrolled: RwSignal::new(false),
        }
    }

    pub fn toggle_sessions_panel(&self) {
        let v = self.show_sessions_panel.get_untracked();
        self.show_sessions_panel.set(!v);
    }

    pub fn toggle_usage_panel(&self) {
        let v = self.show_usage_panel.get_untracked();
        self.show_usage_panel.set(!v);
    }

    pub fn toggle_side_panel(&self) {
        let v = self.show_side_panel.get_untracked();
        self.show_side_panel.set(!v);
    }

    /// 更新用户滚动位置状态（距离底部小于 450px 视为在底部）
    pub fn update_scroll_position(&self, scroll_top: f64, scroll_height: f64, client_height: f64) {
        let distance_from_bottom = scroll_height - scroll_top - client_height;
        let near_bottom = distance_from_bottom < 450.0;
        self.user_near_bottom.set(near_bottom);
        if near_bottom {
            self.new_messages_below.set(false);
        }
    }

    /// 标记有新消息在下方
    pub fn mark_new_messages_below(&self) {
        if !self.user_near_bottom.get_untracked() {
            self.new_messages_below.set(true);
        }
    }

    /// 清除新消息提示
    pub fn clear_new_messages_below(&self) {
        self.new_messages_below.set(false);
    }
}

impl Default for ChatUIState {
    fn default() -> Self {
        Self::new()
    }
}

/// 会话 UI 状态
#[derive(Clone, Debug)]
pub struct SessionUIState {
    /// 是否正在编辑标题
    pub is_editing_title: RwSignal<bool>,
    /// 编辑中的标题
    pub editing_title: RwSignal<String>,
    /// 选中的消息 ID（用于复制等操作）
    pub selected_message_id: RwSignal<Option<String>>,
    /// 是否显示附件上传
    pub show_attachment_upload: RwSignal<bool>,
    /// 上传中的文件
    pub uploading_files: RwSignal<Vec<String>>,
}

impl SessionUIState {
    pub fn new() -> Self {
        Self {
            is_editing_title: RwSignal::new(false),
            editing_title: RwSignal::new(String::new()),
            selected_message_id: RwSignal::new(None),
            show_attachment_upload: RwSignal::new(false),
            uploading_files: RwSignal::new(Vec::new()),
        }
    }

    pub fn start_editing_title(&self, current_title: &str) {
        self.editing_title.set(current_title.to_string());
        self.is_editing_title.set(true);
    }

    pub fn finish_editing_title(&self) {
        self.is_editing_title.set(false);
    }

    pub fn cancel_editing_title(&self) {
        self.is_editing_title.set(false);
        self.editing_title.set(String::new());
    }
}

impl Default for SessionUIState {
    fn default() -> Self {
        Self::new()
    }
}

/// WebSocket 事件处理器 —— 将 Gateway 事件转换为 WebchatState 更新
#[derive(Clone)]
pub struct WebchatWsHandler {
    state: WebchatState,
}

impl WebchatWsHandler {
    pub fn new(state: WebchatState) -> Self {
        Self { state }
    }
}

impl WsEventHandler for WebchatWsHandler {
    fn on_status_change(&self, status: WsConnectionStatus) {
        self.state.ws_status.set(status);
    }

    fn on_chat_event(&self, event: ChatEventPayload) {
        // 使用 setTimeout(0) 延迟处理，避免 WASM TaskQueue RefCell 双重借用
        let state = self.state.clone();
        let window = web_sys::window().unwrap();
        let closure = wasm_bindgen::closure::Closure::once(move || {
            state.handle_chat_event(event);
        });
        window
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                closure.as_ref().unchecked_ref(),
                0,
            )
            .unwrap();
        closure.forget();
    }

    fn on_error(&self, error: String) {
        self.state.set_error(Some(error));
    }
}

/// 提供 WebChat 状态到上下文
pub fn provide_webchat_state() {
    provide_context(WebchatState::new());
    provide_context(ChatUIState::new());
    provide_context(SessionUIState::new());
}

/// 使用 WebChat 状态
pub fn use_webchat_state() -> WebchatState {
    use_context::<WebchatState>().expect("WebchatState not provided")
}

/// 使用 WebChat UI 状态
pub fn use_chat_ui_state() -> ChatUIState {
    use_context::<ChatUIState>().expect("ChatUIState not provided")
}

/// 使用会话 UI 状态
pub fn use_session_ui_state() -> SessionUIState {
    use_context::<SessionUIState>().expect("SessionUIState not provided")
}
