//! Skill Loader (OpenClaw Compatible)
//!
//! 从目录加载 OpenClaw 格式的 Skill（YAML frontmatter + Markdown body）。
//! 支持 6 个来源优先级：extra > bundled > managed > personal > project > workspace。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::skills::registry::Version;

// ───────────────────────────────────────────────────────────────
// 辅助函数
// ───────────────────────────────────────────────────────────────

/// 获取绝对路径，但不使用 `canonicalize()`（避免 Windows UNC 路径 \\?\E:\...）
fn normalize_abs_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(path)
    }
}

// ───────────────────────────────────────────────────────────────
// 类型定义
// ───────────────────────────────────────────────────────────────

/// Skill 来源优先级（数值越大优先级越高）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SkillSource {
    /// 安装包自带（最低优先级）
    Bundled = 1,
    /// ClawHub / BeeHub 安装
    Managed = 2,
    /// 用户个人目录 ~/.agents/skills
    Personal = 3,
    /// 项目级目录 <workspace>/.agents/skills
    Project = 4,
    /// 工作空间目录 <workspace>/skills
    Workspace = 5,
    /// 额外配置目录（最高优先级）
    Extra = 6,
}

impl SkillSource {
    pub fn label(&self) -> &'static str {
        match self {
            SkillSource::Bundled => "bundled",
            SkillSource::Managed => "managed",
            SkillSource::Personal => "personal",
            SkillSource::Project => "project",
            SkillSource::Workspace => "workspace",
            SkillSource::Extra => "extra",
        }
    }
}

/// Skill 资源目录发现结果
#[derive(Debug, Clone, Default)]
pub struct SkillResources {
    pub has_scripts: bool,
    pub has_references: bool,
    pub has_assets: bool,
}

/// OpenClaw metadata.openclaw 块
#[derive(Debug, Clone, Default)]
pub struct OpenClawMetadata {
    pub emoji: Option<String>,
    pub skill_key: Option<String>,
    pub always: bool,
    pub os: Vec<String>,
    pub requires: RequiresSpec,
    pub primary_env: Option<String>,
    pub install: Vec<InstallSpec>,
}

/// 依赖要求规范
#[derive(Debug, Clone, Default)]
pub struct RequiresSpec {
    pub bins: Vec<String>,
    pub any_bins: Vec<String>,
    pub env: Vec<String>,
    pub config: Vec<String>,
}

/// 安装规范（枚举）
#[derive(Debug, Clone)]
pub enum InstallSpec {
    Brew {
        formula: String,
        os: Option<Vec<String>>,
    },
    Node {
        package: String,
        os: Option<Vec<String>>,
    },
    Go {
        module: String,
        os: Option<Vec<String>>,
    },
    Uv {
        package: String,
        os: Option<Vec<String>>,
    },
    Download {
        url: String,
        archive: Option<String>,
        extract: Option<bool>,
        strip_components: Option<u32>,
        target_dir: Option<String>,
        os: Option<Vec<String>>,
    },
}

/// Skill 调用策略
#[derive(Debug, Clone)]
pub struct SkillInvocationPolicy {
    pub user_invocable: bool,
    pub disable_model_invocation: bool,
    pub command_dispatch: CommandDispatch,
    pub command_tool: Option<String>,
    pub slash_commands: Vec<String>,
}

impl Default for SkillInvocationPolicy {
    fn default() -> Self {
        Self {
            user_invocable: true,
            disable_model_invocation: false,
            command_dispatch: CommandDispatch::Model,
            command_tool: None,
            slash_commands: Vec::new(),
        }
    }
}

/// 命令分发方式
#[derive(Debug, Clone, PartialEq)]
pub enum CommandDispatch {
    Model,
    Tool,
}

impl Default for CommandDispatch {
    fn default() -> Self {
        CommandDispatch::Model
    }
}

impl std::fmt::Display for CommandDispatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandDispatch::Model => write!(f, "model"),
            CommandDispatch::Tool => write!(f, "tool"),
        }
    }
}

impl std::str::FromStr for CommandDispatch {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "model" | "" => Ok(CommandDispatch::Model),
            "tool" => Ok(CommandDispatch::Tool),
            _ => Err(format!("unknown dispatch kind: {}", s)),
        }
    }
}

/// Skill manifest（OpenClaw 格式）
#[derive(Debug, Clone)]
pub struct SkillManifest {
    pub name: String,
    pub description: String,
    pub version: Version,
    pub author: String,
    pub license: String,
    pub user_invocable: bool,
    pub disable_model_invocation: bool,
    pub command_dispatch: Option<String>,
    pub command_tool: Option<String>,
    pub slash_commands: Vec<String>,
    pub metadata: OpenClawMetadata,
}

