//! LLM Service
//!
//! Handles LLM interactions for incoming messages from various platforms.
//! Providers are loaded from database at startup and can be hot-reloaded.
//! Supports fallback chain: if primary provider fails, try next in chain.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use beebotos_agents::communication::Message as ChannelMessage;
use beebotos_agents::llm::{
    AnthropicConfig, AnthropicProvider, Content, FailoverProvider, FailoverProviderBuilder,
    LLMProvider, Message as LLMMessage, ModelInfo, OpenAIConfig, OpenAIProvider, RequestConfig,
    RetryPolicy, Role,
};
use beebotos_agents::media::multimodal::{MultimodalContent, MultimodalProcessor};
use sqlx::SqlitePool;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::error::GatewayError;
use crate::services::encryption_service::EncryptionService;
use crate::services::llm_provider_db as db;

/// Metrics for LLM service
#[derive(Debug, Default)]
pub struct LlmMetrics {
    /// Total number of requests
    pub total_requests: AtomicU64,
    /// Number of successful requests
    pub successful_requests: AtomicU64,
    /// Number of failed requests
    pub failed_requests: AtomicU64,
    /// Total latency in milliseconds
    pub total_latency_ms: AtomicU64,
    /// Total tokens used (input + output)
    pub total_tokens: AtomicU64,
    /// Total input tokens
    pub input_tokens: AtomicU64,
    /// Total output tokens
    pub output_tokens: AtomicU64,
    /// Request latency histogram (in ms)
    latency_histogram: RwLock<Vec<u64>>,
}

impl LlmMetrics {
    /// Record a successful request
    pub async fn record_success(&self, latency_ms: u64, input_tokens: u32, output_tokens: u32) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.successful_requests.fetch_add(1, Ordering::Relaxed);
        self.total_latency_ms
            .fetch_add(latency_ms, Ordering::Relaxed);
        self.input_tokens
            .fetch_add(input_tokens as u64, Ordering::Relaxed);
        self.output_tokens
            .fetch_add(output_tokens as u64, Ordering::Relaxed);
        self.total_tokens
            .fetch_add((input_tokens + output_tokens) as u64, Ordering::Relaxed);

        // Add to latency histogram
        let mut hist = self.latency_histogram.write().await;
        hist.push(latency_ms);
        // Keep only last 1000 entries
        if hist.len() > 1000 {
            hist.remove(0);
        }
    }

    /// Record a failed request
    pub fn record_failure(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.failed_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Get average latency in milliseconds
    pub async fn average_latency_ms(&self) -> f64 {
        let total = self.total_latency_ms.load(Ordering::Relaxed);
        let requests = self.successful_requests.load(Ordering::Relaxed);
        if requests == 0 {
            0.0
        } else {
            total as f64 / requests as f64
        }
    }

    /// Get latency percentiles
    pub async fn latency_percentiles(&self) -> (f64, f64, f64) {
        let hist = self.latency_histogram.read().await;
        if hist.is_empty() {
            return (0.0, 0.0, 0.0);
        }

        let mut sorted = hist.clone();
        sorted.sort_unstable();

        let p50 = sorted[sorted.len() * 50 / 100] as f64;
        let p95 = sorted[sorted.len() * 95 / 100] as f64;
        let p99 = sorted[sorted.len() * 99 / 100] as f64;

        (p50, p95, p99)
    }

    /// Get metrics summary
    pub fn get_summary(&self) -> MetricsSummary {
        MetricsSummary {
            total_requests: self.total_requests.load(Ordering::Relaxed),
            successful_requests: self.successful_requests.load(Ordering::Relaxed),
            failed_requests: self.failed_requests.load(Ordering::Relaxed),
            total_tokens: self.total_tokens.load(Ordering::Relaxed),
            input_tokens: self.input_tokens.load(Ordering::Relaxed),
            output_tokens: self.output_tokens.load(Ordering::Relaxed),
        }
    }
}

