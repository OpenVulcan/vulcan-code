//! The multi-version install root shared by the setup program, the installers and the updater.
//! 安装器、安装包与更新器共用的多版本安装根目录。
//!
//! ```text
//! <root>/versions/<version>/   one complete release tree each, never modified after the switch
//! <root>/updates/<version>.partial/  assembly area, kept across interruptions
//! <root>/updates/.lock         held while anything assembles or switches
//! <root>/current.json          Windows: {"version","path"}, read by the launcher
//! <root>/current               Linux and macOS: symbolic link to versions/<version>
//! ```
//!
//! A running process keeps executing from its real version directory, so switching never disturbs
//! it (plan section 4).
//! 运行中的进程始终从其真实版本目录执行，因此切换不会干扰它（方案第四节）。

use std::fs::File;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{FormatError, FormatResult};
use crate::names::ReleaseVersion;

/// Directory holding one subdirectory per installed version.
/// 每个已安装版本各占一个子目录的目录。
pub const VERSIONS_DIR: &str = "versions";

/// Directory holding partial assemblies and the install lock.
/// 存放未完成组装与安装锁的目录。
pub const UPDATES_DIR: &str = "updates";

/// Suffix of a version's assembly directory below [`UPDATES_DIR`].
/// [`UPDATES_DIR`] 下版本组装目录的后缀。
pub const PARTIAL_SUFFIX: &str = ".partial";

/// Lock file below [`UPDATES_DIR`].
/// [`UPDATES_DIR`] 下的锁文件。
const LOCK_FILE: &str = ".lock";

/// Windows pointer to the current version, read by the launcher.
/// Windows 上指向当前版本的文件，由启动器读取。
pub const CURRENT_FILE: &str = "current.json";

/// Unix symbolic link to the current version.
/// Unix 上指向当前版本的符号链接。
pub const CURRENT_LINK: &str = "current";

/// Content of `current.json`.
/// `current.json` 的内容。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentVersion {
    /// The current version.
    /// 当前版本。
    pub version: ReleaseVersion,
    /// Its directory relative to the root, with the platform separator.
    /// 其相对根目录的路径，使用平台分隔符。
    pub path: String,
}

/// An install root.
/// 安装根目录。
#[derive(Clone, Debug)]
pub struct InstallRoot {
    /// Root directory.
    /// 根目录。
    path: PathBuf,
}

/// Exclusive hold on an install root; released when dropped.
/// 对安装根目录的独占持有；释放时解锁。
#[derive(Debug)]
pub struct InstallLock {
    /// The locked file.
    /// 被锁定的文件。
    file: File,
}

impl Drop for InstallLock {
    fn drop(&mut self) {
        let _ = fs4::FileExt::unlock(&self.file);
    }
}

impl InstallRoot {
    /// Uses `path` as the install root.
    /// 使用 `path` 作为安装根目录。
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The root directory.
    /// 根目录。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Directory of an installed version.
    /// 已安装版本的目录。
    pub fn version_dir(&self, version: &ReleaseVersion) -> PathBuf {
        self.path.join(VERSIONS_DIR).join(version.to_string())
    }

    /// Assembly directory of a version.
    /// 版本的组装目录。
    pub fn staging_dir(&self, version: &ReleaseVersion) -> PathBuf {
        self.path
            .join(UPDATES_DIR)
            .join(format!("{version}{PARTIAL_SUFFIX}"))
    }

    /// Takes the install lock, failing at once when another process holds it.
    /// 获取安装锁；其他进程持有时立即失败。
    pub fn lock(&self) -> FormatResult<InstallLock> {
        let fail = |source| FormatError::io("opening the install lock", source);
        let updates = self.path.join(UPDATES_DIR);
        std::fs::create_dir_all(&updates).map_err(fail)?;
        let file = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(updates.join(LOCK_FILE))
            .map_err(fail)?;
        match fs4::FileExt::try_lock(&file) {
            Ok(()) => Ok(InstallLock { file }),
            Err(fs4::TryLockError::WouldBlock) => Err(FormatError::InstallBusy),
            Err(fs4::TryLockError::Error(source)) => Err(fail(source)),
        }
    }

