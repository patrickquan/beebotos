//! Channel HTTP Handlers (QwenPaw-style)
//!
//! 提供 `/config/channels/*` 路径下的频道管理 API，完全对齐 QwenPaw 控制台契约。

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::Json;
use beebotos_agents::communication::channel::{Channel, ChannelEvent, PersonalWeChatChannel};
use beebotos_agents::communication::{Message, MessageType, PlatformType};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::error::GatewayError;
use crate::AppState;
use gateway::middleware::AuthUser;

// ==================== 数据模型 ====================

/// 频道基础配置（所有频道共有字段）
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ChannelConfig {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter_tool_messages: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter_thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_from: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_mention: Option<bool>,
    // 通用凭证字段
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_reconnect: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept_bot_messages: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at_sender_on_reply: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_session_in_group: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ack_message: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconnect_interval_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning_before_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_before_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_merge_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_merge_delay_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_allowlist_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_allowlist: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_allowlist_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_allowlist: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_sid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tts_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tts_voice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stt_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub welcome_greeting: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sip_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sip_server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub livekit_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub livekit_api_key: Option<String>,
    // 扩展字段，用于自定义频道或未来新增字段
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

/// 频道类型信息
#[derive(Debug, Serialize, Clone)]
pub struct ChannelTypeInfo {
    pub key: String,
    pub name: String,
    pub description: String,
    pub is_builtin: bool,
}

/// 频道信息（列表展示用）
#[derive(Debug, Serialize, Clone)]
pub struct ChannelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub enabled: bool,
    pub is_builtin: bool,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<ChannelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// 频道类型列表响应
#[derive(Debug, Serialize)]
pub struct ChannelTypesResponse {
    pub types: Vec<String>,
}

/// 全量频道配置响应（key -> config map）
pub type ChannelsConfigResponse = HashMap<String, ChannelConfig>;

/// 二维码响应
#[derive(Debug, Serialize)]
pub struct ChannelQrcodeResponse {
    pub qrcode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qrcode_img_content: Option<String>,
    pub poll_token: String,
    pub expires_in: u64,
}

/// 二维码状态响应
#[derive(Debug, Serialize)]
pub struct ChannelQrcodeStatusResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// 更新频道配置请求
#[derive(Debug, Deserialize)]
pub struct UpdateChannelConfigRequest {
    #[serde(flatten)]
    pub config: ChannelConfig,
}

/// 二维码状态查询参数
#[derive(Debug, Deserialize)]
pub struct QrcodeStatusQuery {
    pub token: String,
}

// ==================== 内置频道定义 ====================

fn builtin_channel_types() -> Vec<ChannelTypeInfo> {
    vec![
        ChannelTypeInfo { key: "console".to_string(), name: "控制台".to_string(), description: "本地控制台输出".to_string(), is_builtin: true },
        ChannelTypeInfo { key: "dingtalk".to_string(), name: "钉钉".to_string(), description: "DingTalk".to_string(), is_builtin: true },
        ChannelTypeInfo { key: "feishu".to_string(), name: "飞书".to_string(), description: "Lark / Feishu".to_string(), is_builtin: true },
        ChannelTypeInfo { key: "wecom".to_string(), name: "企业微信".to_string(), description: "WeCom".to_string(), is_builtin: true },
        ChannelTypeInfo { key: "weixin".to_string(), name: "微信".to_string(), description: "WeChat".to_string(), is_builtin: true },
        ChannelTypeInfo { key: "telegram".to_string(), name: "Telegram".to_string(), description: "Telegram Bot".to_string(), is_builtin: true },
        ChannelTypeInfo { key: "discord".to_string(), name: "Discord".to_string(), description: "Discord Bot".to_string(), is_builtin: true },
        ChannelTypeInfo { key: "qq".to_string(), name: "QQ".to_string(), description: "QQ Bot".to_string(), is_builtin: true },
        ChannelTypeInfo { key: "matrix".to_string(), name: "Matrix".to_string(), description: "Matrix Protocol".to_string(), is_builtin: true },
        ChannelTypeInfo { key: "mattermost".to_string(), name: "Mattermost".to_string(), description: "Mattermost".to_string(), is_builtin: true },
        ChannelTypeInfo { key: "mqtt".to_string(), name: "MQTT".to_string(), description: "MQTT Broker".to_string(), is_builtin: true },
        ChannelTypeInfo { key: "imessage".to_string(), name: "iMessage".to_string(), description: "Apple iMessage".to_string(), is_builtin: true },
        ChannelTypeInfo { key: "onebot".to_string(), name: "OneBot".to_string(), description: "OneBot Protocol".to_string(), is_builtin: true },
        ChannelTypeInfo { key: "voice".to_string(), name: "语音".to_string(), description: "Twilio Voice".to_string(), is_builtin: true },
        ChannelTypeInfo { key: "sip".to_string(), name: "SIP".to_string(), description: "SIP Phone".to_string(), is_builtin: true },
        ChannelTypeInfo { key: "xiaoyi".to_string(), name: "小艺".to_string(), description: "Huawei XiaoYi".to_string(), is_builtin: true },
        ChannelTypeInfo { key: "webchat".to_string(), name: "WebChat".to_string(), description: "Web Admin Chat".to_string(), is_builtin: true },
    ]
}

fn get_builtin_order() -> Vec<&'static str> {
    vec![
        "console", "dingtalk", "feishu", "imessage", "discord",
        "telegram", "qq", "matrix", "sip", "xiaoyi", "wecom",
        "weixin", "mqtt", "mattermost", "onebot", "voice", "webchat",
    ]
}

