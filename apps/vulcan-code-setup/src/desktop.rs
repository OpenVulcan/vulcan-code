//! File-manager entry for the graphical installation package.
//! 从文件管理器打开图形安装包的入口。
#![cfg_attr(windows, windows_subsystem = "windows")]

/// Starts the packaged installer and propagates its exit status.
/// 启动内嵌安装器并传递其退出状态。
fn main() {
    #[cfg(windows)]
    std::process::exit(vulcan_code_setup::desktop_package(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/console-setup"
    ))));
    #[cfg(unix)]
    std::process::exit(vulcan_code_setup::unix_desktop_package(include_bytes!(
        concat!(env!("OUT_DIR"), "/console-setup")
    )));
}