/// Metrics summary for serialization
#[derive(Debug, Clone, serde::Serialize)]
pub struct MetricsSummary {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// LLM Service for processing messages
///
/// Loads providers from database at startup. Supports hot-reload when
/// provider configuration changes via the admin API.
pub struct LlmService {
    db: Arc<SqlitePool>,
    encryption: Arc<EncryptionService>,
    multimodal_processor: MultimodalProcessor,
    failover_provider: Arc<RwLock<Arc<FailoverProvider>>>,
    metrics: Arc<LlmMetrics>,
}

impl LlmService {
    /// Create a new LLM service from database
    pub async fn new(
        db: Arc<SqlitePool>,
        encryption: Arc<EncryptionService>,
    ) -> Result<Self, GatewayError> {
        // Seed preset providers on first startup
        db::seed_providers(&db)
            .await
            .map_err(|e| GatewayError::internal(format!("Failed to seed providers: {}", e)))?;

        // Load providers from database
        let failover = Self::build_failover_provider(&db, &encryption).await?;

        Ok(Self {
            db,
            encryption,
            multimodal_processor: MultimodalProcessor::new(),
            failover_provider: Arc::new(RwLock::new(Arc::new(failover))),
            metrics: Arc::new(LlmMetrics::default()),
        })
    }

    /// Reload providers from database (hot reload)
    pub async fn reload_providers(&self) -> Result<(), GatewayError> {
        let new_failover = Self::build_failover_provider(&self.db, &self.encryption).await?;
        let mut guard = self.failover_provider.write().await;
        *guard = Arc::new(new_failover);
        info!("LLM providers reloaded from database");
        Ok(())
    }

    /// Build failover provider from database configuration
    async fn build_failover_provider(
        db: &SqlitePool,
        encryption: &EncryptionService,
    ) -> Result<FailoverProvider, GatewayError> {
        let providers = db::list_providers_with_models(db)
            .await
            .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

        if providers.is_empty() {
            return Err(GatewayError::internal(
                "No LLM providers configured. Please configure providers via the Web UI."
                    .to_string(),
            ));
        }

        // Find default provider index
        let default_idx = providers
            .iter()
            .position(|(p, _)| p.is_default_provider)
            .unwrap_or(0);

        let mut primary: Option<Arc<dyn LLMProvider>> = None;
        let mut fallbacks: Vec<Arc<dyn LLMProvider>> = Vec::new();

        // Build provider list: default first, then enabled others
        let mut ordered = Vec::new();
        ordered.push(providers[default_idx].clone());
        for (i, (p, models)) in providers.iter().enumerate() {
            if i != default_idx && p.enabled {
                ordered.push((p.clone(), models.clone()));
            }
        }

        for (idx, (provider, models)) in ordered.iter().enumerate() {
            let api_key = match &provider.api_key_encrypted {
                Some(encrypted) => match encryption.decrypt(encrypted) {
                    Ok(key) => key,
                    Err(e) => {
                        warn!(
                            "Failed to decrypt API key for provider '{}': {}",
                            provider.provider_id, e
                        );
                        continue;
                    }
                },
                None => {
                    if provider.provider_id != "ollama" {
                        warn!(
                            "Provider '{}' has no API key configured, skipping",
                            provider.provider_id
                        );
                        continue;
                    }
                    String::new()
                }
            };

            let default_model = models
                .iter()
                .find(|m| m.is_default_model)
                .map(|m| m.name.clone())
                .or_else(|| models.first().map(|m| m.name.clone()))
                .unwrap_or_else(|| match provider.protocol.as_str() {
                    "anthropic" => "claude-3-sonnet-20240229".to_string(),
                    _ => "gpt-4o-mini".to_string(),
                });

            let base_url =
                provider
                    .base_url
                    .clone()
                    .unwrap_or_else(|| match provider.protocol.as_str() {
                        "anthropic" => "https://api.anthropic.com/v1".to_string(),
                        _ => "https://api.openai.com/v1".to_string(),
                    });

            match Self::create_provider_from_db(
                &provider.protocol,
                base_url,
                api_key,
                default_model,
            ) {
                Ok(p) => {
                    if idx == 0 {
                        primary = Some(p);
                        info!("Primary provider '{}' initialized", provider.provider_id);
                    } else {
                        fallbacks.push(p);
                        info!("Fallback provider '{}' initialized", provider.provider_id);
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to initialize provider '{}': {}",
                        provider.provider_id, e
                    );
                }
            }
        }

        // If no primary was set but we have fallbacks, use the first fallback as
        // primary
        let primary = if let Some(p) = primary {
            p
        } else if !fallbacks.is_empty() {
            let p = fallbacks.remove(0);
            info!("Using fallback provider as primary since default provider is unavailable");
            p
        } else {
            return Err(GatewayError::internal(
                "No primary LLM provider available".to_string(),
            ));
        };

        // Build failover provider
        let mut builder = FailoverProviderBuilder::new()
            .primary(primary)
            .timeout_secs(180);

        for fallback in fallbacks {
            builder = builder.fallback(fallback);
        }

        builder.build().map_err(|e| {
            GatewayError::internal(format!("Failed to build failover provider: {}", e))
        })
    }

