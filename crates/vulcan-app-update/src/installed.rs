//! Recognizing the installed layout this process runs from.
//! 识别本进程所运行的已安装布局。

use std::path::{Path, PathBuf};

use vulcan_install_core::install_root::{CURRENT_FILE, CURRENT_LINK, InstallRoot, VERSIONS_DIR};
use vulcan_install_core::names::{ComponentName, Platform, ReleaseVersion};
use vulcan_install_core::record::{InstallRecord, RECORD_FILE_NAME};
use vulcan_service_api::AppUpdateUnavailableReason;

/// The installed version this process runs from.
/// 本进程运行所在的已安装版本。
#[derive(Clone, Debug)]
pub(crate) struct Installed {
    /// The install root.
    /// 安装根目录。
    pub(crate) root: InstallRoot,
    /// The running version.
    /// 正在运行的版本。
    pub(crate) version: ReleaseVersion,
    /// Its platform.
    /// 其平台。
    pub(crate) platform: Platform,
    /// Install epoch of the running version.
    /// 正在运行版本的安装纪元。
    pub(crate) install_epoch: u32,
    /// Installed components.
    /// 已安装的组件。
    pub(crate) components: Vec<ComponentName>,
    /// The running executable relative to its version directory.
    /// 正在运行的可执行文件相对其版本目录的路径。
    pub(crate) executable: PathBuf,
}

impl Installed {
    /// The same executable inside another installed version.
    /// 另一个已安装版本中的同一可执行文件。
    pub(crate) fn executable_of(&self, version: &ReleaseVersion) -> PathBuf {
        self.root.version_dir(version).join(&self.executable)
    }
}

/// Finds `<root>/versions/<version>/` above `executable`, where `<version>` is the running product
/// version, the root has a current pointer and the version has its install record.
/// 在 `executable` 之上查找 `<root>/versions/<version>/`：其中 `<version>` 为正在运行的产品版本，根目录
/// 有当前版本指针，且该版本有安装记录。
pub(crate) fn detect(
    executable: &Path,
    running: &str,
) -> Result<Installed, AppUpdateUnavailableReason> {
    if cfg!(debug_assertions) {
        return Err(AppUpdateUnavailableReason::DevelopmentBuild);
    }
    let platform = Platform::current().ok_or(AppUpdateUnavailableReason::UnsupportedPlatform)?;
    let executable = executable
        .canonicalize()
        .map_err(|_| AppUpdateUnavailableReason::NotInstalled)?;
    let version: ReleaseVersion = running
        .parse()
        .map_err(|_| AppUpdateUnavailableReason::NotInstalled)?;
    let version_dir = executable
        .ancestors()
        .find(|dir| {
            dir.file_name().and_then(|name| name.to_str()) == Some(&version.to_string())
                && dir
                    .parent()
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str())
                    == Some(VERSIONS_DIR)
        })
        .ok_or(AppUpdateUnavailableReason::NotInstalled)?;
    let root_path = version_dir
        .parent()
        .and_then(Path::parent)
        .ok_or(AppUpdateUnavailableReason::NotInstalled)?;
    let pointer = if platform.is_windows() {
        CURRENT_FILE
    } else {
        CURRENT_LINK
    };
    if std::fs::symlink_metadata(root_path.join(pointer)).is_err() {
        return Err(AppUpdateUnavailableReason::NotInstalled);
    }
    if !version_dir.join(RECORD_FILE_NAME).is_file() {
        return Err(AppUpdateUnavailableReason::InstallRecordMissing);
    }
    let record = InstallRecord::read(version_dir)
        .map_err(|_| AppUpdateUnavailableReason::InstallRecordMissing)?;
    if record.version != version || record.platform != platform {
        return Err(AppUpdateUnavailableReason::InstallRecordMissing);
    }
    let relative = executable
        .strip_prefix(version_dir)
        .map_err(|_| AppUpdateUnavailableReason::NotInstalled)?
        .to_path_buf();
    Ok(Installed {
        root: InstallRoot::new(root_path),
        version,
        platform,
        install_epoch: record.install_epoch,
        components: record.components.into_keys().collect(),
        executable: relative,
    })
}
