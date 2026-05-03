//! LLM Provider Admin API Handlers (QwenPaw-style)

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use gateway::error::GatewayError;
use gateway::middleware::{require_any_role, AuthUser};
use serde::{Deserialize, Serialize};

use crate::services::llm_provider_db as db;
use crate::AppState;

// ---- Request/Response DTOs ----

#[derive(Debug, Deserialize)]
pub struct CreateProviderRequest {
    pub provider_id: String,
    pub name: String,
    pub protocol: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProviderRequest {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProviderConfigRequest {
    pub generate_kwargs: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct AddModelRequest {
    pub name: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateModelConfigRequest {
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetActiveLlmRequest {
    pub provider_id: i64,
    pub model_name: String,
}

#[derive(Debug, Serialize)]
pub struct ModelResponse {
    pub id: i64,
    pub name: String,
    pub display_name: Option<String>,
    pub is_default_model: bool,
    pub supports_image: Option<bool>,
    pub supports_video: Option<bool>,
    pub supports_multimodal: Option<bool>,
    pub is_builtin: bool,
}

#[derive(Debug, Serialize)]
pub struct ProviderResponse {
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
    pub models: Vec<ModelResponse>,
    pub extra_models: Vec<ModelResponse>,
}

#[derive(Debug, Serialize)]
pub struct ProvidersListResponse {
    pub providers: Vec<ProviderResponse>,
}

#[derive(Debug, Serialize)]
pub struct ActiveLlmResponse {
    pub provider_id: i64,
    pub provider_name: String,
    pub model_name: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProbeMultimodalResponse {
    pub success: bool,
    pub supports_image: bool,
    pub supports_video: bool,
    pub message: String,
}

// ---- Helper ----

fn mask_encrypted_key(encrypted: Option<&str>) -> Option<String> {
    encrypted.map(|_| "******".to_string())
}

fn provider_to_response(p: db::LlmProviderDb, models: Vec<db::LlmModelDb>) -> ProviderResponse {
    let all_models: Vec<ModelResponse> = models
        .into_iter()
        .map(|m| ModelResponse {
            id: m.id,
            name: m.name,
            display_name: m.display_name,
            is_default_model: m.is_default_model,
            supports_image: m.supports_image,
            supports_video: m.supports_video,
            supports_multimodal: m.supports_multimodal,
            is_builtin: m.is_builtin,
        })
        .collect();

    let (builtin_models, extra_models): (Vec<_>, Vec<_>) =
        all_models.into_iter().partition(|m| m.is_builtin);

    let generate_kwargs = p
        .generate_kwargs
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok());

    ProviderResponse {
        id: p.id,
        provider_id: p.provider_id,
        name: p.name,
        protocol: p.protocol,
        base_url: p.base_url,
        api_key_masked: mask_encrypted_key(p.api_key_encrypted.as_deref()),
        enabled: p.enabled,
        is_default_provider: p.is_default_provider,
        icon: p.icon,
        icon_color: p.icon_color,
        type_label: p.type_label,
        is_custom: p.is_custom,
        generate_kwargs,
        support_model_discovery: p.support_model_discovery,
        support_connection_check: p.support_connection_check,
        freeze_url: p.freeze_url,
        require_api_key: p.require_api_key,
        models: builtin_models,
        extra_models,
    }
}

// ---- Handlers ----

/// GET /api/v1/models
pub async fn list_providers(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<Json<ProvidersListResponse>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    let rows = db::list_providers_with_models(&state.db)
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    let providers = rows
        .into_iter()
        .map(|(p, models)| provider_to_response(p, models))
        .collect();

    Ok(Json(ProvidersListResponse { providers }))
}

/// POST /api/v1/models
/// Create a custom provider
pub async fn create_provider(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<CreateProviderRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    // Validate protocol
    if req.protocol != "openai-compatible" && req.protocol != "anthropic" {
        return Err(GatewayError::bad_request_field(
            "protocol must be 'openai-compatible' or 'anthropic'",
            "protocol",
        ));
    }

    // Encrypt API key if provided
    let api_key_encrypted = match req.api_key {
        Some(key) if !key.is_empty() => Some(
            state
                .encryption_service
                .encrypt(&key)
                .map_err(|e| GatewayError::internal(format!("Encryption failed: {}", e)))?,
        ),
        _ => None,
    };

    let id = db::create_provider(
        &state.db,
        &req.provider_id,
        &req.name,
        &req.protocol,
        req.base_url.as_deref(),
        api_key_encrypted.as_deref(),
    )
    .await
    .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    // Reload providers after creation
    state.llm_service.reload_providers().await.ok();

    Ok(Json(
        serde_json::json!({ "id": id, "message": "Provider created" }),
    ))
}

/// PUT /api/v1/models/:id
pub async fn update_provider(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<UpdateProviderRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    // Encrypt API key if provided
    let api_key_encrypted = match req.api_key {
        Some(key) if !key.is_empty() => Some(
            state
                .encryption_service
                .encrypt(&key)
                .map_err(|e| GatewayError::internal(format!("Encryption failed: {}", e)))?,
        ),
        Some(_) => Some(String::new()), // empty string means clear
        None => None,                   // not provided means don't change
    };

    db::update_provider(
        &state.db,
        id,
        req.name.as_deref(),
        req.base_url.as_deref(),
        api_key_encrypted.as_deref(),
        req.enabled,
    )
    .await
    .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    // Reload providers after update
    state.llm_service.reload_providers().await.ok();

    Ok(Json(serde_json::json!({ "message": "Provider updated" })))
}

/// PUT /api/v1/models/:id/config
pub async fn update_provider_config(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<UpdateProviderConfigRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    let generate_kwargs_str = req.generate_kwargs.map(|v| v.to_string());

    db::update_provider_config(
        &state.db,
        id,
        generate_kwargs_str.as_deref(),
    )
    .await
    .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    // Reload providers after update
    state.llm_service.reload_providers().await.ok();

    Ok(Json(serde_json::json!({ "message": "Provider config updated" })))
}

/// DELETE /api/v1/models/:id
pub async fn delete_provider(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    // Only allow deleting custom providers
    let provider = db::get_provider_by_id(&state.db, id)
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?
        .ok_or_else(|| GatewayError::bad_request("Provider not found"))?;

    if !provider.is_custom {
        return Err(GatewayError::bad_request(
            "Cannot delete built-in providers",
        ));
    }

    db::delete_provider(&state.db, id)
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    // Reload providers after deletion
    state.llm_service.reload_providers().await.ok();

    Ok(Json(serde_json::json!({ "message": "Provider deleted" })))
}

/// POST /api/v1/models/:id/models
pub async fn add_model(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<AddModelRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    let model_id = db::add_model(&state.db, id, &req.name, req.display_name.as_deref())
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    Ok(Json(
        serde_json::json!({ "id": model_id, "message": "Model added" }),
    ))
}

/// DELETE /api/v1/models/:id/models/:model_id
pub async fn delete_model(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path((_provider_id, model_id)): Path<(i64, i64)>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    // Only allow deleting non-built-in models
    let models = db::get_models_for_provider(&state.db, _provider_id)
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    let model = models
        .into_iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| GatewayError::bad_request("Model not found"))?;

    if model.is_builtin {
        return Err(GatewayError::bad_request(
            "Cannot delete built-in models",
        ));
    }

    db::delete_model(&state.db, model_id)
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    // Reload providers after model deletion
    state.llm_service.reload_providers().await.ok();

    Ok(Json(serde_json::json!({ "message": "Model deleted" })))
}

/// PUT /api/v1/models/:id/models/:model_id/config
pub async fn update_model_config(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path((_provider_id, model_id)): Path<(i64, i64)>,
    Json(req): Json<UpdateModelConfigRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    db::update_model_config(&state.db, model_id, req.display_name.as_deref())
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    Ok(Json(serde_json::json!({ "message": "Model config updated" })))
}

/// POST /api/v1/models/:id/models/:model_id/probe-multimodal
pub async fn probe_multimodal(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path((provider_id, model_id)): Path<(i64, i64)>,
) -> Result<Json<ProbeMultimodalResponse>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    let models = db::get_models_for_provider(&state.db, provider_id)
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    let model = models
        .into_iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| GatewayError::bad_request("Model not found"))?;

