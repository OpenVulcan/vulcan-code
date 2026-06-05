//! Shared agent runtime for provider execution, tools, and streaming turns.
//! 共享代理运行时，负责 Provider 执行、工具调用与流式轮次。

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
pub mod image_understanding_compat;
pub mod luaskills_runtime;
pub mod mcp;
pub mod media_generation_compat;
pub mod provider_input_normalizer;
pub mod session_event_runtime;
pub mod skills;
pub mod tooling;
pub mod turn_stats;
pub mod web_search_compat;
pub mod workbench_ai_capability_router;
pub mod workbench_prompt_turn;
pub mod workspace_path;

pub use agent_runtime::{
    ExecutionPreparation, ExecutionPreparationRequest, ExecutionPromptPurpose, ExecutionRunReport,
    ExecutionStreamEvent, RuntimeFoundation, RuntimePromptIntervention, RuntimeSkillKind,
    ensure_runtime_defaults, stream_runtime_run, stream_runtime_run_with_intervention,
};
pub use tooling::bridge as vulcan_tools_bridge;
pub use vulcan_host_tools::project_temp_maintenance;
pub use vulcan_host_tools::result_format as tool_result_format;
pub use vulcan_provider_config::ai_tool_configuration;
pub use vulcan_provider_config::catalog::ai_capability_provider_catalog;
pub use vulcan_provider_config::{
    capabilities as provider_capabilities, config as provider_config,
};
pub use vulcan_runtime_assets as runtime_assets;
pub use vulcan_workbench_model as model;
pub use vulcan_workspace_store::provider_usage;
