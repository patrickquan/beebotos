//! Tool: exec
//!
//! Executes a shell command and returns stdout/stderr.

use std::path::Path;
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::llm::types::{FunctionDefinition, Tool};
use crate::llm::ToolHandler;

/// Exec tool — execute shell commands
pub struct ExecTool;

impl ExecTool {
    /// Create new exec tool
    pub fn new() -> Self {
        Self
    }
}

impl Default for ExecTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ToolHandler for ExecTool {
    fn definition(&self) -> Tool {
        Tool {
            r#type: "function".to_string(),
            function: FunctionDefinition {
                name: "exec".to_string(),
                description: Some("执行 shell 命令，返回标准输出和标准错误".to_string()),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "要执行的命令"
                        },
                        "timeout": {
                            "type": "integer",
                            "description": "超时秒数（可选，默认30）"
                        },
                        "cwd": {
                            "type": "string",
                            "description": "工作目录（可选）"
                        }
                    },
                    "required": ["command"]
                }),
            },
        }
    }

    async fn execute(&self, arguments: &str) -> Result<String, String> {
        let args: serde_json::Value = serde_json::from_str(arguments)
            .map_err(|e| format!("Invalid arguments: {}", e))?;

        let command = args["command"].as_str().ok_or("Missing command")?;
        let timeout_secs = args["timeout"].as_u64().unwrap_or(30);
        let cwd = args["cwd"].as_str();

        // 智能解析 skill 目录中的脚本命令
        let resolved_command = resolve_skill_command(command, cwd);
        let command = if resolved_command != command {
            &resolved_command
        } else {
            command
        };

        info!("exec tool: command='{}', cwd={:?}, timeout={}s", command, cwd, timeout_secs);

        // Use shell to execute the command so quoting and pipes work naturally
        #[cfg(target_os = "windows")]
        let mut cmd = {
            let mut c = tokio::process::Command::new("cmd");
            c.args(&["/C", command]);
            c
        };
        #[cfg(not(target_os = "windows"))]
        let mut cmd = {
            let mut c = tokio::process::Command::new("sh");
            c.args(&["-c", command]);
            c
        };

        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }

        let output = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            cmd.output(),
        )
        .await
        .map_err(|_| format!("Command timed out after {} seconds", timeout_secs))?
        .map_err(|e| format!("Failed to execute command: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let mut result = String::new();

        if !stdout.is_empty() {
            result.push_str("STDOUT:\n");
            result.push_str(&truncate_output(&stdout, 16 * 1024));
        }

        if !stderr.is_empty() {
            if !result.is_empty() {
                result.push_str("\n\n");
            }
            result.push_str("STDERR:\n");
            result.push_str(&truncate_output(&stderr, 8 * 1024));
        }

        if result.is_empty() {
            result = format!(
                "Command completed with exit code: {}",
                output.status.code().unwrap_or(-1)
            );
        } else {
            result.push_str(&format!(
                "\n\nExit code: {}",
                output.status.code().unwrap_or(-1)
            ));
        }

        if output.status.success() {
            info!("exec tool: success, exit_code={}", output.status.code().unwrap_or(-1));
            debug!("exec tool stdout (first 500 chars): {}", &stdout[..stdout.len().min(500)]);
        } else {
            warn!(
                "exec tool: failed, exit_code={:?}, stderr (first 500 chars)={}",
                output.status.code(),
                &stderr[..stderr.len().min(500)]
            );
        }

        Ok(result)
    }
}

/// 当 cwd 为 skill 目录时，自动识别脚本类型并添加解释器前缀。
///
/// 例如 skill 目录下有 `a-stock.py`，但指令写的是 `a-stock sh600519`，
/// 自动重写为 `python a-stock.py sh600519`。
fn resolve_skill_command(command: &str, cwd: Option<&str>) -> String {
    let Some(cwd) = cwd else {
        return command.to_string();
    };

    // 仅当 cwd 包含 SKILL.md 时才视为 skill 目录
    let skill_md = Path::new(cwd).join("SKILL.md");
    if !skill_md.exists() {
        return command.to_string();
    }

    // 提取第一个 token
    let first_token = match command.split_whitespace().next() {
        Some(t) => t,
        None => return command.to_string(),
    };

    // 如果 token 已包含路径分隔符，不处理
    if first_token.contains('/') || first_token.contains('\\') {
        return command.to_string();
    }

    let skill_dir = Path::new(cwd);

    // 检查该 token 是否已对应可直接执行的文件（Windows: bat/cmd/exe/com；Unix: 无扩展名可执行文件）
    #[cfg(target_os = "windows")]
    let direct_extensions: &[&str] = &[".bat", ".cmd", ".exe", ".com"];
    #[cfg(not(target_os = "windows"))]
    let direct_extensions: &[&str] = &[""];

    for ext in direct_extensions {
        if skill_dir.join(format!("{}{}", first_token, ext)).exists() {
            return command.to_string(); // 已可直接执行
        }
    }

    // 查找需要解释器的脚本文件，并按优先级排序
    // 注意：不同平台优先级不同
    #[cfg(target_os = "windows")]
    let script_candidates: &[(&str, &[&str])] = &[
        (".py", &["python", "py"]),
        (".js", &["node", "nodejs"]),
        (".sh", &["sh", "bash"]),
    ];
    #[cfg(not(target_os = "windows"))]
    let script_candidates: &[(&str, &[&str])] = &[
        (".py", &["python3", "python"]),
        (".js", &["node", "nodejs"]),
        (".sh", &["sh", "bash"]),
    ];

    for (ext, interpreters) in script_candidates {
        let script_path = skill_dir.join(format!("{}{}", first_token, ext));
        if !script_path.exists() {
            continue;
        }

        // 选择第一个可用的解释器
        for interpreter in *interpreters {
            if is_command_available(interpreter) {
                let rest = command.trim_start_matches(first_token).trim_start();
                let resolved = if rest.is_empty() {
                    format!("{} {}.{}", interpreter, first_token, ext)
                } else {
                    format!("{} {}.{}{}", interpreter, first_token, ext, rest)
                };
                info!(
                    "exec tool: resolved skill command '{}' -> '{}' in {}",
                    command, resolved, cwd
                );
                return resolved;
            }
        }
    }

    command.to_string()
}

/// 检查系统上是否存在某个命令
fn is_command_available(cmd: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("where")
            .arg(cmd)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("which")
            .arg(cmd)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

/// Truncate output if it exceeds max bytes (safe UTF-8)
fn truncate_output(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        s.to_string()
    } else {
        let end = s[..max_bytes]
            .char_indices()
            .last()
            .map(|(i, _)| i)
            .unwrap_or(max_bytes);
        format!(
            "{}\n\n[... {} more bytes truncated ...]",
            &s[..end],
            s.len() - end
        )
    }
}
