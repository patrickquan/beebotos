//! Skill Install Manager
//!
//! 处理 Skill 的依赖安装：brew、node、go、uv、download。

use std::path::PathBuf;

use crate::skills::loader::InstallSpec;

/// 安装错误
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("Install command failed: {0}")]
    CommandFailed(String),
    #[error("Unsupported install kind: {0}")]
    UnsupportedKind(String),
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Missing prerequisite: {0}")]
    MissingPrerequisite(String),
}

/// Skill 安装管理器
pub struct SkillInstallManager {
    /// 工具安装前缀目录（如 data/tools/node）
    tools_dir: PathBuf,
}

impl SkillInstallManager {
    pub fn new(tools_dir: PathBuf) -> Self {
        Self { tools_dir }
    }

    /// 检查并安装单个 InstallSpec
    pub async fn install(&self,
        spec: &InstallSpec,
    ) -> Result<(), InstallError> {
        match spec {
            InstallSpec::Brew { formula, .. } => {
                self.ensure_brew().await?;
                self.run_command("brew", &["install", formula]).await
            }
            InstallSpec::Node { package, .. } => {
                let prefix = self.tools_dir.join("node");
                tokio::fs::create_dir_all(&prefix)
                    .await
                    .map_err(|e| InstallError::IoError(e.to_string()))?;
                self.run_command(
                    "npm",
                    &[
                        "install",
                        "-g",
                        package,
                        "--prefix",
                        &prefix.to_string_lossy(),
                    ],
                )
                .await
            }
            InstallSpec::Go { module, .. } => {
                self.ensure_go().await?;
                self.run_command("go", &["install", module]).await
            }
            InstallSpec::Uv { package, .. } => {
                self.ensure_uv().await?;
                self.run_command("uv", &["tool", "install", package]).await
            }
            InstallSpec::Download {
                url,
                archive,
                extract,
                strip_components,
                target_dir,
                ..
            } => {
                self.install_download(url, archive.clone(), *extract, *strip_components, target_dir.clone())
                    .await
            }
        }
    }

    /// 批量安装 Skill 的所有依赖
    pub async fn install_all(
        &self,
        specs: &[InstallSpec],
    ) -> Result<Vec<()>, InstallError> {
        let mut results = Vec::new();
        for spec in specs {
            results.push(self.install(spec).await?);
        }
        Ok(results)
    }

    // ─── 私有辅助 ───

    async fn run_command(
        &self,
        cmd: &str,
        args: &[&str],
    ) -> Result<(), InstallError> {
        let output = tokio::process::Command::new(cmd)
            .args(args)
            .output()
            .await
            .map_err(|e| InstallError::CommandFailed(format!("{}: {}", cmd, e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(InstallError::CommandFailed(format!(
                "{} {:?} failed: {}",
                cmd, args, stderr
            )));
        }
        Ok(())
    }

    async fn ensure_brew(&self) -> Result<(), InstallError> {
        if which::which("brew").is_ok() {
            return Ok(());
        }
        Err(InstallError::MissingPrerequisite(
            "Homebrew is required but not installed. Please install it from https://brew.sh".to_string(),
        ))
    }

    async fn ensure_go(&self) -> Result<(), InstallError> {
        if which::which("go").is_ok() {
            return Ok(());
        }
        // 尝试自动安装 go（通过系统包管理器，这里简化处理）
        Err(InstallError::MissingPrerequisite(
            "Go is required but not installed. Please install Go from https://go.dev/dl/".to_string(),
        ))
    }

    async fn ensure_uv(&self) -> Result<(), InstallError> {
        if which::which("uv").is_ok() {
            return Ok(());
        }
        // 尝试通过 pipx 或 curl 安装 uv
        if which::which("pipx").is_ok() {
            let _ = self.run_command("pipx", &["install", "uv"]).await;
            if which::which("uv").is_ok() {
                return Ok(());
            }
        }
        // 尝试通过官方脚本安装
        let _ = tokio::process::Command::new("sh")
            .args([
                "-c",
                "curl -LsSf https://astral.sh/uv/install.sh | sh",
            ])
            .output()
            .await;
        if which::which("uv").is_ok() {
            return Ok(());
        }
        Err(InstallError::MissingPrerequisite(
            "uv is required but could not be auto-installed. Please install from https://github.com/astral-sh/uv".to_string(),
        ))
    }

    async fn install_download(
        &self,
        url: &str,
        _archive: Option<String>,
        extract: Option<bool>,
        _strip_components: Option<u32>,
        target_dir: Option<String>,
    ) -> Result<(), InstallError> {
        let target = match target_dir {
            Some(td) => PathBuf::from(td),
            None => self.tools_dir.join("downloads"),
        };
        tokio::fs::create_dir_all(&target)
            .await
            .map_err(|e| InstallError::IoError(e.to_string()))?;

        // 下载文件
        let response = reqwest::get(url)
            .await
            .map_err(|e| InstallError::IoError(format!("Download failed: {}", e)))?;
        let bytes = response
            .bytes()
            .await
            .map_err(|e| InstallError::IoError(format!("Read download failed: {}", e)))?;

        let file_name = url.rsplit('/').next().unwrap_or("download");
        let file_path = target.join(file_name);
        tokio::fs::write(&file_path, &bytes)
            .await
            .map_err(|e| InstallError::IoError(e.to_string()))?;

        // 自动解压（如果文件是压缩包且 extract 为 true 或未指定）
        let should_extract = extract.unwrap_or(true);
        if should_extract {
            let _ = super::archive::extract_auto(&file_path, &target);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_error_display() {
        let err = InstallError::MissingPrerequisite("foo".to_string());
        assert_eq!(err.to_string(), "Missing prerequisite: foo");
    }
}
