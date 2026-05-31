//! Tauri desktop shell for the new Vulcan Code WebUI.
//! 新版 Vulcan Code WebUI 的 Tauri 桌面壳。

mod luaskills_bridge;
mod mcp_bridge;
mod service_bridge;
mod token_budget_bridge;

/// Starts the Tauri desktop application.
/// 启动 Tauri 桌面应用。
fn main() {
    service_bridge::run();
}
