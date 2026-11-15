//! In-place adoption of the Windows install registered by the old Inno package.
//! 原地接管由旧 Inno 安装包登记的 Windows 安装。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use vulcan_install_core::fetch::Route;
use vulcan_install_core::install_root::{InstallRoot, UPDATES_DIR, VERSIONS_DIR};
use vulcan_install_core::names::{Channel, DESKTOP_COMPONENT, Platform, ReleaseVersion};
use vulcan_install_core::record::InstallRecord;

use crate::integrate::{CLI_DIR, WINDOWS_SHORTCUT, windows_uninstall_key};
use crate::root::Version;
use crate::state::{Item, SETUP_DIR, STATE_SCHEMA, SetupState};
use crate::{paths, windows};

/// Internal option carrying the original graphical package path during a legacy update.
/// 旧版更新期间传递原始图形包路径的内部选项。
pub const PACKAGE_PATH_OPTION: &str = "--legacy-update-package";

/// Accepts the exact invocation sent by already shipped Windows updaters.
/// 接受已发布 Windows 更新器发送的确切调用形式。
pub fn run_legacy_update(arguments: &[String], desktop_package: Option<&Path>) -> i32 {
    if arguments != ["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/RELAUNCH=1"] {
        eprintln!("vulcan-code-setup: unexpected Windows updater arguments");
        return 2;
    }
    // The compatibility invocation uses the published filename even when the engine was unpacked.
    // 即使安装引擎已经解包，兼容调用也使用发布产物的文件名。
    let program = match desktop_package {
        Some(path) => Ok(path.to_path_buf()),
        None => std::env::current_exe(),
    };
    let mut install_root = None;
    let result = program
        .map_err(|error| error.to_string())
        .and_then(|program| {
            let (root, version) = legacy_update_target(&program)?;
            install_root = Some(root.clone());
            crate::install::adopt_release(&root, &version, legacy_channel(&version))?;
            relaunch(&root)?;
            Ok(())
        });
    if let Err(error) = result {
        if let Some(root) = install_root {
            let _ = std::fs::write(
                root.join(UPDATES_DIR).join("setup-adoption-error.log"),
                &error,
            );
        }
        eprintln!("vulcan-code-setup: {error}");
        return 1;
    }
    if let Some(root) = install_root {
        let _ = std::fs::remove_file(root.join(UPDATES_DIR).join("setup-adoption-error.log"));
    }
    0
}

/// Uses the release version's prerelease status when the original updater passes no channel.
/// 旧更新器未传通道时，按版本的预发布标记选择通道。
fn legacy_channel(version: &ReleaseVersion) -> Channel {
    if version.is_prerelease() {
        Channel::Preview
    } else {
        Channel::Stable
    }
}

/// Starts the updated application from the stable Windows launcher.
/// 通过稳定的 Windows 启动器启动更新后的应用。
pub(super) fn relaunch(root: &Path) -> Result<(), String> {
    // The GUI launcher can outlive the unpacked installer, so start it in the installation root
    // with valid streams independent of any console the installer detached from.
    // 图形启动器可以在临时安装器退出后继续运行，因此从安装根目录启动它，
    // 并提供独立于安装器已分离控制台的有效标准流。
    Command::new(root.join(crate::integrate::ROOT_LAUNCHER))
        .current_dir(root)
        .env_remove("VULCAN_RUNTIME_LAYOUT")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("cannot restart Vulcan Code: {error}"))
}

/// Extracts the version and root from the shipped update cache and artifact naming contract.
/// 按已发布的更新缓存及产物命名契约提取版本和根目录。
fn legacy_update_target(program: &Path) -> Result<(PathBuf, ReleaseVersion), String> {
    // Both historical artifact names carry the exact requested product version.
    // 两种历史产物名称都包含请求的确切产品版本。
    let name = program
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or("invalid Windows update name")?;
    let version = name
        .strip_prefix("VulcanCode-")
        .and_then(|file| {
            file.strip_suffix("-windows-x64-setup.exe")
                .or_else(|| file.strip_suffix("-windows-x64-update.exe"))
        })
        .ok_or("unexpected Windows update filename")?
        .parse::<ReleaseVersion>()
        .map_err(|error| error.to_string())?;
    let cache = program
        .parent()
        .filter(|path| path.ends_with(Path::new(UPDATES_DIR).join("installers")))
        .ok_or("Windows updater did not run from the installation update cache")?;
    let root = cache
        .parent()
        .and_then(Path::parent)
        .ok_or("Windows update cache has no installation directory")?;
    Ok((root.to_path_buf(), version))
}

