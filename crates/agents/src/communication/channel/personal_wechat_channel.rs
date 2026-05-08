//! Personal WeChat Channel Implementation (iLink Protocol) — QwenPaw 1:1 复刻
//!
//! 基于腾讯 iLink Bot API (https://ilinkai.weixin.qq.com) 的个人微信通道实现。
//! 支持二维码登录、长轮询收消息、消息去重、消息合并、白名单、引用消息、
//! 多媒体消息（文本/图片/语音/文件/视频）收发、AES 解密、typing 状态、
//! 会话持久化、健康检查等完整功能。
//!
//! 核心特性（13 项）：
//! 1. 完整配置结构体（含白名单策略）
//! 2. Session ID 解析（wxid_ 前缀处理）
//! 3. Token 持久化（JSON 文件）
//! 4. 消息去重（HashMap + VecDeque FIFO，最大 2000 条）
//! 5. 消息合并（同用户 2 秒内多条消息合并）
//! 6. 白名单检查（DM 和群聊独立策略）
//! 7. 引用消息处理（ref_msg 提取）
//! 8. 入站消息处理（文本/图片/语音/文件/视频，含 AES 解密）
//! 9. 发送功能（文本智能分段 ~1500 字）
//! 10. Typing 状态（ticket 缓存 24 小时）
//! 11. 健康检查（会话有效期监控）
//! 12. 长轮询循环（35 秒 server hold）
//! 13. 自动重连与过期提醒

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::ilink_client::{
    BotSession, ILinkClient, ILinkConfig, QrCodeResponse, WeChatMessage,
};
use super::r#trait::{BaseChannelConfig, ConnectionMode, ContentType};
use super::{Channel, ChannelConfig, ChannelEvent, ChannelInfo, MemberInfo};
use crate::communication::{Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

// ==================== 常量 ====================

/// 消息去重队列最大长度
const DEDUP_MAX_SIZE: usize = 2000;
/// 消息合并时间窗口（毫秒）
const MERGE_WINDOW_MS: u64 = 2000;
/// 文本消息单条最大长度（微信限制约 2000，留余量）
const MAX_TEXT_LENGTH: usize = 1500;
/// Typing ticket 缓存有效期（秒）
const TYPING_TICKET_TTL_SECS: u64 = 24 * 3600;
/// 长轮询错误后重试间隔（秒）
const POLL_RETRY_SECS: u64 = 5;
/// 健康检查间隔（秒）
const HEALTH_CHECK_INTERVAL_SECS: u64 = 60;
/// 会话过期前警告阈值（秒）
const SESSION_WARNING_SECS: u64 = 7200;
/// 会话过期前强制重连阈值（秒）
const SESSION_FORCE_RECONNECT_SECS: u64 = 1800;

// ==================== 配置 ====================

/// 白名单策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AllowlistPolicy {
    /// 开放模式：允许所有用户
    #[default]
    Open,
    /// 白名单模式：仅允许列表内用户
    Allowlist,
}

/// 个人微信通道配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalWeChatConfig {
    /// iLink API 基础 URL
    #[serde(default = "default_base_url")]
    pub base_url: String,
    /// Bot token（登录后获得，可选）
    pub bot_token: Option<String>,
    /// Bot 基础 URL（登录后可能不同）
    pub bot_base_url: Option<String>,
    /// 自动重连
    #[serde(default = "default_auto_reconnect")]
    pub auto_reconnect: bool,
    /// 重连检查间隔（秒）
    #[serde(default = "default_reconnect_interval")]
    pub reconnect_interval_secs: u64,
    /// 过期前警告时间（秒）
    #[serde(default = "default_warning_before")]
    pub warning_before_secs: u64,
    /// 强制重连时间（秒）
    #[serde(default = "default_force_before")]
    pub force_before_secs: u64,
    /// DM（私聊）白名单策略
    #[serde(default)]
    pub dm_allowlist_policy: AllowlistPolicy,
    /// DM 白名单用户 ID 列表
    #[serde(default)]
    pub dm_allowlist: Vec<String>,
    /// 群聊白名单策略
    #[serde(default)]
    pub group_allowlist_policy: AllowlistPolicy,
    /// 群聊白名单群 ID 列表
    #[serde(default)]
    pub group_allowlist: Vec<String>,
    /// Bot 前缀（用于命令识别，如 "/"）
    #[serde(default)]
    pub bot_prefix: Option<String>,
    /// 是否启用消息合并
    #[serde(default = "default_message_merge_enabled")]
    pub message_merge_enabled: bool,
    /// 消息合并延迟（毫秒）
    #[serde(default = "default_message_merge_delay_ms")]
    pub message_merge_delay_ms: u64,
    /// 媒体文件下载目录
    #[serde(default)]
    pub media_dir: Option<String>,
    /// 基础通道配置
    #[serde(flatten)]
    pub base: BaseChannelConfig,
}

fn default_message_merge_enabled() -> bool {
    true
}

fn default_message_merge_delay_ms() -> u64 {
    2000
}

fn default_base_url() -> String {
    "https://ilinkai.weixin.qq.com".to_string()
}

fn default_auto_reconnect() -> bool {
    true
}

fn default_reconnect_interval() -> u64 {
    300
}

fn default_warning_before() -> u64 {
    SESSION_WARNING_SECS
}

fn default_force_before() -> u64 {
    SESSION_FORCE_RECONNECT_SECS
}

impl Default for PersonalWeChatConfig {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            bot_token: None,
            bot_base_url: None,
            auto_reconnect: true,
            reconnect_interval_secs: 300,
            warning_before_secs: SESSION_WARNING_SECS,
            force_before_secs: SESSION_FORCE_RECONNECT_SECS,
            dm_allowlist_policy: AllowlistPolicy::Open,
            dm_allowlist: vec![],
            group_allowlist_policy: AllowlistPolicy::Open,
            group_allowlist: vec![],
            bot_prefix: Some("/".to_string()),
            message_merge_enabled: true,
            message_merge_delay_ms: 2000,
            media_dir: None,
            base: BaseChannelConfig {
                connection_mode: ConnectionMode::Polling,
                auto_reconnect: true,
                max_reconnect_attempts: 10,
                ..Default::default()
            },
        }
    }
}

