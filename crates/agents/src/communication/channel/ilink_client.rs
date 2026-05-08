//! iLink Protocol Client for WeChat Personal Account (QwenPaw 一比一复刻)
//!
//! Direct implementation of Tencent's iLink Bot API for WeChat personal
//! accounts. Based on the official OpenClaw/iLink protocol.
//!
//! API Base: https://ilinkai.weixin.qq.com
//! Protocol: HTTP/JSON, channel_version = "2.0.1"

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::error::{AgentError, Result};

/// iLink API base URL
const ILINK_API_BASE: &str = "https://ilinkai.weixin.qq.com";
/// CDN base URL for media download/upload
const CDN_BASE: &str = "https://novac2c.cdn.weixin.qq.com/c2c";
/// Default session duration (24 hours)
pub const SESSION_DURATION_SECS: u64 = 24 * 3600;
/// Long-polling timeout (35 seconds, matches server behavior)
pub const POLLING_TIMEOUT_SECS: u64 = 35;
/// Request timeout for long-polling
pub const REQUEST_TIMEOUT_SECS: u64 = 45;
/// QR status poll timeout (must be < web server proxy timeout of 30s)
pub const QRCODE_STATUS_TIMEOUT_SECS: u64 = 25;
/// Default API request timeout
pub const DEFAULT_TIMEOUT_SECS: u64 = 15;
/// Channel version
const CHANNEL_VERSION: &str = "2.0.1";

// ==================== AES-128-ECB 工具 (复刻 QwenPaw utils.py) ====================

/// AES-128-ECB 解密 + PKCS7 unpadding
///
/// 支持三种 key 格式（与 QwenPaw 的 `parseAesKey` 对齐）：
/// - 32/48/64 字符 hex 字符串（如 image_item.aeskey）
/// - base64(16 raw bytes)（标准格式，用于图片）
/// - base64(32-char hex string)（用于文件/语音/视频）
pub fn aes_ecb_decrypt(data: &[u8], key_b64: &str) -> Result<Vec<u8>> {
    use aes::cipher::{BlockDecrypt, KeyInit};
    use aes::Aes128;

    let key = parse_aes_key(key_b64)?;
    let cipher = Aes128::new_from_slice(&key)
        .map_err(|e| AgentError::platform(format!("AES key init failed: {:?}", e)))?;

    let block_size = 16;
    if data.len() % block_size != 0 {
        return Err(AgentError::platform(format!(
            "AES decrypt: data length {} is not a multiple of block size {}",
            data.len(),
            block_size
        )));
    }

    let mut decrypted = vec![0u8; data.len()];
    for (src, dst) in data.chunks_exact(block_size).zip(decrypted.chunks_exact_mut(block_size)) {
        let mut block = *aes::cipher::generic_array::GenericArray::from_slice(src);
        cipher.decrypt_block(&mut block);
        dst.copy_from_slice(&block);
    }

    // PKCS7 unpadding
    if decrypted.is_empty() {
        return Ok(decrypted);
    }
    let pad_len = decrypted[decrypted.len() - 1] as usize;
    if pad_len == 0 || pad_len > block_size {
        return Err(AgentError::platform(format!(
            "AES decrypt: invalid PKCS7 padding length {}",
            pad_len
        )));
    }
    // Validate padding
    for i in 1..=pad_len {
        if decrypted[decrypted.len() - i] != pad_len as u8 {
            return Err(AgentError::platform(
                "AES decrypt: invalid PKCS7 padding".to_string(),
            ));
        }
    }
    decrypted.truncate(decrypted.len() - pad_len);
    Ok(decrypted)
}

/// AES-128-ECB 加密 + PKCS7 padding
///
/// key_b64: base64-encoded 16-byte AES key
pub fn aes_ecb_encrypt(data: &[u8], key_b64: &str) -> Result<Vec<u8>> {
    use aes::cipher::{BlockEncrypt, KeyInit};
    use aes::Aes128;

    let key = base64::decode(key_b64)
        .map_err(|e| AgentError::platform(format!("Invalid base64 AES key: {}", e)))?;
    if key.len() != 16 {
        return Err(AgentError::platform(format!(
            "AES key must be 16 bytes, got {}",
            key.len()
        )));
    }

    let cipher = Aes128::new_from_slice(&key)
        .map_err(|e| AgentError::platform(format!("AES key init failed: {:?}", e)))?;

    let block_size = 16;
    let pad_len = block_size - (data.len() % block_size);
    let mut padded = Vec::with_capacity(data.len() + pad_len);
    padded.extend_from_slice(data);
    padded.extend(std::iter::repeat(pad_len as u8).take(pad_len));

    let mut encrypted = vec![0u8; padded.len()];
    for (src, dst) in padded.chunks_exact(block_size).zip(encrypted.chunks_exact_mut(block_size)) {
        let mut block = *aes::cipher::generic_array::GenericArray::from_slice(src);
        cipher.encrypt_block(&mut block);
        dst.copy_from_slice(&block);
    }

    Ok(encrypted)
}

