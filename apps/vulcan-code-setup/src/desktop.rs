//! Windows desktop entry that never allocates a console.
//! 从不分配控制台的 Windows 桌面入口。
#![cfg_attr(windows, windows_subsystem = "windows")]

/// Starts the packaged installer and propagates its exit status.
/// 启动内嵌安装器并传递其退出状态。
fn main() {
    #[cfg(windows)]
    std::process::exit(vulcan_code_setup::desktop_package(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/console-setup.exe"
    ))));
}
