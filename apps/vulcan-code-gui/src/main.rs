//! Application entry module for the Rust + iced desktop prototype.
//! Rust + iced 桌面原型的应用入口模块。

mod agent_runtime;
mod app_shell;
mod attachments;
mod compat;
mod mcp;
mod profiles;
mod provider;
mod runtime_support;
mod session;
mod shared;
mod skills;
mod tooling;
mod ui;
mod workspace_store;

pub(crate) use app_shell as app;
pub(crate) use app_shell::model;
pub(crate) use profiles::agent_profile;
pub(crate) use attachments::{asset_index as attachment_asset_index, assets as attachment_assets};
pub(crate) use compat::{
    image_understanding as image_understanding_compat,
    media_generation as media_generation_compat,
    web_search as web_search_compat,
};
pub(crate) use provider::ai_tool_configuration;
pub(crate) use provider::catalog::ai_capability_provider_catalog;
pub(crate) use provider::{
    capabilities as provider_capabilities,
    config as provider_config,
    input_normalizer as provider_input_normalizer,
    usage as provider_usage,
};
pub(crate) use runtime_support::{luaskills_runtime, project_temp_maintenance, runtime_assets};
pub(crate) use session::{agent_view, context_budget, conversation_protocol};
pub(crate) use shared::{code_view, composer_parts, data_plane, diff, workspace_path};
pub(crate) use tooling::bridge as vulcan_tools_bridge;
pub(crate) use tooling::result_format as tool_result_format;
pub(crate) use ui::{style, theme};
pub(crate) use ui::widgets::{selectable_transcript, wheel_passthrough};

/// Starts the GUI application bootstrap sequence.
/// 启动图形界面应用的引导流程。
fn main() -> iced::Result {
    app::run()
}