/// 生成随机 16-byte AES key (base64-encoded)
pub fn generate_aes_key_b64() -> String {
    let key: [u8; 16] = rand::random();
    base64::encode(key)
}

/// 解析 AES key（复刻 QwenPaw parseAesKey 逻辑）
fn parse_aes_key(key_b64: &str) -> Result<Vec<u8>> {
    let raw = key_b64.trim();

    // 格式1: 32/48/64 字符 hex 字符串
    let hex_lens = [32usize, 48, 64];
    if hex_lens.contains(&raw.len()) && raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return hex::decode(raw)
            .map_err(|e| AgentError::platform(format!("Hex decode failed: {}", e)));
    }

    // 格式2: base64 编码
    let decoded = match base64::decode(raw) {
        Ok(d) => d,
        Err(_) => base64::decode(&(raw.to_string() + "=="))
            .map_err(|e| AgentError::platform(format!("Base64 decode failed: {}", e)))?,
    };

    if decoded.len() == 16 {
        // 格式 A: base64(raw 16 bytes) — 用于图片
        Ok(decoded)
    } else if decoded.len() == 32 && decoded.iter().all(|b| b.is_ascii_hexdigit()) {
        // 格式 B: base64(hex string) — 用于文件/语音/视频
        let hex_str = String::from_utf8(decoded)
            .map_err(|e| AgentError::platform(format!("Invalid hex bytes: {}", e)))?;
        hex::decode(&hex_str)
            .map_err(|e| AgentError::platform(format!("Hex decode failed: {}", e)))
    } else {
        // 其他: 直接当作 raw key
        if ![16usize, 24, 32].contains(&decoded.len()) {
            return Err(AgentError::platform(format!(
                "Invalid AES key length: {} (from key_b64={:?})",
                decoded.len(),
                &raw[..raw.len().min(20)]
            )));
        }
        Ok(decoded)
    }
}

// ==================== 数据模型 ====================

/// iLink client configuration
#[derive(Debug, Clone)]
pub struct ILinkConfig {
    pub base_url: String,
    pub timeout_secs: u64,
    pub max_retries: u32,
}

impl Default for ILinkConfig {
    fn default() -> Self {
        Self {
            base_url: ILINK_API_BASE.to_string(),
            timeout_secs: REQUEST_TIMEOUT_SECS,
            max_retries: 3,
        }
    }
}

/// Bot session info
#[derive(Debug, Clone)]
pub struct BotSession {
    pub bot_token: String,
    pub base_url: String,
    pub login_time: std::time::Instant,
    pub wxid: Option<String>,
    pub nickname: Option<String>,
}

impl BotSession {
    /// Check if session is still valid (not expired)
    pub fn is_valid(&self) -> bool {
        let elapsed = self.login_time.elapsed().as_secs();
        elapsed < SESSION_DURATION_SECS
    }

    /// Get remaining seconds before expiration
    pub fn remaining_secs(&self) -> u64 {
        let elapsed = self.login_time.elapsed().as_secs();
        if elapsed >= SESSION_DURATION_SECS {
            0
        } else {
            SESSION_DURATION_SECS - elapsed
        }
    }

    /// Format remaining time as human-readable string
    pub fn remaining_text(&self) -> String {
        let secs = self.remaining_secs();
        let hours = secs / 3600;
        let minutes = (secs % 3600) / 60;
        let seconds = secs % 60;

        if hours > 0 {
            format!("{} 小时 {} 分钟", hours, minutes)
        } else if minutes > 0 {
            format!("{} 分钟 {} 秒", minutes, seconds)
        } else {
            format!("{} 秒", seconds)
        }
    }
}

/// QR Code response from get_bot_qrcode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrCodeResponse {
    pub qrcode: String,
    #[serde(rename = "qrcode_img_content")]
    pub qrcode_img_content: Option<String>,
}

/// QR Code status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrCodeStatusResponse {
    pub status: String,
    #[serde(rename = "bot_token")]
    pub bot_token: Option<String>,
    #[serde(rename = "baseurl")]
    pub base_url: Option<String>,
}

/// Message from WeChat (inbound)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeChatMessage {
    #[serde(rename = "message_id")]
    pub message_id: Option<i64>,
    #[serde(rename = "from_user_id")]
    pub from_user_id: String,
    #[serde(rename = "to_user_id")]
    pub to_user_id: String,
    #[serde(rename = "message_type")]
    pub message_type: i32,
    #[serde(rename = "message_state")]
    pub message_state: i32,
    #[serde(rename = "context_token")]
    pub context_token: String,
    #[serde(rename = "group_id", default)]
    pub group_id: String,
    #[serde(rename = "item_list")]
    pub item_list: Vec<MessageItem>,
    #[serde(rename = "seq", default)]
    pub seq: Option<i64>,
}

impl WeChatMessage {
    /// Get text content from message
    pub fn text(&self) -> Option<String> {
        for item in &self.item_list {
            if item.item_type == 1 {
                return item.text_item.as_ref().map(|t| t.text.clone());
            }
        }
        None
    }

