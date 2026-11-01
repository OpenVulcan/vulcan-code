//! Installer and status reader for luaskills-debug, the standalone LuaSkills developer tool.
//! 独立 LuaSkills 开发工具 luaskills-debug 的安装器与状态读取器。
//!
//! The tool follows the major and minor version of the LuaSkills runtime this host embeds. It is
//! downloaded from the GitHub release of `LuaSkills/luaskills-debug` through the LuaSkills network
//! settings (GitHub endpoints, mirror and token), verified against the release checksums file, and
//! published atomically under `<user-vulcan-code-root>/tools/luaskills-debug/<version>`.
//! 该工具跟随本宿主内嵌 LuaSkills 运行时的主次版本。安装器经由 LuaSkills 网络设置（GitHub 端点、
//! 镜像与令牌）从 `LuaSkills/luaskills-debug` 的 GitHub release 下载，按 release 校验文件验证，
//! 并原子发布到 `<user-vulcan-code-root>/tools/luaskills-debug/<version>`。

use crate::luaskills_bridge::TauriLuaSkillsHandle;
use crate::managed_runtime_bootstrap::{
    ManagedRuntimeBootstrapProgress, managed_runtime_platform_key,
};
use crate::managed_runtime_installer::{
    commit_staging_directory, download_file, emit_progress, extract_archive, mark_executable,
    official_download_client, parse_named_sha256, prepare_clean_directory, remove_path_if_exists,
    sibling_work_path, verify_sha256,
};
use fs2::FileExt;
use luaskills::GithubReleaseSourceSpec;
use luaskills::download::manager::{DownloadManager, DownloadManagerConfig};
use serde::Deserialize;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

/// LuaSkills release whose developer-tool series this host installs.
/// 本宿主安装其开发工具系列所对应的 LuaSkills 版本。
const EMBEDDED_LUASKILLS_VERSION: &str = env!("VULCAN_LUASKILLS_DEPENDENCY_VERSION");

/// GitHub repository that publishes luaskills-debug releases.
/// 发布 luaskills-debug release 的 GitHub 仓库。
pub const LUASKILLS_DEBUG_REPOSITORY: &str = "LuaSkills/luaskills-debug";

/// Release asset listing the SHA-256 of every other asset.
/// 列出其他所有资产 SHA-256 的 release 资产。
const CHECKSUMS_ASSET_NAME: &str = "luaskills-debug-checksums.txt";

/// Manifest written at the root of every luaskills-debug tool archive.
/// 每个 luaskills-debug 工具归档根部写入的清单。
const TOOL_MANIFEST_FILE_NAME: &str = "debug-tool-manifest.json";

/// File the archive's setup_runtime script writes once the Lua runtime packages are installed.
/// 归档内 setup_runtime 脚本在 Lua 运行时包安装完成后写入的文件。
const RUNTIME_PACKAGES_MANIFEST: &[&str] = &["resources", "luaskills-packages-manifest.json"];

/// Lock file that serializes installs across processes sharing one user root.
/// 在共享同一用户根的进程之间串行化安装的锁文件。
const INSTALL_LOCK_FILE_NAME: &str = ".install.lock";

/// Installed state of luaskills-debug as the settings page and the model tool read it.
/// 设置页与模型工具读取的 luaskills-debug 安装状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaSkillsDebugToolStatus {
    /// `major.minor` series that matches the embedded LuaSkills runtime.
    /// 与内嵌 LuaSkills 运行时匹配的 `major.minor` 系列。
    pub series: String,
    /// Release platform key of this host, when luaskills-debug publishes one for it.
    /// 本宿主的发布平台键（luaskills-debug 为其发布时存在）。
    pub platform: Option<String>,
    /// Directory that holds every installed version.
    /// 存放所有已安装版本的目录。
    pub install_root: Option<PathBuf>,
    /// Newest valid installation, preferring the matching series.
    /// 最新的有效安装，优先匹配系列。
    pub installed: Option<LuaSkillsDebugInstallation>,
}

