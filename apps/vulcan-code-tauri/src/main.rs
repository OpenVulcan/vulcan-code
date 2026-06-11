//! Tauri desktop shell for the new Vulcan Code WebUI.
//! 新版 Vulcan Code WebUI 的 Tauri 桌面壳。

mod desktop_shell;

/// Starts the Tauri desktop application.
/// 启动 Tauri 桌面应用。
fn main() {
    desktop_shell::run();
}
