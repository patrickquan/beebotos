//! WebSocket 客户端（OpenClaw 风格协议）
//!
//! 使用浏览器原生 WebSocket API，支持：
//! - 连接生命周期管理（challenge → auth → subscribe）
//! - 自动重连
//! - chat 事件处理（Delta / Final / Error / Aborted）
//! - 回调接口供 Leptos 状态更新

use std::cell::RefCell;
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{ErrorEvent, MessageEvent, WebSocket as BrowserWebSocket};

/// WebSocket 连接状态
#[derive(Clone, Debug, PartialEq)]
pub enum WsConnectionStatus {
    Disconnected,
    Connecting,
    Reconnecting,
    AwaitingChallenge,
    Authenticating,
    Connected,
    Subscribed,
    Error(String),
}

/// 聊天事件类型（来自 Gateway）
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatEventType {
    Delta,
    Final,
    Aborted,
    Error,
}

/// 聊天消息内容块
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    ImageUrl { image_url: ImageUrlContent },
}

/// 图片 URL 内容
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageUrlContent {
    pub url: String,
}

/// Assistant 消息
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub role: String,
    pub content: Vec<ContentBlock>,
    pub timestamp: u64,
    #[serde(default)]
    pub stop_reason: Option<String>,
}

/// 聊天事件 payload
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatEventPayload {
    pub run_id: String,
    pub session_key: String,
    pub seq: u64,
    pub state: ChatEventType,
    #[serde(default)]
    pub message: Option<AssistantMessage>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub error_kind: Option<String>,
    #[serde(default)]
    pub stop_reason: Option<String>,
}

/// Gateway 事件（外层包装）
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GatewayEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub event: String,
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}

/// WebSocket 回调 trait
pub trait WsEventHandler: 'static {
    fn on_status_change(&self, status: WsConnectionStatus);
    fn on_chat_event(&self, event: ChatEventPayload);
    fn on_error(&self, error: String);
}

/// WebSocket 客户端（OpenClaw 风格）
pub struct WebSocketClient {
    url: String,
    token: Rc<RefCell<Option<String>>>,
    ws: Rc<RefCell<Option<BrowserWebSocket>>>,
    status: Rc<RefCell<WsConnectionStatus>>,
    handler: Rc<RefCell<Option<Box<dyn WsEventHandler>>>>,
    reconnect_attempts: Rc<RefCell<u32>>,
    max_reconnect_attempts: u32,
    reconnect_interval_ms: u64,
    _on_message: Rc<RefCell<Option<Closure<dyn FnMut(MessageEvent)>>>>,
    _on_open: Rc<RefCell<Option<Closure<dyn FnMut()>>>>,
    _on_close: Rc<RefCell<Option<Closure<dyn FnMut()>>>>,
    _on_error: Rc<RefCell<Option<Closure<dyn FnMut(ErrorEvent)>>>>,
}

impl Clone for WebSocketClient {
    fn clone(&self) -> Self {
        Self {
            url: self.url.clone(),
            token: self.token.clone(),
            ws: Rc::clone(&self.ws),
            status: Rc::clone(&self.status),
            handler: Rc::clone(&self.handler),
            reconnect_attempts: Rc::clone(&self.reconnect_attempts),
            max_reconnect_attempts: self.max_reconnect_attempts,
            reconnect_interval_ms: self.reconnect_interval_ms,
            _on_message: Rc::clone(&self._on_message),
            _on_open: Rc::clone(&self._on_open),
            _on_close: Rc::clone(&self._on_close),
            _on_error: Rc::clone(&self._on_error),
        }
    }
}

impl std::fmt::Debug for WebSocketClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebSocketClient")
            .field("url", &self.url)
            .field("status", &self.status.borrow())
            .field("reconnect_attempts", &self.reconnect_attempts.borrow())
            .finish()
    }
}