    /// Get picture content from message
    pub fn picture(&self) -> Option<&PicItem> {
        for item in &self.item_list {
            if item.item_type == 2 {
                return item.pic_item.as_ref();
            }
        }
        None
    }

    /// Get voice content from message
    pub fn voice(&self) -> Option<&VoiceItem> {
        for item in &self.item_list {
            if item.item_type == 3 {
                return item.voice_item.as_ref();
            }
        }
        None
    }

    /// Get video content from message
    pub fn video(&self) -> Option<&VideoItem> {
        for item in &self.item_list {
            if item.item_type == 4 {
                return item.video_item.as_ref();
            }
        }
        None
    }

    /// Get file content from message
    pub fn file(&self) -> Option<&FileItem> {
        for item in &self.item_list {
            if item.item_type == 4 {
                return item.file_item.as_ref();
            }
        }
        None
    }

    /// Get message type name
    pub fn message_type_name(&self) -> &'static str {
        match self.message_type {
            1 => "text",
            2 => "image",
            3 => "voice",
            4 => "video",
            5 => "location",
            6 => "link",
            7 => "business_card",
            8 => "file",
            9 => "quote",
            10 => "system",
            _ => "unknown",
        }
    }
}

/// Message item in item_list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageItem {
    #[serde(rename = "type")]
    pub item_type: i32,
    #[serde(rename = "text_item")]
    pub text_item: Option<TextItem>,
    #[serde(rename = "pic_item")]
    pub pic_item: Option<PicItem>,
    #[serde(rename = "voice_item")]
    pub voice_item: Option<VoiceItem>,
    #[serde(rename = "video_item")]
    pub video_item: Option<VideoItem>,
    #[serde(rename = "file_item")]
    pub file_item: Option<FileItem>,
    /// Quoted (replied-to) message
    #[serde(rename = "ref_msg", default, skip_serializing_if = "Option::is_none")]
    pub ref_msg: Option<Box<RefMsg>>,
}

/// Text item content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextItem {
    pub text: String,
}

/// Picture item content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PicItem {
    #[serde(rename = "file_name")]
    pub file_name: String,
    #[serde(rename = "pic_url")]
    pub pic_url: String,
    #[serde(rename = "thumb_url")]
    pub thumb_url: Option<String>,
    #[serde(rename = "pic_size")]
    pub pic_size: i32,
    #[serde(rename = "pic_width")]
    pub pic_width: i32,
    #[serde(rename = "pic_height")]
    pub pic_height: i32,
    /// Hex-encoded AES key (32 chars = 16 bytes)
    #[serde(rename = "aeskey", default)]
    pub aeskey: String,
}

/// Voice item content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceItem {
    #[serde(rename = "file_name")]
    pub file_name: String,
    #[serde(rename = "voice_url")]
    pub voice_url: String,
    #[serde(rename = "voice_size")]
    pub voice_size: i32,
    #[serde(rename = "voice_duration")]
    pub voice_duration: i32,
    /// ASR transcription text
    #[serde(rename = "text_item")]
    pub text_item: Option<TextItem>,
}

/// Video item content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoItem {
    #[serde(rename = "file_name")]
    pub file_name: String,
    #[serde(rename = "video_url")]
    pub video_url: String,
    #[serde(rename = "thumb_url")]
    pub thumb_url: Option<String>,
    #[serde(rename = "video_size")]
    pub video_size: i32,
    #[serde(rename = "video_duration")]
    pub video_duration: i32,
}

/// File item content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileItem {
    #[serde(rename = "file_name")]
    pub file_name: String,
    #[serde(rename = "media")]
    pub media: Option<MediaInfo>,
}

/// Media info for CDN download
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaInfo {
    #[serde(rename = "encrypt_query_param")]
    pub encrypt_query_param: String,
    #[serde(rename = "aes_key")]
    pub aes_key: String,
    #[serde(rename = "encrypt_type")]
    pub encrypt_type: i32,
}

/// Quoted message wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefMsg {
    #[serde(rename = "message_item")]
    pub message_item: Option<Box<MessageItem>>,
}

/// GetUpdates response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetUpdatesResponse {
    #[serde(rename = "ret", default)]
    pub ret: i32,
    #[serde(rename = "msgs", default)]
    pub msgs: Option<Vec<WeChatMessage>>,
    #[serde(rename = "get_updates_buf", default)]
    pub get_updates_buf: Option<String>,
    #[serde(rename = "longpolling_timeout_ms", default)]
    pub longpolling_timeout_ms: Option<u64>,
}

