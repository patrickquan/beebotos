//! LLM Provider Admin API Service (QwenPaw-style)

use serde::{Deserialize, Serialize};

use super::client::{ApiClient, ApiError};

/// LLM Model
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmModel {
    pub id: i64,
    pub name: String,
    pub display_name: Option<String>,
    pub is_default_model: bool,
    pub supports_image: Option<bool>,
    pub supports_video: Option<bool>,
    pub supports_multimodal: Option<bool>,
    pub is_builtin: bool,
}

/// LLM Provider (admin view with full fields)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmProvider {
    pub id: i64,
    pub provider_id: String,
    pub name: String,
    pub protocol: String,
    pub base_url: Option<String>,
    pub api_key_masked: Option<String>,
    pub enabled: bool,
    pub is_default_provider: bool,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
    pub type_label: Option<String>,
    pub is_custom: bool,
    pub generate_kwargs: Option<serde_json::Value>,
    pub support_model_discovery: bool,
    pub support_connection_check: bool,
    pub freeze_url: bool,
    pub require_api_key: bool,
    pub models: Vec<LlmModel>,
    pub extra_models: Vec<LlmModel>,
}

impl LlmProvider {
    /// 获取所有模型（内置 + 用户添加）
    pub fn all_models(&self) -> Vec<&LlmModel> {
        self.models.iter().chain(self.extra_models.iter()).collect()
    }

    /// 查找默认模型
    pub fn default_model(&self) -> Option<&LlmModel> {
        self.all_models()
            .into_iter()
            .find(|m| m.is_default_model)
            .or_else(|| self.all_models().into_iter().next())
    }
}

/// 连接测试响应
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestConnectionResponse {
    pub success: bool,
    pub message: String,
}

/// 发现的模型信息
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiscoveredModel {
    pub id: String,
    pub name: String,
}

/// 发现模型响应
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiscoverModelsResponse {
    pub success: bool,
    pub models: Vec<DiscoveredModel>,
    pub added_count: i64,
}

/// 多模态探测响应
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProbeMultimodalResponse {
    pub success: bool,
    pub supports_image: bool,
    pub supports_video: bool,
    pub message: String,
}

/// 活跃 LLM 响应
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActiveLlmResponse {
    pub provider_id: i64,
    pub provider_name: String,
    pub model_name: String,
    pub display_name: Option<String>,
}

/// Response from listing providers
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProvidersResponse {
    pub providers: Vec<LlmProvider>,
}

/// Request to create a provider
#[derive(Clone, Debug, Serialize)]
pub struct CreateProviderRequest {
    pub provider_id: String,
    pub name: String,
    pub protocol: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
}

/// Request to update a provider
#[derive(Clone, Debug, Serialize)]
pub struct UpdateProviderRequest {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub enabled: Option<bool>,
}

/// Request to update provider config (generate_kwargs)
#[derive(Clone, Debug, Serialize)]
pub struct UpdateProviderConfigRequest {
    pub generate_kwargs: Option<serde_json::Value>,
}

/// Request to add a model
#[derive(Clone, Debug, Serialize)]
pub struct AddModelRequest {
    pub name: String,
    pub display_name: Option<String>,
}

/// Request to update model config
#[derive(Clone, Debug, Serialize)]
pub struct UpdateModelConfigRequest {
    pub display_name: Option<String>,
}

/// Request to set active LLM
#[derive(Clone, Debug, Serialize)]
pub struct SetActiveLlmRequest {
    pub provider_id: i64,
    pub model_name: String,
}

/// LLM Provider Admin Service
#[derive(Clone)]
pub struct LlmProviderService {
    client: ApiClient,
}

impl LlmProviderService {
    pub fn new(client: ApiClient) -> Self {
        Self { client }
    }

    /// List all providers with their models
    pub async fn list_providers(&self) -> Result<ProvidersResponse, ApiError> {
        self.client.get("/models").await
    }

    /// Create a new provider
    pub async fn create_provider(
        &self,
        req: CreateProviderRequest,
    ) -> Result<serde_json::Value, ApiError> {
        self.client.post("/models", &req).await
    }