fn channel_display_name(key: &str) -> String {
    match key {
        "console" => "控制台".to_string(),
        "dingtalk" => "钉钉".to_string(),
        "feishu" => "飞书".to_string(),
        "wecom" => "企业微信".to_string(),
        "weixin" => "微信".to_string(),
        "telegram" => "Telegram".to_string(),
        "discord" => "Discord".to_string(),
        "qq" => "QQ".to_string(),
        "matrix" => "Matrix".to_string(),
        "mattermost" => "Mattermost".to_string(),
        "mqtt" => "MQTT".to_string(),
        "imessage" => "iMessage".to_string(),
        "onebot" => "OneBot".to_string(),
        "voice" => "语音".to_string(),
        "sip" => "SIP".to_string(),
        "xiaoyi" => "小艺".to_string(),
        "webchat" => "WebChat".to_string(),
        _ => {
            // 自定义频道：转为 PascalCase
            let mut result = String::new();
            let mut capitalize = true;
            for c in key.chars() {
                if c == '_' || c == '-' {
                    capitalize = true;
                } else if capitalize {
                    result.push(c.to_ascii_uppercase());
                    capitalize = false;
                } else {
                    result.push(c);
                }
            }
            result
        }
    }
}

fn channel_description(key: &str) -> String {
    match key {
        "console" => "本地控制台调试输出".to_string(),
        "dingtalk" => "钉钉群机器人与 Stream 协议".to_string(),
        "feishu" => "飞书开放平台的 WebSocket 客户端".to_string(),
        "wecom" => "企业微信自建应用".to_string(),
        "weixin" => "微信 iLink 个人号协议".to_string(),
        "telegram" => "Telegram Bot API 长轮询".to_string(),
        "discord" => "Discord Gateway WebSocket".to_string(),
        "qq" => "QQ Bot 官方 Gateway".to_string(),
        "matrix" => "Matrix 协议（支持 E2EE）".to_string(),
        "mattermost" => "Mattermost WS + REST".to_string(),
        "mqtt" => "MQTT 发布/订阅".to_string(),
        "imessage" => "macOS iMessage 本地数据库轮询".to_string(),
        "onebot" => "OneBot 反向 WebSocket".to_string(),
        "voice" => "Twilio ConversationRelay 电话".to_string(),
        "sip" => "SIP 协议电话（LiveKit/pyVoIP）".to_string(),
        "xiaoyi" => "华为小艺 A2A 协议".to_string(),
        "webchat" => "Web 管理后台内置聊天".to_string(),
        _ => format!("{} 频道", key),
    }
}