    let (supports_image, supports_video, message) = state
        .llm_service
        .probe_model_multimodal(provider_id, &model.name)
        .await
        .map_err(|e| GatewayError::internal(format!("探测多模态能力失败: {}", e)))?;

    // Update model capabilities in database
    let _ = db::update_model_capabilities(
        &state.db,
        model_id,
        Some(supports_image),
        Some(supports_video),
        Some(supports_image || supports_video),
    )
    .await;

    Ok(Json(ProbeMultimodalResponse {
        success: true,
        supports_image,
        supports_video,
        message,
    }))
}

/// PUT /api/v1/models/:id/default
pub async fn set_default_provider(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    db::set_default_provider(&state.db, id)
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    // Reload providers after setting default
    state.llm_service.reload_providers().await.ok();

    Ok(Json(
        serde_json::json!({ "message": "Default provider set" }),
    ))
}

/// PUT /api/v1/models/:id/models/:model_id/default
pub async fn set_default_model(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path((provider_id, model_id)): Path<(i64, i64)>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    db::set_default_model(&state.db, provider_id, model_id)
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    // Reload providers after setting default model
    state.llm_service.reload_providers().await.ok();

    Ok(Json(serde_json::json!({ "message": "Default model set" })))
}

/// POST /api/v1/models/:id/test
pub async fn test_provider_connection(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    let (success, message) = state
        .llm_service
        .test_provider_connection(id)
        .await
        .map_err(|e| GatewayError::internal(format!("测试连接失败: {}", e)))?;

    Ok(Json(serde_json::json!({
        "success": success,
        "message": message,
    })))
}

/// POST /api/v1/models/:id/discover
pub async fn discover_models(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    let (models, added_count) = state
        .llm_service
        .discover_models(id)
        .await
        .map_err(|e| GatewayError::internal(format!("发现模型失败: {}", e)))?;

    // Reload providers after adding new models
    state.llm_service.reload_providers().await.ok();

    let model_list: Vec<serde_json::Value> = models
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "id": m.id,
                "name": m.name,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "success": true,
        "models": model_list,
        "added_count": added_count,
    })))
}

