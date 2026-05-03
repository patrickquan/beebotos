//! LLM Provider Database Access Layer (QwenPaw-style)

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

/// Database model for LLM provider
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct LlmProviderDb {
    pub id: i64,
    pub provider_id: String,
    pub name: String,
    pub protocol: String,
    pub base_url: Option<String>,
    pub api_key_encrypted: Option<String>,
    pub enabled: bool,
    pub is_default_provider: bool,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
    pub type_label: Option<String>,
    pub is_custom: bool,
    pub generate_kwargs: Option<String>,
    pub support_model_discovery: bool,
    pub support_connection_check: bool,
    pub freeze_url: bool,
    pub require_api_key: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Database model for LLM model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct LlmModelDb {
    pub id: i64,
    pub provider_id: i64,
    pub name: String,
    pub display_name: Option<String>,
    pub is_default_model: bool,
    pub supports_image: Option<bool>,
    pub supports_video: Option<bool>,
    pub supports_multimodal: Option<bool>,
    pub is_builtin: bool,
    pub created_at: String,
}

/// Preset models for each provider: (provider_id, model_name, display_name, is_default, supports_image, supports_video)
const PRESET_ALL_MODELS: &[(&str, &str, Option<&str>, bool, bool, bool)] = &[
    // ModelScope 模型 (2个)
    (
        "modelscope",
        "Qwen/Qwen3.5-122B-A10B",
        Some("Qwen3.5-122B-A10B"),
        true,
        true,
        true,
    ),
    ("modelscope", "ZhipuAI/GLM-5", Some("GLM-5"), false, false, false),
    // DashScope 模型 (3个)
    ("dashscope", "qwen3-max", Some("Qwen3 Max"), true, false, false),
    (
        "dashscope",
        "qwen3-235b-a22b-thinking-2507",
        Some("Qwen3 235B A22B Thinking"),
        false,
        false,
        false,
    ),
    (
        "dashscope",
        "deepseek-v3.2",
        Some("DeepSeek-V3.2"),
        false,
        false,
        false,
    ),
    // Aliyun Coding Plan 模型 (9个)
    (
        "aliyun-codingplan",
        "qwen3.6-plus",
        Some("Qwen3.6 Plus"),
        true,
        true,
        true,
    ),
    (
        "aliyun-codingplan",
        "qwen3.5-plus",
        Some("Qwen3.5 Plus"),
        false,
        true,
        true,
    ),
    ("aliyun-codingplan", "glm-5", Some("GLM-5"), false, false, false),
    ("aliyun-codingplan", "glm-4.7", Some("GLM-4.7"), false, false, false),
    (
        "aliyun-codingplan",
        "MiniMax-M2.5",
        Some("MiniMax M2.5"),
        false,
        false,
        false,
    ),
    (
        "aliyun-codingplan",
        "kimi-k2.5",
        Some("Kimi K2.5"),
        false,
        true,
        true,
    ),
    (
        "aliyun-codingplan",
        "qwen3-max-2026-01-23",
        Some("Qwen3 Max 2026-01-23"),
        false,
        false,
        false,
    ),
    (
        "aliyun-codingplan",
        "qwen3-coder-next",
        Some("Qwen3 Coder Next"),
        false,
        false,
        false,
    ),
    (
        "aliyun-codingplan",
        "qwen3-coder-plus",
        Some("Qwen3 Coder Plus"),
        false,
        false,
        false,
    ),
    // Aliyun Coding Plan Intl 模型 (9个)
    (
        "aliyun-codingplan-intl",
        "qwen3.6-plus",
        Some("Qwen3.6 Plus"),
        true,
        true,
        true,
    ),
    (
        "aliyun-codingplan-intl",
        "qwen3.5-plus",
        Some("Qwen3.5 Plus"),
        false,
        true,
        true,
    ),
    (
        "aliyun-codingplan-intl",
        "glm-5",
        Some("GLM-5"),
        false,
        false,
        false,
    ),
    (
        "aliyun-codingplan-intl",
        "glm-4.7",
        Some("GLM-4.7"),
        false,
        false,
        false,
    ),
    (
        "aliyun-codingplan-intl",
        "MiniMax-M2.5",
        Some("MiniMax M2.5"),
        false,
        false,
        false,
    ),
    (
        "aliyun-codingplan-intl",
        "kimi-k2.5",
        Some("Kimi K2.5"),
        false,
        true,
        true,
    ),
    (
        "aliyun-codingplan-intl",
        "qwen3-max-2026-01-23",
        Some("Qwen3 Max 2026-01-23"),
        false,
        false,
        false,
    ),
    (
        "aliyun-codingplan-intl",
        "qwen3-coder-next",
        Some("Qwen3 Coder Next"),
        false,
        false,
        false,
    ),
    (
        "aliyun-codingplan-intl",
        "qwen3-coder-plus",
        Some("Qwen3 Coder Plus"),
        false,
        false,
        false,
    ),
    // Zhipu (BigModel) 模型 (4个)
    ("zhipu-cn", "glm-5", Some("GLM-5"), false, false, false),
    ("zhipu-cn", "glm-5.1", Some("GLM-5.1"), false, false, false),
    ("zhipu-cn", "glm-5-turbo", Some("GLM-5 Turbo"), false, false, false),
    ("zhipu-cn", "glm-5v-turbo", Some("GLM-5V Turbo"), true, true, false),
    // Zhipu Coding Plan (BigModel) 模型 (4个)
    (
        "zhipu-cn-codingplan",
        "glm-5",
        Some("GLM-5"),
        false,
        false,
        false,
    ),
    (
        "zhipu-cn-codingplan",
        "glm-5.1",
        Some("GLM-5.1"),
        false,
        false,
        false,
    ),
    (
        "zhipu-cn-codingplan",
        "glm-5-turbo",
        Some("GLM-5 Turbo"),
        false,
        false,
        false,
    ),
    (
        "zhipu-cn-codingplan",
        "glm-5v-turbo",
        Some("GLM-5V Turbo"),
        true,
        true,
        false,
    ),
    // Zhipu (Z.AI) 模型 (4个)
    ("zhipu-intl", "glm-5", Some("GLM-5"), false, false, false),
    ("zhipu-intl", "glm-5.1", Some("GLM-5.1"), false, false, false),
    (
        "zhipu-intl",
        "glm-5-turbo",
        Some("GLM-5 Turbo"),
        false,
        false,
        false,
    ),
    (
        "zhipu-intl",
        "glm-5v-turbo",
        Some("GLM-5V Turbo"),
        true,
        true,
        false,
    ),
    // Zhipu Coding Plan (Z.AI) 模型 (4个)
    (
        "zhipu-intl-codingplan",
        "glm-5",
        Some("GLM-5"),
        false,
        false,
        false,
    ),
    (
        "zhipu-intl-codingplan",
        "glm-5.1",
        Some("GLM-5.1"),
        false,
        false,
        false,
    ),
    (
        "zhipu-intl-codingplan",
        "glm-5-turbo",
        Some("GLM-5 Turbo"),
        false,
        false,
        false,
    ),
    (
        "zhipu-intl-codingplan",
        "glm-5v-turbo",
        Some("GLM-5V Turbo"),
        true,
        true,
        false,
    ),
    // OpenAI 模型 (11个)
    ("openai", "gpt-5.2", Some("GPT-5.2"), false, true, true),
    ("openai", "gpt-5", Some("GPT-5"), false, true, true),
    ("openai", "gpt-5-mini", Some("GPT-5 Mini"), false, true, true),
    ("openai", "gpt-5-nano", Some("GPT-5 Nano"), false, true, true),
    ("openai", "gpt-4.1", Some("GPT-4.1"), false, true, true),
    ("openai", "gpt-4.1-mini", Some("GPT-4.1 Mini"), false, true, true),
    ("openai", "gpt-4.1-nano", Some("GPT-4.1 Nano"), false, true, true),
    ("openai", "o3", Some("o3"), false, true, false),
    ("openai", "o4-mini", Some("o4-mini"), false, true, true),
    ("openai", "gpt-4o", Some("GPT-4o"), true, true, true),
    ("openai", "gpt-4o-mini", Some("GPT-4o Mini"), false, true, true),
    // OpenCode 模型 (2个)
    ("opencode", "big-pickle", Some("Big Pickle"), true, false, false),
    (
        "opencode",
        "nemotron-3-super-free",
        Some("Nemotron 3 Super Free"),
        false,
        false,
        false,
    ),
    // Azure OpenAI 模型 (8个)
    (
        "azure-openai",
        "gpt-5-chat",
        Some("GPT-5 Chat"),
        true,
        true,
        true,
    ),
    (
        "azure-openai",
        "gpt-5-mini",
        Some("GPT-5 Mini"),
        false,
        true,
        true,
    ),
    (
        "azure-openai",
        "gpt-5-nano",
        Some("GPT-5 Nano"),
        false,
        true,
        true,
    ),
    ("azure-openai", "gpt-4.1", Some("GPT-4.1"), false, true, true),
    (
        "azure-openai",
        "gpt-4.1-mini",
        Some("GPT-4.1 Mini"),
        false,
        true,
        true,
    ),
    (
        "azure-openai",
        "gpt-4.1-nano",
        Some("GPT-4.1 Nano"),
        false,
        true,
        true,
    ),
    ("azure-openai", "gpt-4o", Some("GPT-4o"), false, true, true),
    (
        "azure-openai",
        "gpt-4o-mini",
        Some("GPT-4o Mini"),
        false,
        true,
        true,
    ),
    // Google Gemini 模型 (7个)
    (
        "gemini",
        "gemini-3.1-pro-preview",
        Some("Gemini 3.1 Pro Preview"),
        true,
        true,
        true,
    ),
    (
        "gemini",
        "gemini-3-flash-preview",
        Some("Gemini 3 Flash Preview"),
        false,
        true,
        true,
    ),
    (
        "gemini",
        "gemini-3.1-flash-lite-preview",
        Some("Gemini 3.1 Flash Lite Preview"),
        false,
        true,
        true,
    ),
    (
        "gemini",
        "gemini-2.5-pro",
        Some("Gemini 2.5 Pro"),
        false,
        true,
        true,
    ),
    (
        "gemini",
        "gemini-2.5-flash",
        Some("Gemini 2.5 Flash"),
        false,
        true,
        true,
    ),
    (
        "gemini",
        "gemini-2.5-flash-lite",
        Some("Gemini 2.5 Flash Lite"),
        false,
        true,
        true,
    ),
    (
        "gemini",
        "gemini-2.0-flash",
        Some("Gemini 2.0 Flash"),
        false,
        true,
        true,
    ),
    // MiniMax Intl 模型 (4个)
    (
        "minimax",
        "MiniMax-M2.5",
        Some("MiniMax M2.5"),
        true,
        false,
        false,
    ),
    (
        "minimax",
        "MiniMax-M2.5-highspeed",
        Some("MiniMax M2.5 Highspeed"),
        false,
        false,
        false,
    ),
    (
        "minimax",
        "MiniMax-M2.7",
        Some("MiniMax M2.7"),
        false,
        false,
        false,
    ),
    (
        "minimax",
        "MiniMax-M2.7-highspeed",
        Some("MiniMax M2.7 Highspeed"),
        false,
        false,
        false,
    ),
    // MiniMax China 模型 (4个)
    (
        "minimax-cn",
        "MiniMax-M2.5",
        Some("MiniMax M2.5"),
        true,
        false,
        false,
    ),
    (
        "minimax-cn",
        "MiniMax-M2.5-highspeed",
        Some("MiniMax M2.5 Highspeed"),
        false,
        false,
        false,
    ),
    (
        "minimax-cn",
        "MiniMax-M2.7",
        Some("MiniMax M2.7"),
        false,
        false,
        false,
    ),
    (
        "minimax-cn",
        "MiniMax-M2.7-highspeed",
        Some("MiniMax M2.7 Highspeed"),
        false,
        false,
        false,
    ),
    // Kimi (China) 模型 (6个)
    ("kimi-cn", "kimi-k2.5", Some("Kimi K2.5"), true, true, true),
    (
        "kimi-cn",
        "kimi-k2-0905-preview",
        Some("Kimi K2 0905 Preview"),
        false,
        false,
        false,
    ),
    (
        "kimi-cn",
        "kimi-k2-0711-preview",
        Some("Kimi K2 0711 Preview"),
        false,
        false,
        false,
    ),
    (
        "kimi-cn",
        "kimi-k2-turbo-preview",
        Some("Kimi K2 Turbo Preview"),
        false,
        false,
        false,
    ),
    (
        "kimi-cn",
        "kimi-k2-thinking",
        Some("Kimi K2 Thinking"),
        false,
        false,
        false,
    ),
    (
        "kimi-cn",
        "kimi-k2-thinking-turbo",
        Some("Kimi K2 Thinking Turbo"),
        false,
        false,
        false,
    ),
    // Kimi (International) 模型 (6个)
    ("kimi-intl", "kimi-k2.5", Some("Kimi K2.5"), true, true, true),
    (
        "kimi-intl",
        "kimi-k2-0905-preview",
        Some("Kimi K2 0905 Preview"),
        false,
        false,
        false,
    ),
    (
        "kimi-intl",
        "kimi-k2-0711-preview",
        Some("Kimi K2 0711 Preview"),
        false,
        false,
        false,
    ),
    (
        "kimi-intl",
        "kimi-k2-turbo-preview",
        Some("Kimi K2 Turbo Preview"),
        false,
        false,
        false,
    ),
    (
        "kimi-intl",
        "kimi-k2-thinking",
        Some("Kimi K2 Thinking"),
        false,
        false,
        false,
    ),
    (
        "kimi-intl",
        "kimi-k2-thinking-turbo",
        Some("Kimi K2 Thinking Turbo"),
        false,
        false,
        false,
    ),
    // DeepSeek 模型 (4个)
    (
        "deepseek",
        "deepseek-chat",
        Some("DeepSeek Chat"),
        true,
        false,
        false,
    ),
    (
        "deepseek",
        "deepseek-reasoner",
        Some("DeepSeek Reasoner"),
        false,
        false,
        false,
    ),
    (
        "deepseek",
        "deepseek-v4-flash",
        Some("DeepSeek V4 Flash"),
        false,
        false,
        false,
    ),
    (
        "deepseek",
        "deepseek-v4-pro",
        Some("DeepSeek V4 Pro"),
        false,
        false,
        false,
    ),
    // Anthropic 模型 (4个)
    (
        "anthropic",
        "claude-3-5-sonnet-20241022",
        Some("Claude 3.5 Sonnet"),
        true,
        false,
        false,
    ),
    (
        "anthropic",
        "claude-3-7-sonnet-20250219",
        Some("Claude 3.7 Sonnet"),
        false,
        false,
        false,
    ),
    (
        "anthropic",
        "claude-3-opus-20240229",
        Some("Claude 3 Opus"),
        false,
        false,
        false,
    ),
    (
        "anthropic",
        "claude-3-5-haiku-20241022",
        Some("Claude 3.5 Haiku"),
        false,
        false,
        false,
    ),
    // Ollama
    ("ollama", "llama3.2", Some("Llama 3.2"), true, false, false),
    // LM Studio (空，支持模型发现)
    // OpenRouter (空)
    // SiliconFlow China (空)
    // SiliconFlow Intl (空)
];

/// Preset provider data: (provider_id, name, protocol, base_url, icon, icon_color, type_label, require_api_key, freeze_url, support_model_discovery, support_connection_check)
const PRESET_PROVIDERS: &[(&str, &str, &str, &str, &str, &str, &str, bool, bool, bool, bool)] = &[
    // 内置提供商
    (
        "ollama",
        "Ollama",
        "openai-compatible",
        "http://localhost:11434",
        "🦙",
        "#ff6b6b",
        "内置",
        false,
        true,
        true,
        true,
    ),
    (
        "lmstudio",
        "LM Studio",
        "openai-compatible",
        "http://localhost:1234/v1",
        "💻",
        "#6366f1",
        "内置",
        false,
        true,
        true,
        true,
    ),
    (
        "kimi-cn",
        "Kimi (China)",
        "openai-compatible",
        "https://api.moonshot.cn/v1",
        "🌙",
        "#4f6ef7",
        "内置",
        true,
        true,
        true,
        true,
    ),
    (
        "kimi-intl",
        "Kimi (International)",
        "openai-compatible",
        "https://api.moonshot.ai/v1",
        "🌙",
        "#4f6ef7",
        "内置",
        true,
        true,
        true,
        true,
    ),
    (
        "openai",
        "OpenAI",
        "openai-compatible",
        "https://api.openai.com/v1",
        "🤖",
        "#10a37f",
        "内置",
        true,
        true,
        true,
        true,
    ),
    (
        "zhipu-cn",
        "Zhipu (BigModel)",
        "openai-compatible",
        "https://open.bigmodel.cn/api/paas/v4",
        "🧠",
        "#3b82f6",
        "内置",
        true,
        true,
        true,
        true,
    ),
    (
        "zhipu-cn-codingplan",
        "Zhipu Coding Plan (BigModel)",
        "openai-compatible",
        "https://open.bigmodel.cn/api/coding/paas/v4",
        "💻",
        "#3b82f6",
        "内置",
        true,
        true,
        false,
        false,
    ),
    (
        "zhipu-intl",
        "Zhipu (Z.AI)",
        "openai-compatible",
        "https://api.z.ai/api/paas/v4",
        "🌐",
        "#3b82f6",
        "内置",
        true,
        true,
        true,
        true,
    ),
    (
        "zhipu-intl-codingplan",
        "Zhipu Coding Plan (Z.AI)",
        "openai-compatible",
        "https://api.z.ai/api/coding/paas/v4",
        "💻",
        "#3b82f6",
        "内置",
        true,
        true,
        false,
        false,
    ),
    (
        "deepseek",
        "DeepSeek",
        "openai-compatible",
        "https://api.deepseek.com/v1",
        "🔍",
        "#4d6bfa",
        "内置",
        true,
        true,
        true,
        true,
    ),
    (
        "anthropic",
        "Anthropic",
        "anthropic",
        "https://api.anthropic.com/v1",
        "🅰️",
        "#d4a574",
        "内置",
        true,
        true,
        false,
        true,
    ),
    (
        "gemini",
        "Google Gemini",
        "openai-compatible",
        "https://generativelanguage.googleapis.com",
        "♊",
        "#4285f4",
        "内置",
        true,
        true,
        false,
        true,
    ),
    (
        "minimax",
        "MiniMax (International)",
        "anthropic",
        "https://api.minimax.io/anthropic",
        "🎭",
        "#7c3aed",
        "内置",
        true,
        true,
        false,
        false,
    ),
    (
        "minimax-cn",
        "MiniMax (China)",
        "anthropic",
        "https://api.minimaxi.com/anthropic",
        "🎭",
        "#7c3aed",
        "内置",
        true,
        true,
        false,
        false,
    ),
    (
        "openrouter",
        "OpenRouter",
        "openai-compatible",
        "https://openrouter.ai/api/v1",
        "🔀",
        "#f59e0b",
        "内置",
        true,
        true,
        true,
        true,
    ),
    (
        "modelscope",
        "ModelScope",
        "openai-compatible",
        "https://api-inference.modelscope.cn/v1",
        "🔬",
        "#8b5cf6",
        "内置",
        true,
        true,
        true,
        true,
    ),
    (
        "dashscope",
        "DashScope",
        "openai-compatible",
        "https://dashscope.aliyuncs.com/compatible-mode/v1",
        "🌐",
        "#1677ff",
        "内置",
        true,
        true,
        true,
        true,
    ),
    (
        "aliyun-codingplan",
        "Aliyun Coding Plan (China)",
        "openai-compatible",
        "https://coding.dashscope.aliyuncs.com/v1",
        "☁️",
        "#ff6a00",
        "内置",
        true,
        true,
        false,
        false,
    ),
    (
        "aliyun-codingplan-intl",
        "Aliyun Coding Plan (International)",
        "openai-compatible",
        "https://coding-intl.dashscope.aliyuncs.com/v1",
        "🌍",
        "#ff6a00",
        "内置",
        true,
        true,
        false,
        false,
    ),
    (
        "opencode",
        "OpenCode",
        "openai-compatible",
        "https://opencode.ai/zen/v1",
        "🔓",
        "#10b981",
        "内置",
        false,
        true,
        true,
        true,
    ),
    (
        "azure-openai",
        "Azure OpenAI",
        "openai-compatible",
        "",
        "🔷",
        "#0078d4",
        "内置",
        true,
        false,
        false,
        true,
    ),
    (
        "siliconflow-cn",
        "SiliconFlow (China)",
        "openai-compatible",
        "https://api.siliconflow.cn/v1",
        "🌊",
        "#2563eb",
        "内置",
        true,
        true,
        true,
        true,
    ),
    (
        "siliconflow-intl",
        "SiliconFlow (International)",
        "openai-compatible",
        "https://api.siliconflow.com/v1",
        "🌊",
        "#2563eb",
        "内置",
        true,
        true,
        true,
        true,
    ),
];

/// Seed preset providers and their default models into database if they don't exist
pub async fn seed_providers(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // 清理旧的内置提供商（已重命名或移除的）
    let old_provider_ids = ["kimi", "kimi-china", "zhipu", "qwenpaw-local"];
    for old_id in &old_provider_ids {
        sqlx::query("DELETE FROM llm_providers WHERE provider_id = ? AND is_custom = false")
            .bind(old_id)
            .execute(pool)
            .await?;
    }

    for (
        provider_id,
        name,
        protocol,
        base_url,
        icon,
        icon_color,
        type_label,
        require_api_key,
        freeze_url,
        support_model_discovery,
        support_connection_check,
    ) in PRESET_PROVIDERS
    {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM llm_providers WHERE provider_id = ?)")
                .bind(provider_id)
                .fetch_one(pool)
                .await?;

        if !exists {
            sqlx::query(
                "INSERT INTO llm_providers (
                    provider_id, name, protocol, base_url, enabled,
                    is_default_provider, icon, icon_color, type_label, is_custom,
                    require_api_key, freeze_url, support_model_discovery, support_connection_check
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(provider_id)
            .bind(name)
            .bind(protocol)
            // Handle empty string as NULL for optional base_url
            .bind(if base_url.is_empty() { None } else { Some(*base_url) })
            .bind(true)
            .bind(false)
            .bind(icon)
            .bind(icon_color)
            .bind(type_label)
            .bind(false)
            .bind(require_api_key)
            .bind(freeze_url)
            .bind(support_model_discovery)
            .bind(support_connection_check)
            .execute(pool)
            .await?;
        }
    }

    // Seed default models for preset providers
    seed_default_models(pool).await?;

    Ok(())
}

/// Seed all preset models for preset providers
async fn seed_default_models(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    for (provider_id, model_name, display_name, is_default, supports_image, supports_video) in
        PRESET_ALL_MODELS
    {
        let provider_row: Option<(i64,)> =
            sqlx::query_as("SELECT id FROM llm_providers WHERE provider_id = ?")
                .bind(provider_id)
                .fetch_optional(pool)
                .await?;

        let Some((provider_db_id,)) = provider_row else {
            continue;
        };

        let model_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM llm_models WHERE provider_id = ? AND name = ?)",
        )
        .bind(provider_db_id)
        .bind(model_name)
        .fetch_one(pool)
        .await?;

        if !model_exists {
            sqlx::query(
                "INSERT INTO llm_models (
                    provider_id, name, display_name, is_default_model,
                    supports_image, supports_video, is_builtin
                ) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(provider_db_id)
            .bind(model_name)
            .bind(display_name)
            .bind(*is_default)
            .bind(*supports_image)
            .bind(*supports_video)
            .bind(true)
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

/// List all providers with their models
pub async fn list_providers_with_models(
    pool: &SqlitePool,
) -> Result<Vec<(LlmProviderDb, Vec<LlmModelDb>)>, sqlx::Error> {
    let providers: Vec<LlmProviderDb> =
        sqlx::query_as("SELECT * FROM llm_providers ORDER BY created_at")
            .fetch_all(pool)
            .await?;

    let mut result = Vec::new();
    for provider in providers {
        let models: Vec<LlmModelDb> =
            sqlx::query_as("SELECT * FROM llm_models WHERE provider_id = ? ORDER BY created_at")
                .bind(provider.id)
                .fetch_all(pool)
                .await?;
        result.push((provider, models));
    }
    Ok(result)
}

/// Get provider by ID
pub async fn get_provider_by_id(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<LlmProviderDb>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM llm_providers WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Create a custom provider
pub async fn create_provider(
    pool: &SqlitePool,
    provider_id: &str,
    name: &str,
    protocol: &str,
    base_url: Option<&str>,
    api_key_encrypted: Option<&str>,
) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO llm_providers (
            provider_id, name, protocol, base_url, api_key_encrypted,
            enabled, is_custom, require_api_key, freeze_url,
            support_model_discovery, support_connection_check
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(provider_id)
    .bind(name)
    .bind(protocol)
    .bind(base_url)
    .bind(api_key_encrypted)
    .bind(true)
    .bind(true)
    .bind(true)
    .bind(false)
    .bind(true)
    .bind(true)
    .execute(pool)
    .await?;

    Ok(result.last_insert_rowid())
}

/// Update provider basic fields
pub async fn update_provider(
    pool: &SqlitePool,
    id: i64,
    name: Option<&str>,
    base_url: Option<&str>,
    api_key_encrypted: Option<&str>,
    enabled: Option<bool>,
) -> Result<(), sqlx::Error> {
    let mut updates = Vec::new();
    let mut query = String::from("UPDATE llm_providers SET updated_at = CURRENT_TIMESTAMP");

    if let Some(name) = name {
        query.push_str(", name = ?");
        updates.push(name.to_string());
    }
    if let Some(base_url) = base_url {
        query.push_str(", base_url = ?");
        updates.push(base_url.to_string());
    }
    if let Some(api_key) = api_key_encrypted {
        query.push_str(", api_key_encrypted = ?");
        updates.push(api_key.to_string());
    }
    if let Some(enabled) = enabled {
        query.push_str(", enabled = ?");
        updates.push(if enabled { "1" } else { "0" }.to_string());
    }

    query.push_str(" WHERE id = ?");

    let mut q = sqlx::query(&query);
    for val in &updates {
        q = q.bind(val);
    }
    q.bind(id).execute(pool).await?;
    Ok(())
}

/// Update provider config (generate_kwargs, etc.)
pub async fn update_provider_config(
    pool: &SqlitePool,
    id: i64,
    generate_kwargs: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE llm_providers SET generate_kwargs = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(generate_kwargs)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete provider (cascades to models)
pub async fn delete_provider(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM llm_providers WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Set default provider (clears previous default)
pub async fn set_default_provider(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE llm_providers SET is_default_provider = false")
        .execute(pool)
        .await?;
    sqlx::query("UPDATE llm_providers SET is_default_provider = true WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Add model to provider
pub async fn add_model(
    pool: &SqlitePool,
    provider_id: i64,
    name: &str,
    display_name: Option<&str>,
) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO llm_models (provider_id, name, display_name, is_builtin) VALUES (?, ?, ?, ?)",
    )
    .bind(provider_id)
    .bind(name)
    .bind(display_name)
    .bind(false)
    .execute(pool)
    .await?;

    Ok(result.last_insert_rowid())
}

/// Delete model
pub async fn delete_model(pool: &SqlitePool, model_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM llm_models WHERE id = ?")
        .bind(model_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Set default model for provider
pub async fn set_default_model(
    pool: &SqlitePool,
    provider_id: i64,
    model_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE llm_models SET is_default_model = false WHERE provider_id = ?")
        .bind(provider_id)
        .execute(pool)
        .await?;
    sqlx::query("UPDATE llm_models SET is_default_model = true WHERE id = ? AND provider_id = ?")
        .bind(model_id)
        .bind(provider_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Update model config (display_name, etc.)
pub async fn update_model_config(
    pool: &SqlitePool,
    model_id: i64,
    display_name: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE llm_models SET display_name = ? WHERE id = ?")
        .bind(display_name)
        .bind(model_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Get default provider
pub async fn get_default_provider(pool: &SqlitePool) -> Result<Option<LlmProviderDb>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM llm_providers WHERE is_default_provider = true LIMIT 1")
        .fetch_optional(pool)
        .await
}

/// Get default model for provider
pub async fn get_default_model(
    pool: &SqlitePool,
    provider_id: i64,
) -> Result<Option<LlmModelDb>, sqlx::Error> {
    sqlx::query_as(
        "SELECT * FROM llm_models WHERE provider_id = ? AND is_default_model = true LIMIT 1",
    )
    .bind(provider_id)
    .fetch_optional(pool)
    .await
}

/// Get all models for a provider
pub async fn get_models_for_provider(
    pool: &SqlitePool,
    provider_id: i64,
) -> Result<Vec<LlmModelDb>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM llm_models WHERE provider_id = ? ORDER BY created_at")
        .bind(provider_id)
        .fetch_all(pool)
        .await
}

/// Get provider with models by ID
pub async fn get_provider_with_models(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<(LlmProviderDb, Vec<LlmModelDb>)>, sqlx::Error> {
    let provider: Option<LlmProviderDb> =
        sqlx::query_as("SELECT * FROM llm_providers WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;

    let Some(provider) = provider else {
        return Ok(None);
    };

    let models: Vec<LlmModelDb> =
        sqlx::query_as("SELECT * FROM llm_models WHERE provider_id = ? ORDER BY created_at")
            .bind(id)
            .fetch_all(pool)
            .await?;

    Ok(Some((provider, models)))
}

/// Update model multimodal flags
pub async fn update_model_capabilities(
    pool: &SqlitePool,
    model_id: i64,
    supports_image: Option<bool>,
    supports_video: Option<bool>,
    supports_multimodal: Option<bool>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE llm_models SET supports_image = ?, supports_video = ?, supports_multimodal = ? WHERE id = ?",
    )
    .bind(supports_image)
    .bind(supports_video)
    .bind(supports_multimodal)
    .bind(model_id)
    .execute(pool)
    .await?;
    Ok(())
}