fn platform_icon(key: &str) -> &'static str {
    match key {
        "weixin" | "wecom" => "💬",
        "webchat" => "🌐",
        "dingtalk" => "💼",
        "feishu" | "lark" => "🚀",
        "slack" => "💻",
        "telegram" => "✈️",
        "discord" => "🎮",
        "whatsapp" => "📱",
        "teams" => "🏢",
        "twitter" => "🐦",
        "qq" => "🐧",
        "matrix" => "🔷",
        "mqtt" => "📡",
        "imessage" => "💬",
        "onebot" => "🤖",
        "voice" => "📞",
        "sip" => "☎️",
        "xiaoyi" => "🌸",
        "console" => "🖥️",
        _ => "📡",
    }
}

// ==================== 内存配置存储 ====================

/// 全局频道配置存储（简化实现，生产环境应持久化到数据库）
static CHANNEL_CONFIG_STORE: once_cell::sync::Lazy<RwLock<HashMap<String, ChannelConfig>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(HashMap::new()));

/// 初始化默认配置
async fn init_default_configs() {
    let mut store = CHANNEL_CONFIG_STORE.write().await;
    let builtins = get_builtin_order();
    for key in builtins {
        if !store.contains_key(key) {
            let mut config = ChannelConfig::default();
            config.enabled = matches!(key, "webchat" | "console");
            store.insert(key.to_string(), config);
        }
    }
}

// ==================== QwenPaw-style API Handlers ====================

/// GET /config/channels/types
/// 返回所有可用频道类型 key 列表
pub async fn list_channel_types() -> Result<Json<ChannelTypesResponse>, GatewayError> {
    let types: Vec<String> = get_builtin_order()
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    Ok(Json(ChannelTypesResponse { types }))
}