    /// Create a single provider from database configuration
    fn create_provider_from_db(
        protocol: &str,
        base_url: String,
        api_key: String,
        default_model: String,
    ) -> Result<Arc<dyn LLMProvider>, String> {
        match protocol {
            "openai-compatible" => {
                let config = OpenAIConfig {
                    base_url,
                    api_key,
                    default_model,
                    timeout: Duration::from_secs(180),
                    retry_policy: RetryPolicy::default(),
                    organization: None,
                };
                let provider = OpenAIProvider::new(config)
                    .map_err(|e| format!("Failed to create OpenAI provider: {}", e))?;
                Ok(Arc::new(provider))
            }
            "anthropic" => {
                let config = AnthropicConfig {
                    base_url,
                    api_key,
                    default_model,
                    timeout: Duration::from_secs(180),
                    retry_policy: RetryPolicy::default(),
                    version: "2023-06-01".to_string(),
                };
                let provider = AnthropicProvider::new(config)
                    .map_err(|e| format!("Failed to create Anthropic provider: {}", e))?;
                Ok(Arc::new(provider))
            }
            _ => Err(format!("Unknown protocol: {}", protocol)),
        }
    }

    /// Get metrics reference
    pub fn metrics(&self) -> Arc<LlmMetrics> {
        self.metrics.clone()
    }

    /// Get metrics summary
    pub fn get_metrics_summary(&self) -> MetricsSummary {
        self.metrics.get_summary()
    }

    /// Get the underlying failover provider for building LLMClient
    pub async fn get_provider(&self) -> Arc<dyn LLMProvider> {
        self.failover_provider.read().await.clone()
    }

    /// Process a message with optional custom image download function
    pub async fn process_message_with_images<F, Fut>(
        &self,
        message: &ChannelMessage,
        image_downloader: Option<F>,
    ) -> Result<String, GatewayError>
    where
        F: Fn(&str, Option<&str>) -> Fut + Send + Sync,
        Fut: std::future::Future<
                Output = std::result::Result<Vec<u8>, beebotos_agents::error::AgentError>,
            > + Send,
    {
        let multimodal_content = if let Some(downloader) = &image_downloader {
            self.multimodal_processor
                .process_message_with_downloader(message, downloader)
                .await
        } else {
            self.multimodal_processor
                .process_message(message, message.platform, None)
                .await
        };
        self.execute_llm_request(multimodal_content, message.content.clone(), false)
            .await
    }

    /// Process an incoming message and generate a response
    pub async fn process_message(&self, message: &ChannelMessage) -> Result<String, GatewayError> {
        let multimodal_content = self
            .multimodal_processor
            .process_message(message, message.platform, None)
            .await;
        self.execute_llm_request(multimodal_content, message.content.clone(), true)
            .await
    }

    /// Execute a chat completion with pre-built messages
    pub async fn chat(&self, messages: Vec<LLMMessage>) -> Result<String, GatewayError> {
        let start_time = std::time::Instant::now();

        let request_config = RequestConfig {
            model: self.get_default_model().await,
            // 🟢 P1 FIX: Don't hardcode temperature — let the provider API use its
            // own default. Some models (e.g. kimi-k2.5) only accept specific
            // temperature values, and sending an incompatible value causes the
            // request to fail with "invalid temperature".
            temperature: None,
            max_tokens: Some(4096),
            stream: Some(false),
            ..Default::default()
        };

        let request = beebotos_agents::llm::types::LLMRequest {
            messages,
            config: request_config,
        };

        let failover = self.failover_provider.read().await.clone();
        let result = failover.complete(request).await;
        let latency_ms = start_time.elapsed().as_millis() as u64;

        match result {
            Ok(response) => {
                let content = response
                    .choices
                    .first()
                    .map(|choice| choice.message.text_content())
                    .unwrap_or_default();

                let (input_tokens, output_tokens) = response
                    .usage
                    .as_ref()
                    .map_or((0, 0), |u| (u.prompt_tokens, u.completion_tokens));

                self.metrics
                    .record_success(latency_ms, input_tokens, output_tokens)
                    .await;

                info!(
                    "Received LLM response: length={}, latency={}ms, tokens={}/{}",
                    content.len(),
                    latency_ms,
                    input_tokens,
                    output_tokens
                );
                Ok(content)
            }
            Err(e) => {
                self.metrics.record_failure();
                Err(GatewayError::Internal {
                    message: format!("LLM request failed: {}", e),
                    correlation_id: uuid::Uuid::new_v4().to_string(),
                })
            }
        }
    }

