//! Skill Registry (OpenClaw Compatible)
//!
//! 中央注册表，支持 Skill 发现、管理和按来源优先级合并。

use std::collections::HashMap;

use tokio::sync::RwLock;

use crate::skills::loader::LoadedSkill;

/// Skill registry
pub struct SkillRegistry {
    skills: RwLock<HashMap<String, RegisteredSkill>>,
    categories: RwLock<HashMap<String, Vec<String>>>,
}

/// Semantic version
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl serde::Serialize for Version {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for Version {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Version::parse(&s).map_err(serde::de::Error::custom)
    }
}

impl Version {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub fn parse(version: &str) -> Result<Self, VersionError> {
        let parts: Vec<&str> = version.split('.').collect();
        if parts.is_empty() {
            return Err(VersionError::InvalidFormat(version.to_string()));
        }

        let major = parts[0]
            .parse()
            .map_err(|_| VersionError::InvalidNumber(parts[0].to_string()))?;
        let minor = parts
            .get(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let patch = parts
            .get(2)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Version errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum VersionError {
    #[error("Invalid version format: {0}")]
    InvalidFormat(String),
    #[error("Invalid version number: {0}")]
    InvalidNumber(String),
}

/// Skill definition for registry (向后兼容)
#[derive(Debug, Clone)]
pub struct SkillDefinition {
    pub id: String,
    pub name: String,
    pub version: Version,
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
}

/// Registered skill
#[derive(Debug, Clone)]
pub struct RegisteredSkill {
    pub skill: LoadedSkill,
    pub category: String,
    pub tags: Vec<String>,
    pub installed_at: u64,
    pub usage_count: u64,
    pub enabled: bool,
    /// 是否允许用户手动调用（来自 manifest.user_invocable）
    pub user_invocable: bool,
    /// Slash 命令列表（预留）
    pub slash_commands: Vec<String>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: RwLock::new(HashMap::new()),
            categories: RwLock::new(HashMap::new()),
        }
    }

    /// 注册一个 Skill
    pub async fn register(
        &self,
        skill: LoadedSkill,
        category: impl Into<String>,
        tags: Vec<String>,
    ) {
        let skill_name = skill.name.clone();
        let category = category.into();

        let registered = RegisteredSkill {
            user_invocable: skill.manifest.user_invocable,
            slash_commands: Vec::new(), // Phase 5 会填充
            skill,
            category: category.clone(),
            tags,
            installed_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or(std::time::Duration::from_secs(0))
                .as_secs(),
            usage_count: 0,
            enabled: true,
        };

        // Lock order: skills first, then categories to avoid deadlocks
        {
            let mut skills = self.skills.write().await;
            skills.insert(skill_name.clone(), registered);
        }

        {
            let mut categories = self.categories.write().await;
            categories
                .entry(category)
                .or_insert_with(Vec::new)
                .push(skill_name);
        }
    }

    /// 批量注册 Skill（用于 load_all 后一次性注册）
    pub async fn register_many(&self, skills: Vec<LoadedSkill>, source_label: &str) {
        for skill in skills {
            // 使用 source label 作为默认 category
            let category = format!("{}/{}", source_label, skill.source.label());
            let tags = vec![skill.source.label().to_string()];
            self.register(skill, category, tags).await;
        }
    }

    /// 按来源加载并注册所有 Skill（支持优先级合并）
    pub async fn load_all(
        &self, sources: Vec<crate::skills::loader::SkillSourceDir>,
    ) -> Result<usize, crate::skills::loader::SkillLoadError> {
        use crate::skills::loader::SkillLoader;

        let loaded = SkillLoader::load_all_skills(sources).await?;
        let count = loaded.len();

        for skill in loaded {
            let category = skill.source.label().to_string();
            let tags = vec![category.clone()];
            self.register(skill, category, tags).await;
        }

        Ok(count)
    }

    /// Get skill by name (OpenClaw 使用 name 作为唯一标识)
    pub async fn get(&self, skill_name: &str) -> Option<RegisteredSkill> {
        let skills = self.skills.read().await;
        skills.get(skill_name).cloned()
    }