impl LuaSkillsDebugToolStatus {
    /// Returns whether the selected installation belongs to the matching series.
    /// 返回所选安装是否属于匹配系列。
    pub fn compatible(&self) -> bool {
        self.installed
            .as_ref()
            .is_some_and(|installation| version_series(&installation.version) == self.series)
    }
}

/// One valid luaskills-debug installation.
/// 一个有效的 luaskills-debug 安装。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaSkillsDebugInstallation {
    /// Installed release version without a leading `v`.
    /// 不带前导 `v` 的已安装 release 版本。
    pub version: String,
    /// Extracted tool folder.
    /// 解压后的工具目录。
    pub package_root: PathBuf,
    /// luaskills-debug executable.
    /// luaskills-debug 可执行文件。
    pub executable: PathBuf,
    /// Private debug runtime root inside the tool folder.
    /// 工具目录内的私有调试运行时根。
    pub runtime_root: PathBuf,
    /// Whether setup_runtime has installed the Lua runtime packages.
    /// setup_runtime 是否已安装 Lua 运行时包。
    pub runtime_ready: bool,
}

/// Fields of `debug-tool-manifest.json` the installer relies on; other fields are ignored.
/// 安装器依赖的 `debug-tool-manifest.json` 字段；其余字段被忽略。
#[derive(Debug, Deserialize)]
struct ToolManifest {
    /// Platform key the archive was built for.
    /// 归档构建所针对的平台键。
    platform: String,
    /// Executable path relative to the tool folder.
    /// 相对工具目录的可执行文件路径。
    binary: String,
    /// Debug runtime root relative to the tool folder.
    /// 相对工具目录的调试运行时根。
    runtime_root: String,
    /// Release tag the archive belongs to.
    /// 归档所属的 release 标签。
    release_tag: String,
}

/// Returns the `major.minor` series of the embedded LuaSkills runtime.
/// 返回内嵌 LuaSkills 运行时的 `major.minor` 系列。
pub fn luaskills_debug_series() -> String {
    version_series(EMBEDDED_LUASKILLS_VERSION)
}

/// Returns the directory that holds every installed luaskills-debug version.
/// 返回存放所有已安装 luaskills-debug 版本的目录。
pub fn luaskills_debug_install_root() -> Option<PathBuf> {
    vulcan_runtime_assets::resolve_user_vulcan_code_root()
        .map(|root| root.join("tools").join("luaskills-debug"))
}

/// Reads the installed state without touching the network.
/// 在不访问网络的情况下读取安装状态。
///
/// # Errors
/// Returns an English error when the install root exists but cannot be listed.
/// 安装根存在但无法列出时返回英文错误。
pub fn read_luaskills_debug_status() -> Result<LuaSkillsDebugToolStatus, String> {
    let series = luaskills_debug_series();
    let platform = managed_runtime_platform_key().ok();
    let install_root = luaskills_debug_install_root();
    let installed = match (&install_root, &platform) {
        (Some(root), Some(platform)) => select_installation(root, platform, &series)?,
        _ => None,
    };
    Ok(LuaSkillsDebugToolStatus {
        series,
        platform,
        install_root,
        installed,
    })
}

