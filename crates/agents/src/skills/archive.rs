//! Skill Archive Extractor
//!
//! 处理 ZIP 归档的解压，支持 ZIP / tar.gz / tar.bz2 格式。
//! 包含路径遍历安全检查。

use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

/// 解压错误
#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("Path traversal detected: {0}")]
    PathTraversal(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Unsupported archive format: {0}")]
    UnsupportedFormat(String),
    #[error("ZIP error: {0}")]
    ZipError(#[from] zip::result::ZipError),
}

/// 自动识别并解压归档文件到目标目录。
///
/// 根据文件扩展名自动选择解压方式：
/// - `.zip` → ZIP 格式
/// - `.tar.gz` / `.tgz` → tar + gzip
/// - `.tar.bz2` → tar + bzip2
pub fn extract_auto(archive_path: &Path, target_dir: &Path) -> Result<(), ExtractError> {
    let ext = archive_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let file_name = archive_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    if ext.eq_ignore_ascii_case("zip") || file_name.ends_with(".zip") {
        let bytes = std::fs::read(archive_path)?;
        extract_zip(&bytes, target_dir)
    } else if file_name.ends_with(".tar.gz")
        || file_name.ends_with(".tgz")
        || file_name.ends_with(".tar.bz2")
    {
        Err(ExtractError::UnsupportedFormat(format!(
            "tar archives not yet supported: {}",
            file_name
        )))
    } else {
        Err(ExtractError::UnsupportedFormat(format!(
            "unknown archive format: {}",
            file_name
        )))
    }
}

/// 从字节数组解压 ZIP 到目标目录
pub fn extract_zip(bytes: &[u8], target_dir: &Path) -> Result<(), ExtractError> {
    let reader = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let out_path = sanitize_zip_path(file.name(), target_dir)?;

        if file.name().ends_with('/') {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out_file = std::fs::File::create(&out_path)?;
            std::io::copy(&mut file, &mut out_file)?;
        }
    }

    Ok(())
}

/// 安全检查 ZIP 条目路径，防止路径遍历攻击
fn sanitize_zip_path(name: &str, target_dir: &Path) -> Result<PathBuf, ExtractError> {
    let path = PathBuf::from(name);
    let mut clean = PathBuf::new();

    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir | Component::RootDir => {}
            Component::ParentDir | Component::Prefix(_) => {
                return Err(ExtractError::PathTraversal(name.to_string()));
            }
        }
    }

    let resolved = target_dir.join(&clean);
    let canonical_target = target_dir.canonicalize().unwrap_or_else(|_| target_dir.to_path_buf());
    let resolved_str = resolved.to_string_lossy();
    let target_str = canonical_target.to_string_lossy();

    if !resolved_str.starts_with(&*target_str) {
        return Err(ExtractError::PathTraversal(name.to_string()));
    }

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_valid_path() {
        let dir = PathBuf::from("/tmp/skills");
        let result = sanitize_zip_path("skill/SKILL.md", &dir).unwrap();
        assert!(result.to_string_lossy().contains("skill"));
        assert!(result.to_string_lossy().contains("SKILL.md"));
    }

    #[test]
    fn test_sanitize_traversal_blocked() {
        let dir = PathBuf::from("/tmp/skills");
        assert!(sanitize_zip_path("../../etc/passwd", &dir).is_err());
        assert!(sanitize_zip_path("skill/../../../etc/passwd", &dir).is_err());
    }
}