impl ChannelConfig for PersonalWeChatConfig {
    fn from_env() -> Option<Self>
    where
        Self: Sized,
    {
        let base_url =
            std::env::var("PERSONAL_WECHAT_BASE_URL").unwrap_or_else(|_| default_base_url());
        let bot_token = std::env::var("PERSONAL_WECHAT_BOT_TOKEN").ok();
        let bot_base_url = std::env::var("PERSONAL_WECHAT_BOT_BASE_URL").ok();

        let mut base = BaseChannelConfig::from_env("PERSONAL_WECHAT").unwrap_or_default();
        base.connection_mode = ConnectionMode::Polling;

        let dm_allowlist_policy = std::env::var("PERSONAL_WECHAT_DM_POLICY")
            .ok()
            .and_then(|s| match s.as_str() {
                "allowlist" => Some(AllowlistPolicy::Allowlist),
                _ => Some(AllowlistPolicy::Open),
            })
            .unwrap_or_default();

        let dm_allowlist = std::env::var("PERSONAL_WECHAT_DM_ALLOWLIST")
            .ok()
            .map(|s| s.split(',').map(|x| x.trim().to_string()).collect())
            .unwrap_or_default();

        let group_allowlist_policy = std::env::var("PERSONAL_WECHAT_GROUP_POLICY")
            .ok()
            .and_then(|s| match s.as_str() {
                "allowlist" => Some(AllowlistPolicy::Allowlist),
                _ => Some(AllowlistPolicy::Open),
            })
            .unwrap_or_default();

        let group_allowlist = std::env::var("PERSONAL_WECHAT_GROUP_ALLOWLIST")
            .ok()
            .map(|s| s.split(',').map(|x| x.trim().to_string()).collect())
            .unwrap_or_default();

        Some(Self {
            base_url,
            bot_token,
            bot_base_url,
            auto_reconnect: std::env::var("PERSONAL_WECHAT_AUTO_RECONNECT")
                .map(|v| v.parse().unwrap_or(true))
                .unwrap_or(true),
            reconnect_interval_secs: std::env::var("PERSONAL_WECHAT_RECONNECT_INTERVAL")
                .map(|v| v.parse().unwrap_or(300))
                .unwrap_or(300),
            warning_before_secs: std::env::var("PERSONAL_WECHAT_WARNING_BEFORE")
                .map(|v| v.parse().unwrap_or(SESSION_WARNING_SECS))
                .unwrap_or(SESSION_WARNING_SECS),
            force_before_secs: std::env::var("PERSONAL_WECHAT_FORCE_BEFORE")
                .map(|v| v.parse().unwrap_or(SESSION_FORCE_RECONNECT_SECS))
                .unwrap_or(SESSION_FORCE_RECONNECT_SECS),
            dm_allowlist_policy,
            dm_allowlist,
            group_allowlist_policy,
            group_allowlist,
            bot_prefix: std::env::var("PERSONAL_WECHAT_BOT_PREFIX").ok(),
            message_merge_enabled: std::env::var("PERSONAL_WECHAT_MESSAGE_MERGE_ENABLED")
                .map(|v| v.parse().unwrap_or(true))
                .unwrap_or(true),
            message_merge_delay_ms: std::env::var("PERSONAL_WECHAT_MESSAGE_MERGE_DELAY_MS")
                .map(|v| v.parse().unwrap_or(2000))
                .unwrap_or(2000),
            media_dir: std::env::var("PERSONAL_WECHAT_MEDIA_DIR").ok(),
            base,
        })
    }

    fn is_valid(&self) -> bool {
        true
    }

    fn allowlist(&self) -> Vec<String> {
        self.dm_allowlist.clone()
    }

    fn connection_mode(&self) -> ConnectionMode {
        ConnectionMode::Polling
    }

    fn auto_reconnect(&self) -> bool {
        self.auto_reconnect
    }

    fn max_reconnect_attempts(&self) -> u32 {
        self.base.max_reconnect_attempts
    }
}

// ==================== 内部状态结构 ====================

/// 持久化会话数据
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedSession {
    bot_token: String,
    base_url: String,
    login_time: chrono::DateTime<chrono::Utc>,
    wxid: Option<String>,
    nickname: Option<String>,
}

/// Typing ticket 缓存信息
#[derive(Debug, Clone)]
struct TypingTicketInfo {
    ticket: String,
    cached_at: Instant,
}

impl TypingTicketInfo {
    fn is_valid(&self) -> bool {
        self.cached_at.elapsed().as_secs() < TYPING_TICKET_TTL_SECS
    }
}

/// 消息去重记录
#[derive(Debug, Clone)]
struct DedupEntry {
    msg_key: String,
    timestamp: Instant,
}

/// 消息合并缓冲区条目
#[derive(Debug, Clone)]
struct MergeBufferEntry {
    from_user_id: String,
    context_token: String,
    contents: Vec<String>,
    metadata: HashMap<String, String>,
    message_type: MessageType,
    first_received: Instant,
}

/// QR 登录信息
#[derive(Debug, Clone)]
struct QrLoginInfo {
    qrcode: String,
    qrcode_url: Option<String>,
}

// ==================== 通道实现 ====================

/// 个人微信通道（iLink 协议）
pub struct PersonalWeChatChannel {
    config: PersonalWeChatConfig,
    ilink_client: Arc<RwLock<ILinkClient>>,
    connected: Arc<RwLock<bool>>,
    session: Arc<RwLock<Option<BotSession>>>,
    /// QR 登录信息
    qr_login_info: Arc<RwLock<Option<QrLoginInfo>>>,
    /// Typing ticket 缓存
    typing_tickets: Arc<RwLock<HashMap<String, TypingTicketInfo>>>,
    /// 最后联系人（用于重连通知）
    last_contact: Arc<RwLock<Option<(String, String)>>>,
    /// 重连待确认标志
    reconnect_pending: Arc<RwLock<bool>>,
    /// 监听器任务句柄
    listener_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    /// 重连监控任务句柄
    reconnect_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    /// 健康检查任务句柄
    health_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    /// 事件发送器
    event_sender: Arc<RwLock<Option<mpsc::Sender<ChannelEvent>>>>,
    /// 已欢迎用户
    welcomed_users: Arc<RwLock<HashMap<String, bool>>>,
    /// Session 持久化路径
    session_store_path: PathBuf,
    /// 消息去重队列
    dedup_queue: Arc<RwLock<VecDeque<DedupEntry>>>,
    /// 消息合并缓冲区
    merge_buffer: Arc<RwLock<HashMap<String, MergeBufferEntry>>>,
    /// 消息轮询锁，防止重复启动多个 poll_messages 循环
    is_polling: Arc<AtomicBool>,
}

/// 生成 base64 PNG 二维码图片（QwenPaw 风格）
fn generate_qrcode_image(scan_url: &str) -> Result<String> {
    let png_data =
        qrcode_generator::to_png_to_vec(scan_url, qrcode_generator::QrCodeEcc::Medium, 256)
            .map_err(|e| AgentError::platform(format!("二维码生成失败: {}", e)))?;
    let base64_img = format!("data:image/png;base64,{}", base64::encode(&png_data));
    Ok(base64_img)
}

