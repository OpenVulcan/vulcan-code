//! Shared agent runtime for provider execution, tools, and streaming turns.
//! 共享代理运行时，负责 Provider 执行、工具调用与流式轮次。

#![expect(
    clippy::too_many_arguments,
    reason = "agent-runtime provider and tool boundary adapters currently mirror protocol and runtime context fields; objectifying every adapter is a dedicated architecture refactor."
)]
#![expect(
    clippy::large_enum_variant,
    reason = "execution stream events intentionally carry owned terminal reports across async boundaries until the stream envelope is redesigned."
)]
#![expect(
    clippy::result_large_err,
    reason = "runtime launch preparation returns owned terminal reports for existing stream error paths."
)]

// Re-export the extracted protocol core under the historical source name during namespace migration.
// 在命名空间迁移期间以历史源码名称引用已提取的协议核心。
extern crate vulcan_llm_core as vulcan_ai_protocols;

pub mod agent_profile;
pub mod agent_runtime;
pub mod attachment_asset_index;
pub mod attachment_assets;
pub mod context_budget;
pub mod conversation_protocol;
pub mod data_plane {
    //! Shared data-plane helpers re-exported for extracted runtime modules.
    //! 为抽取后的运行时模块重新导出的共享数据平面辅助函数。

    pub use vulcan_workspace_store::data_plane::*;
}
pub mod luaskills_runtime;
pub mod mcp;
pub mod provider_input_normalizer;
mod provider_tool_bridge;
pub mod session_event_runtime;
pub mod skills;
pub mod tooling;
pub mod turn_stats;
pub mod workbench_prompt_turn;
pub mod workspace_path;

pub use agent_runtime::{
    ExecutionKnowledgeAccessContext, ExecutionPreparation, ExecutionPreparationRequest,
    ExecutionPromptPurpose, ExecutionProviderVisibleAgentMessage, ExecutionRunReport,
    ExecutionSessionGoalContext, ExecutionStreamEvent, KnowledgeSearchHandler, RuntimeFoundation,
    RuntimePromptIntervention, RuntimePromptInterventionError, RuntimeSkillKind,
    ensure_runtime_defaults, stream_runtime_run, stream_runtime_run_with_intervention,
    stream_runtime_run_with_intervention_and_plugins,
    stream_runtime_run_with_intervention_plugins_and_public_ai,
};
pub use tooling::bridge as vulcan_tools_bridge;
pub use vulcan_host_tools::project_temp_maintenance;
pub use vulcan_host_tools::result_format as tool_result_format;
pub use vulcan_provider_config::config as provider_config;
pub use vulcan_runtime_assets as runtime_assets;
pub use vulcan_workbench_model as model;
pub use vulcan_workspace_store::provider_usage;
