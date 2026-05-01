//! Skill Eligibility Checker
//!
//! 检查 Skill 在当前平台是否可用。

use crate::skills::loader::{LoadedSkill, OpenClawMetadata, RequiresSpec};

/// 资格检查错误
#[derive(Debug, Clone, PartialEq)]
pub enum EligibilityError {
    UnsupportedOs { required: Vec<String>, current: String },
    MissingBinary(String),
    MissingAnyBinary(Vec<String>),
    MissingEnv(String),
    MissingConfig(String),
}

impl std::fmt::Display for EligibilityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EligibilityError::UnsupportedOs { required, current } => {
                write!(f, "Unsupported OS: current={}, required={:?}", current, required)
            }
            EligibilityError::MissingBinary(bin) => write!(f, "Missing required binary: {}", bin),
            EligibilityError::MissingAnyBinary(bins) => {
                write!(f, "Missing any of required binaries: {:?}", bins)
            }
            EligibilityError::MissingEnv(env) => write!(f, "Missing required env var: {}", env),
            EligibilityError::MissingConfig(cfg) => write!(f, "Missing required config: {}", cfg),
        }
    }
}

impl std::error::Error for EligibilityError {}

/// 检查 Skill 资格
pub fn check_skill_eligibility(skill: &LoadedSkill) -> Result<(), EligibilityError> {
    let meta = &skill.manifest.metadata;

    // 1. 平台检查
    check_os(meta)?;

    // 2. 依赖检查
    check_requires(&meta.requires)?;

    Ok(())
}

/// 检查操作系统兼容性
fn check_os(meta: &OpenClawMetadata) -> Result<(), EligibilityError> {
    if meta.os.is_empty() {
        return Ok(());
    }

    let current = current_os();
    if meta.os.iter().any(|os| os.eq_ignore_ascii_case(&current)) {
        Ok(())
    } else {
        Err(EligibilityError::UnsupportedOs {
            required: meta.os.clone(),
            current,
        })
    }
}

/// 检查 requires 字段
fn check_requires(reqs: &RequiresSpec) -> Result<(), EligibilityError> {
    // bins: 所有都必须存在
    for bin in &reqs.bins {
        if !has_binary(bin) {
            return Err(EligibilityError::MissingBinary(bin.clone()));
        }
    }

    // anyBins: 至少一个存在
    if !reqs.any_bins.is_empty() && !reqs.any_bins.iter().any(|b| has_binary(b)) {
        return Err(EligibilityError::MissingAnyBinary(reqs.any_bins.clone()));
    }

    // env: 环境变量
    for env in &reqs.env {
        if std::env::var(env).is_err() {
            return Err(EligibilityError::MissingEnv(env.clone()));
        }
    }

    // config: 配置文件路径（相对于 skill 目录或绝对路径）
    for cfg in &reqs.config {
        let path = std::path::Path::new(cfg);
        if !path.exists() {
            return Err(EligibilityError::MissingConfig(cfg.clone()));
        }
    }

    Ok(())
}

fn has_binary(name: &str) -> bool {
    which::which(name).is_ok()
}

fn current_os() -> String {
    if cfg!(target_os = "macos") {
        "macos".to_string()
    } else if cfg!(target_os = "linux") {
        "linux".to_string()
    } else if cfg!(target_os = "windows") {
        "windows".to_string()
    } else {
        "unknown".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_os_not_empty() {
        let os = current_os();
        assert!(!os.is_empty());
    }

    #[test]
    fn test_check_os_empty() {
        let meta = OpenClawMetadata::default();
        assert!(check_os(&meta).is_ok());
    }

    #[test]
    fn test_check_os_match() {
        let mut meta = OpenClawMetadata::default();
        meta.os = vec![current_os()];
        assert!(check_os(&meta).is_ok());
    }

    #[test]
    fn test_check_os_mismatch() {
        let mut meta = OpenClawMetadata::default();
        meta.os = vec!["nonexistent-os".to_string()];
        assert!(check_os(&meta).is_err());
    }
}