impl PersonalWeChatChannel {
    /// 创建新通道
    pub fn new(config: PersonalWeChatConfig) -> Self {
        let ilink_config = ILinkConfig {
            base_url: config.base_url.clone(),
            ..Default::default()
        };
        let ilink_client = ILinkClient::new(Some(ilink_config));

        let session_store_path = std::env::var("PERSONAL_WECHAT_SESSION_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("data/personal_wechat_session.json"));

        Self {
            config,
            ilink_client: Arc::new(RwLock::new(ilink_client)),
            connected: Arc::new(RwLock::new(false)),
            session: Arc::new(RwLock::new(None)),
            qr_login_info: Arc::new(RwLock::new(None)),
            typing_tickets: Arc::new(RwLock::new(HashMap::new())),
            last_contact: Arc::new(RwLock::new(None)),
            reconnect_pending: Arc::new(RwLock::new(false)),
            listener_handle: Arc::new(RwLock::new(None)),
            reconnect_handle: Arc::new(RwLock::new(None)),
            health_handle: Arc::new(RwLock::new(None)),
            event_sender: Arc::new(RwLock::new(None)),
            welcomed_users: Arc::new(RwLock::new(HashMap::new())),
            session_store_path,
            dedup_queue: Arc::new(RwLock::new(VecDeque::with_capacity(DEDUP_MAX_SIZE))),
            merge_buffer: Arc::new(RwLock::new(HashMap::new())),
            is_polling: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 更新配置（配置保存后调用）
    pub fn update_config(&mut self, config: PersonalWeChatConfig) {
        self.config = config;
    }

    /// 设置 bot_token 和 base_url（扫码登录后调用）
    pub fn set_bot_token(&mut self, bot_token: String, base_url: Option<String>) {
        self.config.bot_token = Some(bot_token);
        if let Some(base) = base_url {
            self.config.bot_base_url = Some(base);
        }
    }

    // ---------- Session 持久化 ----------

    /// 保存 session 到磁盘
    async fn save_session(&self) {
        if let Some(session) = self.session.read().await.as_ref() {
            let persisted = PersistedSession {
                bot_token: session.bot_token.clone(),
                base_url: session.base_url.clone(),
                login_time: chrono::Utc::now(),
                wxid: session.wxid.clone(),
                nickname: session.nickname.clone(),
            };
            if let Ok(json) = serde_json::to_string_pretty(&persisted) {
                if let Some(parent) = self.session_store_path.parent() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }
                match tokio::fs::write(&self.session_store_path, json).await {
                    Ok(_) => info!("个人微信 session 已持久化到 {:?}", self.session_store_path),
                    Err(e) => warn!("保存个人微信 session 失败: {}", e),
                }
            }
        }
    }

    /// 从磁盘加载 session
    async fn load_session(&self) -> Option<BotSession> {
        match tokio::fs::read_to_string(&self.session_store_path).await {
            Ok(content) => match serde_json::from_str::<PersistedSession>(&content) {
                Ok(persisted) => {
                    info!("从 {:?} 恢复个人微信 session", self.session_store_path);
                    Some(BotSession {
                        bot_token: persisted.bot_token,
                        base_url: persisted.base_url,
                        login_time: Instant::now(),
                        wxid: persisted.wxid,
                        nickname: persisted.nickname,
                    })
                }
                Err(e) => {
                    warn!("解析个人微信 session 文件失败: {}", e);
                    None
                }
            },
            Err(e) => {
                debug!("未找到个人微信 session 文件 {:?}: {}", self.session_store_path, e);
                None
            }
        }
    }

    /// 清除持久化 session
    async fn clear_session(&self) {
        let _ = tokio::fs::remove_file(&self.session_store_path).await;
    }

    // ---------- QR 登录 ----------

    /// 获取登录二维码
    pub async fn get_qr_code(&self) -> Result<QrCodeResponse> {
        // 确保 ILinkClient 已启动
        {
            let mut client = self.ilink_client.write().await;
            if client.start().await.is_err() {
                warn!("ILinkClient 已经启动");
            }
        }

        let client = self.ilink_client.read().await;
        let qr_resp = client.get_bot_qrcode().await?;
        drop(client);

        // 构造扫码 URL（对齐 QwenPaw）
        let scan_url = if let Some(ref img) = qr_resp.qrcode_img_content {
            if img.starts_with("http") {
                img.clone()
            } else {
                format!(
                    "https://liteapp.weixin.qq.com/q/7GiQu1?qrcode={}&bot_type=3",
                    qr_resp.qrcode
                )
            }
        } else {
            format!(
                "https://liteapp.weixin.qq.com/q/7GiQu1?qrcode={}&bot_type=3",
                qr_resp.qrcode
            )
        };

        // 生成 base64 PNG 二维码图片（QwenPaw 风格）
        let qrcode_img_content = match generate_qrcode_image(&scan_url) {
            Ok(img) => Some(img),
            Err(e) => {
                warn!("二维码图片生成失败: {}, 使用原始链接", e);
                None
            }
        };

        let login_info = QrLoginInfo {
            qrcode: qr_resp.qrcode.clone(),
            qrcode_url: Some(scan_url),
        };
        *self.qr_login_info.write().await = Some(login_info);

        info!("个人微信登录二维码已生成: {}", qr_resp.qrcode);

        // 如果未连接且有 event_sender，自动重启 QR 轮询（解决重新扫码后无人轮询的问题）
        if !*self.connected.read().await {
            if let Some(event_bus) = self.event_sender.read().await.clone() {
                info!("🔄 检测到新二维码且未连接，自动重启 QR 状态轮询");
                let channel = self.clone();
                tokio::spawn(async move {
                    if let Err(e) = channel.start_listener(event_bus).await {
                        warn!("自动重启 QR 轮询失败: {}", e);
                    }
                });
            }
        }

        Ok(QrCodeResponse {
            qrcode: qr_resp.qrcode,
            qrcode_img_content,
        })
    }

    /// 检查二维码扫描状态并完成登录
    pub async fn check_qr_status(&self) -> Result<bool> {
        // 确保 ILinkClient 已启动
        {
            let mut client = self.ilink_client.write().await;
            if client.start().await.is_err() {
                warn!("ILinkClient 已经启动");
            }
        }

        let qrcode = match self.qr_login_info.read().await.as_ref() {
            Some(info) => info.qrcode.clone(),
            None => return Err(AgentError::platform("尚未生成 QR 码").into()),
        };

        let client = self.ilink_client.read().await;
        let status = client.get_qrcode_status(&qrcode).await?;
        drop(client);

        if status.status == "confirmed" {
            if let (Some(token), Some(base_url)) = (status.bot_token, status.base_url) {
                // 更新 ilink_client 的 token 和 base_url
                {
                    let mut client = self.ilink_client.write().await;
                    client.bot_token = token.clone();
                    client.base_url = base_url.clone();
                }

                let session = BotSession {
                    bot_token: token,
                    base_url,
                    login_time: Instant::now(),
                    wxid: None,
                    nickname: None,
                };

                *self.session.write().await = Some(session);
                *self.connected.write().await = true;
                self.save_session().await;
                info!("个人微信登录成功！");

                self.start_reconnect_monitor();
                self.start_health_check();

                return Ok(true);
            }
        }

        Ok(false)
    }

    // ---------- 消息去重 ----------

    /// 检查消息是否重复
    async fn is_duplicate(&self, msg: &WeChatMessage) -> bool {
        let msg_key = format!(
            "{}:{}:{}",
            msg.from_user_id,
            msg.message_id.unwrap_or(0),
            msg.context_token
        );

        let mut queue = self.dedup_queue.write().await;

        // 检查是否已存在
        if queue.iter().any(|e| e.msg_key == msg_key) {
            return true;
        }

        // 添加到队列
        queue.push_back(DedupEntry {
            msg_key,
            timestamp: Instant::now(),
        });

        // 限制队列长度
        while queue.len() > DEDUP_MAX_SIZE {
            queue.pop_front();
        }

        false
    }

    // ---------- 白名单检查 ----------

    /// 检查用户/群是否在白名单中
    async fn check_allowlist(&self, from_user_id: &str, group_id: &str) -> bool {
        let is_group = !group_id.is_empty();

        if is_group {
            // 群聊检查
            match self.config.group_allowlist_policy {
                AllowlistPolicy::Open => true,
                AllowlistPolicy::Allowlist => self.config.group_allowlist.iter().any(|g| g == group_id),
            }
        } else {
            // DM 检查
            match self.config.dm_allowlist_policy {
                AllowlistPolicy::Open => true,
                AllowlistPolicy::Allowlist => {
                    self.config.dm_allowlist.iter().any(|u| u == from_user_id)
                }
            }
        }
    }

    // ---------- Session ID 解析 ----------

    /// 解析 session ID，处理 wxid_ 前缀
    fn parse_session_id(&self, user_id: &str) -> String {
        if user_id.starts_with("wxid_") {
            user_id.to_string()
        } else {
            user_id.to_string()
        }
    }

    // ---------- 消息合并 ----------

    /// 处理消息合并逻辑
    async fn handle_merge_or_dispatch(
        &self,
        from_user_id: String,
        context_token: String,
        content: String,
        metadata: HashMap<String, String>,
        message_type: MessageType,
        event_sender: &mpsc::Sender<ChannelEvent>,
    ) -> Result<()> {
        let mut buffer = self.merge_buffer.write().await;

        if let Some(entry) = buffer.get_mut(&from_user_id) {
            // 检查是否在合并窗口内且消息类型相同
            let elapsed = entry.first_received.elapsed().as_millis() as u64;
            if elapsed < MERGE_WINDOW_MS && entry.message_type == message_type {
                // 合并内容
                entry.contents.push(content);
                entry.metadata.extend(metadata);
                return Ok(());
            } else {
                // 超时或类型不同，先刷新旧消息
                let old_entry = buffer.remove(&from_user_id).unwrap();
                drop(buffer);
                self.dispatch_merged_message(old_entry, event_sender).await?;
                // 继续处理新消息
                let mut buffer = self.merge_buffer.write().await;
                self.insert_merge_buffer(
                    &mut buffer,
                    from_user_id,
                    context_token,
                    content,
                    metadata,
                    message_type,
                );
                return Ok(());
            }
        }

        // 新消息，插入合并缓冲区
        let from_id_clone = from_user_id.clone();
        self.insert_merge_buffer(
            &mut buffer,
            from_user_id,
            context_token,
            content,
            metadata,
            message_type,
        );
        drop(buffer);

        // 启动自动刷新定时器，避免消息永远卡在缓冲区
        let channel = self.clone();
        let sender = event_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(MERGE_WINDOW_MS + 100)).await;
            if let Err(e) = channel.flush_merge_buffer(&from_id_clone, &sender).await {
                debug!("自动刷新合并缓冲区失败: {}", e);
            }
        });

