//! Skill Prompt Builder (OpenClaw Compatible)
//!
//! 构造 `<available_skills>` XML 提示段，严格对齐 OpenClaw 格式。

use std::sync::Arc;

use crate::skills::registry::SkillRegistry;

/// Skill 数量上限
const MAX_SKILLS_IN_PROMPT: usize = 150;
/// 总字符预算上限
const MAX_SKILL_PROMPT_CHARS: usize = 18_000;

/// 构建 OpenClaw 风格的 `<available_skills>` XML 提示。
///
/// 三级 fallback：
/// 1. full：包含 name + description + location
/// 2. compact：仅 name + location（去掉 description）
/// 3. truncated：compact + 按字符预算截断
///
/// 输出格式：
/// ```text
/// The following skills provide specialized instructions for specific tasks.
/// Use the read tool to load a skill's file when the task matches its description.
/// When a skill file references a relative path, resolve it against the skill directory
/// (parent of SKILL.md / dirname of the path) and use that absolute path in tool commands.
///
/// <available_skills>
///   <skill>
///     <name>crypto-trading-bot</name>
///     <description>加密货币交易机器人开发...</description>
///     <location>data/skills/crypto-trading-bot/SKILL.md</location>
///   </skill>
/// </available_skills>
/// ```
pub async fn build_skills_prompt(registry: &Arc<SkillRegistry>) -> String {
    let skills = registry.list_enabled().await;

    if skills.is_empty() {
        return String::from(
            "The following skills provide specialized instructions for specific tasks.\n\
             Use the read tool to load a skill's file when the task matches its description.\n\
             When a skill file references a relative path, resolve it against the skill directory\n\
             (parent of SKILL.md / dirname of the path) and use that absolute path in tool commands.\n\
             When executing skill scripts with the exec tool, ALWAYS set the cwd parameter to the skill directory\n\
             so that relative paths in commands work correctly.\n\
             \n\
             <available_skills>\n\
             </available_skills>\n\
             \n\
             No skills are currently installed. If asked about available skills, report that none are installed."
        );
    }

    // 按使用频率排序，高优先级 skill 在前
    let mut skills = skills;
    skills.sort_by(|a, b| b.usage_count.cmp(&a.usage_count));
    let skills = &skills[..skills.len().min(MAX_SKILLS_IN_PROMPT)];

    // 尝试 full 模式
    let full = build_xml(skills, true);
    if full.chars().count() <= MAX_SKILL_PROMPT_CHARS {
        return full;
    }

    // fallback 1: compact 模式（去掉 description）
    let compact = build_xml(skills, false);
    if compact.chars().count() <= MAX_SKILL_PROMPT_CHARS {
        return compact;
    }

    // fallback 2: 截断到预算
    truncate_to_budget(compact, MAX_SKILL_PROMPT_CHARS)
}

fn build_xml(skills: &[crate::skills::registry::RegisteredSkill], with_desc: bool) -> String {
    let mut lines: Vec<String> = vec![
        String::new(),
        String::from(
            "The following skills provide specialized instructions for specific tasks.",
        ),
        String::from(
            "Use the read tool to load a skill's file when the task matches its description.",
        ),
        String::from(
            "When a skill file references a relative path, resolve it against the skill directory",
        ),
        String::from(
            "(parent of SKILL.md / dirname of the path) and use that absolute path in tool commands.",
        ),
        String::from(
            "When executing skill scripts with the exec tool, ALWAYS set the cwd parameter to the skill directory",
        ),
        String::from(
            "so that relative paths in commands work correctly.",
        ),
        String::new(),
        String::from("<available_skills>"),
    ];

    for skill in skills {
        let location = skill
            .skill
            .skill_md_path
            .to_string_lossy()
            .replace('\\', "/");
        lines.push(String::from("  <skill>"));
        lines.push(format!(
            "    <name>{}</name>",
            escape_xml(&skill.skill.name)
        ));
        if with_desc {
            lines.push(format!(
                "    <description>{}</description>",
                escape_xml(&skill.skill.manifest.description)
            ));
        }
        lines.push(format!(
            "    <location>{}</location>",
            escape_xml(&location)
        ));
        lines.push(String::from("  </skill>"));
    }

    lines.push(String::from("</available_skills>"));
    lines.join("\n")
}

fn truncate_to_budget(xml: String, budget: usize) -> String {
    if xml.chars().count() <= budget {
        return xml;
    }

    // 在预算附近找到最后一个完整的 </skill>（允许略微超出预算以保留标签完整性）
    let mut last_skill_end = 0;
    let mut search_start = 0;
    while let Some(pos) = xml[search_start..].find("</skill>") {
        let end = search_start + pos + "</skill>".len();
        // 允许 end 略微超出 budget，确保至少能保留一个完整的 skill
        if end <= budget + "</skill>".len() {
            last_skill_end = end;
            search_start = end;
        } else {
            break;
        }
    }

    if last_skill_end > 0 {
        let mut truncated = xml[..last_skill_end].to_string();
        if !truncated.trim_end().ends_with("</available_skills>") {
            truncated.push('\n');
            truncated.push_str("</available_skills>");
        }
        return truncated;
    }

    // 极端情况：完全截断
    xml[..budget].to_string()
}

/// XML 特殊字符转义（含单引号）。
fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_xml() {
        assert_eq!(
            escape_xml("foo & bar <script> \"x\" 'y'"),
            "foo &amp; bar &lt;script&gt; &quot;x&quot; &apos;y&apos;"
        );
    }

    #[test]
    fn test_truncate_keeps_valid_xml() {
        let xml = "<available_skills>\n  <skill>\n    <name>a</name>\n    <location>/x</location>\n  </skill>\n  <skill>\n    <name>b</name>\n    <location>/y</location>\n  </skill>\n</available_skills>";
        let truncated = truncate_to_budget(xml.to_string(), 80);
        assert!(truncated.contains("</available_skills>"));
        assert!(!truncated.contains("name>b")); // 第二个 skill 被截断
    }
}