/// GET /config/channels
/// 返回所有频道的完整配置（key -> config map）
pub async fn list_channels_config(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ChannelsConfigResponse>, GatewayError> {
    init_default_configs().await;

    let mut result = HashMap::new();
    let store = CHANNEL_CONFIG_STORE.read().await;

    // 合并内置频道和已保存的配置
    for key in get_builtin_order() {
        let mut config = store.get(key).cloned().unwrap_or_default();
        // 从运行时 registry 获取连接状态，更新 enabled
        if let Some(ref registry) = state.channel_registry {
            if let Some(ch) = registry.get_channel(key).await {
                let guard = ch.read().await;
                let is_connected = guard.is_connected();
                // 如果运行时状态与配置不一致，以运行时为准（仅用于展示）
                if config.enabled && !is_connected {
                    // 保持配置值，前端通过 status 字段展示运行时状态
                }
            }
        }
        result.insert(key.to_string(), config);
    }

    Ok(Json(result))
}

/// PUT /config/channels/:name
/// 更新单个频道配置
pub async fn update_channel_config(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<UpdateChannelConfigRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    info!("更新频道配置: {}", name);

    let mut store = CHANNEL_CONFIG_STORE.write().await;
    store.insert(name.clone(), req.config.clone());

    // 如果 enabled 变化，尝试连接/断开
    if req.config.enabled {
        info!("频道 {} 已启用", name);

        // 个人微信：更新 channel 的 bot_token 并重新连接
        if name == "weixin" {
            if let Some(ref registry) = state.channel_registry {
                if let Some(ch) = registry.get_channel("personal_wechat").await {
                    let mut guard = ch.write().await;
                    let channel_mut: &mut dyn Channel = &mut *guard;
                    if let Some(pwc) = channel_mut.as_any_mut().downcast_mut::<PersonalWeChatChannel>() {
                        if let Some(ref token) = req.config.bot_token {
                            if !token.is_empty() {
                                pwc.set_bot_token(
                                    token.clone(),
                                    req.config.bot_base_url.clone(),
                                );
                                info!("个人微信 bot_token 已更新，断开旧连接并重新连接...");
                                if let Err(e) = pwc.disconnect().await {
                                    warn!("断开旧连接失败: {}", e);
                                }
                                if let Err(e) = pwc.connect().await {
                                    warn!("个人微信重新连接失败: {}", e);
                                } else {
                                    info!("✅ 个人微信重新连接成功");
                                    // 启动消息监听器（长轮询）
                                    if let Some(ref event_bus) = state.channel_event_bus {
                                        let channel_clone = ch.clone();
                                        let event_bus_clone = event_bus.clone();
                                        tokio::spawn(async move {
                                            let guard = channel_clone.write().await;
                                            if let Err(e) = guard.start_listener(event_bus_clone).await {
                                                warn!("❌ 启动个人微信消息监听失败: {}", e);
                                            } else {
                                                info!("✅ 个人微信消息监听已启动");
                                            }
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    } else {
        info!("频道 {} 已禁用", name);
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "配置已保存",
        "channel": name,
    })))
}

/// GET /config/channels/:name/qrcode
/// 获取频道二维码（微信/钉钉/企微）
pub async fn get_channel_qrcode(
    State(state): State<Arc<AppState>>,
    Path(channel): Path<String>,
) -> Result<Json<ChannelQrcodeResponse>, GatewayError> {
    info!("获取 {} 频道二维码", channel);

    match channel.as_str() {
        "weixin" => {
            let registry = state
                .channel_registry
                .as_ref()
                .ok_or_else(|| GatewayError::internal("频道注册表未初始化"))?;

            let ch = if let Some(c) = registry.get_channel("personal_wechat").await {
                c
            } else {
                registry
                    .get_channel_by_platform(PlatformType::WeChat)
                    .await
                    .ok_or_else(|| GatewayError::internal("微信频道未初始化"))?
            };

            let qr_resp = {
                let guard = ch.read().await;
                let pwc = guard
                    .as_any()
                    .downcast_ref::<PersonalWeChatChannel>()
                    .ok_or_else(|| GatewayError::internal("频道类型不匹配"))?;
                pwc.get_qr_code().await
            }
            .map_err(|e| GatewayError::internal(format!("获取二维码失败: {}", e)))?;

            Ok(Json(ChannelQrcodeResponse {
                qrcode: qr_resp.qrcode.clone(),
                qrcode_img_content: qr_resp.qrcode_img_content,
                poll_token: qr_resp.qrcode,
                expires_in: 300,
            }))
        }
        _ => Err(GatewayError::bad_request(format!(
            "频道 {} 不支持二维码认证",
            channel
        ))),
    }
}

/// GET /config/channels/:name/qrcode/status?token=
/// 检查二维码扫描状态
pub async fn get_channel_qrcode_status(
    State(_state): State<Arc<AppState>>,
    Path(channel): Path<String>,
    Query(query): Query<QrcodeStatusQuery>,
) -> Result<Json<ChannelQrcodeStatusResponse>, GatewayError> {
    info!("检查 {} 频道二维码状态, token={}", channel, query.token);

    match channel.as_str() {
        "weixin" => {
            let mut client = beebotos_agents::communication::channel::ILinkClient::new(
                Some(beebotos_agents::communication::channel::ILinkConfig {
                    base_url: "https://ilinkai.weixin.qq.com".to_string(),
                    timeout_secs: 28,
                    max_retries: 1,
                }),
            );
            if let Err(e) = client.start().await {
                warn!("ILinkClient 启动失败: {}", e);
            }

            match client.get_qrcode_status(&query.token).await {
                Ok(qr_status) => {
                    info!("iLink QR 状态返回: status={}, has_bot_token={}, has_base_url={}",
                        qr_status.status, qr_status.bot_token.is_some(), qr_status.base_url.is_some());
                    let (status, message) = match qr_status.status.as_str() {
                        "confirmed" => ("confirmed".to_string(), Some("扫码成功，登录完成".to_string())),
                        "scanned" => ("scanned".to_string(), Some("已扫码，等待确认".to_string())),
                        "expired" => ("expired".to_string(), Some("二维码已过期".to_string())),
                        other => {
                            info!("iLink QR 未就绪状态: {}", other);
                            ("pending".to_string(), Some("等待扫码...".to_string()))
                        }
                    };

                    Ok(Json(ChannelQrcodeStatusResponse {
                        status,
                        bot_token: qr_status.bot_token,
                        base_url: qr_status.base_url,
                        message,
                    }))
                }
                Err(e) => {
                    warn!("iLink QR 状态查询失败，返回 pending: {}", e);
                    // iLink 超时或不可用时返回 pending，让前端继续轮询
                    Ok(Json(ChannelQrcodeStatusResponse {
                        status: "pending".to_string(),
                        bot_token: None,
                        base_url: None,
                        message: Some("等待扫码...".to_string()),
                    }))
                }
            }
        }
        _ => Err(GatewayError::bad_request(format!(
            "频道 {} 不支持二维码状态查询",
            channel
        ))),
    }
}

// ==================== 兼容旧版 API（保留） ====================

/// WeChat QR code response (legacy)
#[derive(Debug, Serialize)]
pub struct WeChatQrResponse {
    pub qrcode: String,
    pub qrcode_img_content: Option<String>,
    pub expires_in: u64,
}

/// QR code status response (legacy)
#[derive(Debug, Serialize)]
pub struct QrStatusResponse {
    pub status: String,
    pub bot_token: Option<String>,
    pub base_url: Option<String>,
    pub message: Option<String>,
}

/// Get WeChat QR code for login (legacy endpoint)
pub async fn get_wechat_qr(
    State(state): State<Arc<AppState>>,
) -> Result<Json<WeChatQrResponse>, GatewayError> {
    let registry = state
        .channel_registry
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Channel registry not initialized"))?
        .clone();

    let channel = if let Some(ch) = registry.get_channel("personal_wechat").await {
        ch
    } else {
        registry
            .get_channel_by_platform(PlatformType::WeChat)
            .await
            .ok_or_else(|| GatewayError::internal("Personal WeChat channel not initialized"))?
    };

    let qr_resp = {
        let guard = channel.read().await;
        let pwc = guard
            .as_any()
            .downcast_ref::<PersonalWeChatChannel>()
            .ok_or_else(|| GatewayError::internal("Channel is not PersonalWeChatChannel"))?;
        pwc.get_qr_code().await
    }
    .map_err(|e| GatewayError::internal(format!("Failed to get QR code: {}", e)))?;

    Ok(Json(WeChatQrResponse {
        qrcode: qr_resp.qrcode,
        qrcode_img_content: qr_resp.qrcode_img_content,
        expires_in: 300,
    }))
}

/// Check WeChat QR code scan status (legacy endpoint)
#[derive(Debug, Deserialize)]
pub struct CheckQrRequest {
    pub qr_code: String,
}

pub async fn check_wechat_qr(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CheckQrRequest>,
) -> Result<Json<QrStatusResponse>, GatewayError> {
    let client = beebotos_agents::communication::channel::ILinkClient::new(None);
    let qr_status = client
        .get_qrcode_status(&req.qr_code)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to check QR status: {}", e)))?;

    let status = if qr_status.status == "confirmed" {
        if let (Some(token), Some(base_url)) =
            (qr_status.bot_token.clone(), qr_status.base_url.clone())
        {
            let registry = state
                .channel_registry
                .as_ref()
                .ok_or_else(|| GatewayError::internal("Channel registry not initialized"))?
                .clone();
            let channel = registry
                .get_channel_by_platform(PlatformType::WeChat)
                .await
                .ok_or_else(|| GatewayError::internal("Personal WeChat channel not initialized"))?;
            let event_bus = state
                .channel_event_bus
                .as_ref()
                .ok_or_else(|| GatewayError::internal("Channel event bus not initialized"))?
                .clone();

            let login_result = {
                let guard = channel.read().await;
                let pwc = guard
                    .as_any()
                    .downcast_ref::<PersonalWeChatChannel>()
                    .ok_or_else(|| GatewayError::internal("Channel is not PersonalWeChatChannel"))?;
                pwc.complete_login(token, base_url, event_bus).await
            };

            if let Err(e) = login_result {
                error!("Failed to complete login: {}", e);
                return Err(GatewayError::internal(format!(
                    "Failed to complete login: {}",
                    e
                )));
            }
        }

        QrStatusResponse {
            status: "confirmed".to_string(),
            bot_token: qr_status.bot_token,
            base_url: qr_status.base_url,
            message: Some("Login successful".to_string()),
        }
    } else if qr_status.status == "scanned" {
        QrStatusResponse {
            status: "scanned".to_string(),
            bot_token: None,
            base_url: None,
            message: Some("QR code scanned, waiting for confirmation".to_string()),
        }
    } else if qr_status.status == "expired" {
        QrStatusResponse {
            status: "expired".to_string(),
            bot_token: None,
            base_url: None,
            message: Some("QR code expired".to_string()),
        }
    } else {
        QrStatusResponse {
            status: "pending".to_string(),
            bot_token: None,
            base_url: None,
            message: Some("Waiting for scan".to_string()),
        }
    };

    Ok(Json(status))
}

/// Legacy ChannelInfo (backward compatible)
#[derive(Debug, Serialize)]
pub struct LegacyChannelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub enabled: bool,
    pub status: String,
    pub config: Option<serde_json::Value>,
    pub last_error: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// List all channels (legacy endpoint)
pub async fn list_channels(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<LegacyChannelInfo>>, GatewayError> {
    let mut channels = Vec::new();

    if let Some(ref registry) = state.channel_registry {
        let registered = registry.list_channels().await;
        for info in registered {
            let platform_str = info.platform.to_string();
            channels.push(LegacyChannelInfo {
                id: info.channel_type.clone(),
                name: platform_str.clone(),
                description: format!("{} channel", platform_str),
                icon: platform_icon(&info.channel_type).to_string(),
                enabled: info.enabled,
                status: if info.is_connected {
                    "connected".to_string()
                } else {
                    "disconnected".to_string()
                },
                config: None,
                last_error: None,
                created_at: None,
                updated_at: None,
            });
        }
    }

    if channels.is_empty() {
        // 返回默认列表
        let defaults = vec![
            ("wechat", "微信", "WeChat", "💬", true, "connected"),
            ("webchat", "WebChat", "Web Admin Chat", "🌐", true, "connected"),
            ("dingtalk", "钉钉", "DingTalk", "💼", false, "disabled"),
            ("feishu", "飞书", "Lark", "🚀", false, "disabled"),
        ];
        for (id, name, desc, icon, enabled, status) in defaults {
            channels.push(LegacyChannelInfo {
                id: id.to_string(),
                name: name.to_string(),
                description: desc.to_string(),
                icon: icon.to_string(),
                enabled,
                status: status.to_string(),
                config: None,
                last_error: None,
                created_at: None,
                updated_at: None,
            });
        }
    }

    Ok(Json(channels))
}

/// Get channel by ID (legacy endpoint)
pub async fn get_channel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<LegacyChannelInfo>, GatewayError> {
    if let Some(ref registry) = state.channel_registry {
        if let Some(channel) = registry.get_channel(&id).await {
            let guard = channel.read().await;
            let platform = guard.platform();
            let platform_str = platform.to_string();
            let is_connected = guard.is_connected();

            return Ok(Json(LegacyChannelInfo {
                id: id.clone(),
                name: platform_str.clone(),
                description: format!("{} channel", platform_str),
                icon: platform_icon(&id).to_string(),
                enabled: true,
                status: if is_connected {
                    "connected".to_string()
                } else {
                    "disconnected".to_string()
                },
                config: None,
                last_error: None,
                created_at: None,
                updated_at: None,
            }));
        }
    }

    // Fallback
    let channel = match id.as_str() {
        "wechat" => LegacyChannelInfo {
            id: "wechat".to_string(),
            name: "微信".to_string(),
            description: "WeChat".to_string(),
            icon: "💬".to_string(),
            enabled: true,
            status: "connected".to_string(),
            config: None,
            last_error: None,
            created_at: None,
            updated_at: None,
        },
        "webchat" => LegacyChannelInfo {
            id: "webchat".to_string(),
            name: "WebChat".to_string(),
            description: "Web Admin Chat".to_string(),
            icon: "🌐".to_string(),
            enabled: true,
            status: "connected".to_string(),
            config: None,
            last_error: None,
            created_at: None,
            updated_at: None,
        },
        _ => return Err(GatewayError::not_found("Channel", &id)),
    };

    Ok(Json(channel))
}

/// Update channel configuration (legacy endpoint)
#[derive(Debug, Deserialize)]
pub struct UpdateChannelRequest {
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub bot_token: Option<String>,
    #[serde(default)]
    pub auto_reconnect: Option<bool>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

pub async fn update_channel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateChannelRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    if let Some(ref registry) = state.channel_registry {
        if let Some(channel) = registry.get_channel(&id).await {
            let mut guard = channel.write().await;
            if let Some(enabled) = req.enabled {
                if enabled {
                    guard.connect().await.map_err(|e| {
                        GatewayError::internal(format!("Failed to connect channel: {}", e))
                    })?;
                } else {
                    guard.disconnect().await.map_err(|e| {
                        GatewayError::internal(format!("Failed to disconnect channel: {}", e))
                    })?;
                }
            }
        }
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Channel updated",
        "channel_id": id,
    })))
}

/// Enable or disable a channel (legacy endpoint)
pub async fn set_channel_enabled(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let enabled = req
        .get("enabled")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| GatewayError::bad_request("Missing 'enabled' field"))?;

    if let Some(ref registry) = state.channel_registry {
        if let Some(channel) = registry.get_channel(&id).await {
            let mut guard = channel.write().await;
            if enabled {
                guard.connect().await.map_err(|e| {
                    GatewayError::internal(format!("Failed to connect channel: {}", e))
                })?;
            } else {
                guard.disconnect().await.map_err(|e| {
                    GatewayError::internal(format!("Failed to disconnect channel: {}", e))
                })?;
            }
        }
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": if enabled { "Channel enabled" } else { "Channel disabled" },
        "channel_id": id,
        "enabled": enabled,
    })))
}

/// Test channel connection (legacy endpoint)
pub async fn test_channel_connection(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let connected = if let Some(ref registry) = state.channel_registry {
        if let Some(channel) = registry.get_channel(&id).await {
            let guard = channel.read().await;
            guard.is_connected()
        } else {
            false
        }
    } else {
        false
    };

    Ok(Json(serde_json::json!({
        "success": connected,
        "message": if connected { "Channel connection OK" } else { "Channel not connected" },
        "channel_id": id,
    })))
}

/// Send a message to the WebChat channel (legacy endpoint)
#[derive(Debug, Deserialize)]
pub struct SendWebChatMessageRequest {
    pub user_id: String,
    pub content: String,
    #[serde(default)]
    pub session_id: Option<String>,
}

pub async fn send_webchat_message(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<SendWebChatMessageRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let event_bus = state
        .channel_event_bus
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Channel event bus not initialized"))?
        .clone();

    let session_id = req.session_id.unwrap_or_else(|| "default".to_string());
    let thread_id = Uuid::new_v4();

    let mut metadata = std::collections::HashMap::new();
    metadata.insert("sender_id".to_string(), user.user_id.clone());
    metadata.insert("session_id".to_string(), session_id.clone());
    metadata.insert(
        "message_id".to_string(),
        format!(
            "webchat_{}_{}",
            user.user_id,
            chrono::Utc::now().timestamp_millis()
        ),
    );

    let message = Message {
        id: Uuid::new_v4(),
        thread_id,
        platform: PlatformType::WebChat,
        message_type: MessageType::Text,
        content: req.content,
        metadata,
        timestamp: chrono::Utc::now(),
    };

    let event = ChannelEvent::MessageReceived {
        platform: PlatformType::WebChat,
        channel_id: session_id,
        message,
    };

    event_bus
        .send(event)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to send channel event: {}", e)))?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Message sent to WebChat channel"
    })))
}
