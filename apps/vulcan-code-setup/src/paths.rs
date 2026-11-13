//! Default locations of a per-user installation.
//! 每用户安装的默认位置。

use std::path::{Path, PathBuf};

use vulcan_install_core::names::Platform;

/// Directory name of the install root on Linux, below the XDG data directory; `scripts/install.sh`
/// used the same one.
/// Linux 上安装根目录在 XDG 数据目录下的名称；`scripts/install.sh` 使用同一名称。
const LINUX_ROOT_NAME: &str = "vulcan-code";

/// Directory name of the install root on Windows and macOS. On Windows it matches the Inno
/// installer's `DefaultDirName={autopf}\VulcanCode` for a per-user install.
/// Windows 与 macOS 上安装根目录的名称。Windows 上与 Inno 安装器每用户安装的
/// `DefaultDirName={autopf}\VulcanCode` 一致。
const ROOT_NAME: &str = "VulcanCode";

/// The default install root of `platform`.
/// `platform` 的默认安装根目录。
pub fn default_root(platform: Platform) -> Result<PathBuf, String> {
    let missing = || String::from("cannot find the user's data directory; pass --prefix");
    if platform.is_windows() {
        // `{autopf}` of a per-user Inno install is `%LOCALAPPDATA%\Programs`.
        // 每用户 Inno 安装的 `{autopf}` 为 `%LOCALAPPDATA%\Programs`。
        Ok(dirs::data_local_dir()
            .ok_or_else(missing)?
            .join("Programs")
            .join(ROOT_NAME))
    } else if platform.is_macos() {
        Ok(dirs::data_dir().ok_or_else(missing)?.join(ROOT_NAME))
    } else {
        Ok(dirs::data_dir().ok_or_else(missing)?.join(LINUX_ROOT_NAME))
    }
}

/// Directory of the command launchers on Linux and macOS.
/// Linux 与 macOS 上命令启动器所在的目录。
pub fn bin_dir() -> Result<PathBuf, String> {
    Ok(home()?.join(".local").join("bin"))
}

/// Directory of desktop entries on Linux.
/// Linux 上桌面项所在的目录。
pub fn applications_dir() -> Result<PathBuf, String> {
    Ok(dirs::data_dir()
        .ok_or_else(|| String::from("cannot find the user's data directory"))?
        .join("applications"))
}

/// The user's Start menu programs folder on Windows, `{userprograms}` of the Inno installer.
/// Windows 上用户的开始菜单程序文件夹，即 Inno 安装程序的 `{userprograms}`。
pub fn start_menu_dir() -> Result<PathBuf, String> {
    Ok(dirs::data_dir()
        .ok_or_else(|| String::from("cannot find the user's roaming data directory"))?
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs"))
}

/// The user's desktop folder, when the system names one.
/// 用户的桌面文件夹（系统给出时）。
pub fn desktop_dir() -> Option<PathBuf> {
    dirs::desktop_dir()
}

/// Directory of the user's applications on macOS.
/// macOS 上用户应用程序所在的目录。
pub fn user_applications_dir() -> Result<PathBuf, String> {
    Ok(home()?.join("Applications"))
}

/// The systemd user unit `vulcan-code-service --install-user-service` writes.
/// `vulcan-code-service --install-user-service` 写入的 systemd 用户单元。
pub fn systemd_user_unit() -> Result<PathBuf, String> {
    let config = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(config) => PathBuf::from(config),
        None => home()?.join(".config"),
    };
    Ok(config.join("systemd/user/vulcan-code.service"))
}

/// The home directory.
/// 主目录。
///
/// The setup program stays outside workspace-hack, so it cannot use
/// `vulcan_runtime_assets::user_home_dir`; its tests never resolve user paths.
/// 安装器不使用 workspace-hack，因此无法使用 `vulcan_runtime_assets::user_home_dir`；其测试从不解析
/// 用户路径。
#[allow(clippy::disallowed_methods)]
pub(crate) fn home() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or_else(|| String::from("cannot find the home directory"))
}

/// Makes `path` absolute against the current directory without touching the file system.
/// 以当前目录将 `path` 变为绝对路径，不访问文件系统。
pub fn absolute(path: &Path) -> Result<PathBuf, String> {
    std::path::absolute(path).map_err(|error| format!("cannot resolve {}: {error}", path.display()))
}