impl Default for SkillManifest {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            version: Version::new(1, 0, 0),
            author: String::new(),
            license: String::new(),
            user_invocable: true,
            disable_model_invocation: false,
            command_dispatch: None,
            command_tool: None,
            slash_commands: Vec::new(),
            metadata: OpenClawMetadata::default(),
        }
    }
}

/// 已加载的 Skill
#[derive(Debug, Clone)]
pub struct LoadedSkill {
    /// Skill 唯一标识（等于 name）
    pub name: String,
    pub description: String,
    pub skill_md_path: PathBuf,
    pub manifest: SkillManifest,
    pub skill_dir: PathBuf,
    pub source: SkillSource,
    pub resource_dirs: SkillResources,
    pub install_specs: Vec<InstallSpec>,
    pub dependencies_satisfied: bool,
    pub invocation: SkillInvocationPolicy,
}

/// Skill 来源目录配置
#[derive(Debug, Clone)]
pub struct SkillSourceDir {
    pub dir: PathBuf,
    pub source: SkillSource,
}

// ───────────────────────────────────────────────────────────────
// Loader
// ───────────────────────────────────────────────────────────────

/// Skill 加载器
pub struct SkillLoader;

impl SkillLoader {
    pub fn new() -> Self {
        Self
    }

    /// 从多个来源加载 Skill，按优先级合并同名 Skill（高优先级覆盖低优先级）
    pub async fn load_all_skills(
        sources: Vec<SkillSourceDir>,
    ) -> Result<Vec<LoadedSkill>, SkillLoadError> {
        let mut merged: HashMap<String, LoadedSkill> = HashMap::new();

        // 按优先级排序（低优先级先加载，高优先级后覆盖）
        let mut sorted = sources;
        sorted.sort_by_key(|s| s.source as u8);

        for source_dir in sorted {
            let skills = Self::load_skills_from_dir(&source_dir.dir, source_dir.source).await?;
            for skill in skills {
                merged.insert(skill.name.clone(), skill);
            }
        }

        Ok(merged.into_values().collect())
    }