    /// 向后兼容：按 id 查找（OpenClaw 中 id == name）
    pub async fn get_by_id(&self, skill_id: &str) -> Option<RegisteredSkill> {
        self.get(skill_id).await
    }

    /// Find skills by category
    pub async fn by_category(&self, category: &str) -> Vec<RegisteredSkill> {
        let categories = self.categories.read().await;
        let skills = self.skills.read().await;

        categories
            .get(category)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| skills.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Find skills by tag
    pub async fn by_tag(&self, tag: &str) -> Vec<RegisteredSkill> {
        let skills = self.skills.read().await;
        let tag = tag.to_string();
        skills
            .values()
            .filter(|s| s.tags.contains(&tag))
            .cloned()
            .collect()
    }

    /// Search skills by name or description with keyword overlap scoring.
    /// 🆕 FIX: 适配 OpenClaw 格式（无 capabilities 字段）
    pub async fn search(&self, query: &str) -> Vec<RegisteredSkill> {
        let skills = self.skills.read().await;
        let query_lower = query.to_lowercase();
        let query_words: std::collections::HashSet<String> = query_lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() >= 2)
            .map(|w| w.to_string())
            .collect();

        let mut scored: Vec<(usize, RegisteredSkill)> = skills
            .values()
            .filter_map(|s| {
                let name_lower = s.skill.name.to_lowercase();
                let desc_lower = s.skill.manifest.description.to_lowercase();

                // Direct substring match gets highest priority
                if name_lower.contains(&query_lower)
                    || desc_lower.contains(&query_lower)
                {
                    return Some((100, s.clone()));
                }

                // Keyword overlap scoring
                let text = format!("{} {}", name_lower, desc_lower);
                let text_words: std::collections::HashSet<String> = text
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|w| w.len() >= 2)
                    .map(|w| w.to_string())
                    .collect();

                let overlap = query_words.intersection(&text_words).count();
                if overlap > 0 {
                    Some((overlap, s.clone()))
                } else {
                    None
                }
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().map(|(_, s)| s).collect()
    }

    /// List all skills
    pub async fn list_all(&self) -> Vec<RegisteredSkill> {
        let skills = self.skills.read().await;
        skills.values().cloned().collect()
    }

    /// List only enabled skills
    pub async fn list_enabled(&self) -> Vec<RegisteredSkill> {
        let skills = self.skills.read().await;
        skills.values().filter(|s| s.enabled).cloned().collect()
    }

    /// List user-invocable skills
    pub async fn list_user_invocable(&self) -> Vec<RegisteredSkill> {
        let skills = self.skills.read().await;
        skills
            .values()
            .filter(|s| s.enabled && s.user_invocable)
            .cloned()
            .collect()
    }

    /// Increment usage count
    pub async fn record_usage(&self, skill_name: &str) {
        let mut skills = self.skills.write().await;
        if let Some(skill) = skills.get_mut(skill_name) {
            skill.usage_count += 1;
        }
    }

    /// Enable a skill
    pub async fn enable(&self, skill_name: &str) -> bool {
        let mut skills = self.skills.write().await;
        if let Some(skill) = skills.get_mut(skill_name) {
            skill.enabled = true;
            true
        } else {
            false
        }
    }

    /// Disable a skill
    pub async fn disable(&self, skill_name: &str) -> bool {
        let mut skills = self.skills.write().await;
        if let Some(skill) = skills.get_mut(skill_name) {
            skill.enabled = false;
            true
        } else {
            false
        }
    }

    /// Unregister skill
    pub async fn unregister(&self, skill_name: &str) -> Option<RegisteredSkill> {
        let mut skills = self.skills.write().await;
        let removed = skills.remove(skill_name);
        drop(skills);

        if removed.is_some() {
            let mut categories = self.categories.write().await;
            for ids in categories.values_mut() {
                ids.retain(|id| id != skill_name);
            }
            categories.retain(|_, ids| !ids.is_empty());
        }

        removed
    }

    /// Get categories
    pub async fn categories(&self) -> Vec<String> {
        let categories = self.categories.read().await;
        categories.keys().cloned().collect()
    }

    /// 获取所有 skill 名称列表
    pub async fn skill_names(&self) -> Vec<String> {
        let skills = self.skills.read().await;
        skills.keys().cloned().collect()
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}
