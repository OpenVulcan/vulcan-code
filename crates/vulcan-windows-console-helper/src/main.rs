//! Windows native console helper for Vulcan terminal sessions.
//! Vulcan 终端会话使用的 Windows 原生控制台 helper。

#[cfg(windows)]
mod windows_helper;

#[cfg(windows)]
fn main() {
    std::process::exit(windows_helper::run());
}

#[cfg(not(windows))]
fn main() {
    eprintln!("vulcan-windows-console-helper is only supported on Windows.");
    std::process::exit(1);
}
