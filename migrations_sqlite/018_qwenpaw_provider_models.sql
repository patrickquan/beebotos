-- 仿照 QwenPaw 数据模型扩展 LLM Provider 和 Model 表

-- 供应商级别字段
ALTER TABLE llm_providers ADD COLUMN generate_kwargs TEXT DEFAULT NULL;
ALTER TABLE llm_providers ADD COLUMN support_model_discovery BOOLEAN NOT NULL DEFAULT true;
ALTER TABLE llm_providers ADD COLUMN support_connection_check BOOLEAN NOT NULL DEFAULT true;
ALTER TABLE llm_providers ADD COLUMN freeze_url BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE llm_providers ADD COLUMN require_api_key BOOLEAN NOT NULL DEFAULT true;

-- 更新预设供应商的特性标记
UPDATE llm_providers SET require_api_key = false WHERE provider_id = 'ollama';
UPDATE llm_providers SET freeze_url = true WHERE provider_id IN ('openai', 'anthropic', 'kimi', 'kimi-china', 'deepseek', 'zhipu', 'ollama');
UPDATE llm_providers SET support_model_discovery = false WHERE provider_id = 'anthropic';

-- 模型级别：区分内置模型与用户添加模型
ALTER TABLE llm_models ADD COLUMN is_builtin BOOLEAN NOT NULL DEFAULT false;

-- 将当前已存在的预设模型标记为内置
UPDATE llm_models SET is_builtin = true WHERE name IN (
    'kimi-k2.5', 'gpt-4o', 'glm-4', 'deepseek-chat',
    'claude-3-5-sonnet-20241022', 'llama3.2'
);
