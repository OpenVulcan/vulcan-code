//! Desktop integration helpers for the independent Vulcan Code service.
//! 独立 Vulcan Code 服务使用的桌面集成辅助函数。
//!
//! The service process owns no workbench window. These helpers expose only the host integrations
//! the service still needs: revealing a local path and opening a trusted URL with the operating
//! system. Closing a client window never routes through this module.
//! 服务进程不拥有任何工作台窗口。本模块只提供服务工作仍需的宿主集成：在操作系统中展示本地路径
//! 以及打开可信 URL。关闭客户端窗口绝不经过本模块。

use std::{path::Path, process::Command};

use vulcan_host_tools::process_creation::apply_windows_release_no_window_to_std_command;

/// Opens one filesystem directory with the host file manager.
/// 使用宿主文件管理器打开单个文件系统目录。
///
/// # Parameters
/// 参数
/// `path` is the local filesystem path to reveal or open.
/// `path` 是需要展示或打开的本地文件系统路径。
///
/// # Returns
/// 返回值
/// `Ok(())` when the operating system accepted the open request.
/// 当操作系统接受打开请求时返回 `Ok(())`。
pub fn open_path_in_system_file_manager(path: &str) -> Result<(), String> {
    // native_path is the standard host spelling passed to every platform file-manager process.
    // native_path 是传给各平台文件管理器进程的标准宿主路径写法。
    let native_path = vulcan_runtime_assets::native_host_path_text(path);
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer");
        if Path::new(&native_path).is_file() {
            command.arg(format!("/select,{native_path}"));
        } else {
            command.arg(&native_path);
        }
        command
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        if Path::new(&native_path).is_file() {
            command.arg("-R").arg(&native_path);
        } else {
            command.arg(&native_path);
        }
        command
    };

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        if Path::new(&native_path).is_file() {
            if let Some(parent) = Path::new(&native_path).parent() {
                command.arg(parent);
            } else {
                command.arg(&native_path);
            }
        } else {
            command.arg(&native_path);
        }
        command
    };

    apply_windows_release_no_window_to_std_command(&mut command);
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Builds one platform browser command without routing URL metacharacters through a shell.
/// 构建平台浏览器命令，避免 URL 元字符再次经过命令行 Shell 解析。
///
/// # Parameters
/// 参数
/// `url` is the complete validated external URL that must remain one process argument.
/// `url` 是必须作为单个进程参数保留的完整已校验外部 URL。
///
/// # Returns
/// 返回值
/// A platform command ready for the common release-window and spawn policies.
/// 返回可应用公共 Release 窗口策略并启动的平台命令。
fn system_browser_command(url: &str) -> Command {
    #[cfg(target_os = "windows")]
    let command = {
        // rundll32 invokes the registered URL protocol handler directly, so `&` remains URL data.
        // rundll32 直接调用已注册的 URL 协议处理器，因此 `&` 保持为 URL 数据。
        let mut command = Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler").arg(url);
        command
    };

    #[cfg(target_os = "macos")]
    let command = {
        let mut command = Command::new("open");
        command.arg(url);
        command
    };

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };

    command
}

/// Opens one URL with the host default browser.
/// 使用宿主默认浏览器打开单个 URL。
///
/// # Parameters
/// 参数
/// `url` is the trusted URL to open in the host browser.
/// `url` 是需要在宿主浏览器中打开的可信 URL。
///
/// # Returns
/// 返回值
/// `Ok(())` when the operating system accepted the open request.
/// 当操作系统接受打开请求时返回 `Ok(())`。
pub fn open_url_in_system_browser(url: &str) -> Result<(), String> {
    let mut command = system_browser_command(url);
    apply_windows_release_no_window_to_std_command(&mut command);
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::system_browser_command;
    use std::ffi::OsStr;

    /// Verifies Windows preserves a multi-parameter authorization URL as one process argument.
    /// 验证 Windows 将包含多个参数的授权 URL 完整保留为单个进程参数。
    #[test]
    fn windows_browser_command_preserves_complete_authorization_url() {
        // authorizationUrl reproduces the CommandCode callback and state separator that cmd.exe split.
        // authorizationUrl 复现曾被 cmd.exe 拆分的 CommandCode 回调与 state 分隔符。
        let authorization_url = "https://commandcode.ai/studio/auth/cli?callback=http%3A%2F%2Flocalhost%3A5959%2Fcallback&state=test-state-value";
        let command = system_browser_command(authorization_url);
        let arguments = command.get_args().collect::<Vec<_>>();

        assert_eq!(command.get_program(), OsStr::new("rundll32.exe"));
        assert_eq!(
            arguments,
            vec![
                OsStr::new("url.dll,FileProtocolHandler"),
                OsStr::new(authorization_url),
            ]
        );
    }
}