/// POST /api/v1/models/:id/models/:model_name/test
pub async fn test_model_connection(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path((provider_id, model_name)): Path<(i64, String)>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    let (success, message) = state
        .llm_service
        .test_model_connection(provider_id, &model_name)
        .await
        .map_err(|e| GatewayError::internal(format!("测试模型连接失败: {}", e)))?;

    Ok(Json(serde_json::json!({
        "success": success,
        "message": message,
    })))
}

/// GET /api/v1/models/active
pub async fn get_active_llm(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<Json<ActiveLlmResponse>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    let provider = db::get_default_provider(&state.db)
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?
        .ok_or_else(|| GatewayError::bad_request("No default provider set"))?;

    let model = db::get_default_model(&state.db, provider.id)
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    Ok(Json(ActiveLlmResponse {
        provider_id: provider.id,
        provider_name: provider.name,
        model_name: model.as_ref().map(|m| m.name.clone()).unwrap_or_default(),
        display_name: model.and_then(|m| m.display_name),
    }))
}

/// PUT /api/v1/models/active
pub async fn set_active_llm(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<SetActiveLlmRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    // Set default provider
    db::set_default_provider(&state.db, req.provider_id)
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    // Find model by name and set as default
    let models = db::get_models_for_provider(&state.db, req.provider_id)
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    let model = models
        .into_iter()
        .find(|m| m.name == req.model_name)
        .ok_or_else(|| GatewayError::bad_request("Model not found for provider"))?;

    db::set_default_model(&state.db, req.provider_id, model.id)
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    // Reload providers
    state.llm_service.reload_providers().await.ok();

    Ok(Json(serde_json::json!({ "message": "Active LLM updated" })))
}