    /// 从单个目录加载所有 Skill
    /// 目录可以是：<dir>/SKILL.md（单个 skill）或 <dir>/*/SKILL.md（多个 skill 子目录）
    pub async fn load_skills_from_dir(
        dir: &Path,
        source: SkillSource,
    ) -> Result<Vec<LoadedSkill>, SkillLoadError> {
        let dir = dir.canonicalize().map_err(|e| {
            SkillLoadError::IoError(format!("Cannot canonicalize dir {:?}: {}", dir, e))
        })?;

        // 尝试根目录直接是 skill
        let root_skill_md = dir.join("SKILL.md");
        if root_skill_md.exists() {
            if let Some(skill) = Self::load_skill_from_dir(&dir, source).await? {
                return Ok(vec![skill]);
            }
        }

        // 扫描子目录
        let mut skills = Vec::new();
        let mut entries = tokio::fs::read_dir(&dir).await.map_err(|e| {
            SkillLoadError::IoError(format!("Cannot read dir {:?}: {}", dir, e))
        })?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            SkillLoadError::IoError(format!("Read dir entry failed: {}", e))
        })? {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with('.') || name_str == "node_modules" {
                continue;
            }

            if let Some(skill) = Self::load_skill_from_dir(&path, source).await? {
                skills.push(skill);
            }
        }

        Ok(skills)
    }

    /// 从单个 skill 目录加载
    pub async fn load_skill_from_dir(
        skill_dir: &Path,
        source: SkillSource,
    ) -> Result<Option<LoadedSkill>, SkillLoadError> {
        let skill_md_path = skill_dir.join("SKILL.md");
        if !skill_md_path.exists() {
            return Ok(None);
        }

        let content = tokio::fs::read_to_string(&skill_md_path)
            .await
            .map_err(|e| SkillLoadError::IoError(format!("Read SKILL.md failed: {}", e)))?;

        let manifest = Self::parse_manifest(&content,
            skill_dir.file_name().and_then(|n| n.to_str()).unwrap_or("unknown"),
        )?;

        // name 和 description 必须存在
        if manifest.name.is_empty() || manifest.description.is_empty() {
            return Ok(None);
        }

        let resource_dirs = Self::discover_resources(skill_dir);
        let install_specs = manifest.metadata.install.clone();

        // 基础依赖检查（仅检查 requires.bins 是否存在）
        let dependencies_satisfied = Self::check_basic_dependencies(&manifest.metadata.requires);

        // 构建 invocation policy
        let command_dispatch = manifest
            .command_dispatch
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or_default();
        let command_tool = manifest.command_tool.clone();

        let invocation = SkillInvocationPolicy {
            user_invocable: manifest.user_invocable,
            disable_model_invocation: manifest.disable_model_invocation,
            command_dispatch,
            command_tool,
            slash_commands: manifest.slash_commands.clone(),
        };

        let skill = LoadedSkill {
            name: manifest.name.clone(),
            description: manifest.description.clone(),
            skill_md_path: normalize_abs_path(&skill_md_path),
            manifest,
            skill_dir: normalize_abs_path(skill_dir),
            source,
            resource_dirs,
            install_specs,
            dependencies_satisfied,
            invocation,
        };

        Ok(Some(skill))
    }

    /// 解析 SKILL.md 的 YAML frontmatter + metadata.openclaw
    /// 若无 frontmatter，则从 Markdown body 自动提取 name 和 description
    fn parse_manifest(content: &str, fallback_name: &str) -> Result<SkillManifest, SkillLoadError> {
        let (frontmatter_yaml, body) = parse_frontmatter(content)
            .map_err(|e| SkillLoadError::ParseError(e))?;

        // 无 frontmatter：从 Markdown body 提取基本信息
        if frontmatter_yaml.is_empty() {
            let name = extract_title_from_body(&body).unwrap_or_else(|| fallback_name.to_string());
            let description = extract_first_paragraph(&body);
            return Ok(SkillManifest {
                name,
                description,
                ..Default::default()
            });
        }

        let frontmatter: serde_yaml::Value = serde_yaml::from_str(&frontmatter_yaml)
            .map_err(|e| SkillLoadError::ParseError(format!("YAML frontmatter: {}", e)))?;

        let name = get_yaml_str(&frontmatter, "name").unwrap_or_default();
        let name = if name.is_empty() {
            fallback_name.to_string()
        } else {
            name
        };

        let description = get_yaml_str(&frontmatter, "description").unwrap_or_default();

        let version = get_yaml_str(&frontmatter, "version")
            .and_then(|v| Version::parse(&v).ok())
            .unwrap_or_else(|| Version::new(1, 0, 0));

        let author = get_yaml_str(&frontmatter, "author").unwrap_or_default();
        let license = get_yaml_str(&frontmatter, "license").unwrap_or_default();

        // invocation policy
        let user_invocable =
            get_yaml_bool(&frontmatter, "user-invocable", true);
        let disable_model_invocation =
            get_yaml_bool(&frontmatter, "disable-model-invocation", false);

        // command dispatch
        let command_dispatch = get_yaml_str(&frontmatter, "command-dispatch");
        let command_tool = get_yaml_str(&frontmatter, "command-tool");

        // slash-commands 数组
        let slash_commands = get_yaml_str_array(&frontmatter, "slash-commands");

        // metadata.openclaw
        let metadata = parse_openclaw_metadata(&frontmatter);

        Ok(SkillManifest {
            name,
            description,
            version,
            author,
            license,
            user_invocable,
            disable_model_invocation,
            command_dispatch,
            command_tool,
            slash_commands,
            metadata,
        })
    }

    /// 发现资源目录
    fn discover_resources(skill_dir: &Path) -> SkillResources {
        SkillResources {
            has_scripts: skill_dir.join("scripts").is_dir(),
            has_references: skill_dir.join("references").is_dir(),
            has_assets: skill_dir.join("assets").is_dir(),
        }
    }

    /// 基础依赖检查：检查 requires.bins 中列出的二进制是否存在于 PATH
    fn check_basic_dependencies(requires: &RequiresSpec) -> bool {
        if requires.bins.is_empty() && requires.any_bins.is_empty() {
            return true;
        }

        // 检查所有 required bins
        for bin in &requires.bins {
            if !Self::has_binary(bin) {
                return false;
            }
        }

        // 检查 any_bins：至少有一个存在即可
        if !requires.any_bins.is_empty() && !requires.any_bins.iter().any(|b| Self::has_binary(b)) {
            return false;
        }

        true
    }

    fn has_binary(name: &str) -> bool {
        which::which(name).is_ok()
    }
}

impl Default for SkillLoader {
    fn default() -> Self {
        Self::new()
    }
}

// ───────────────────────────────────────────────────────────────
// Frontmatter 解析辅助函数
// ───────────────────────────────────────────────────────────────

/// 提取 YAML frontmatter，返回 (frontmatter_yaml, markdown_body)
fn parse_frontmatter(content: &str) -> Result<(String, String), String> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Ok((String::new(), content.to_string()));
    }

    let after_first = &trimmed[3..];
    let Some(end_pos) = after_first.find("---") else {
        return Err("Unclosed frontmatter: missing closing ---".to_string());
    };

    let frontmatter = after_first[..end_pos].trim().to_string();
    let body = after_first[end_pos + 3..].trim_start().to_string();

    Ok((frontmatter, body))
}

/// 从 Markdown body 中提取第一级标题 (# Title) 作为 name
fn extract_title_from_body(body: &str) -> Option<String> {
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("# ") {
            return Some(title.trim().to_string());
        }
    }
    None
}

