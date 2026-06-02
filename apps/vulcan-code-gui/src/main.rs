//! Application entry module for the Rust + iced desktop prototype.
//! Rust + iced 桌面原型的应用入口模块。

mod app_shell;
mod context_compaction;
mod i18n;
mod session;
mod shared;
mod ui;

pub(crate) use app_shell as app;
pub(crate) use app_shell::model;
pub(crate) use i18n::{
    AppLocale, I18nArg, I18nKey, LanguageManager, format_for_locale, text_for_locale,
};
pub(crate) use session::agent_view;
pub(crate) use shared::{code_view, composer_parts, data_plane, diff, workspace_path};
pub(crate) use ui::widgets::{selectable_transcript, wheel_passthrough};
pub(crate) use ui::{style, theme};
pub(crate) use vulcan_agent_runtime::{
    agent_profile, agent_runtime, attachment_asset_index, attachment_assets, context_budget,
    conversation_protocol, luaskills_runtime, mcp, project_temp_maintenance, provider_usage, skills,
    tooling,
};
pub(crate) use vulcan_provider_config::ai_tool_configuration;
pub(crate) use vulcan_provider_config::catalog::ai_capability_provider_catalog;
pub(crate) use vulcan_provider_config::{
    capabilities as provider_capabilities, config as provider_config,
};
pub(crate) use vulcan_runtime_assets as runtime_assets;
pub(crate) use vulcan_workspace_store::workspace_store;

/// Starts the GUI application bootstrap sequence.
/// 启动图形界面应用的引导流程。
fn main() -> iced::Result {
    app::mark_startup_trace("main:start");
    app::run()
}
