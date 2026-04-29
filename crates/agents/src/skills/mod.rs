//! Skills Module
//!
//! Skill system for agent capabilities (ClawHub integration).
//! Phase 2 重构：全面兼容 OpenClaw Skill 格式。

pub mod archive;
pub mod command_handler;
pub mod eligibility;
pub mod install;
pub mod link_handler;
pub mod loader;
pub mod lockfile;
pub mod rating;
pub mod registry;
pub mod skill_prompt;
pub mod slash_commands;

pub use archive::extract_auto;
pub use command_handler::{
    CommandContext, CommandHandler, CommandResult, RuntimeInfo, RuntimeStatus,
};
pub use eligibility::{check_skill_eligibility, EligibilityError};
pub use install::{InstallError, SkillInstallManager};
pub use link_handler::{format_summary_for_display, ContentType, LinkHandler, LinkSummary};
pub use loader::{
    CommandDispatch, InstallSpec, LoadedSkill, OpenClawMetadata, RequiresSpec,
    SkillInvocationPolicy, SkillLoadError, SkillLoader, SkillManifest, SkillResources,
    SkillSource, SkillSourceDir,
};
pub use rating::{RatingSummary, SkillRating, SkillRatingStore};
pub use lockfile::{LockEntry, SkillLockfile};
pub use registry::{RegisteredSkill, SkillDefinition, SkillRegistry, Version, VersionError};
pub use skill_prompt::build_skills_prompt;
pub use slash_commands::{SlashCommand, SlashCommandDispatcher};
