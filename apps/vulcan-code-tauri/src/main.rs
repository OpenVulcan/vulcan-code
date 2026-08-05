//! Tauri desktop shell for the new Vulcan Code WebUI.
//! 新版 Vulcan Code WebUI 的 Tauri 桌面壳。

#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod desktop_shell;

/// Starts the Tauri desktop application.
/// 启动 Tauri 桌面应用。
fn main() {
    // cryptoProvider is installed before Tauri or any background service can construct a TLS client.
    // cryptoProvider 会在 Tauri 或任何后台服务构造 TLS 客户端之前完成安装。
    vulcan_provider_adapters::install_process_rustls_crypto_provider()
        .expect("the process-wide Rustls CryptoProvider must install before desktop startup");
    desktop_shell::run();
}
