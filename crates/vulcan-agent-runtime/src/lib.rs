//! Shared agent runtime for provider execution, tools, and streaming turns.
//! 共享代理运行时，负责 Provider 执行、工具调用与流式轮次。

// Test fixtures may spawn helper processes directly; production code goes through the shared factories.
// 测试夹具可直接启动辅助进程；生产代码一律经由共享工厂。
#![cfg_attr(test, allow(clippy::disallowed_methods))]
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

/// Worker-thread stack that any thread polling agent streams must reserve; deeply nested stream
/// futures exceed the platform default in debug builds.
/// 轮询智能体流的任何线程都必须预留的工作线程栈；深层嵌套的流 Future 在 debug 构建中会超过平台默认值。
pub const AGENT_STREAM_WORKER_STACK_BYTES: usize = 8 * 1024 * 1024;

pub mod agent_profile;
pub mod agent_runtime;
pub mod attachment_asset_index;
pub mod attachment_assets;
pub mod context_budget;
pub mod conversation_protocol;
pub mod image_conversion;
pub mod data_plane {
    //! Shared data-plane helpers re-exported for extracted runtime modules.
    //! 为抽取后的运行时模块重新导出的共享数据平面辅助函数。

    pub use vulcan_workspace_store::data_plane::*;
}
pub mod luaskills_runtime;
pub mod mcp;
pub mod media_resource_runtime;
mod media_understanding;
pub mod native_media_input;
pub mod prompt_media_projection;
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
    TransientReasoningStatus, TransientReasoningStatusKind,
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