        Ok(())
    }

    fn insert_merge_buffer(
        &self,
        buffer: &mut HashMap<String, MergeBufferEntry>,
        from_user_id: String,
        context_token: String,
        content: String,
        metadata: HashMap<String, String>,
        message_type: MessageType,
    ) {
        buffer.insert(
            from_user_id.clone(),
            MergeBufferEntry {
                from_user_id,
                context_token,
                contents: vec![content],
                metadata,
                message_type,
                first_received: Instant::now(),
            },
        );
    }

    /// 刷新合并缓冲区中的消息
    async fn flush_merge_buffer(
        &self,
        user_id: &str,
        event_sender: &mpsc::Sender<ChannelEvent>,
    ) -> Result<()> {
        let entry = {
            let mut buffer = self.merge_buffer.write().await;
            buffer.remove(user_id)
        };

        if let Some(entry) = entry {
            let from_id = entry.from_user_id.clone();
            let content_len = entry.contents.join("\n").len();
            info!(
                "🔄 刷新合并缓冲区: from={}, content_len={}",
                from_id, content_len
            );
            self.dispatch_merged_message(entry, event_sender).await?;
        }

        Ok(())
    }

    /// 发送合并后的消息到事件总线
    async fn dispatch_merged_message(
        &self,
        entry: MergeBufferEntry,
        event_sender: &mpsc::Sender<ChannelEvent>,
    ) -> Result<()> {
        let merged_content = entry.contents.join("\n");
        let from_id = entry.from_user_id.clone();
        let content_len = merged_content.len();

        let message = Message {
            id: Uuid::new_v4(),
            thread_id: Uuid::new_v4(),
            platform: PlatformType::WeChat,
            message_type: entry.message_type,
            content: merged_content,
            metadata: entry.metadata,
            timestamp: chrono::Utc::now(),
        };

        let event = ChannelEvent::MessageReceived {
            platform: PlatformType::WeChat,
            channel_id: entry.from_user_id,
            message,
        };

        if let Err(e) = event_sender.send(event).await {
            return Err(AgentError::platform(format!("事件总线错误: {}", e)).into());
        }

        info!(
            "📤 ChannelEvent 已发送: platform=WeChat, channel_id={}, content_len={}",
            from_id, content_len
        );

        Ok(())
    }

    // ---------- 入站消息处理 ----------

    /// 处理单条入站消息
    async fn process_message(
        &self,
        msg: WeChatMessage,
        event_sender: &mpsc::Sender<ChannelEvent>,
    ) -> Result<()> {
        // 去重检查
        if self.is_duplicate(&msg).await {
            debug!("消息去重: 跳过重复消息 from={}", msg.from_user_id);
            return Ok(());
        }

        let from_id = msg.from_user_id.clone();
        let context_token = msg.context_token.clone();
        let group_id = msg.group_id.clone();

        // 更新最后联系人
        *self.last_contact.write().await = Some((from_id.clone(), context_token.clone()));

        // 白名单检查
        if !self.check_allowlist(&from_id, &group_id).await {
            debug!("白名单拦截: from={}, group={}", from_id, group_id);
            return Ok(());
        }

        // 解析实际消息类型（优先使用 item_list[0].item_type）
        let actual_type = msg
            .item_list
            .first()
            .map(|item| item.item_type)
            .unwrap_or(msg.message_type);

        debug!(
            "收到个人微信消息 from={}, message_type={}, actual_type={}",
            from_id, msg.message_type, actual_type
        );

        // 构建 metadata
        let mut metadata = HashMap::new();
        metadata.insert("from_user_id".to_string(), from_id.clone());
        metadata.insert("sender_id".to_string(), from_id.clone());
        metadata.insert("channel_id".to_string(), from_id.clone());
        metadata.insert("to_user_id".to_string(), msg.to_user_id.clone());
        metadata.insert("context_token".to_string(), context_token.clone());
        if let Some(msg_id) = msg.message_id {
            metadata.insert("msg_id".to_string(), msg_id.to_string());
        }
        if !group_id.is_empty() {
            metadata.insert("group_id".to_string(), group_id.clone());
        }

        // 处理引用消息
        if let Some(ref_msg) = msg.item_list.first().and_then(|item| item.ref_msg.as_ref()) {
            if let Some(ref_item) = ref_msg.message_item.as_ref() {
                let ref_text = ref_item
                    .text_item
                    .as_ref()
                    .map(|t| t.text.clone())
                    .unwrap_or_default();
                metadata.insert("ref_msg_text".to_string(), ref_text);
                metadata.insert(
                    "ref_msg_type".to_string(),
                    ref_item.item_type.to_string(),
                );
            }
        }

        // 根据消息类型处理
        let (message_type, content) = match actual_type {
            1 => {
                // 文本消息
                let text = msg.text().unwrap_or_default();
                info!("📨 文本消息 from={}: {}", from_id, text);
                (MessageType::Text, text)
            }
            2 => {
                // 图片消息
                self.process_image_message(&msg, &from_id, &mut metadata).await
            }
            3 => {
                // 语音消息
                self.process_voice_message(&msg, &from_id, &mut metadata).await
            }
            4 => {
                // 视频消息
                self.process_video_message(&msg, &from_id, &mut metadata).await
            }
            5 => {
                // 文件消息
                self.process_file_message(&msg, &from_id, &mut metadata).await
            }
            _ => {
                info!(
                    "📨 其他类型消息 from={}, type={}",
                    from_id, msg.message_type
                );
                metadata.insert("raw_type".to_string(), msg.message_type.to_string());
                (
                    MessageType::Text,
                    format!("[{}消息]", msg.message_type_name()),
                )
            }
        };

        // 使用消息合并机制发送
        self.handle_merge_or_dispatch(
            from_id,
            context_token,
            content,
            metadata,
            message_type,
            event_sender,
        )
        .await
    }

    /// 处理图片消息
    async fn process_image_message(
        &self,
        msg: &WeChatMessage,
        from_id: &str,
        metadata: &mut HashMap<String, String>,
    ) -> (MessageType, String) {
        if let Some(pic) = msg.picture() {
            info!("🖼️ 图片消息 from={}, url={}", from_id, pic.pic_url);
            metadata.insert("image_url".to_string(), pic.pic_url.clone());
            metadata.insert("image_width".to_string(), pic.pic_width.to_string());
            metadata.insert("image_height".to_string(), pic.pic_height.to_string());
            metadata.insert("image_size".to_string(), pic.pic_size.to_string());
            metadata.insert("image_name".to_string(), pic.file_name.clone());
            if !pic.aeskey.is_empty() {
                metadata.insert("aes_key".to_string(), pic.aeskey.clone());
            }
            (
                MessageType::Image,
                format!("[图片] image_key: {}", pic.pic_url),
            )
        } else {
            info!("🖼️ 图片消息 from={} (无图片 URL)", from_id);
            (
                MessageType::Image,
                "[图片] 说明：由于 iLink 协议限制，Bot 无法直接查看图片内容。"
                    .to_string(),
            )
        }
    }

    /// 处理语音消息
    async fn process_voice_message(
        &self,
        msg: &WeChatMessage,
        from_id: &str,
        metadata: &mut HashMap<String, String>,
    ) -> (MessageType, String) {
        if let Some(voice) = msg.voice() {
            info!("🎤 语音消息 from={}, url={}", from_id, voice.voice_url);
            metadata.insert("voice_url".to_string(), voice.voice_url.clone());
            metadata.insert("voice_duration".to_string(), voice.voice_duration.to_string());
            metadata.insert("voice_size".to_string(), voice.voice_size.to_string());
            metadata.insert("voice_name".to_string(), voice.file_name.clone());

            // ASR 转写文本
            if let Some(text_item) = &voice.text_item {
                metadata.insert("asr_text".to_string(), text_item.text.clone());
            }

            (
                MessageType::Voice,
                format!(
                    "[语音 {}秒] {} {}",
                    voice.voice_duration,
                    voice.file_name,
                    voice
                        .text_item
                        .as_ref()
                        .map(|t| format!("(转写: {})", t.text))
                        .unwrap_or_default()
                ),
            )
        } else {
            info!("🎤 语音消息 from={} (无语音 URL)", from_id);
            (
                MessageType::Voice,
                "[语音] 说明：由于 iLink 协议限制，Bot 无法收听语音消息。"
                    .to_string(),
            )
        }
    }

    /// 处理视频消息
    async fn process_video_message(
        &self,
        msg: &WeChatMessage,
        from_id: &str,
        metadata: &mut HashMap<String, String>,
    ) -> (MessageType, String) {
        if let Some(video) = msg.video() {
            info!("🎬 视频消息 from={}, url={}", from_id, video.video_url);
            metadata.insert("video_url".to_string(), video.video_url.clone());
            metadata.insert("video_duration".to_string(), video.video_duration.to_string());
            metadata.insert("video_size".to_string(), video.video_size.to_string());
            metadata.insert("video_name".to_string(), video.file_name.clone());
            if let Some(thumb) = &video.thumb_url {
                metadata.insert("video_thumb".to_string(), thumb.clone());
            }
            (
                MessageType::Video,
                format!("[视频 {}秒] {}", video.video_duration, video.file_name),
            )
        } else {
            info!("🎬 视频消息 from={} (无视频 URL)", from_id);
            (
                MessageType::Video,
                "[视频] 说明：由于 iLink 协议限制，Bot 无法查看视频内容。".to_string(),
            )
        }
    }

    /// 处理文件消息
    async fn process_file_message(
        &self,
        msg: &WeChatMessage,
        from_id: &str,
        metadata: &mut HashMap<String, String>,
    ) -> (MessageType, String) {
        if let Some(file) = msg.file() {
            info!("📎 文件消息 from={}, name={}", from_id, file.file_name);
            metadata.insert("file_name".to_string(), file.file_name.clone());
            if let Some(media) = &file.media {
                metadata.insert("encrypt_query_param".to_string(), media.encrypt_query_param.clone());
                metadata.insert("aes_key".to_string(), media.aes_key.clone());
                metadata.insert("encrypt_type".to_string(), media.encrypt_type.to_string());
            }
            (MessageType::File, format!("[文件] {}", file.file_name))
        } else {
            info!("📎 文件消息 from={}", from_id);
            (MessageType::File, "[文件]".to_string())
        }
    }

    // ---------- 长轮询 ----------

    /// 消息长轮询循环
    async fn poll_messages(&self, event_sender: mpsc::Sender<ChannelEvent>) -> Result<()> {
        // 确保只有一个 poll_messages 实例在运行
        if self
            .is_polling
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            warn!("消息轮询已在运行，跳过重复启动");
            return Ok(());
        }

        // 确保 ilink_client 已启动
        {
            let mut client = self.ilink_client.write().await;
            if client.bot_token.is_empty() {
                let session = self
                    .session
                    .read()
                    .await
                    .as_ref()
                    .cloned()
                    .ok_or_else(|| AgentError::platform("Bot session 未初始化"))?;
                client.bot_token = session.bot_token.clone();
                client.base_url = session.base_url.clone();
            }
            if client.start().await.is_err() {
                warn!("ILinkClient 已经启动");
            }
        }

        let mut updates_buf = String::new();
        info!("开始 iLink 消息长轮询...");

        while *self.connected.read().await {
            let client = self.ilink_client.read().await;
            let result = client.get_updates(&updates_buf).await;
            drop(client);

            match result {
                Ok(updates) => {
                    // 更新 cursor
                    if let Some(new_buf) = updates.get_updates_buf {
                        updates_buf = new_buf;
                    }

                    // 处理消息
                    if let Some(msgs) = updates.msgs {
                        for msg in msgs {
                            // 跳过系统消息（type 10）和未知类型
                            if msg.message_type == 10 || msg.message_type < 1 || msg.message_type > 9
                            {
                                debug!("跳过系统消息或未知类型: type={}", msg.message_type);
                                continue;
                            }

                            if let Err(e) = self.process_message(msg, &event_sender).await {
                                error!("处理消息失败: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    if error_msg.contains("expired") || error_msg.contains("401") {
                        error!("iLink 会话已过期，需要重新登录");
                        *self.connected.write().await = false;
                        break;
                    }
                    warn!("长轮询错误: {}，{}秒后重试...", e, POLL_RETRY_SECS);
                    tokio::time::sleep(Duration::from_secs(POLL_RETRY_SECS)).await;
                }
            }
        }

        info!("消息轮询已停止");
        self.is_polling.store(false, Ordering::SeqCst);
        Ok(())
    }

    // ---------- 发送消息 ----------

    /// 内部发送消息（支持文本分段）
    async fn send_message_internal(&self, to_user_id: &str, text: &str) -> Result<()> {
        let session = self
            .session
            .read()
            .await
            .as_ref()
            .cloned()
            .ok_or_else(|| AgentError::platform("未登录"))?;

        // 确保 ilink_client 有正确的 token
        {
            let mut client = self.ilink_client.write().await;
            client.bot_token = session.bot_token.clone();
            client.base_url = session.base_url.clone();
        }

        // 获取 typing ticket
        let typing_ticket = self.get_typing_ticket(to_user_id).await?;

        // 发送 typing 状态
        {
            let client = self.ilink_client.read().await;
            if let Err(e) = client.sendtyping(to_user_id, &typing_ticket, 1).await {
                warn!("发送 typing 状态失败: {}", e);
            }
        }

        // 获取 context_token
        let context_token = self
            .last_contact
            .read()
            .await
            .as_ref()
            .filter(|(id, _)| id == to_user_id)
            .map(|(_, ctx)| ctx.clone())
            .unwrap_or_default();

        // 智能分段发送
        let segments = split_text_semantic(text, MAX_TEXT_LENGTH);
        for (i, segment) in segments.iter().enumerate() {
            if i > 0 {
                // 分段之间稍作延迟
                tokio::time::sleep(Duration::from_millis(500)).await;
            }

            let client = self.ilink_client.read().await;
            if let Err(e) = client.send_text(to_user_id, segment, &context_token).await {
                warn!("发送消息分段 {} 失败: {}", i + 1, e);
            }
            drop(client);
        }

        // 停止 typing 状态
        {
            let client = self.ilink_client.read().await;
            if let Err(e) = client.sendtyping(to_user_id, &typing_ticket, 2).await {
                warn!("停止 typing 状态失败: {}", e);
            }
        }

        Ok(())
    }

    /// 获取 typing ticket（带缓存）
    async fn get_typing_ticket(&self, user_id: &str) -> Result<String> {
        // 检查缓存
        {
            let tickets = self.typing_tickets.read().await;
            if let Some(info) = tickets.get(user_id) {
                if info.is_valid() {
                    return Ok(info.ticket.clone());
                }
            }
        }

        // 获取新 ticket
        let context_token = self
            .last_contact
            .read()
            .await
            .as_ref()
            .filter(|(id, _)| id == user_id)
            .map(|(_, ctx)| ctx.clone())
            .unwrap_or_default();

        let client = self.ilink_client.read().await;
        let config_resp = client.getconfig(user_id, &context_token).await?;
        drop(client);

        let ticket = config_resp
            .typing_ticket
            .ok_or_else(|| AgentError::platform("无法获取 typing_ticket"))?;

        // 缓存 ticket
        let info = TypingTicketInfo {
            ticket: ticket.clone(),
            cached_at: Instant::now(),
        };
        self.typing_tickets
            .write()
            .await
            .insert(user_id.to_string(), info);

        Ok(ticket)
    }

    // ---------- 重连监控 ----------

    /// 启动重连监控任务
    fn start_reconnect_monitor(&self) {
        if !self.config.auto_reconnect {
            return;
        }

        let channel = self.clone();
        let handle = tokio::spawn(async move {
            let mut check_interval =
                interval(Duration::from_secs(channel.config.reconnect_interval_secs));

            loop {
                check_interval.tick().await;

                let should_warn = {
                    if let Some(ref session) = *channel.session.read().await {
                        let remaining = session.remaining_secs();
                        remaining < channel.config.warning_before_secs
                            && remaining > channel.config.force_before_secs
                    } else {
                        false
                    }
                };

                if should_warn && !*channel.reconnect_pending.read().await {
                    if let Some((user_id, _)) = channel.last_contact.read().await.clone() {
                        let remaining_text = channel
                            .session
                            .read()
                            .await
                            .as_ref()
                            .map(|s| s.remaining_text())
                            .unwrap_or_default();

                        warn!(
                            "个人微信会话即将过期 (剩余 {})，发送提醒消息...",
                            remaining_text
                        );

                        let warning_msg = format!(
                            "[提醒] 微信 Bot 连接还剩 {}，即将需要重新扫码登录。\n回复 Y 立即重连。",
                            remaining_text
                        );

                        if let Err(e) = channel.send_message_internal(&user_id, &warning_msg).await {
                            error!("发送重连提醒失败: {}", e);
                        } else {
                            *channel.reconnect_pending.write().await = true;
                        }
                    }
                }

                // 检查是否需要强制重连
                let should_force = {
                    if let Some(ref session) = *channel.session.read().await {
                        session.remaining_secs() < channel.config.force_before_secs
                    } else {
                        false
                    }
                };

                if should_force {
                    error!("个人微信会话即将过期，强制断开连接!");
                    *channel.connected.write().await = false;
                    break;
                }
            }
        });

        let reconnect_handle = self.reconnect_handle.clone();
        tokio::spawn(async move {
            *reconnect_handle.write().await = Some(handle);
        });
    }

    // ---------- 健康检查 ----------

    /// 启动健康检查任务
    fn start_health_check(&self) {
        let channel = self.clone();
        let handle = tokio::spawn(async move {
            let mut check_interval = interval(Duration::from_secs(HEALTH_CHECK_INTERVAL_SECS));

            loop {
                check_interval.tick().await;

                let is_valid = {
                    if let Some(ref session) = *channel.session.read().await {
                        session.is_valid()
                    } else {
                        false
                    }
                };

                if !is_valid {
                    warn!("个人微信会话已过期，标记为断开");
                    *channel.connected.write().await = false;
                    break;
                }

                debug!("个人微信健康检查通过");
            }
        });

        let health_handle = self.health_handle.clone();
        tokio::spawn(async move {
            *health_handle.write().await = Some(handle);
        });
    }

    // ---------- 公共方法 ----------

    /// 获取当前 session 信息
    pub async fn get_session_info(&self) -> Option<BotSession> {
        self.session.read().await.clone()
    }

    /// 检查 session 是否有效
    pub async fn is_session_valid(&self) -> bool {
        if let Some(ref session) = *self.session.read().await {
            session.is_valid()
        } else {
            false
        }
    }

    /// 获取 QR 码 URL
    pub async fn get_qr_url(&self) -> Option<String> {
        let info = self.qr_login_info.read().await;
        info.as_ref()
            .and_then(|i| i.qrcode_url.clone())
            .or_else(|| info.as_ref().map(|i| i.qrcode.clone()))
    }

    /// 使用已有 token 完成登录
    pub async fn complete_login(
        &self,
        bot_token: String,
        base_url: String,
        event_bus: mpsc::Sender<ChannelEvent>,
    ) -> Result<()> {
        // 更新 ilink_client
        {
            let mut client = self.ilink_client.write().await;
            client.bot_token = bot_token.clone();
            client.base_url = base_url.clone();
            client.start().await?;
        }

        let session = BotSession {
            bot_token,
            base_url,
            login_time: Instant::now(),
            wxid: None,
            nickname: None,
        };

        *self.session.write().await = Some(session);
        *self.connected.write().await = true;
        self.save_session().await;

        info!("========================================");
        info!("个人微信登录成功!");
        info!("========================================");

        self.start_reconnect_monitor();
        self.start_health_check();

        info!("🎧 启动个人微信消息监听...");
        if let Err(e) = self.start_listener(event_bus).await {
            error!("❌ 启动个人微信消息监听失败: {}", e);
            return Err(e);
        }

        info!("✅ 个人微信消息监听已启动");
        Ok(())
    }

    /// 下载媒体文件（含 AES 解密）
    async fn download_media_internal(
        &self,
        url: &str,
        aes_key: &str,
        encrypt_query_param: &str,
    ) -> Result<Vec<u8>> {
        let client = self.ilink_client.read().await;
        let result = client
            .download_media(url, aes_key, encrypt_query_param)
            .await;
        drop(client);
        result
    }
}

// ==================== Channel Trait 实现 ====================

#[async_trait]
impl Channel for PersonalWeChatChannel {
    fn name(&self) -> &str {
        "personal_wechat"
    }

    fn platform(&self) -> PlatformType {
        PlatformType::WeChat
    }

    fn is_connected(&self) -> bool {
        self.connected.try_read().map(|g| *g).unwrap_or(false)
    }

    async fn connect(&mut self) -> Result<()> {
        info!("🔌 PersonalWeChatChannel::connect() 被调用");

        // 确保 ILinkClient 已启动（创建 HTTP client）
        {
            let mut client = self.ilink_client.write().await;
            if client.start().await.is_err() {
                warn!("ILinkClient 已经启动");
            }
        }

        // 检查是否已连接
        let already_connected = *self.connected.read().await;
        let session_valid = self.is_session_valid().await;

        if already_connected && session_valid {
            info!("个人微信已连接且会话有效");
            return Ok(());
        }

        // 尝试恢复持久化 session
        if !already_connected && !session_valid {
            if let Some(session) = self.load_session().await {
                info!("从持久化存储恢复个人微信 session");
                // 更新 ilink_client
                {
                    let mut client = self.ilink_client.write().await;
                    client.bot_token = session.bot_token.clone();
                    client.base_url = session.base_url.clone();
                }
                *self.session.write().await = Some(session);
                *self.connected.write().await = true;
                self.start_reconnect_monitor();
                self.start_health_check();
                info!("使用持久化 session 连接到个人微信");
                return Ok(());
            }
        }

        // 使用配置中的 token
        if let Some(ref token) = self.config.bot_token {
            if !token.is_empty() {
                let base_url = self
                    .config
                    .bot_base_url
                    .clone()
                    .unwrap_or_else(|| self.config.base_url.clone());

                // 更新 ilink_client
                {
                    let mut client = self.ilink_client.write().await;
                    client.bot_token = token.clone();
                    client.base_url = base_url.clone();
                }

                let session = BotSession {
                    bot_token: token.clone(),
                    base_url,
                    login_time: Instant::now(),
                    wxid: None,
                    nickname: None,
                };

                *self.session.write().await = Some(session);
                *self.connected.write().await = true;
                self.save_session().await;
                self.start_reconnect_monitor();
                self.start_health_check();

                info!("使用现有 bot_token 连接到个人微信");
                return Ok(());
            }
        }

        // 需要 QR 登录
        info!("🔄 开始获取 QR 码...");
        let qr_resp = match self.get_qr_code().await {
            Ok(resp) => resp,
            Err(e) => {
                error!("❌ 获取 QR 码失败: {}", e);
                return Err(e);
            }
        };

        info!("========================================");
        info!("个人微信登录");
        info!("========================================");
        info!(
            "请使用微信扫描以下二维码或打开链接: {}",
            qr_resp.qrcode_img_content.as_deref().unwrap_or(&qr_resp.qrcode)
        );
        info!("QR Code: {}", qr_resp.qrcode);
        info!("========================================");

        info!("🕐 等待用户扫码（不阻塞 Gateway 启动）...");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        // 停止监听器
        if let Some(handle) = self.listener_handle.write().await.take() {
            handle.abort();
        }
        // 停止重连监控
        if let Some(handle) = self.reconnect_handle.write().await.take() {
            handle.abort();
        }
        // 停止健康检查
        if let Some(handle) = self.health_handle.write().await.take() {
            handle.abort();
        }
        // 刷新合并缓冲区
        {
            let mut buffer = self.merge_buffer.write().await;
            buffer.clear();
        }

        *self.connected.write().await = false;
        info!("个人微信已断开连接");
        Ok(())
    }

    async fn send(&self, channel_id: &str, message: &Message) -> Result<()> {
        if !self.is_session_valid().await {
            return Err(AgentError::platform("会话已过期，请重新登录").into());
        }

        let text = match message.message_type {
            MessageType::Text => message.content.clone(),
            _ => {
                warn!("个人微信仅支持文本消息，已转换");
                message.content.clone()
            }
        };

        self.send_message_internal(channel_id, &text).await
    }

    async fn start_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        self.stop_listener().await?;

        // 保存 event_bus 供后续使用（如 get_qr_code 后重启轮询）
        *self.event_sender.write().await = Some(event_bus.clone());

        // 如果有 QR 登录信息，后台轮询扫码状态
        if self.qr_login_info.read().await.is_some() && !*self.connected.read().await {
            let channel = self.clone();
            let handle = tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    match channel.check_qr_status().await {
                        Ok(true) => {
                            info!("✅ 用户扫码成功，启动个人微信消息监听");
                            if let Err(e) = channel.start_listener(event_bus).await {
                                error!("启动个人微信消息监听失败: {}", e);
                            }
                            break;
                        }
                        Ok(false) => {}
                        Err(e) => {
                            let err_str = e.to_string();
                            // 网络超时属于临时错误，继续轮询
                            if err_str.contains("timed out") || err_str.contains("timeout") {
                                warn!("QR 状态检查超时，继续轮询...");
                                continue;
                            }
                            error!("QR 状态检查失败: {}", e);
                            break;
                        }
                    }
                }
            });
            *self.listener_handle.write().await = Some(handle);
            info!("个人微信 QR 状态轮询已启动");
            return Ok(());
        }

        if !*self.connected.read().await {
            return Err(AgentError::platform("未连接").into());
        }

        let channel = self.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = channel.poll_messages(event_bus).await {
                error!("消息轮询错误: {}", e);
            }
        });

        *self.listener_handle.write().await = Some(handle);
        info!("个人微信消息监听已启动");
        Ok(())
    }

    async fn stop_listener(&self) -> Result<()> {
        if let Some(handle) = self.listener_handle.write().await.take() {
            handle.abort();
            // 等待旧轮询循环完全停止（最多 2 秒）
            for _ in 0..20 {
                if !self.is_polling.load(Ordering::SeqCst) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            info!("消息监听已停止");
        }
        Ok(())
    }

    fn supported_content_types(&self) -> Vec<ContentType> {
        vec![
            ContentType::Text,
            ContentType::Image,
            ContentType::Audio,
            ContentType::Video,
            ContentType::File,
        ]
    }

    async fn download_image(
        &self,
        file_key: &str,
        _message_id: Option<&str>,
    ) -> Result<Vec<u8>> {
        info!("🖼️ 下载图片: {}", file_key);

        // file_key 是图片 URL，尝试直接下载
        let session = self.session.read().await.clone();
        if session.is_none() {
            return Err(AgentError::platform("未登录，无法下载图片").into());
        }

        // 从 metadata 中获取 aes_key（如果可用）
        // 这里简化处理，直接下载不解密
        let client = self.ilink_client.read().await;
        let result = client.download_media(file_key, "", "").await;
        drop(client);

        match result {
            Ok(data) => {
                info!("✅ 图片下载成功: {} bytes", data.len());
                Ok(data)
            }
            Err(e) => {
                error!("❌ 图片下载失败: {}", e);
                Err(e)
            }
        }
    }

    async fn list_channels(&self) -> Result<Vec<ChannelInfo>> {
        Ok(vec![])
    }

    async fn list_members(&self, _channel_id: &str) -> Result<Vec<MemberInfo>> {
        Ok(vec![])
    }

    fn connection_mode(&self) -> ConnectionMode {
        ConnectionMode::Polling
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// ==================== Clone 实现 ====================

impl Clone for PersonalWeChatChannel {
    fn clone(&self) -> Self {
        let ilink_config = ILinkConfig {
            base_url: self.config.base_url.clone(),
            ..Default::default()
        };

        Self {
            config: self.config.clone(),
            ilink_client: Arc::new(RwLock::new(ILinkClient::new(Some(ilink_config)))),
            connected: self.connected.clone(),
            session: self.session.clone(),
            qr_login_info: self.qr_login_info.clone(),
            typing_tickets: self.typing_tickets.clone(),
            last_contact: self.last_contact.clone(),
            reconnect_pending: self.reconnect_pending.clone(),
            listener_handle: Arc::new(RwLock::new(None)),
            reconnect_handle: Arc::new(RwLock::new(None)),
            health_handle: Arc::new(RwLock::new(None)),
            event_sender: self.event_sender.clone(),
            welcomed_users: self.welcomed_users.clone(),
            session_store_path: self.session_store_path.clone(),
            dedup_queue: self.dedup_queue.clone(),
            merge_buffer: self.merge_buffer.clone(),
            is_polling: self.is_polling.clone(),
        }
    }
}

// ==================== 工具函数 ====================

/// 语义化文本分段
///
/// 按照语义边界（段落、句子）将长文本切分为不超过 max_len 的段。
fn split_text_semantic(text: &str, max_len: usize) -> Vec<String> {
    if text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut result = Vec::new();
    let mut current = String::with_capacity(max_len);

    // 按段落分割
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            continue;
        }

        // 如果当前段落本身超过限制，按句子分割
        if paragraph.len() > max_len {
            // 先刷新当前缓冲区
            if !current.is_empty() {
                result.push(current.clone());
                current.clear();
            }

            // 按句子分割段落
            let sentences: Vec<&str> = paragraph.split(|c| c == '。' || c == '！' || c == '？').collect();
            for sentence in sentences {
                let sentence = sentence.trim();
                if sentence.is_empty() {
                    continue;
                }
                let sentence_with_punct = format!("{}。", sentence);

                if current.len() + sentence_with_punct.len() > max_len {
                    if !current.is_empty() {
                        result.push(current.clone());
                        current.clear();
                    }
                    // 如果单句就超过限制，强制截断
                    if sentence_with_punct.len() > max_len {
                        let mut remaining = sentence_with_punct.as_str();
                        while !remaining.is_empty() {
                            let split_at = remaining.chars().take(max_len).count();
                            let (chunk, rest) = remaining.split_at(
                                remaining.char_indices().nth(split_at).map(|(i, _)| i).unwrap_or(remaining.len())
                            );
                            result.push(chunk.to_string());
                            remaining = rest;
                        }
                    } else {
                        current.push_str(&sentence_with_punct);
                    }
                } else {
                    current.push_str(&sentence_with_punct);
                }
            }
        } else if current.len() + paragraph.len() + 1 > max_len {
            // 当前缓冲区放不下这个段落
            if !current.is_empty() {
                result.push(current.clone());
                current.clear();
            }
            current.push_str(paragraph);
            current.push('\n');
        } else {
            current.push_str(paragraph);
            current.push('\n');
        }
    }

    // 刷新最后的缓冲区
    if !current.is_empty() {
        result.push(current.trim_end().to_string());
    }

    if result.is_empty() {
        result.push(text.to_string());
    }

    result
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = PersonalWeChatConfig::default();
        assert_eq!(config.base_url, "https://ilinkai.weixin.qq.com");
        assert!(config.auto_reconnect);
        assert_eq!(config.reconnect_interval_secs, 300);
        assert_eq!(config.dm_allowlist_policy, AllowlistPolicy::Open);
        assert_eq!(config.group_allowlist_policy, AllowlistPolicy::Open);
    }

    #[test]
    fn test_split_text_semantic_short() {
        let text = "这是一段短文本。";
        let segments = split_text_semantic(text, 100);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0], text);
    }

    #[test]
    fn test_split_text_semantic_long() {
        let text = "这是第一段。这是第二段。这是第三段。\n这是第四段，比较长，需要测试分段功能。";
        let segments = split_text_semantic(text, 20);
        assert!(!segments.is_empty());
        for seg in &segments {
            assert!(seg.len() <= 20, "分段超过最大长度: {}", seg.len());
        }
    }

    #[test]
    fn test_allowlist_policy_serde() {
        let open: AllowlistPolicy = serde_json::from_str("\"open\"").unwrap();
        assert_eq!(open, AllowlistPolicy::Open);
        let allowlist: AllowlistPolicy = serde_json::from_str("\"allowlist\"").unwrap();
        assert_eq!(allowlist, AllowlistPolicy::Allowlist);
    }

    #[test]
    fn test_parse_session_id() {
        let config = PersonalWeChatConfig::default();
        let channel = PersonalWeChatChannel::new(config);
        assert_eq!(channel.parse_session_id("wxid_abc123"), "wxid_abc123");
        assert_eq!(channel.parse_session_id("abc123"), "abc123");
    }

    #[test]
    fn test_typing_ticket_validity() {
        let info = TypingTicketInfo {
            ticket: "test".to_string(),
            cached_at: Instant::now(),
        };
        assert!(info.is_valid());
    }
}
