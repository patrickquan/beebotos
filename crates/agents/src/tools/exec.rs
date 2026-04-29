//! Tool: exec
//!
//! Executes a shell command and returns stdout/stderr.

use std::path::Path;
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::llm::types::{FunctionDefinition, Tool};
use crate::llm::ToolHandler;

/// 总输出限制（对齐 OpenClaw）
const OUTPUT_CAP: usize = 200_000;
/// stdout 限制（占总限制的 3/4，约 150KB）
const STDOUT_CAP: usize = OUTPUT_CAP * 3 / 4;
/// stderr 限制（占总限制的 1/4，约 50KB）
const STDERR_CAP: usize = OUTPUT_CAP / 4;

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
            result.push_str(&truncate_output(&stdout, STDOUT_CAP));
        }

        if !stderr.is_empty() {
            if !result.is_empty() {
                result.push_str("\n\n");
            }
            result.push_str("STDERR:\n");
            result.push_str(&truncate_output(&stderr, STDERR_CAP));
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

    // 先检查 token 本身是否就是一个已存在的文件（包含扩展名的情况，如 "a-stock.py"）
    let token_as_file = skill_dir.join(first_token);
    if token_as_file.exists() && token_as_file.is_file() {
        let ext = Path::new(first_token)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let resolved = resolve_by_extension(command, first_token, ext, cwd);
        if resolved != command {
            return resolved;
        }
        // 扩展名无法识别，但至少文件存在，尝试原样执行（如 .bat/.cmd/.exe 等）
        return command.to_string();
    }

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

    // 查找需要解释器的脚本文件（token 不带扩展名的情况，如 "a-stock"）
    let resolved = find_script_and_resolve(command, first_token, skill_dir, cwd);
    if resolved != command {
        return resolved;
    }

    command.to_string()
}

/// 根据文件扩展名解析 skill 命令（token 本身已包含扩展名，如 "a-stock.py"）
fn resolve_by_extension(command: &str, token: &str, ext: &str, cwd: &str) -> String {
    // 可直接执行的扩展名，无需解释器
    if matches!(ext, "bat" | "cmd" | "exe" | "com") {
        return command.to_string();
    }

    // 需要解释器的扩展名
    let interpreters: &[&str] = match ext {
        "py" => {
            #[cfg(target_os = "windows")]
            {
                &["python", "py"]
            }
            #[cfg(not(target_os = "windows"))]
            {
                &["python3", "python"]
            }
        }
        "js" => &["node", "nodejs"],
        "sh" => &["sh", "bash"],
        _ => return command.to_string(),
    };

    for interpreter in interpreters {
        if is_command_available(interpreter) {
            let rest = command.trim_start_matches(token).trim_start();
            let resolved = if rest.is_empty() {
                format!("{} {}", interpreter, token)
            } else {
                format!("{} {}{}", interpreter, token, rest)
            };
            info!(
                "exec tool: resolved skill command '{}' -> '{}' in {}",
                command, resolved, cwd
            );
            return resolved;
        }
    }

    command.to_string()
}

/// 查找与 token 同名的脚本文件并解析（token 不带扩展名，如 "a-stock"）
fn find_script_and_resolve(command: &str, token: &str, skill_dir: &Path, cwd: &str) -> String {
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
        let script_path = skill_dir.join(format!("{}{}", token, ext));
        if !script_path.exists() {
            continue;
        }

        for interpreter in *interpreters {
            if is_command_available(interpreter) {
                let rest = command.trim_start_matches(token).trim_start();
                let resolved = if rest.is_empty() {
                    format!("{} {}{}", interpreter, token, ext)
                } else {
                    format!("{} {}{}{}", interpreter, token, ext, rest)
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