/// Downloads, verifies and installs the newest luaskills-debug release of the matching series.
/// 下载、校验并安装匹配系列的最新 luaskills-debug release。
///
/// The latest release is used when it belongs to the series; otherwise the series' first release
/// `v<major>.<minor>.0` is installed, and a missing one is reported as an error.
/// 最新 release 属于该系列时使用它；否则安装该系列的首个 release `v<major>.<minor>.0`，
/// 不存在时报告错误。
///
/// # Errors
/// Returns an English error when downloads are disabled, the platform is unsupported, the release
/// or its checksum cannot be resolved, verification fails, or the folder cannot be published.
/// 下载被禁用、平台不受支持、无法解析 release 或其校验值、校验失败或无法发布目录时返回英文错误。
pub fn install_luaskills_debug<F>(
    handle: &TauriLuaSkillsHandle,
    progress: &mut F,
) -> Result<LuaSkillsDebugToolStatus, String>
where
    F: FnMut(&ManagedRuntimeBootstrapProgress),
{
    let platform = managed_runtime_platform_key()
        .map_err(|_| String::from("luaskills-debug is not published for this platform."))?;
    let install_root = luaskills_debug_install_root().ok_or_else(|| {
        String::from(
            "The user home directory is unavailable, so luaskills-debug cannot be installed.",
        )
    })?;
    fs::create_dir_all(&install_root).map_err(|error| {
        format!(
            "Failed to create the luaskills-debug folder {}: {error}",
            install_root.display()
        )
    })?;
    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(install_root.join(INSTALL_LOCK_FILE_NAME))
        .map_err(|error| format!("Failed to open the luaskills-debug install lock: {error}"))?;
    lock_file
        .try_lock_exclusive()
        .map_err(|_| String::from("Another luaskills-debug install is already running."))?;

    let series = luaskills_debug_series();
    let asset_name = format!("luaskills-debug-tool-{platform}.tar.gz");
    emit_progress(
        progress,
        "resolving",
        None,
        "Resolving the luaskills-debug release",
        2,
        None,
        None,
    );
    let manager = download_manager(handle, &install_root)?;
    let source = GithubReleaseSourceSpec {
        repo: LUASKILLS_DEBUG_REPOSITORY.to_string(),
        tag_api: None,
    };
    let latest = manager.resolve_github_release_asset(&source, &asset_name, None)?;
    let archive = if version_series(&latest.version) == series {
        latest
    } else {
        manager.resolve_github_release_asset(&source, &asset_name, Some(&format!("{series}.0")))?
    };
    let checksums = manager.resolve_github_release_asset(
        &source,
        CHECKSUMS_ASSET_NAME,
        Some(&archive.version),
    )?;

    let target = install_root.join(&archive.version);
    let downloads = sibling_work_path(&target, "download")?;
    prepare_clean_directory(&downloads)?;
    let result = (|| {
        let client = official_download_client("luaskills-debug")?;
        let checksums_path = downloads.join(CHECKSUMS_ASSET_NAME);
        download_file(
            &client,
            &checksums.download_url,
            &checksums_path,
            "luaskills-debug",
            "Downloading the luaskills-debug checksums",
            4,
            6,
            progress,
        )?;
        let checksums_text = fs::read_to_string(&checksums_path).map_err(|error| {
            format!("Failed to read the luaskills-debug checksums file: {error}")
        })?;
        let expected = parse_named_sha256(&checksums_text, &asset_name)?;
        if let Some(reported) = archive.sha256.as_deref()
            && !reported.eq_ignore_ascii_case(&expected)
        {
            return Err(format!(
                "GitHub reports a different SHA-256 for {asset_name} than the release checksums file."
            ));
        }
        let archive_path = downloads.join(&asset_name);
        download_file(
            &client,
            &archive.download_url,
            &archive_path,
            "luaskills-debug",
            "Downloading luaskills-debug",
            6,
            90,
            progress,
        )?;
        emit_progress(
            progress,
            "verifying",
            None,
            "Verifying luaskills-debug",
            92,
            None,
            None,
        );
        verify_sha256(&archive_path, &expected)?;

        emit_progress(
            progress,
            "extracting",
            None,
            "Extracting luaskills-debug",
            94,
            None,
            None,
        );
        let staging = sibling_work_path(&target, "installing")?;
        prepare_clean_directory(&staging)?;
        extract_archive(&archive_path, &staging)?;
        let installation = validate_installation(&staging, &platform)?;
        if installation.version != archive.version {
            remove_path_if_exists(&staging)?;
            return Err(format!(
                "The luaskills-debug archive belongs to v{}, not to the resolved release {}.",
                installation.version, archive.tag_name
            ));
        }
        mark_executable(&installation.executable)?;
        commit_staging_directory(&staging, &target)
    })();
    let cleanup = remove_path_if_exists(&downloads);
    result?;
    cleanup?;
    emit_progress(
        progress,
        "completed",
        None,
        "luaskills-debug is installed",
        100,
        None,
        None,
    );
    drop(lock_file);
    read_luaskills_debug_status()
}