    /// Update a provider
    pub async fn update_provider(
        &self,
        id: i64,
        req: UpdateProviderRequest,
    ) -> Result<serde_json::Value, ApiError> {
        self.client
            .put(&format!("/models/{}", id), &req)
            .await
    }

    /// Update provider config (generate_kwargs)
    pub async fn update_provider_config(
        &self,
        id: i64,
        req: UpdateProviderConfigRequest,
    ) -> Result<serde_json::Value, ApiError> {
        self.client
            .put(&format!("/models/{}/config", id), &req)
            .await
    }

    /// Delete a provider (only custom providers)
    pub async fn delete_provider(&self, id: i64) -> Result<(), ApiError> {
        self.client
            .delete(&format!("/models/{}", id))
            .await
    }

    /// Add a model to a provider
    pub async fn add_model(
        &self,
        provider_id: i64,
        req: AddModelRequest,
    ) -> Result<serde_json::Value, ApiError> {
        self.client
            .post(
                &format!("/models/{}/models", provider_id),
                &req,
            )
            .await
    }

    /// Delete a model (only non-built-in models)
    pub async fn delete_model(&self, provider_id: i64, model_id: i64) -> Result<(), ApiError> {
        self.client
            .delete(&format!(
                "/models/{}/models/{}",
                provider_id, model_id
            ))
            .await
    }

    /// Update model config
    pub async fn update_model_config(
        &self,
        provider_id: i64,
        model_id: i64,
        req: UpdateModelConfigRequest,
    ) -> Result<serde_json::Value, ApiError> {
        self.client
            .put(
                &format!("/models/{}/models/{}/config", provider_id, model_id),
                &req,
            )
            .await
    }

    /// Probe model multimodal capabilities
    pub async fn probe_model_multimodal(
        &self,
        provider_id: i64,
        model_id: i64,
    ) -> Result<ProbeMultimodalResponse, ApiError> {
        self.client
            .post(
                &format!(
                    "/models/{}/models/{}/probe-multimodal",
                    provider_id, model_id
                ),
                &serde_json::json!({}),
            )
            .await
    }

    /// Set default provider
    pub async fn set_default_provider(&self, id: i64) -> Result<serde_json::Value, ApiError> {
        self.client
            .put(
                &format!("/models/{}/default", id),
                &serde_json::json!({}),
            )
            .await
    }

    /// Set default model for a provider
    pub async fn set_default_model(
        &self,
        provider_id: i64,
        model_id: i64,
    ) -> Result<serde_json::Value, ApiError> {
        self.client
            .put(
                &format!(
                    "/models/{}/models/{}/default",
                    provider_id, model_id
                ),
                &serde_json::json!({}),
            )
            .await
    }

    /// 测试供应商连接
    pub async fn test_provider_connection(
        &self,
        id: i64,
    ) -> Result<TestConnectionResponse, ApiError> {
        self.client
            .post(&format!("/models/{}/test", id), &serde_json::json!({}))
            .await
    }

    /// 自动发现供应商可用模型
    pub async fn discover_models(
        &self,
        id: i64,
    ) -> Result<DiscoverModelsResponse, ApiError> {
        self.client
            .post(&format!("/models/{}/discover", id), &serde_json::json!({}))
            .await
    }

    /// 测试特定模型连接
    pub async fn test_model_connection(
        &self,
        provider_id: i64,
        model_name: String,
    ) -> Result<TestConnectionResponse, ApiError> {
        self.client
            .post(
                &format!(
                    "/models/{}/models/{}/test",
                    provider_id, model_name
                ),
                &serde_json::json!({}),
            )
            .await
    }

    /// Get active LLM
    pub async fn get_active_llm(&self) -> Result<ActiveLlmResponse, ApiError> {
        self.client.get("/models/active").await
    }

    /// Set active LLM
    pub async fn set_active_llm(
        &self,
        req: SetActiveLlmRequest,
    ) -> Result<serde_json::Value, ApiError> {
        self.client.put("/models/active", &req).await
    }
}