    /// Execute LLM request with processed multimodal content
    async fn execute_llm_request(
        &self,
        multimodal_result: Result<MultimodalContent, beebotos_agents::error::AgentError>,
        fallback_text: String,
        _include_system_prompt: bool,
    ) -> Result<String, GatewayError> {
        let start_time = std::time::Instant::now();

        // Handle multimodal processing result
        let multimodal_content = multimodal_result.unwrap_or_else(|e| {
            warn!(
                "Failed to process multimodal content: {}, using text only",
                e
            );
            MultimodalContent {
                text: fallback_text,
                images: vec![],
                metadata: HashMap::new(),
            }
        });

        info!(
            "Processing LLM request: text='{}...', images={}",
            multimodal_content.text.chars().take(50).collect::<String>(),
            multimodal_content.images.len()
        );

        // Build LLM contents from multimodal content
        let mut contents: Vec<Content> = vec![Content::Text {
            text: multimodal_content.text,
        }];

        // Add images as content
        for image in &multimodal_content.images {
            let data_url = format!("data:{};base64, {}", image.mime_type, image.base64_data);
            contents.push(Content::ImageUrl {
                image_url: beebotos_agents::llm::types::ImageUrlContent {
                    url: data_url,
                    detail: Some("auto".to_string()),
                },
            });
        }

        // Create user message
        let user_message = if contents.len() == 1 {
            match &contents[0] {
                Content::Text { text } => LLMMessage::user(text.clone()),
                _ => LLMMessage::user("".to_string()),
            }
        } else {
            LLMMessage::multimodal(Role::User, contents)
        };

        let messages = vec![user_message];

        // Build request config
        let request_config = RequestConfig {
            model: self.get_default_model().await,
            temperature: Some(0.7),
            max_tokens: Some(4096),
            stream: Some(false),
            ..Default::default()
        };

        let request = beebotos_agents::llm::types::LLMRequest {
            messages,
            config: request_config,
        };

        // Execute request through failover provider
        let failover = self.failover_provider.read().await.clone();
        let result = failover.complete(request).await;
        let latency_ms = start_time.elapsed().as_millis() as u64;

        match result {
            Ok(response) => {
                let content = response
                    .choices
                    .first()
                    .map(|choice| choice.message.text_content())
                    .unwrap_or_default();

                let (input_tokens, output_tokens) = response
                    .usage
                    .as_ref()
                    .map_or((0, 0), |u| (u.prompt_tokens, u.completion_tokens));

                self.metrics
                    .record_success(latency_ms, input_tokens, output_tokens)
                    .await;

                info!(
                    "Received LLM response: length={}, latency={}ms, tokens={}/{}",
                    content.len(),
                    latency_ms,
                    input_tokens,
                    output_tokens
                );
                Ok(content)
            }
            Err(e) => {
                self.metrics.record_failure();
                Err(GatewayError::Internal {
                    message: format!("LLM request failed: {}", e),
                    correlation_id: uuid::Uuid::new_v4().to_string(),
                })
            }
        }
    }