impl WebSocketClient {
    /// 创建新的 WebSocket 客户端
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            token: Rc::new(RefCell::new(None)),
            ws: Rc::new(RefCell::new(None)),
            status: Rc::new(RefCell::new(WsConnectionStatus::Disconnected)),
            handler: Rc::new(RefCell::new(None)),
            reconnect_attempts: Rc::new(RefCell::new(0)),
            max_reconnect_attempts: 5,
            reconnect_interval_ms: 3000,
            _on_message: Rc::new(RefCell::new(None)),
            _on_open: Rc::new(RefCell::new(None)),
            _on_close: Rc::new(RefCell::new(None)),
            _on_error: Rc::new(RefCell::new(None)),
        }
    }

    /// 设置认证 token
    pub fn set_token(&self, token: impl Into<String>) {
        *self.token.borrow_mut() = Some(token.into());
    }

    /// 设置事件处理器
    pub fn set_handler(&self, handler: Box<dyn WsEventHandler>) {
        *self.handler.borrow_mut() = Some(handler);
    }

    /// 获取当前状态
    pub fn status(&self) -> WsConnectionStatus {
        self.status.borrow().clone()
    }

    /// 连接到 WebSocket
    pub fn connect(&self) -> Result<(), WebSocketError> {
        if self.ws.borrow().is_some() {
            return Err(WebSocketError::AlreadyConnected);
        }

        self.set_status(WsConnectionStatus::Connecting);

        let ws = BrowserWebSocket::new(&self.url)
            .map_err(|e| WebSocketError::ConnectionFailed(format!("{:?}", e)))?;

        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        // Store closures to keep them alive
        let on_open = self.build_on_open();
        let on_message = self.build_on_message();
        let on_close = self.build_on_close();
        let on_error = self.build_on_error();

        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));

        *self._on_open.borrow_mut() = Some(on_open);
        *self._on_message.borrow_mut() = Some(on_message);
        *self._on_close.borrow_mut() = Some(on_close);
        *self._on_error.borrow_mut() = Some(on_error);

        *self.ws.borrow_mut() = Some(ws);
        Ok(())
    }

    /// 断开连接
    pub fn disconnect(&self) {
        *self.reconnect_attempts.borrow_mut() = 0;
        self.close_internal();
        self.set_status(WsConnectionStatus::Disconnected);
    }

    /// 发送认证消息
    pub fn authenticate(&self) -> Result<(), WebSocketError> {
        let token = self
            .token
            .borrow()
            .clone()
            .ok_or(WebSocketError::NotAuthenticated)?;

        let msg = serde_json::json!({
            "type": "auth",
            "token": token,
        });

        self.send_json(&msg)
    }

    /// 订阅会话
    pub fn subscribe(&self, session_key: &str) -> Result<(), WebSocketError> {
        let msg = serde_json::json!({
            "type": "subscribe",
            "channel": "webchat",
            "session_key": session_key,
        });

        self.send_json(&msg)
    }

    /// 发送原始 JSON
    fn send_json(&self, value: &serde_json::Value) -> Result<(), WebSocketError> {
        let ws = self.ws.borrow();
        let ws = ws.as_ref().ok_or(WebSocketError::NotConnected)?;

        let text = serde_json::to_string(value)
            .map_err(|e| WebSocketError::Serialization(e.to_string()))?;

        ws.send_with_str(&text)
            .map_err(|e| WebSocketError::SendFailed(format!("{:?}", e)))?;

        Ok(())
    }

    fn set_status(&self, status: WsConnectionStatus) {
        let changed = {
            let mut s = self.status.borrow_mut();
            if *s != status {
                *s = status.clone();
                true
            } else {
                false
            }
        };

        if changed {
            if let Some(handler) = self.handler.borrow().as_ref() {
                handler.on_status_change(status);
            }
        }
    }

    fn close_internal(&self) {
        if let Some(ws) = self.ws.borrow_mut().take() {
            let _ = ws.close();
        }
        *self._on_open.borrow_mut() = None;
        *self._on_message.borrow_mut() = None;
        *self._on_close.borrow_mut() = None;
        *self._on_error.borrow_mut() = None;
    }

    fn try_reconnect(&self) {
        let attempts = *self.reconnect_attempts.borrow();
        if attempts >= self.max_reconnect_attempts {
            self.set_status(WsConnectionStatus::Error(
                "Max reconnect attempts reached".to_string(),
            ));
            return;
        }

        *self.reconnect_attempts.borrow_mut() = attempts + 1;
        let interval = self.reconnect_interval_ms;
        let this = self.clone_ref();

        wasm_bindgen_futures::spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(interval as u32).await;
            let _ = this.connect();
        });
    }

    fn clone_ref(&self) -> Self {
        Self {
            url: self.url.clone(),
            token: self.token.clone(),
            ws: Rc::clone(&self.ws),
            status: Rc::clone(&self.status),
            handler: Rc::clone(&self.handler),
            reconnect_attempts: Rc::clone(&self.reconnect_attempts),
            max_reconnect_attempts: self.max_reconnect_attempts,
            reconnect_interval_ms: self.reconnect_interval_ms,
            _on_message: Rc::clone(&self._on_message),
            _on_open: Rc::clone(&self._on_open),
            _on_close: Rc::clone(&self._on_close),
            _on_error: Rc::clone(&self._on_error),
        }
    }

    fn build_on_open(&self) -> Closure<dyn FnMut()> {
        let this = self.clone_ref();
        Closure::new(move || {
            web_sys::console::log_1(&"[ws] connection opened".into());
            this.set_status(WsConnectionStatus::AwaitingChallenge);
            *this.reconnect_attempts.borrow_mut() = 0;
        })
    }

    fn build_on_message(&self) -> Closure<dyn FnMut(MessageEvent)> {
        let this = self.clone_ref();
        Closure::new(move |event: MessageEvent| {
            if let Ok(text) = event.data().dyn_into::<js_sys::JsString>() {
                let text = String::from(text);
                this.handle_message(&text);
            }
        })
    }

    fn build_on_close(&self) -> Closure<dyn FnMut()> {
        let this = self.clone_ref();
        Closure::new(move || {
            web_sys::console::log_1(&"[ws] connection closed".into());
            this.close_internal();

            let status = this.status.borrow().clone();
            if status != WsConnectionStatus::Disconnected {
                this.set_status(WsConnectionStatus::Reconnecting);
                this.try_reconnect();
            }
        })
    }

    fn build_on_error(&self) -> Closure<dyn FnMut(ErrorEvent)> {
        let this = self.clone_ref();
        Closure::new(move |event: ErrorEvent| {
            let msg = event.message();
            web_sys::console::error_1(&format!("[ws] error: {}", msg).into());
            this.set_status(WsConnectionStatus::Error(msg));
        })
    }

    fn handle_message(&self, text: &str) {
        let event: GatewayEvent = match serde_json::from_str(text) {
            Ok(e) => e,
            Err(err) => {
                web_sys::console::warn_1(&format!("[ws] failed to parse message: {}", err).into());
                return;
            }
        };

        match (event.event_type.as_str(), event.event.as_str()) {
            ("event", "connect.challenge") => {
                web_sys::console::log_1(&"[ws] received challenge, sending auth".into());
                self.set_status(WsConnectionStatus::Authenticating);
                if let Err(e) = self.authenticate() {
                    web_sys::console::error_1(&format!("[ws] auth send failed: {}", e).into());
                }
            }
            ("event", "auth.success") => {
                web_sys::console::log_1(&"[ws] auth success".into());
                self.set_status(WsConnectionStatus::Connected);
            }
            ("event", "chat") => {
                if let Some(payload) = event.payload {
                    match serde_json::from_value::<ChatEventPayload>(payload) {
                        Ok(chat_event) => {
                            if let Some(handler) = self.handler.borrow().as_ref() {
                                handler.on_chat_event(chat_event);
                            }
                        }
                        Err(e) => {
                            web_sys::console::warn_1(
                                &format!("[ws] failed to parse chat event: {}", e).into(),
                            );
                        }
                    }
                }
            }
            _ => {
                web_sys::console::log_1(
                    &format!("[ws] unhandled event: {}.{}", event.event_type, event.event).into(),
                );
            }
        }
    }
}