/// Reads the registered installation's component record and existing Windows integrations.
/// 读取已登记安装的组件记录和现有 Windows 集成。
pub fn choices(path: &Path) -> Result<SetupState, String> {
    // Compare the pointer identity with the version's record before treating files as ours.
    // 先核对版本指针和记录的标识，再将文件视为本产品所有。
    let root = InstallRoot::new(path);
    let version = root
        .current_version()
        .map_err(|error| error.to_string())?
        .ok_or("the Inno installation has no current version")?;
    let directory = root.version_dir(&version);
    windows::reject_reparse_point(&directory)?;
    let record = InstallRecord::read(&directory).map_err(|error| error.to_string())?;
    if record.version != version || record.platform != Platform::WindowsX64 {
        return Err(format!(
            "{} does not match its Windows installation record",
            directory.display()
        ));
    }
    let layout = Version::read(&directory)?.layout;
    let cli = path.join(CLI_DIR);
    let (path_enabled, service) = windows::inno_integration(&path.join(VERSIONS_DIR), &cli)?;
    let launcher = path.join(crate::integrate::ROOT_LAUNCHER);
    let menu_path = paths::start_menu_dir()?.join(WINDOWS_SHORTCUT);
    let menu = windows::shortcut_targets(&menu_path, &launcher)?;
    let mut items = Vec::new();
    if menu {
        items.push(Item::Shortcut { path: menu_path });
    }
    let mut desktop_shortcut = false;
    if let Some(directory) = paths::desktop_dir() {
        let shortcut = directory.join(WINDOWS_SHORTCUT);
        desktop_shortcut = windows::shortcut_targets(&shortcut, &launcher)?;
        if desktop_shortcut {
            items.push(Item::Shortcut { path: shortcut });
        }
    }
    let gui = record
        .components
        .keys()
        .any(|name| name.as_str() == DESKTOP_COMPONENT);
    if gui != layout.desktop_executable.is_some() {
        return Err(String::from(
            "Inno component record and desktop layout disagree",
        ));
    }
    Ok(SetupState {
        schema: STATE_SCHEMA,
        channel: Channel::Stable,
        route: Route::Auto,
        gui,
        with: record
            .components
            .into_keys()
            .filter(|name| !matches!(name.as_str(), "core" | "tui" | DESKTOP_COMPONENT))
            .collect(),
        menu,
        desktop_shortcut,
        path: path_enabled,
        service,
        linger: false,
        items,
    })
}

/// Commits ownership only after the new integration has succeeded.
/// 新集成成功后才提交安装归属。
pub fn complete(root: &Path, state: &SetupState) -> Result<(), String> {
    state.write(root)?;
    retire(root)
}

/// Retires only the registered Inno executable for this root, retaining its log for diagnosis.
/// 只停用当前目录登记的 Inno 卸载程序，并保留其日志供诊断。
pub fn retire(root: &Path) -> Result<(), String> {
    let Some(uninstaller) = windows::registered_inno(root)? else {
        return Ok(());
    };
    // Rename before deleting the registration. A retry can finish if the process exits between them.
    // 删除注册项之前先改名。如果进程在两步之间结束，重试可继续完成。
    let retired = root.join(SETUP_DIR).join("inno-retired.exe.bak");
    match std::fs::rename(&uninstaller, &retired) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && retired.is_file() => (),
        Err(error) => return Err(format!("cannot retire {}: {error}", uninstaller.display())),
    }
    if let Err(error) = windows::delete_uninstall_entry(windows::INNO_ENTRY) {
        std::fs::rename(&retired, &uninstaller)
            .map_err(|restore| format!("{error}; cannot restore old uninstaller: {restore}"))?;
        return Err(error);
    }
    // Keep Inno's associated log under setup, where the unified uninstaller can remove it.
    // 将 Inno 的关联日志保存在 setup 下，方便统一卸载时移除。
    for extension in ["dat", "msg"] {
        let log = uninstaller.with_extension(extension);
        if log.is_file() {
            let _ = std::fs::rename(&log, retired.with_extension(extension));
        }
    }
    Ok(())
}

/// Clears a partially written Rust uninstall registration only when it points at this root.
/// 仅当部分写入的 Rust 卸载登记指向当前目录时清理它。
pub fn remove_new_entry(root: &Path) -> Result<(), String> {
    let key = windows_uninstall_key(root)?;
    if windows::uninstall_location(&key)?.is_some_and(|path| windows::same_install_dir(&path, root))
    {
        windows::delete_uninstall_entry(&key)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Checks both updater entry names resolve only the exact cached release and install root.
    /// 验证两种更新入口名称都只解析确切的缓存版本和安装目录。
    #[test]
    fn legacy_arguments_work_for_both_windows_release_assets() {
        let root = Path::new(r"C:\Custom Vulcan 安装");
        for name in [
            "VulcanCode-0.1.25-windows-x64-setup.exe",
            "VulcanCode-0.1.25-windows-x64-update.exe",
        ] {
            let program = root.join("updates").join("installers").join(name);
            let (actual, version) = legacy_update_target(&program).expect("registered cache path");
            assert_eq!(actual, root);
            assert_eq!(version.to_string(), "0.1.25");
            assert_eq!(legacy_channel(&version), Channel::Stable);
            assert!(legacy_update_target(&root.join(name)).is_err());
        }
    }
}