    /// Process a message with streaming response
    pub async fn process_message_stream(
        &self,
        message: &ChannelMessage,
    ) -> Result<tokio::sync::mpsc::Receiver<String>, GatewayError> {
        let start_time = std::time::Instant::now();

        // Process multimodal content
        let multimodal_content = self
            .multimodal_processor
            .process_message(message, message.platform, None)
            .await
            .unwrap_or_else(|e| {
                warn!(
                    "Failed to process multimodal content: {}, using text only",
                    e
                );
                MultimodalContent {
                    text: message.content.clone(),
                    images: vec![],
                    metadata: HashMap::new(),
                }
            });

        // Build contents
        let mut contents: Vec<Content> = vec![Content::Text {
            text: multimodal_content.text.clone(),
        }];

        // Add images
        for image in &multimodal_content.images {
            let data_url = format!("data:{};base64, {}", image.mime_type, image.base64_data);
            contents.push(Content::ImageUrl {
                image_url: beebotos_agents::llm::types::ImageUrlContent {
                    url: data_url,
                    detail: Some("auto".to_string()),
                },
            });
        }

        // Create user message
        let user_message = if contents.len() == 1 {
            LLMMessage::user(multimodal_content.text.clone())
        } else {
            LLMMessage::multimodal(Role::User, contents)
        };

        let messages = vec![user_message];

        // Build streaming request config
        let request_config = RequestConfig {
            model: self.get_default_model().await,
            temperature: Some(0.7),
            max_tokens: Some(4096),
            stream: Some(true),
            ..Default::default()
        };

        let request = beebotos_agents::llm::types::LLMRequest {
            messages,
            config: request_config,
        };

        // Execute streaming request
        let failover = self.failover_provider.read().await.clone();
        let mut stream_rx =
            failover
                .complete_stream(request)
                .await
                .map_err(|e| GatewayError::Internal {
                    message: format!("LLM streaming request failed: {}", e),
                    correlation_id: uuid::Uuid::new_v4().to_string(),
                })?;

        // Create output channel
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let metrics = self.metrics.clone();

        // Spawn task to handle streaming
        tokio::spawn(async move {
            while let Some(chunk) = stream_rx.recv().await {
                for choice in &chunk.choices {
                    if let Some(content) = &choice.delta.content {
                        if tx.send(content.clone()).await.is_err() {
                            let latency_ms = start_time.elapsed().as_millis() as u64;
                            metrics.record_success(latency_ms, 0, 0).await;
                            return;
                        }
                    }
                    if choice.finish_reason.is_some() {
                        let latency_ms = start_time.elapsed().as_millis() as u64;
                        metrics.record_success(latency_ms, 0, 0).await;
                        return;
                    }
                }
            }
            let latency_ms = start_time.elapsed().as_millis() as u64;
            metrics.record_success(latency_ms, 0, 0).await;
        });

        info!("Started LLM streaming response");
        Ok(rx)
    }

    /// Send a reply back to the platform
    pub async fn send_reply(
        &self,
        platform: beebotos_agents::communication::PlatformType,
        channel_id: &str,
        content: &str,
    ) -> Result<(), GatewayError> {
        debug!(
            "Sending reply to {:?} channel {}: content_length={}",
            platform,
            channel_id,
            content.len()
        );

        info!(
            "Reply ready for {:?} channel {}: preview={:.50}...",
            platform, channel_id, content
        );

        Ok(())
    }