/// WebSocket 错误
#[derive(Clone, Debug)]
pub enum WebSocketError {
    NotConnected,
    AlreadyConnected,
    NotAuthenticated,
    ConnectionFailed(String),
    Serialization(String),
    SendFailed(String),
}

impl std::fmt::Display for WebSocketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WebSocketError::NotConnected => write!(f, "WebSocket not connected"),
            WebSocketError::AlreadyConnected => write!(f, "WebSocket already connected"),
            WebSocketError::NotAuthenticated => write!(f, "Not authenticated"),
            WebSocketError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            WebSocketError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            WebSocketError::SendFailed(msg) => write!(f, "Send failed: {}", msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_error_display() {
        let err = WebSocketError::NotConnected;
        assert_eq!(err.to_string(), "WebSocket not connected");
    }

    #[test]
    fn test_chat_event_deserialization() {
        let json = r#"{
            "run_id": "run_1",
            "session_key": "user:1",
            "seq": 1,
            "state": "delta",
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "Hello"}],
                "timestamp": 1234567890
            }
        }"#;

        let event: ChatEventPayload = serde_json::from_str(json).unwrap();
        assert_eq!(event.run_id, "run_1");
        assert!(matches!(event.state, ChatEventType::Delta));
        assert!(event.message.is_some());
    }

    #[test]
    fn test_gateway_event_deserialization() {
        let json = r#"{
            "type": "event",
            "event": "connect.challenge",
            "payload": { "nonce": "abc123", "ts": 1234567890 }
        }"#;

        let event: GatewayEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "event");
        assert_eq!(event.event, "connect.challenge");
        assert!(event.payload.is_some());
    }
}
