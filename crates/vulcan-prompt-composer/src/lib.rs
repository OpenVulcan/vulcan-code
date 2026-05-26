//! Prompt composition framework crate.
//! 提示词组合框架 crate。

/// Shared cache primitives.
/// 共享缓存基础能力。
pub mod cache;
/// Composition entrypoint models.
/// 组合入口模型。
pub mod composer;
/// Shared error definitions.
/// 共享错误定义。
pub mod error;
/// Stable identifiers used by prompt nodes and strategies.
/// 提示词节点与策略使用的稳定标识。
pub mod ids;
/// Prompt node rendering helpers.
/// 提示词节点渲染辅助。
pub mod node;
/// Personality catalog and defaults.
/// 人格目录与默认值。
pub mod personality;
/// Session-scoped baseline helpers.
/// 会话级基线辅助。
pub mod session;
/// Source snapshot definitions.
/// 来源快照定义。
pub mod source;
/// Prompt assembly strategies.
/// 提示词装配策略。
pub mod strategy;
/// Tool-help registry and bundle builders.
/// 工具帮助注册表与打包器。
pub mod tool_help;
/// Shared prompt types.
/// 共享提示词类型。
pub mod types;

pub use cache::invalidation::{PromptInvalidationEvent, PromptInvalidationReason};
pub use cache::memory::MemoryPromptCache;
pub use cache::report::PromptCacheReport;
pub use composer::diagnostics::{PromptCacheEvent, PromptComposerDiagnostics};
pub use composer::engine::PromptComposerEngine;
pub use composer::request::PromptAssemblyInput;
pub use composer::resources::PromptComposerResourceConfig;
pub use composer::response::PromptAssemblyOutput;
pub use error::PromptComposerError;
pub use personality::catalog::{PersonalityCatalog, PersonalityDefinition};
pub use session::baseline::PromptSessionBaseline;
pub use session::compaction::PromptCompactionSignal;
pub use session::delta_state::{ProjectRulesDeltaKind, ProjectRulesDeltaState};
pub use source::project_rules::{DEFAULT_PROJECT_RULES_FILE_NAME, ProjectRulesFileCache};
pub use source::snapshot::{
    AgentPromptSnapshot, EnvironmentContextSnapshot, ProjectRulesSnapshot, ProviderPromptSnapshot,
    SubagentSnapshot, TaskGoalSnapshot, ToolCatalogSnapshot,
};
pub use source::version::PromptSourceVersion;
pub use tool_help::registry::{ToolHelpEntry, ToolHelpRegistry};
pub use types::{
    ModelDescriptor, PersonalitySelection, PromptAssemblyMode, PromptNodeKind, PromptSegment,
    PromptTarget, ToolDescriptor,
};

#[cfg(test)]
mod tests;