/// GetConfig request/response
#[derive(Debug, Clone, Serialize)]
pub struct GetConfigRequest {
    #[serde(rename = "ilink_user_id")]
    pub ilink_user_id: String,
    #[serde(rename = "context_token")]
    pub context_token: String,
    #[serde(rename = "base_info")]
    pub base_info: BaseInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetConfigResponse {
    #[serde(rename = "typing_ticket")]
    pub typing_ticket: Option<String>,
}

/// SendTyping request
#[derive(Debug, Clone, Serialize)]
pub struct SendTypingRequest {
    #[serde(rename = "ilink_user_id")]
    pub ilink_user_id: String,
    #[serde(rename = "typing_ticket")]
    pub typing_ticket: String,
    pub status: i32, // 1 = typing, 2 = stop typing
}

/// SendMessage request
#[derive(Debug, Clone, Serialize)]
pub struct SendMessageRequest {
    pub msg: OutboundMessage,
    #[serde(rename = "base_info")]
    pub base_info: BaseInfo,
}

/// Outbound message structure
#[derive(Debug, Clone, Serialize)]
pub struct OutboundMessage {
    #[serde(rename = "from_user_id")]
    pub from_user_id: String,
    #[serde(rename = "to_user_id")]
    pub to_user_id: String,
    #[serde(rename = "client_id")]
    pub client_id: String,
    #[serde(rename = "message_type")]
    pub message_type: i32,
    #[serde(rename = "message_state")]
    pub message_state: i32,
    #[serde(rename = "context_token")]
    pub context_token: String,
    #[serde(rename = "item_list")]
    pub item_list: Vec<OutboundMessageItem>,
}

/// Outbound message item
#[derive(Debug, Clone, Serialize)]
pub struct OutboundMessageItem {
    #[serde(rename = "type")]
    pub item_type: i32,
    #[serde(rename = "text_item")]
    pub text_item: Option<TextItemContent>,
    #[serde(rename = "image_item")]
    pub image_item: Option<ImageItemContent>,
    #[serde(rename = "file_item")]
    pub file_item: Option<FileItemContent>,
    #[serde(rename = "video_item")]
    pub video_item: Option<VideoItemContent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TextItemContent {
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageItemContent {
    pub media: MediaItem,
    #[serde(rename = "mid_size")]
    pub mid_size: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileItemContent {
    pub media: MediaItem,
    #[serde(rename = "file_name")]
    pub file_name: String,
    pub len: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoItemContent {
    pub media: MediaItem,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaItem {
    #[serde(rename = "encrypt_query_param")]
    pub encrypt_query_param: String,
    #[serde(rename = "aes_key")]
    pub aes_key: String,
    #[serde(rename = "encrypt_type")]
    pub encrypt_type: i32,
}

/// Base info required in all requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseInfo {
    #[serde(rename = "channel_version")]
    pub channel_version: String,
}

impl Default for BaseInfo {
    fn default() -> Self {
        Self {
            channel_version: CHANNEL_VERSION.to_string(),
        }
    }
}

/// Upload URL response
#[derive(Debug, Clone, Deserialize)]
pub struct GetUploadUrlResponse {
    #[serde(rename = "upload_param")]
    pub upload_param: Option<String>,
    #[serde(rename = "upload_full_url")]
    pub upload_full_url: Option<String>,
}

// ==================== iLink HTTP Client ====================

/// Async HTTP client for the WeChat iLink Bot API.
pub struct ILinkClient {
    pub bot_token: String,
    pub base_url: String,
    config: ILinkConfig,
    http_client: Option<reqwest::Client>,
}

impl ILinkClient {
    /// Create a new iLink client (not started yet)
    pub fn new(config: Option<ILinkConfig>) -> Self {
        let config = config.unwrap_or_default();
        Self {
            bot_token: String::new(),
            base_url: config.base_url.clone(),
            config,
            http_client: None,
        }
    }

    // ------------------------------------------------------------------
    // Lifecycle
    // ------------------------------------------------------------------

    /// Create the underlying HTTP client.
    pub async fn start(&mut self) -> Result<()> {
        self.http_client = Some(
            reqwest::Client::builder()
                .timeout(Duration::from_secs(self.config.timeout_secs))
                .connect_timeout(Duration::from_secs(10))
                .pool_max_idle_per_host(10)
                .build()
                .expect("Failed to create HTTP client"),
        );
        Ok(())
    }

    /// Close the underlying HTTP client.
    pub async fn stop(&mut self) {
        self.http_client = None;
    }

    fn client(&self) -> Result<&reqwest::Client> {
        self.http_client
            .as_ref()
            .ok_or_else(|| AgentError::platform("ILinkClient not started").into())
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url.trim_end_matches('/'), path.trim_start_matches('/'))
    }

    fn make_headers(&self, token: Option<&str>) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        headers.insert("AuthorizationType", "ilink_bot_token".parse().unwrap());
        headers.insert(
            "X-WECHAT-UIN",
            generate_uin_header().parse().unwrap(),
        );
        if let Some(t) = token {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", t).parse().unwrap(),
            );
        }
        headers
    }

    async fn get(
        &self,
        path: &str,
        params: Option<&[(&str, &str)]>,
        timeout_secs: u64,
    ) -> Result<serde_json::Value> {
        let client = self.client()?;
        let mut req = client
            .get(self.url(path))
            .headers(self.make_headers(Some(&self.bot_token)));
        if let Some(p) = params {
            req = req.query(p);
        }
        let resp = req
            .timeout(Duration::from_secs(timeout_secs))
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("HTTP GET failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!("HTTP GET {}: {}", status, text)).into());
        }
        resp.json().await
            .map_err(|e| AgentError::platform(format!("JSON parse failed: {}", e)).into())
    }

    async fn post(
        &self,
        path: &str,
        body: impl Serialize,
        timeout_secs: u64,
    ) -> Result<serde_json::Value> {
        let client = self.client()?;
        let resp = client
            .post(self.url(path))
            .headers(self.make_headers(Some(&self.bot_token)))
            .json(&body)
            .timeout(Duration::from_secs(timeout_secs))
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("HTTP POST failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!("HTTP POST {}: {}", status, text)).into());
        }
        resp.json().await
            .map_err(|e| AgentError::platform(format!("JSON parse failed: {}", e)).into())
    }

    // ------------------------------------------------------------------
    // Auth APIs
    // ------------------------------------------------------------------

    /// Fetch login QR code.
    pub async fn get_bot_qrcode(&self) -> Result<QrCodeResponse> {
        let data = self
            .get("ilink/bot/get_bot_qrcode", Some(&[("bot_type", "3")]), DEFAULT_TIMEOUT_SECS)
            .await?;
        let qrcode = data
            .get("qrcode")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentError::platform("Missing qrcode in response"))?
            .to_string();
        let qrcode_img_content = data
            .get("qrcode_img_content")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        Ok(QrCodeResponse { qrcode, qrcode_img_content })
    }