    /// The version the root currently points to, if any.
    /// 根目录当前指向的版本（若有）。
    pub fn current_version(&self) -> FormatResult<Option<ReleaseVersion>> {
        if cfg!(windows) {
            let bytes = match std::fs::read(self.path.join(CURRENT_FILE)) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(source) => return Err(FormatError::io("reading current.json", source)),
            };
            let current: CurrentVersion =
                serde_json::from_slice(&bytes).map_err(|source| FormatError::Json {
                    what: "current.json",
                    source,
                })?;
            Ok(Some(current.version))
        } else {
            let target = match std::fs::read_link(self.path.join(CURRENT_LINK)) {
                Ok(target) => target,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(source) => return Err(FormatError::io("reading the current link", source)),
            };
            let name = target
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| {
                    FormatError::invalid(
                        "current link",
                        format!("{} names no version", target.display()),
                    )
                })?;
            name.parse().map(Some)
        }
    }

    /// Points the root at an installed version atomically; the caller holds the install lock.
    /// 原子地让根目录指向一个已安装版本；调用方须持有安装锁。
    pub fn switch_to(&self, _lock: &InstallLock, version: &ReleaseVersion) -> FormatResult<()> {
        let fail = |source| FormatError::io("switching the current version", source);
        if !self.version_dir(version).is_dir() {
            return Err(FormatError::invalid(
                "current version",
                format!("{version} is not installed"),
            ));
        }
        if cfg!(windows) {
            let current = CurrentVersion {
                version: version.clone(),
                path: format!("{VERSIONS_DIR}\\{version}"),
            };
            let bytes = serde_json::to_vec(&current).map_err(|source| FormatError::Json {
                what: "current.json",
                source,
            })?;
            let partial = self.path.join(format!("{CURRENT_FILE}{PARTIAL_SUFFIX}"));
            std::fs::write(&partial, bytes).map_err(fail)?;
            // Rename replaces the destination atomically on Windows as well.
            // 在 Windows 上 rename 同样原子地替换目标。
            std::fs::rename(&partial, self.path.join(CURRENT_FILE)).map_err(fail)
        } else {
            let partial = self.path.join(format!("{CURRENT_LINK}{PARTIAL_SUFFIX}"));
            match std::fs::remove_file(&partial) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => return Err(fail(source)),
            }
            create_symlink(&format!("{VERSIONS_DIR}/{version}"), &partial).map_err(fail)?;
            std::fs::rename(&partial, self.path.join(CURRENT_LINK)).map_err(fail)
        }
    }
}

/// Marker below [`UPDATES_DIR`] for a version that has not yet started successfully.
/// [`UPDATES_DIR`] 下标记尚未成功启动的版本的文件。
///
/// The Windows launcher (`apps/vulcan-code-launcher/src/trial.rs`) counts its attempts and
/// switches back once the marker's `max_attempts` were made; the new service removes it once it is
/// ready. The launcher mirrors this file name and the fields of [`StartupTrial`] and [`Rollback`];
/// change both together.
/// Windows 启动器（`apps/vulcan-code-launcher/src/trial.rs`）统计尝试次数，达到标记中的
/// `max_attempts` 后切换回去；新服务就绪后删除它。启动器镜像此文件名以及 [`StartupTrial`] 与
/// [`Rollback`] 的字段，两处须同时修改。
pub const STARTUP_TRIAL_FILE: &str = "startup-trial.json";

/// Marker below [`UPDATES_DIR`] left by the launcher after it switched back to the previous version.
/// 启动器切换回上一版本后留在 [`UPDATES_DIR`] 下的标记。
pub const ROLLBACK_FILE: &str = "rollback.json";

/// Launches of a new version that may fail to start before the launcher switches back; written
/// into each [`StartupTrial`], so the launcher reads it instead of keeping a copy.
/// 启动器切换回去之前新版本允许启动失败的次数；写入每个 [`StartupTrial`]，启动器读取它而不保留副本。
pub const MAX_STARTUP_ATTEMPTS: u32 = 2;

/// Content of [`STARTUP_TRIAL_FILE`].
/// [`STARTUP_TRIAL_FILE`] 的内容。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartupTrial {
    /// The version on trial.
    /// 试运行的版本。
    pub version: ReleaseVersion,
    /// The version to switch back to.
    /// 要切换回去的版本。
    pub previous: ReleaseVersion,
    /// Launches made so far.
    /// 至今的启动次数。
    pub attempts: u32,
    /// Launches allowed before switching back, [`MAX_STARTUP_ATTEMPTS`] when written.
    /// 切换回去前允许的启动次数，写入时为 [`MAX_STARTUP_ATTEMPTS`]。
    pub max_attempts: u32,
}

/// Content of [`ROLLBACK_FILE`].
/// [`ROLLBACK_FILE`] 的内容。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rollback {
    /// The version that did not start.
    /// 未能启动的版本。
    pub failed: ReleaseVersion,
    /// The version restored.
    /// 已恢复的版本。
    pub restored: ReleaseVersion,
}

