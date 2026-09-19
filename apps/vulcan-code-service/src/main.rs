//! Vulcan Code independent service and user-session tray host.
//! Vulcan Code 独立服务与用户会话托盘宿主。

#![cfg_attr(
    all(target_os = "windows", not(debug_assertions), feature = "desktop-tray"),
    windows_subsystem = "windows"
)]

#[cfg(all(target_os = "windows", debug_assertions, feature = "desktop-tray"))]
mod debug_console;
mod desktop_integration;
#[cfg(feature = "desktop-tray")]
mod desktop_notifications;
mod launch;
mod runtime_layout;
mod service_runtime;
#[cfg(feature = "desktop-tray")]
mod tray;

/// Starts the Vulcan Code service process.
/// 启动 Vulcan Code 服务进程。
fn main() {
    // cryptoProvider is installed before Tauri or any background service can construct a TLS client.
    // cryptoProvider 会在 Tauri 或任何后台服务构造 TLS 客户端之前完成安装。
    vulcan_provider_adapters::install_process_rustls_crypto_provider()
        .expect("the process-wide Rustls CryptoProvider must install before service startup");
    let args: Vec<String> = std::env::args().collect();
    // describe_requested rejects mixed launch flags before test-root or business initialization.
    // describe_requested 在测试根或业务初始化之前拒绝混用启动参数。
    let describe_requested = match launch::service_describe_requested(&args) {
        Ok(requested) => requested,
        Err(error) => {
            eprintln!("[vulcan-code-service] {error}");
            std::process::exit(2);
        }
    };
    #[cfg(feature = "test-support")]
    if let Err(error) = service_runtime::install_isolated_test_root_from_args(&args) {
        eprintln!("[vulcan-code-service] {error}");
        std::process::exit(2);
    }
    #[cfg(not(feature = "test-support"))]
    if args
        .iter()
        .skip(1)
        .any(|arg| arg == crate::launch::ARG_TEST_ROOT || arg.starts_with("--test-root="))
    {
        eprintln!("[vulcan-code-service] --test-root is available only in test-support builds");
        std::process::exit(2);
    }
    if describe_requested {
        if let Err(error) = service_runtime::print_service_description() {
            eprintln!("[vulcan-code-service] describe failed: {error}");
            std::process::exit(2);
        }
        return;
    }
    // launchMode fixes one explicit Core role before the optional Debug console is attached.
    // launchMode 在挂载可选 Debug 控制台之前固定一个明确的 Core 启动角色。
    let launch_mode = match launch::parse_service_launch_args(&args) {
        Ok(mode) => mode,
        Err(error) => {
            eprintln!("[vulcan-code-service] {error}");
            eprintln!(
                "usage: vulcan-code-service (--gui-launch | --service | --headless | --describe) [--no-debug-console]"
            );
            std::process::exit(2);
        }
    };
    #[cfg(all(target_os = "windows", debug_assertions, feature = "desktop-tray"))]
    if matches!(launch_mode, launch::ServiceLaunchMode::GuiLaunch)
        && !args
            .iter()
            .skip(1)
            .any(|arg| arg == launch::ARG_NO_DEBUG_CONSOLE)
    {
        if let Err(error) = debug_console::ensure_gui_debug_console() {
            eprintln!("[vulcan-code-service] Debug console startup failed: {error}");
            std::process::exit(2);
        }
        eprintln!(
            "[vulcan-code-service] Debug Core console ready (pid {})",
            std::process::id()
        );
    }
    match launch_mode {
        launch::ServiceLaunchMode::Headless => service_runtime::run_headless(),
        // Both desktop launches keep only the tray event loop: the service owns no workbench
        // window, and no client window close can drive the service shutdown coordinator.
        // 两种桌面启动都只保留托盘事件循环：服务不拥有工作台窗口，
        // 任何客户端窗口关闭都不会驱动服务关闭协调器。
        #[cfg(feature = "desktop-tray")]
        launch::ServiceLaunchMode::Service | launch::ServiceLaunchMode::GuiLaunch => {
            tray::run_with_tray()
        }
        #[cfg(not(feature = "desktop-tray"))]
        launch::ServiceLaunchMode::Service | launch::ServiceLaunchMode::GuiLaunch => {
            eprintln!(
                "[vulcan-code-service] this headless build was compiled without desktop-tray; use --headless"
            );
            std::process::exit(2);
        }
    }
}
