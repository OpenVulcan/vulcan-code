//! Windows native console helper for Vulcan terminal sessions.
//! Vulcan 终端会话使用的 Windows 原生控制台 helper。

// Keep CLI-only code out of Cargo's synthetic bin test target.
// 将仅 CLI 执行的代码排除在 Cargo 合成的 bin 测试目标之外。
#[cfg(all(windows, not(test)))]
mod windows_helper;

#[cfg(all(windows, not(test)))]
fn main() {
    std::process::exit(windows_helper::run());
}

#[cfg(all(not(windows), not(test)))]
fn main() {
    eprintln!("vulcan-windows-console-helper is only supported on Windows.");
    std::process::exit(1);
}
