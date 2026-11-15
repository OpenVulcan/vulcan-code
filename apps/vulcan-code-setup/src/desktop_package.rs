//! Starts a private copy of the console installer without allocating a console.
//! 启动终端安装器的私有副本，同时避免分配控制台。

use std::os::windows::process::CommandExt;
use std::process::Command;

/// Executes the embedded installer with its original arguments or opens the desktop UI.
/// 以内嵌安装器执行原始参数，或者打开图形界面。
pub fn run(image: &[u8]) -> i32 {
    match launch(image) {
        Ok(code) => code,
        Err(error) => {
            crate::windows::setup_error(&format!("Cannot start Vulcan Code Setup: {error}"));
            1
        }
    }
}

/// Writes the image into a unique directory and returns the child status after cleanup.
/// 将映像写入唯一目录，并在清理后返回子进程退出状态。
fn launch(image: &[u8]) -> std::io::Result<i32> {
    if image.is_empty() {
        return Err(std::io::Error::other(
            "the console installer was not embedded",
        ));
    }
    // Use a per-launch directory so concurrent invocations never share an executable.
    // 为每次启动创建独立目录，避免多个安装进程共用可执行文件。
    let directory = tempfile::Builder::new().prefix("vulcan-setup-").tempdir()?;
    // Use the executable name expected by the setup engine.
    // 使用安装引擎约定的可执行文件名。
    let program = directory.path().join(crate::integrate::SETUP_PROGRAM);
    std::fs::write(&program, image)?;
    // A desktop launch has no arguments; legacy updater arguments must pass through unchanged.
    // 桌面双击没有参数；旧更新器的参数必须原样传递。
    let mut arguments = std::env::args_os().skip(1).peekable();
    // Prevent a fresh console from appearing before the child starts its window.
    // 子进程创建窗口之前也禁止新建控制台。
    let mut command = Command::new(program);
    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    match arguments.peek() {
        None => {
            command.arg("--ui=gui");
        }
        Some(first) if first.to_string_lossy().starts_with('/') => {
            command
                .arg(crate::inno::PACKAGE_PATH_OPTION)
                .arg(std::env::current_exe()?);
        }
        Some(_) => {}
    }
    command.args(arguments);
    Ok(command.status()?.code().unwrap_or(1))
}
