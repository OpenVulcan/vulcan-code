//! Application entry module for the Rust + iced desktop prototype.
//! Rust + iced 桌面原型的应用入口模块。

mod agent_profile;
mod agent_runtime;
mod agent_view;
mod app;
mod attachment_asset_index;
mod attachment_assets;
mod code_view;
mod composer_parts;
mod context_budget;
mod conversation_protocol;
mod data_plane;
mod diff;
mod luaskills_runtime;
mod mcp;
mod model;
mod project_temp_maintenance;
mod provider_capabilities;
mod provider_config;
mod provider_input_normalizer;
mod provider_usage;
mod runtime_assets;
mod selectable_transcript;
mod skills;
mod style;
mod theme;
mod tool_result_format;
mod tooling;
mod vulcan_tools_bridge;
mod web_search_compat;
mod wheel_passthrough;
mod workspace_path;
mod workspace_store;

/// Starts the GUI application bootstrap sequence.
/// 启动图形界面应用的引导流程。
fn main() -> iced::Result {
    app::run()
}