    /// Poll QR code scan status.
    pub async fn get_qrcode_status(&self, qrcode: &str) -> Result<QrCodeStatusResponse> {
        debug!("iLink get_qrcode_status 请求: qrcode={}", qrcode);
        let data = self
            .get(
                "ilink/bot/get_qrcode_status",
                Some(&[("qrcode", qrcode)]),
                QRCODE_STATUS_TIMEOUT_SECS,
            )
            .await?;
        debug!("iLink get_qrcode_status 原始响应: {:?}", data);
        let status = data
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("waiting")
            .to_string();
        let bot_token = data.get("bot_token").and_then(|v| v.as_str()).map(|s| s.to_string());
        let base_url = data.get("baseurl").and_then(|v| v.as_str()).map(|s| s.to_string());
        info!(
            "iLink QR 状态: qrcode={}, status={}, has_bot_token={}, has_base_url={}",
            qrcode,
            status,
            bot_token.is_some(),
            base_url.is_some()
        );
        Ok(QrCodeStatusResponse {
            status,
            bot_token,
            base_url,
        })
    }

    /// Block until QR code is confirmed or timeout.
    pub async fn wait_for_login(
        &self,
        qrcode: &str,
        poll_interval: f64,
        max_wait: f64,
    ) -> Result<(String, String)> {
        let start = std::time::Instant::now();
        while start.elapsed().as_secs_f64() < max_wait {
            match self.get_qrcode_status(qrcode).await {
                Ok(status) => {
                    match status.status.as_str() {
                        "confirmed" => {
                            let token = status.bot_token.ok_or_else(|| {
                                AgentError::platform("Missing bot_token in confirmed response")
                            })?;
                            let base = status
                                .base_url
                                .unwrap_or_else(|| self.base_url.clone());
                            return Ok((token, base));
                        }
                        "expired" => {
                            return Err(AgentError::platform(
                                "WeChat QR code expired, please retry login",
                            )
                            .into());
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("timeout") {
                        warn!("iLink QR status poll timed out, retrying...");
                    } else {
                        warn!("iLink QR status poll error: {}, retrying...", msg);
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs_f64(poll_interval)).await;
        }
        Err(AgentError::platform(format!(
            "WeChat QR code not scanned within {}s",
            max_wait
        ))
        .into())
    }

    // ------------------------------------------------------------------
    // Messaging APIs
    // ------------------------------------------------------------------

    /// Long-poll for incoming messages (holds up to 35 seconds).
    pub async fn get_updates(&self, cursor: &str) -> Result<GetUpdatesResponse> {
        let body = serde_json::json!({
            "get_updates_buf": cursor,
            "base_info": BaseInfo::default(),
        });
        let data = self
            .post("ilink/bot/getupdates", &body, REQUEST_TIMEOUT_SECS)
            .await?;
        let ret = data.get("ret").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
        let msgs = data.get("msgs").and_then(|v| {
            serde_json::from_value(v.clone()).ok()
        });
        let get_updates_buf = data
            .get("get_updates_buf")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let longpolling_timeout_ms = data
            .get("longpolling_timeout_ms")
            .and_then(|v| v.as_u64());
        Ok(GetUpdatesResponse {
            ret,
            msgs,
            get_updates_buf,
            longpolling_timeout_ms,
        })
    }

    /// Send a message to a WeChat user.
    pub async fn sendmessage(&self, msg: &OutboundMessage) -> Result<serde_json::Value> {
        let body = SendMessageRequest {
            msg: msg.clone(),
            base_info: BaseInfo::default(),
        };
        self.post("ilink/bot/sendmessage", &body, DEFAULT_TIMEOUT_SECS).await
    }

    /// Convenience: send a plain text message.
    pub async fn send_text(
        &self,
        to_user_id: &str,
        text: &str,
        context_token: &str,
    ) -> Result<serde_json::Value> {
        let client_id = format!("beebotos-weixin-{:08x}", rand::random::<u32>());
        let msg = OutboundMessage {
            from_user_id: String::new(),
            to_user_id: to_user_id.to_string(),
            client_id,
            message_type: 2,  // BOT
            message_state: 2, // FINISH
            context_token: context_token.to_string(),
            item_list: vec![OutboundMessageItem {
                item_type: 1,
                text_item: Some(TextItemContent { text: text.to_string() }),
                image_item: None,
                file_item: None,
                video_item: None,
            }],
        };
        self.sendmessage(&msg).await
    }

    /// Fetch bot config (e.g. typing_ticket).
    pub async fn getconfig(
        &self,
        ilink_user_id: &str,
        context_token: &str,
    ) -> Result<GetConfigResponse> {
        let body = GetConfigRequest {
            ilink_user_id: ilink_user_id.to_string(),
            context_token: context_token.to_string(),
            base_info: BaseInfo::default(),
        };
        let data = self
            .post("ilink/bot/getconfig", &body, DEFAULT_TIMEOUT_SECS)
            .await?;
        let typing_ticket = data
            .get("typing_ticket")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        Ok(GetConfigResponse { typing_ticket })
    }

    /// Send "typing..." indicator to a user.
    pub async fn sendtyping(
        &self,
        ilink_user_id: &str,
        typing_ticket: &str,
        status: i32,
    ) -> Result<()> {
        let body = SendTypingRequest {
            ilink_user_id: ilink_user_id.to_string(),
            typing_ticket: typing_ticket.to_string(),
            status,
        };
        let resp = self
            .post("ilink/bot/sendtyping", &body, DEFAULT_TIMEOUT_SECS)
            .await?;
        let ret = resp.get("ret").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
        let errcode = resp.get("errcode").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
        debug!("ILinkClient sendtyping response: ret={}, errcode={}", ret, errcode);
        Ok(())
    }

    // ------------------------------------------------------------------
    // Media helpers
    // ------------------------------------------------------------------

    /// Download a CDN media file and optionally decrypt it.
    pub async fn download_media(
        &self,
        url: &str,
        aes_key_b64: &str,
        encrypt_query_param: &str,
    ) -> Result<Vec<u8>> {
        let client = self.client()?;

        let download_url = if !encrypt_query_param.is_empty() {
            let enc = urlencoding::encode(encrypt_query_param);
            format!("{}/download?encrypted_query_param={}", CDN_BASE, enc)
        } else if url.starts_with("http") {
            url.to_string()
        } else {
            return Err(AgentError::platform(format!(
                "Cannot download media: no valid URL. url={}",
                &url[..url.len().min(40)]
            ))
            .into());
        };

        let resp = client
            .get(&download_url)
            .timeout(Duration::from_secs(60))
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Media download failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "Media download error ({}): {}",
                status, text
            ))
            .into());
        }

        let data = resp
            .bytes()
            .await
            .map_err(|e| AgentError::platform(format!("Read media data failed: {}", e)))?;

        if !aes_key_b64.is_empty() {
            aes_ecb_decrypt(&data, aes_key_b64)
        } else {
            Ok(data.to_vec())
        }
    }

