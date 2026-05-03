-- 扩展 LLM Provider 和 Model 表，支持更丰富的模型管理功能

-- 添加 is_custom 字段区分内置/自定义供应商
ALTER TABLE llm_providers ADD COLUMN is_custom BOOLEAN NOT NULL DEFAULT false;

-- 更新现有预设供应商（通过 type_label 判断内置）
UPDATE llm_providers SET is_custom = false;

-- 添加模型多模态能力标记
ALTER TABLE llm_models ADD COLUMN supports_image BOOLEAN DEFAULT NULL;
ALTER TABLE llm_models ADD COLUMN supports_video BOOLEAN DEFAULT NULL;
ALTER TABLE llm_models ADD COLUMN supports_multimodal BOOLEAN DEFAULT NULL;
