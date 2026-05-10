//! Application entry module for the Rust + iced desktop prototype.
//! Rust + iced 桌面原型的应用入口模块。

mod agent_profile;
mod agent_runtime;
mod agent_view;
mod app;
mod code_view;
mod data_plane;
mod diff;
mod luaskills_runtime;
mod mcp;
mod model;
mod provider_config;
mod runtime_assets;
mod selectable_transcript;
mod skills;
mod style;
mod theme;
mod tooling;
mod wheel_passthrough;
mod workspace_store;

/// Starts the GUI application bootstrap sequence.
/// 启动图形界面应用的引导流程。
fn main() -> iced::Result {
    app::run()
}