/// Builds the LuaSkills downloader from the running engine's network options.
/// 依据运行中引擎的网络选项构建 LuaSkills 下载器。
fn download_manager(
    handle: &TauriLuaSkillsHandle,
    install_root: &Path,
) -> Result<DownloadManager, String> {
    let network = handle
        .engine
        .read()
        .map_err(|_| String::from("The LuaSkills engine lock is poisoned."))?
        .network_options();
    if !network.allow_network_download {
        return Err(String::from(
            "LuaSkills downloads are turned off in the LuaSkills network settings.",
        ));
    }
    Ok(DownloadManager::new(DownloadManagerConfig {
        cache_root: install_root.join(".cache"),
        allow_network_download: network.allow_network_download,
        github_base_url: network.github_base_url,
        github_api_base_url: network.github_api_base_url,
        github_token: network.github_token,
    }))
}

/// Selects the newest valid installation, preferring versions of the matching series.
/// 选择最新的有效安装，优先匹配系列的版本。
fn select_installation(
    install_root: &Path,
    platform: &str,
    series: &str,
) -> Result<Option<LuaSkillsDebugInstallation>, String> {
    let entries = match fs::read_dir(install_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Failed to list the luaskills-debug folder {}: {error}",
                install_root.display()
            ));
        }
    };
    let mut best: Option<((bool, Vec<u64>), LuaSkillsDebugInstallation)> = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(numbers) = version_numbers(&name) else {
            continue;
        };
        let Ok(installation) = validate_installation(&entry.path(), platform) else {
            continue;
        };
        if installation.version != name {
            continue;
        }
        let rank = (version_series(&name) == series, numbers);
        if best.as_ref().is_none_or(|(best_rank, _)| rank > *best_rank) {
            best = Some((rank, installation));
        }
    }
    Ok(best.map(|(_, installation)| installation))
}

/// Validates one extracted tool folder against its manifest and this host's platform.
/// 按清单与本宿主平台校验一个已解压的工具目录。
fn validate_installation(
    package_root: &Path,
    platform: &str,
) -> Result<LuaSkillsDebugInstallation, String> {
    let manifest_text =
        fs::read_to_string(package_root.join(TOOL_MANIFEST_FILE_NAME)).map_err(|error| {
            format!("The luaskills-debug archive has no readable manifest: {error}")
        })?;
    let manifest: ToolManifest = serde_json::from_str(&manifest_text)
        .map_err(|error| format!("The luaskills-debug manifest is invalid: {error}"))?;
    if manifest.platform != platform {
        return Err(format!(
            "The luaskills-debug archive is built for {}, not for {platform}.",
            manifest.platform
        ));
    }
    let version = manifest.release_tag.trim_start_matches('v').to_string();
    if version_numbers(&version).is_none() {
        return Err(format!(
            "The luaskills-debug manifest names an invalid release tag `{}`.",
            manifest.release_tag
        ));
    }
    let executable = enclosed_path(package_root, &manifest.binary)?;
    if !executable.is_file() {
        return Err(format!(
            "The luaskills-debug executable is missing: {}",
            executable.display()
        ));
    }
    let runtime_root = enclosed_path(package_root, &manifest.runtime_root)?;
    let runtime_ready = RUNTIME_PACKAGES_MANIFEST
        .iter()
        .fold(runtime_root.clone(), |path, part| path.join(part))
        .is_file();
    Ok(LuaSkillsDebugInstallation {
        version,
        package_root: package_root.to_path_buf(),
        executable,
        runtime_root,
        runtime_ready,
    })
}