impl InstallRoot {
    /// Every installed version, in ascending order; other entries of the versions directory are
    /// ignored.
    /// 全部已安装版本，按升序排列；versions 目录中的其他条目被忽略。
    pub fn installed_versions(&self) -> FormatResult<Vec<ReleaseVersion>> {
        let dir = self.path.join(VERSIONS_DIR);
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(FormatError::io("listing installed versions", source)),
        };
        let mut versions = Vec::new();
        for entry in entries {
            let entry =
                entry.map_err(|source| FormatError::io("listing installed versions", source))?;
            let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
            if let (true, Some(version)) = (
                is_dir,
                entry
                    .file_name()
                    .to_str()
                    .and_then(|name| name.parse::<ReleaseVersion>().ok()),
            ) {
                versions.push(version);
            }
        }
        versions.sort();
        Ok(versions)
    }

    /// Removes every installed version except `current` and the newest one below it (plan section
    /// 8); a version still in use stays and is retried next time. Returns the versions removed.
    /// 删除除 `current` 及其下最新一个版本以外的全部已安装版本（方案第八节）；仍在使用的版本保留，下次
    /// 再试。返回已删除的版本。
    pub fn prune_versions(
        &self,
        _lock: &InstallLock,
        current: &ReleaseVersion,
    ) -> FormatResult<Vec<ReleaseVersion>> {
        let installed = self.installed_versions()?;
        let previous = installed.iter().rev().find(|version| *version < current);
        let mut removed = Vec::new();
        for version in &installed {
            if version == current || Some(version) == previous {
                continue;
            }
            if std::fs::remove_dir_all(self.version_dir(version)).is_ok() {
                removed.push(version.clone());
            }
        }
        Ok(removed)
    }

    /// Reads a marker document below [`UPDATES_DIR`], if present.
    /// 读取 [`UPDATES_DIR`] 下的标记文档（若存在）。
    pub fn read_marker<T: serde::de::DeserializeOwned>(
        &self,
        name: &'static str,
    ) -> FormatResult<Option<T>> {
        match std::fs::read(self.path.join(UPDATES_DIR).join(name)) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|source| FormatError::Json { what: name, source }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(FormatError::io("reading an update marker", source)),
        }
    }

    /// Replaces a marker document below [`UPDATES_DIR`] atomically.
    /// 原子替换 [`UPDATES_DIR`] 下的标记文档。
    pub fn write_marker<T: Serialize>(
        &self,
        _lock: &InstallLock,
        name: &'static str,
        value: &T,
    ) -> FormatResult<()> {
        let fail = |source| FormatError::io("writing an update marker", source);
        let bytes =
            serde_json::to_vec(value).map_err(|source| FormatError::Json { what: name, source })?;
        let dir = self.path.join(UPDATES_DIR);
        std::fs::create_dir_all(&dir).map_err(fail)?;
        let partial = dir.join(format!("{name}{PARTIAL_SUFFIX}"));
        std::fs::write(&partial, bytes).map_err(fail)?;
        std::fs::rename(&partial, dir.join(name)).map_err(fail)
    }

    /// Removes a marker document below [`UPDATES_DIR`]; a missing one is not an error.
    /// 删除 [`UPDATES_DIR`] 下的标记文档；不存在不算错误。
    pub fn remove_marker(&self, _lock: &InstallLock, name: &'static str) -> FormatResult<()> {
        match std::fs::remove_file(self.path.join(UPDATES_DIR).join(name)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(FormatError::io("removing an update marker", source)),
        }
    }
}

/// Creates a symbolic link; Windows release trees never contain one.
/// 创建符号链接；Windows 发行树中从不包含符号链接。
pub(crate) fn create_symlink(target: &str, link: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(not(unix))]
    {
        let _ = (target, link);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "symbolic links are not installed on this platform",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_points_the_root_at_an_installed_version() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = InstallRoot::new(temp.path());
        let lock = root.lock().expect("lock");
        let first: ReleaseVersion = "0.1.3".parse().expect("version");
        let second: ReleaseVersion = "0.1.4".parse().expect("version");
        assert_eq!(root.current_version().expect("read"), None);
        assert!(root.switch_to(&lock, &first).is_err());
        for version in [&first, &second] {
            std::fs::create_dir_all(root.version_dir(version)).expect("version dir");
        }
        root.switch_to(&lock, &first).expect("switch");
        assert_eq!(root.current_version().expect("read"), Some(first));
        root.switch_to(&lock, &second).expect("switch again");
        assert_eq!(root.current_version().expect("read"), Some(second));
    }

    #[test]
    fn the_install_lock_is_exclusive() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = InstallRoot::new(temp.path());
        let held = root.lock().expect("first lock");
        assert!(matches!(root.lock(), Err(FormatError::InstallBusy)));
        drop(held);
        assert!(root.lock().is_ok());
    }
}