/// 从 Markdown body 中提取第一段非空文本作为 description
fn extract_first_paragraph(body: &str) -> String {
    let mut in_code_block = false;
    let mut paragraph = String::new();

    for line in body.lines() {
        let trimmed = line.trim();

        // 跳过代码块标记
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }

        // 跳过空行、标题、分隔线
        if trimmed.is_empty()
            || trimmed.starts_with("#")
            || trimmed.starts_with("---")
            || trimmed.starts_with("|")
        {
            if !paragraph.is_empty() {
                // 已经收集到一段内容，结束
                break;
            }
            continue;
        }

        if !paragraph.is_empty() {
            paragraph.push(' ');
        }
        paragraph.push_str(trimmed);
    }

    paragraph.trim().to_string()
}

fn get_yaml_str(value: &serde_yaml::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn get_yaml_bool(value: &serde_yaml::Value, key: &str, default: bool) -> bool {
    value
        .get(key)
        .and_then(|v| v.as_bool())
        .or_else(|| {
            value
                .get(key)
                .and_then(|v| v.as_str())
                .map(|s| s.eq_ignore_ascii_case("true"))
        })
        .unwrap_or(default)
}

/// 提取 YAML 字符串数组
fn get_yaml_str_array(value: &serde_yaml::Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// 解析 metadata.openclaw 块
fn parse_openclaw_metadata(frontmatter: &serde_yaml::Value) -> OpenClawMetadata {
    let mut metadata = OpenClawMetadata::default();

    let Some(openclaw) = frontmatter.get("metadata").and_then(|m| m.get("openclaw")) else {
        return metadata;
    };

    metadata.emoji = get_yaml_str(openclaw, "emoji");
    metadata.skill_key = get_yaml_str(openclaw, "skillKey");
    metadata.always = get_yaml_bool(openclaw, "always", false);
    metadata.primary_env = get_yaml_str(openclaw, "primaryEnv");

    // os 列表
    if let Some(os_list) = openclaw.get("os").and_then(|v| v.as_sequence()) {
        metadata.os = os_list
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
    }

    // requires
    if let Some(req) = openclaw.get("requires") {
        if let Some(bins) = req.get("bins").and_then(|v| v.as_sequence()) {
            metadata.requires.bins = bins
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
        }
        if let Some(any_bins) = req.get("anyBins").and_then(|v| v.as_sequence()) {
            metadata.requires.any_bins = any_bins
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
        }
        if let Some(env) = req.get("env").and_then(|v| v.as_sequence()) {
            metadata.requires.env = env
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
        }
        if let Some(config) = req.get("config").and_then(|v| v.as_sequence()) {
            metadata.requires.config = config
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
        }
    }

    // install 数组
    if let Some(install_list) = openclaw.get("install").and_then(|v| v.as_sequence()) {
        for item in install_list {
            if let Some(spec) = parse_install_spec(item) {
                metadata.install.push(spec);
            }
        }
    }

    metadata
}

/// 解析单个 install spec
fn parse_install_spec(value: &serde_yaml::Value) -> Option<InstallSpec> {
    let kind = value.get("kind").and_then(|v| v.as_str())?;

    let os = value
        .get("os")
        .and_then(|v| v.as_sequence())
        .map(|seq| seq.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect());

    match kind {
        "brew" => {
            let formula = value.get("formula").and_then(|v| v.as_str())?;
            Some(InstallSpec::Brew {
                formula: formula.to_string(),
                os,
            })
        }
        "node" => {
            let package = value.get("package").and_then(|v| v.as_str())?;
            Some(InstallSpec::Node {
                package: package.to_string(),
                os,
            })
        }
        "go" => {
            let module = value.get("module").and_then(|v| v.as_str())?;
            Some(InstallSpec::Go {
                module: module.to_string(),
                os,
            })
        }
        "uv" => {
            let package = value.get("package").and_then(|v| v.as_str())?;
            Some(InstallSpec::Uv {
                package: package.to_string(),
                os,
            })
        }
        "download" => {
            let url = value.get("url").and_then(|v| v.as_str())?;
            Some(InstallSpec::Download {
                url: url.to_string(),
                archive: value.get("archive").and_then(|v| v.as_str()).map(|s| s.to_string()),
                extract: value.get("extract").and_then(|v| v.as_bool()),
                strip_components: value.get("stripComponents").and_then(|v| v.as_u64()).map(|n| n as u32),
                target_dir: value.get("targetDir").and_then(|v| v.as_str()).map(|s| s.to_string()),
                os,
            })
        }
        _ => None,
    }
}

// ───────────────────────────────────────────────────────────────
// 错误类型
// ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, thiserror::Error)]
pub enum SkillLoadError {
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Invalid manifest: {0}")]
    InvalidManifest(String),
}