/// Joins one manifest-relative path, refusing absolute paths and parent components.
/// 拼接一个相对清单的路径，拒绝绝对路径与上级组件。
fn enclosed_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if relative.is_empty()
        || !path
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
    {
        return Err(format!(
            "The luaskills-debug manifest names a path outside the tool folder: `{relative}`."
        ));
    }
    Ok(root.join(path))
}

/// Returns the `major.minor` prefix of one `major.minor.patch` version.
/// 返回 `major.minor.patch` 版本的 `major.minor` 前缀。
fn version_series(version: &str) -> String {
    version
        .trim_start_matches('v')
        .splitn(3, '.')
        .take(2)
        .collect::<Vec<_>>()
        .join(".")
}

/// Parses one stable `major.minor.patch` version into comparable numbers.
/// 将一个稳定的 `major.minor.patch` 版本解析为可比较的数字。
fn version_numbers(version: &str) -> Option<Vec<u64>> {
    let numbers = version
        .split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    (numbers.len() == 3).then_some(numbers)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes one fake tool folder with the given manifest values.
    /// 写入一个带指定清单值的模拟工具目录。
    fn write_tool(root: &Path, version: &str, platform: &str, runtime_ready: bool) -> PathBuf {
        let folder = root.join(version);
        fs::create_dir_all(folder.join("bin")).unwrap();
        fs::create_dir_all(folder.join("runtime").join("resources")).unwrap();
        fs::write(folder.join("bin").join("luaskills-debug"), b"").unwrap();
        if runtime_ready {
            fs::write(
                folder
                    .join("runtime")
                    .join("resources")
                    .join("luaskills-packages-manifest.json"),
                b"{}",
            )
            .unwrap();
        }
        fs::write(
            folder.join(TOOL_MANIFEST_FILE_NAME),
            serde_json::json!({
                "schema_version": 1,
                "platform": platform,
                "binary": "bin/luaskills-debug",
                "runtime_root": "runtime",
                "release_tag": format!("v{version}"),
            })
            .to_string(),
        )
        .unwrap();
        folder
    }

    /// Selection prefers the matching series, then the highest version, and skips invalid folders.
    /// 选择优先匹配系列，其次最高版本，并跳过无效目录。
    #[test]
    fn selection_prefers_matching_series_and_skips_invalid_folders() {
        let temp = tempfile::tempdir().unwrap();
        write_tool(temp.path(), "0.8.1", "linux-x64", false);
        write_tool(temp.path(), "0.8.3", "linux-x64", true);
        write_tool(temp.path(), "0.9.0", "linux-x64", true);
        write_tool(temp.path(), "0.8.9", "windows-x64", true);
        fs::create_dir_all(temp.path().join(".0.8.4.installing")).unwrap();

        let selected = select_installation(temp.path(), "linux-x64", "0.8")
            .unwrap()
            .expect("a matching installation");
        assert_eq!(selected.version, "0.8.3");
        assert!(selected.runtime_ready);

        let other = select_installation(temp.path(), "linux-x64", "0.7")
            .unwrap()
            .expect("an installation of another series");
        assert_eq!(other.version, "0.9.0");
    }

    /// Manifest paths may not leave the tool folder.
    /// 清单路径不得离开工具目录。
    #[test]
    fn manifest_paths_stay_inside_the_tool_folder() {
        let root = Path::new("tool");
        assert!(enclosed_path(root, "bin/luaskills-debug").is_ok());
        assert!(enclosed_path(root, "../bin").is_err());
        assert!(enclosed_path(root, "").is_err());
    }

    /// Series and version parsing accept only stable three-part versions.
    /// 系列与版本解析只接受稳定的三段版本。
    #[test]
    fn versions_parse_into_series_and_numbers() {
        assert_eq!(version_series("v0.8.12"), "0.8");
        assert_eq!(version_numbers("0.8.12"), Some(vec![0, 8, 12]));
        assert_eq!(version_numbers("0.8"), None);
        assert_eq!(version_numbers("0.8.0-rc1"), None);
    }
}