    /// Get upload URL and parameters for a media file.
    pub async fn getuploadurl(
        &self,
        filekey: &str,
        media_type: i32,
        to_user_id: &str,
        rawsize: i64,
        rawfilemd5: &str,
        filesize: i64,
        aeskey: &str,
        no_need_thumb: bool,
    ) -> Result<GetUploadUrlResponse> {
        let body = serde_json::json!({
            "filekey": filekey,
            "media_type": media_type,
            "to_user_id": to_user_id,
            "rawsize": rawsize,
            "rawfilemd5": rawfilemd5,
            "filesize": filesize,
            "aeskey": aeskey,
            "no_need_thumb": no_need_thumb,
            "base_info": BaseInfo::default(),
        });
        let data = self
            .post("ilink/bot/getuploadurl", &body, DEFAULT_TIMEOUT_SECS)
            .await?;
        let upload_param = data
            .get("upload_param")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let upload_full_url = data
            .get("upload_full_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        Ok(GetUploadUrlResponse {
            upload_param,
            upload_full_url,
        })
    }

    /// Upload and encrypt a media file to WeChat CDN.
    ///
    /// Returns dict with keys:
    /// - encrypt_query_param: For media.encrypt_query_param
    /// - aes_key_b64: Base64-encoded AES key for media.aes_key
    /// - filesize: Encrypted file size
    pub async fn upload_media(
        &self,
        file_path: &Path,
        media_type: i32,
        to_user_id: &str,
    ) -> Result<HashMap<String, String>> {
        use sha2::{Digest, Sha256};

        let raw_data = tokio::fs::read(file_path)
            .await
            .map_err(|e| AgentError::platform(format!("Read file failed: {}", e)))?;
        let rawsize = raw_data.len() as i64;
        let rawfilemd5 = {
            let digest = md5::compute(&raw_data);
            hex::encode(digest.0)
        };

        // Generate AES key and filekey
        let aes_key_raw_bytes: [u8; 16] = rand::random();
        let aes_key_hex = hex::encode(aes_key_raw_bytes);
        let aes_key_for_msg = base64::encode(aes_key_hex.as_bytes());
        let aes_key_b64_for_encrypt = base64::encode(aes_key_raw_bytes);
        let filekey = hex::encode(rand::random::<[u8; 16]>());

        // Encrypt file with AES-128-ECB + PKCS7
        let encrypted_data = aes_ecb_encrypt(&raw_data, &aes_key_b64_for_encrypt)?;
        let filesize = encrypted_data.len() as i64;

        // Get upload URL
        let upload_resp = self
            .getuploadurl(
                &filekey,
                media_type,
                to_user_id,
                rawsize,
                &rawfilemd5,
                filesize,
                &aes_key_hex,
                true,
            )
            .await?;

        let upload_url = if let Some(url) = upload_resp.upload_full_url {
            url
        } else if let Some(param) = upload_resp.upload_param {
            let enc_param = urlencoding::encode(&param);
            format!(
                "{}/upload?encrypted_query_param={}&filekey={}",
                CDN_BASE, enc_param, filekey
            )
        } else {
            return Err(AgentError::platform(
                "No upload_full_url or upload_param in getuploadurl response",
            )
            .into());
        };

        // Upload encrypted file to CDN (no auth headers)
        let client = self.client()?;
        let resp = client
            .post(&upload_url)
            .header("Content-Type", "application/octet-stream")
            .body(encrypted_data)
            .timeout(Duration::from_secs(120))
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Media upload failed: {}", e)))?;

        debug!("Upload response status: {}", resp.status());

        // Get encrypt_query_param from response header
        let encrypt_query_param = resp
            .headers()
            .get("x-encrypted-param")
            .or_else(|| resp.headers().get("X-Encrypted-Param"))
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        if encrypt_query_param.is_empty() {
            let headers: Vec<String> = resp
                .headers()
                .iter()
                .map(|(k, v)| format!("{}: {:?}", k, v))
                .collect();
            error!(
                "upload_media: encrypt_query_param is empty! Response headers: {:?}",
                headers
            );
            return Err(AgentError::platform(
                "CDN did not return encrypt_query_param in response headers",
            )
            .into());
        }

        let mut result = HashMap::new();
        result.insert("encrypt_query_param".to_string(), encrypt_query_param);
        result.insert("aes_key_b64".to_string(), aes_key_for_msg);
        result.insert("filesize".to_string(), filesize.to_string());
        Ok(result)
    }