    /// Health check for LLM service
    pub async fn health_check(&self) -> Result<(), GatewayError> {
        let failover = self.failover_provider.read().await.clone();
        failover
            .health_check()
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("LLM health check failed: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })
    }

    /// Get provider status
    pub async fn get_provider_status(&self) -> Vec<(String, bool, u32)> {
        let failover = self.failover_provider.read().await.clone();
        failover.get_provider_status().await
    }

    /// 测试单个供应商的连接
    ///
    /// 读取供应商配置，构造临时客户端，发送最小化测试请求。
    pub async fn test_provider_connection(
        &self,
        provider_id: i64,
    ) -> Result<(bool, String), GatewayError> {
        let (provider, models) = db::get_provider_with_models(&self.db, provider_id)
            .await
            .map_err(|e| GatewayError::internal(format!("数据库错误: {}", e)))?
            .ok_or_else(|| GatewayError::bad_request("供应商不存在"))?;

        // 获取 API 密钥
        let api_key = match &provider.api_key_encrypted {
            Some(encrypted) => match self.encryption.decrypt(encrypted) {
                Ok(key) => key,
                Err(e) => {
                    return Ok((
                        false,
                        format!("API 密钥解密失败: {}", e),
                    ));
                }
            },
            None => {
                if provider.provider_id != "ollama" {
                    return Ok((false, "未配置 API 密钥".to_string()));
                }
                String::new()
            }
        };

        let base_url = provider
            .base_url
            .clone()
            .unwrap_or_else(|| match provider.protocol.as_str() {
                "anthropic" => "https://api.anthropic.com/v1".to_string(),
                _ => "https://api.openai.com/v1".to_string(),
            });

        let default_model = models
            .iter()
            .find(|m| m.is_default_model)
            .map(|m| m.name.clone())
            .or_else(|| models.first().map(|m| m.name.clone()))
            .unwrap_or_else(|| match provider.protocol.as_str() {
                "anthropic" => "claude-3-sonnet-20240229".to_string(),
                _ => "gpt-4o-mini".to_string(),
            });

        let provider_instance = match Self::create_provider_from_db(
            &provider.protocol,
            base_url,
            api_key,
            default_model.clone(),
        ) {
            Ok(p) => p,
            Err(e) => return Ok((false, format!("创建供应商客户端失败: {}", e))),
        };

        // 构造最小化测试请求
        let request = beebotos_agents::llm::types::LLMRequest {
            messages: vec![LLMMessage::user("ping")],
            config: RequestConfig {
                model: default_model,
                max_tokens: Some(1),
                stream: Some(false),
                ..Default::default()
            },
        };

        // 使用超时执行测试请求
        let result = tokio::time::timeout(Duration::from_secs(10), provider_instance.complete(request)).await;

        match result {
            Ok(Ok(_)) => Ok((true, "连接成功".to_string())),
            Ok(Err(e)) => {
                let msg = format!("请求失败: {}", e);
                // 判断是否为认证错误
                let lower = msg.to_lowercase();
                if lower.contains("unauthorized")
                    || lower.contains("invalid api key")
                    || lower.contains("authentication")
                    || lower.contains("401")
                {
                    Ok((false, "API 密钥无效或已过期".to_string()))
                } else if lower.contains("timeout") || lower.contains("timed out") {
                    Ok((false, "连接超时，请检查 Base URL 是否正确".to_string()))
                } else if lower.contains("connection")
                    || lower.contains("resolve")
                    || lower.contains("dns")
                {
                    Ok((false, "无法连接到服务器，请检查 Base URL".to_string()))
                } else {
                    Ok((false, msg))
                }
            }
            Err(_) => Ok((false, "连接超时（10 秒），请检查网络或 Base URL".to_string())),
        }
    }

    /// 发现供应商可用模型
    ///
    /// 调用供应商的 list_models API，将未在数据库中的模型自动添加。
    pub async fn discover_models(
        &self,
        provider_id: i64,
    ) -> Result<(Vec<ModelInfo>, usize), GatewayError> {
        let (provider, existing_models) = db::get_provider_with_models(&self.db, provider_id)
            .await
            .map_err(|e| GatewayError::internal(format!("数据库错误: {}", e)))?
            .ok_or_else(|| GatewayError::bad_request("供应商不存在"))?;

        let api_key = match &provider.api_key_encrypted {
            Some(encrypted) => match self.encryption.decrypt(encrypted) {
                Ok(key) => key,
                Err(e) => {
                    return Err(GatewayError::bad_request(format!(
                        "API 密钥解密失败: {}",
                        e
                    )));
                }
            },
            None => {
                if provider.provider_id != "ollama" {
                    return Err(GatewayError::bad_request("未配置 API 密钥"));
                }
                String::new()
            }
        };

        let base_url = provider
            .base_url
            .clone()
            .unwrap_or_else(|| match provider.protocol.as_str() {
                "anthropic" => "https://api.anthropic.com/v1".to_string(),
                _ => "https://api.openai.com/v1".to_string(),
            });

        let default_model = existing_models
            .iter()
            .find(|m| m.is_default_model)
            .map(|m| m.name.clone())
            .or_else(|| existing_models.first().map(|m| m.name.clone()))
            .unwrap_or_else(|| match provider.protocol.as_str() {
                "anthropic" => "claude-3-sonnet-20240229".to_string(),
                _ => "gpt-4o-mini".to_string(),
            });

        let provider_instance = match Self::create_provider_from_db(
            &provider.protocol,
            base_url,
            api_key,
            default_model,
        ) {
            Ok(p) => p,
            Err(e) => {
                return Err(GatewayError::internal(format!(
                    "创建供应商客户端失败: {}",
                    e
                )));
            }
        };

        // 调用 list_models，带超时
        let list_result = tokio::time::timeout(
            Duration::from_secs(15),
            provider_instance.list_models(),
        )
        .await;

        let discovered = match list_result {
            Ok(Ok(models)) => models,
            Ok(Err(e)) => {
                return Err(GatewayError::internal(format!("获取模型列表失败: {}", e)));
            }
            Err(_) => {
                return Err(GatewayError::internal(
                    "获取模型列表超时（15 秒）".to_string(),
                ));
            }
        };

        // 将未存在的模型添加到数据库
        let existing_names: std::collections::HashSet<String> = existing_models
            .into_iter()
            .map(|m| m.name)
            .collect();

        let mut added = 0usize;
        for model in &discovered {
            if !existing_names.contains(&model.id) {
                if let Ok(_) = db::add_model(
                    &self.db,
                    provider_id,
                    &model.id,
                    Some(&model.name),
                )
                .await
                {
                    added += 1;
                }
            }
        }

        Ok((discovered, added))
    }

    /// 测试特定模型的连接
    ///
    /// 使用指定模型名称发送测试请求。
    pub async fn test_model_connection(
        &self,
        provider_id: i64,
        model_name: &str,
    ) -> Result<(bool, String), GatewayError> {
        let (provider, _) = db::get_provider_with_models(&self.db, provider_id)
            .await
            .map_err(|e| GatewayError::internal(format!("数据库错误: {}", e)))?
            .ok_or_else(|| GatewayError::bad_request("供应商不存在"))?;

        let api_key = match &provider.api_key_encrypted {
            Some(encrypted) => match self.encryption.decrypt(encrypted) {
                Ok(key) => key,
                Err(e) => return Ok((false, format!("API 密钥解密失败: {}", e))),
            },
            None => {
                if provider.provider_id != "ollama" {
                    return Ok((false, "未配置 API 密钥".to_string()));
                }
                String::new()
            }
        };

        let base_url = provider
            .base_url
            .clone()
            .unwrap_or_else(|| match provider.protocol.as_str() {
                "anthropic" => "https://api.anthropic.com/v1".to_string(),
                _ => "https://api.openai.com/v1".to_string(),
            });

        let provider_instance = match Self::create_provider_from_db(
            &provider.protocol,
            base_url,
            api_key,
            model_name.to_string(),
        ) {
            Ok(p) => p,
            Err(e) => return Ok((false, format!("创建供应商客户端失败: {}", e))),
        };

        let request = beebotos_agents::llm::types::LLMRequest {
            messages: vec![LLMMessage::user("ping")],
            config: RequestConfig {
                model: model_name.to_string(),
                max_tokens: Some(1),
                stream: Some(false),
                ..Default::default()
            },
        };

        let result = tokio::time::timeout(
            Duration::from_secs(10),
            provider_instance.complete(request),
        )
        .await;

        match result {
            Ok(Ok(_)) => Ok((true, "连接成功".to_string())),
            Ok(Err(e)) => {
                let msg = format!("请求失败: {}", e);
                let lower = msg.to_lowercase();
                if lower.contains("unauthorized")
                    || lower.contains("invalid api key")
                    || lower.contains("401")
                {
                    Ok((false, "API 密钥无效".to_string()))
                } else if lower.contains("not found") || lower.contains("404") || lower.contains("model") {
                    Ok((false, "模型不存在或不可用".to_string()))
                } else if lower.contains("timeout") {
                    Ok((false, "连接超时".to_string()))
                } else {
                    Ok((false, msg))
                }
            }
            Err(_) => Ok((false, "连接超时（10 秒）".to_string())),
        }
    }

    /// Get default model from database
    ///
    /// 🟢 P1 FIX: Fallback to the provider's first available model instead of
    /// hardcoded "gpt-4o-mini", which may not exist in the provider.
    async fn get_default_model(&self) -> String {
        match db::get_default_provider(&self.db).await.ok().flatten() {
            Some(provider) => {
                match db::get_default_model(&self.db, provider.id)
                    .await
                    .ok()
                    .flatten()
                {
                    Some(model) => model.name,
                    None => {
                        // Fallback: use the first available model for this provider
                        match db::get_models_for_provider(&self.db, provider.id)
                            .await
                            .ok()
                            .and_then(|models| models.into_iter().next())
                        {
                            Some(first_model) => first_model.name,
                            None => {
                                warn!(
                                    "No models found for default provider '{}', falling back to \
                                     generic model name",
                                    provider.provider_id
                                );
                                "gpt-4o-mini".to_string()
                            }
                        }
                    }
                }
            }
            None => "gpt-4o-mini".to_string(),
        }
    }

    /// 探测模型的多模态能力
    ///
    /// 发送一个包含图片 URL 的测试请求，观察模型是否支持多模态输入。
    /// 返回 (supports_image, supports_video, message)。
    pub async fn probe_model_multimodal(
        &self,
        provider_id: i64,
        model_name: &str,
    ) -> Result<(bool, bool, String), GatewayError> {
        let (provider, _) = db::get_provider_with_models(&self.db, provider_id)
            .await
            .map_err(|e| GatewayError::internal(format!("数据库错误: {}", e)))?
            .ok_or_else(|| GatewayError::bad_request("供应商不存在"))?;

        let api_key = match &provider.api_key_encrypted {
            Some(encrypted) => match self.encryption.decrypt(encrypted) {
                Ok(key) => key,
                Err(e) => return Ok((false, false, format!("API 密钥解密失败: {}", e))),
            },
            None => {
                if provider.provider_id != "ollama" {
                    return Ok((false, false, "未配置 API 密钥".to_string()));
                }
                String::new()
            }
        };

        let base_url = provider
            .base_url
            .clone()
            .unwrap_or_else(|| match provider.protocol.as_str() {
                "anthropic" => "https://api.anthropic.com/v1".to_string(),
                _ => "https://api.openai.com/v1".to_string(),
            });

        let provider_instance = match Self::create_provider_from_db(
            &provider.protocol,
            base_url,
            api_key,
            model_name.to_string(),
        ) {
            Ok(p) => p,
            Err(e) => return Ok((false, false, format!("创建供应商客户端失败: {}", e))),
        };

        // 构造一个包含图片的多模态测试请求
        let test_image_url = "https://upload.wikimedia.org/wikipedia/commons/thumb/4/47/PNG_transparency_demonstration_1.png/300px-PNG_transparency_demonstration_1.png";
        let request = beebotos_agents::llm::types::LLMRequest {
            messages: vec![LLMMessage::multimodal(
                Role::User,
                vec![
                    Content::Text {
                        text: "Describe this image briefly.".to_string(),
                    },
                    Content::ImageUrl {
                        image_url: beebotos_agents::llm::types::ImageUrlContent {
                            url: test_image_url.to_string(),
                            detail: Some("auto".to_string()),
                        },
                    },
                ],
            )],
            config: RequestConfig {
                model: model_name.to_string(),
                max_tokens: Some(50),
                stream: Some(false),
                ..Default::default()
            },
        };

        let result = tokio::time::timeout(
            Duration::from_secs(15),
            provider_instance.complete(request),
        )
        .await;

        match result {
            Ok(Ok(_)) => Ok((
                true,
                false,
                "模型支持图片输入".to_string(),
            )),
            Ok(Err(e)) => {
                let msg = format!("{}", e);
                let lower = msg.to_lowercase();
                // 如果错误信息暗示不支持图片，则认为不支持图片
                if lower.contains("image")
                    || lower.contains("multimodal")
                    || lower.contains("vision")
                    || lower.contains("content type")
                    || lower.contains("unsupported")
                {
                    Ok((
                        false,
                        false,
                        "模型不支持图片输入".to_string(),
                    ))
                } else if lower.contains("unauthorized") || lower.contains("401") {
                    Ok((false, false, "API 密钥无效".to_string()))
                } else if lower.contains("timeout") {
                    Ok((false, false, "连接超时".to_string()))
                } else {
                    // 其他错误，保守认为不支持
                    Ok((
                        false,
                        false,
                        format!("探测失败，模型可能不支持图片输入: {}", msg),
                    ))
                }
            }
            Err(_) => Ok((
                false,
                false,
                "连接超时（15 秒），无法完成探测".to_string(),
            )),
        }
    }
}
