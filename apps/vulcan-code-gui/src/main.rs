//! Application entry module for the Rust + iced desktop prototype.
//! Rust + iced 桌面原型的应用入口模块。

mod app;
mod code_view;
mod diff;
mod model;
mod model_config;
mod rig_runtime;
mod style;
mod theme;
mod wheel_passthrough;

/// Starts the GUI application bootstrap sequence.
/// 启动图形界面应用的引导流程。
fn main() -> iced::Result {
    app::run()
}