    /// Send a file message.
    pub async fn send_file(
        &self,
        to_user_id: &str,
        file_path: &Path,
        filename: &str,
        context_token: &str,
    ) -> Result<serde_json::Value> {
        let upload = self.upload_media(file_path, 3, to_user_id).await?;
        let client_id = format!("beebotos-weixin-{:08x}", rand::random::<u32>());
        let msg = OutboundMessage {
            from_user_id: String::new(),
            to_user_id: to_user_id.to_string(),
            client_id,
            message_type: 2,
            message_state: 2,
            context_token: context_token.to_string(),
            item_list: vec![OutboundMessageItem {
                item_type: 4,
                text_item: None,
                image_item: None,
                file_item: Some(FileItemContent {
                    media: MediaItem {
                        encrypt_query_param: upload
                            .get("encrypt_query_param")
                            .cloned()
                            .unwrap_or_default(),
                        aes_key: upload.get("aes_key_b64").cloned().unwrap_or_default(),
                        encrypt_type: 1,
                    },
                    file_name: filename.to_string(),
                    len: upload.get("filesize").cloned().unwrap_or_default(),
                }),
                video_item: None,
            }],
        };
        self.sendmessage(&msg).await
    }

    /// Send an image message.
    pub async fn send_image(
        &self,
        to_user_id: &str,
        image_path: &Path,
        context_token: &str,
    ) -> Result<serde_json::Value> {
        let upload = self.upload_media(image_path, 1, to_user_id).await?;
        let client_id = format!("beebotos-weixin-{:08x}", rand::random::<u32>());
        let msg = OutboundMessage {
            from_user_id: String::new(),
            to_user_id: to_user_id.to_string(),
            client_id,
            message_type: 2,
            message_state: 2,
            context_token: context_token.to_string(),
            item_list: vec![OutboundMessageItem {
                item_type: 2,
                text_item: None,
                image_item: Some(ImageItemContent {
                    media: MediaItem {
                        encrypt_query_param: upload
                            .get("encrypt_query_param")
                            .cloned()
                            .unwrap_or_default(),
                        aes_key: upload.get("aes_key_b64").cloned().unwrap_or_default(),
                        encrypt_type: 1,
                    },
                    mid_size: upload
                        .get("filesize")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0),
                }),
                file_item: None,
                video_item: None,
            }],
        };
        self.sendmessage(&msg).await
    }

    /// Send a video message.
    pub async fn send_video(
        &self,
        to_user_id: &str,
        video_path: &Path,
        context_token: &str,
    ) -> Result<serde_json::Value> {
        let upload = self.upload_media(video_path, 2, to_user_id).await?;
        let client_id = format!("beebotos-weixin-{:08x}", rand::random::<u32>());
        let msg = OutboundMessage {
            from_user_id: String::new(),
            to_user_id: to_user_id.to_string(),
            client_id,
            message_type: 2,
            message_state: 2,
            context_token: context_token.to_string(),
            item_list: vec![OutboundMessageItem {
                item_type: 5,
                text_item: None,
                image_item: None,
                file_item: None,
                video_item: Some(VideoItemContent {
                    media: MediaItem {
                        encrypt_query_param: upload
                            .get("encrypt_query_param")
                            .cloned()
                            .unwrap_or_default(),
                        aes_key: upload.get("aes_key_b64").cloned().unwrap_or_default(),
                        encrypt_type: 1,
                    },
                }),
            }],
        };
        self.sendmessage(&msg).await
    }
}

// Generate random X-WECHAT-UIN header
fn generate_uin_header() -> String {
    let uin: u32 = rand::random();
    base64::encode(uin.to_string())
}

// Simple base64 encode
mod base64 {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn encode(input: impl AsRef<[u8]>) -> String {
        let bytes = input.as_ref();
        let mut result = String::with_capacity((bytes.len() + 2) / 3 * 4);
        for chunk in bytes.chunks(3) {
            let b = match chunk.len() {
                1 => [chunk[0], 0, 0],
                2 => [chunk[0], chunk[1], 0],
                3 => [chunk[0], chunk[1], chunk[2]],
                _ => unreachable!(),
            };
            let idx0 = (b[0] >> 2) as usize;
            let idx1 = (((b[0] & 0b11) << 4) | (b[1] >> 4)) as usize;
            let idx2 = (((b[1] & 0b1111) << 2) | (b[2] >> 6)) as usize;
            let idx3 = (b[2] & 0b111111) as usize;
            result.push(ALPHABET[idx0] as char);
            result.push(ALPHABET[idx1] as char);
            if chunk.len() > 1 {
                result.push(ALPHABET[idx2] as char);
            } else {
                result.push('=');
            }
            if chunk.len() > 2 {
                result.push(ALPHABET[idx3] as char);
            } else {
                result.push('=');
            }
        }
        result
    }

    pub fn decode(input: &str) -> Result<Vec<u8>, &'static str> {
        let mut result = Vec::with_capacity(input.len() / 4 * 3);
        let mut buf = [0u8; 4];
        let mut buf_len = 0;
        for c in input.chars() {
            if c == '=' {
                break;
            }
            let idx = ALPHABET.iter().position(|&b| b == c as u8).ok_or("Invalid base64 char")?;
            buf[buf_len] = idx as u8;
            buf_len += 1;
            if buf_len == 4 {
                result.push((buf[0] << 2) | (buf[1] >> 4));
                result.push((buf[1] << 4) | (buf[2] >> 2));
                result.push((buf[2] << 6) | buf[3]);
                buf_len = 0;
            }
        }
        if buf_len >= 2 {
            result.push((buf[0] << 2) | (buf[1] >> 4));
        }
        if buf_len >= 3 {
            result.push((buf[1] << 4) | (buf[2] >> 2));
        }
        Ok(result)
    }
}

use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode_decode() {
        let encoded = base64::encode("1234567890");
        assert_eq!(encoded, "MTIzNDU2Nzg5MA==");
        let decoded = base64::decode(&encoded).unwrap();
        assert_eq!(decoded, b"1234567890");
    }

    #[test]
    fn test_session_remaining() {
        let session = BotSession {
            bot_token: "test".to_string(),
            base_url: "https://test".to_string(),
            login_time: std::time::Instant::now(),
            wxid: None,
            nickname: None,
        };
        assert!(session.remaining_secs() > 0);
        assert!(!session.remaining_text().is_empty());
    }

    #[test]
    fn test_aes_encrypt_decrypt() {
        let key = generate_aes_key_b64();
        let plaintext = b"Hello, WeChat iLink!";
        let encrypted = aes_ecb_encrypt(plaintext, &key).unwrap();
        let decrypted = aes_ecb_decrypt(&encrypted, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_aes_with_hex_key() {
        // Test hex key format (32 chars)
        let hex_key = "0123456789abcdef0123456789abcdef";
        let plaintext = b"Test with hex key";
        let encrypted = aes_ecb_encrypt(plaintext, &base64::encode(hex_key)).unwrap();
        let decrypted = aes_ecb_decrypt(&encrypted, hex_key).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}
