//! Reading an install root: who owns it, which version is current, and what that version holds.
//! 读取安装根目录：归属、当前版本以及该版本包含的内容。

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use vulcan_install_core::install_root::{InstallRoot, UPDATES_DIR, VERSIONS_DIR};
use vulcan_install_core::names::{ComponentName, ReleaseVersion};
use vulcan_install_core::record::InstallRecord;

use crate::state::{SETUP_DIR, SetupState};

/// Ownership marker written by the archive-based `scripts/install.sh`, which this program
/// replaces; such a root is taken over on the next installation.
/// 基于归档的 `scripts/install.sh`（本程序取代了它）写出的归属标记；这类根目录在下一次安装时被接管。
pub const LEGACY_MARKER: &str = ".vulcan-code-installation";

/// Content of [`LEGACY_MARKER`].
/// [`LEGACY_MARKER`] 的内容。
const LEGACY_IDENTITY: &str = "vulcan-code-installation-v1";

/// Name prefix of the version directories `scripts/install.sh` created, named after the archive.
/// `scripts/install.sh` 创建的版本目录的名称前缀，以归档命名。
const LEGACY_GENERATION_PREFIX: &str = "vulcan-code-";

/// Entries an interrupted first installation may leave behind.
/// 中断的首次安装可能遗留的条目。
const INSTALL_LEFTOVERS: [&str; 3] = [VERSIONS_DIR, UPDATES_DIR, SETUP_DIR];

/// The release layout file at the top of every version.
/// 每个版本顶层的发行布局文件。
const LAYOUT_FILE: &str = "runtime-layout.json";

/// Who an install root belongs to.
/// 安装根目录的归属。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ownership {
    /// Missing, empty, or left over from an interrupted first installation.
    /// 不存在、为空，或是中断的首次安装遗留的目录。
    Fresh,
    /// Installed by this program.
    /// 由本程序安装。
    Owned(SetupState),
    /// Installed by the archive-based `scripts/install.sh`.
    /// 由基于归档的 `scripts/install.sh` 安装。
    Legacy,
    /// Installed by the Windows installer (`scripts/windows-installer.mjs`), whose uninstaller is
    /// this file; it updates through the app and uninstalls through that uninstaller.
    /// 由 Windows 安装程序（`scripts/windows-installer.mjs`）安装，其卸载程序为此文件；它通过应用更新，
    /// 通过该卸载程序卸载。
    Inno(PathBuf),
}

/// Like [`ownership`], but refuses a root the Windows installer owns, which this program does not
/// install into.
/// 与 [`ownership`] 相同，但拒绝 Windows 安装程序所属的根目录，本程序不向其中安装。
pub fn installable(root: &Path) -> Result<Ownership, String> {
    match ownership(root)? {
        Ownership::Inno(_) => Err(format!(
            "{} was installed by the Windows installer (VulcanCode-Setup.exe), which the app keeps \
             up to date; uninstall it first to install here with this program",
            root.display()
        )),
        ownership => Ok(ownership),
    }
}

/// The uninstaller Inno Setup writes at the top of the root it installed: `unins000.exe`, or a
/// higher number when that name was taken.
/// Inno Setup 在其安装的根目录顶层写出的卸载程序：`unins000.exe`，名称被占用时编号更大。
fn inno_uninstaller(root: &Path) -> Result<Option<PathBuf>, String> {
    let entries = std::fs::read_dir(root)
        .map_err(|error| format!("cannot read {}: {error}", root.display()))?;
    let mut found = None;
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot read {}: {error}", root.display()))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let is_uninstaller = name
            .to_ascii_lowercase()
            .strip_prefix("unins")
            .and_then(|rest| rest.strip_suffix(".exe"))
            .is_some_and(|number| number.len() == 3 && number.bytes().all(|b| b.is_ascii_digit()));
        if is_uninstaller
            && found
                .as_ref()
                .is_none_or(|known: &PathBuf| entry.path() > *known)
        {
            found = Some(entry.path());
        }
    }
    Ok(found)
}

/// Decides who owns `root`; a directory with foreign content is refused, never taken over.
/// 判定 `root` 的归属；含有他人内容的目录一律拒绝，绝不接管。
pub fn ownership(root: &Path) -> Result<Ownership, String> {
    let metadata = match std::fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Ownership::Fresh),
        Err(error) => return Err(format!("cannot read {}: {error}", root.display())),
    };
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "the installation directory must not be a symbolic link: {}",
            root.display()
        ));
    }
    if !metadata.is_dir() {
        return Err(format!("{} is not a directory", root.display()));
    }
    if let Some(state) = SetupState::read(root)? {
        return Ok(Ownership::Owned(state));
    }
    let legacy = root.join(LEGACY_MARKER);
    if legacy.is_file() {
        let identity = std::fs::read_to_string(&legacy)
            .map_err(|error| format!("cannot read {}: {error}", legacy.display()))?;
        if identity.trim() == LEGACY_IDENTITY {
            return Ok(Ownership::Legacy);
        }
        return Err(format!("{} is not recognized", legacy.display()));
    }
    if let Some(uninstaller) = inno_uninstaller(root)? {
        return Ok(Ownership::Inno(uninstaller));
    }
    let entries = std::fs::read_dir(root)
        .map_err(|error| format!("cannot read {}: {error}", root.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot read {}: {error}", root.display()))?;
        let name = entry.file_name();
        if !INSTALL_LEFTOVERS
            .iter()
            .any(|leftover| name.to_str() == Some(leftover))
        {
            return Err(format!(
                "refusing to install into {}: it holds files the setup program did not create",
                root.display()
            ));
        }
    }
    Ok(Ownership::Fresh)
}

/// What the current pointer of a root names.
/// 根目录的当前版本指针所指的内容。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Current {
    /// No current pointer.
    /// 没有当前版本指针。
    None,
    /// An installed release version.
    /// 已安装的发行版本。
    Version(ReleaseVersion),
    /// A generation of `scripts/install.sh`, by directory name; it never existed on Windows.
    /// `scripts/install.sh` 的代际，以目录名表示；Windows 上从未存在。
    #[cfg_attr(not(unix), allow(dead_code))]
    Legacy(String),
}

/// Reads the current pointer of `root`.
/// 读取 `root` 的当前版本指针。
pub fn current(root: &InstallRoot) -> Result<Current, String> {
    #[cfg(unix)]
    {
        let link = root
            .path()
            .join(vulcan_install_core::install_root::CURRENT_LINK);
        match std::fs::read_link(&link) {
            Ok(target) => {
                let name = target
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_owned();
                if name.starts_with(LEGACY_GENERATION_PREFIX) {
                    return Ok(Current::Legacy(name));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Current::None);
            }
            Err(error) => return Err(format!("cannot read {}: {error}", link.display())),
        }
    }
    Ok(
        match root.current_version().map_err(|error| error.to_string())? {
            Some(version) => Current::Version(version),
            None => Current::None,
        },
    )
}

/// Removes the version directories `scripts/install.sh` created, except `keep`; one still in use
/// stays and is retried by the next installation.
/// 删除 `scripts/install.sh` 创建的版本目录（`keep` 除外）；仍在使用的保留，由下一次安装重试。
pub fn remove_legacy_generations(root: &InstallRoot, keep: Option<&str>) {
    let Ok(entries) = std::fs::read_dir(root.path().join(VERSIONS_DIR)) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with(LEGACY_GENERATION_PREFIX)
            && name.parse::<ReleaseVersion>().is_err()
            && Some(name) != keep
        {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// The components recorded for an installed version.
/// 某已安装版本记录的组件。
pub fn installed_components(
    root: &InstallRoot,
    version: &ReleaseVersion,
) -> Result<BTreeSet<ComponentName>, String> {
    let record =
        InstallRecord::read(&root.version_dir(version)).map_err(|error| error.to_string())?;
    Ok(record.components.into_keys().collect())
}

/// The programs a version declares in its release layout, as paths relative to the version.
/// 某版本在其发行布局中声明的程序，以相对版本目录的路径表示。
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    /// The Core service.
    /// Core 服务。
    pub service_executable: String,
    /// The desktop app.
    /// 桌面应用。
    pub desktop_executable: Option<String>,
    /// The terminal client.
    /// 终端客户端。
    pub tui_executable: Option<String>,
    /// Notification identity Windows shortcuts of the desktop app carry; written by layout
    /// format 4 on Windows.
    /// 桌面应用的 Windows 快捷方式携带的通知身份；由布局格式 4 在 Windows 上写出。
    #[serde(default)]
    pub desktop_identity: Option<DesktopIdentity>,
}

/// Notification identity of the Windows desktop app (`runtime-layout.json`, `desktopIdentity`).
/// Windows 桌面应用的通知身份（`runtime-layout.json` 的 `desktopIdentity`）。
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(windows), allow(dead_code))]
pub struct DesktopIdentity {
    /// Name of the Start menu shortcut the desktop app creates for itself.
    /// 桌面应用为自身创建的开始菜单快捷方式的名称。
    pub name: String,
    /// Application user model ID.
    /// 应用用户模型 ID。
    pub app_user_model_id: String,
    /// CLSID of the notification activator, in braces.
    /// 通知激活器的 CLSID，带花括号。
    pub toast_activator_clsid: String,
}

/// An installed version and its layout.
/// 一个已安装版本及其布局。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    /// The version directory.
    /// 版本目录。
    pub dir: PathBuf,
    /// Its layout.
    /// 其布局。
    pub layout: Layout,
}

impl Version {
    /// Reads the version in `dir`.
    /// 读取 `dir` 中的版本。
    pub fn read(dir: &Path) -> Result<Self, String> {
        Ok(Self {
            dir: dir.to_path_buf(),
            layout: Layout::read(dir)?,
        })
    }

    /// The Core service program of this version.
    /// 本版本的 Core 服务程序。
    pub fn service(&self) -> PathBuf {
        self.dir.join(&self.layout.service_executable)
    }
}

impl Layout {
    /// Reads the layout of `version_dir`, refusing paths that leave it.
    /// 读取 `version_dir` 的布局，拒绝离开该目录的路径。
    pub fn read(version_dir: &Path) -> Result<Self, String> {
        let path = version_dir.join(LAYOUT_FILE);
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        let layout: Self = serde_json::from_slice(&bytes)
            .map_err(|error| format!("{} is not valid: {error}", path.display()))?;
        for entry in [
            Some(&layout.service_executable),
            layout.desktop_executable.as_ref(),
            layout.tui_executable.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            check_relative(entry)
                .map_err(|reason| format!("{} names `{entry}`, which {reason}", path.display()))?;
        }
        Ok(layout)
    }
}

/// What a version's service reports with `--describe`; only the fields the setup program uses.
/// 某版本的服务以 `--describe` 报告的内容；只含安装器使用的字段。
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Description {
    /// `release` or `debug`.
    /// `release` 或 `debug`。
    pub profile: String,
    /// The service's data directory, below the Vulcan user directory.
    /// 服务的数据目录，位于 Vulcan 用户目录之下。
    pub data_root: PathBuf,
    /// The service settings file; reported by versions that accept `--set-port-range`.
    /// 服务设置文件；由接受 `--set-port-range` 的版本报告。
    #[serde(default)]
    pub settings_path: Option<PathBuf>,
}

impl Description {
    /// Runs the service of `version_dir` with `--describe`; failing means it cannot load here.
    /// 以 `--describe` 运行 `version_dir` 的服务；失败表示它无法在本机加载。
    pub fn read(version_dir: &Path) -> Result<Self, String> {
        let layout = Layout::read(version_dir)?;
        let program = version_dir.join(&layout.service_executable);
        let output = std::process::Command::new(&program)
            .arg("--describe")
            .output()
            .map_err(|error| format!("cannot run {}: {error}", program.display()))?;
        if !output.status.success() {
            return Err(format!(
                "this build cannot run on this system:\n{}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        serde_json::from_slice(&output.stdout).map_err(|error| {
            format!(
                "{} --describe printed no description: {error}",
                program.display()
            )
        })
    }
}

/// Checks that `entry` stays inside the version and can be embedded in launcher scripts.
/// 检查 `entry` 位于版本目录之内，且可以嵌入启动脚本。
fn check_relative(entry: &str) -> Result<(), &'static str> {
    if entry.is_empty() || entry.contains(['"', '$', '`', '\\', '\n']) {
        return Err("contains characters a launcher cannot hold");
    }
    if !PathBuf::from(entry)
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err("leaves the version directory");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests::plain_state;

    #[test]
    fn only_empty_leftover_owned_or_legacy_roots_are_accepted() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path().join("root");
        assert_eq!(ownership(&root), Ok(Ownership::Fresh));
        std::fs::create_dir_all(root.join(UPDATES_DIR)).expect("updates");
        assert_eq!(ownership(&root), Ok(Ownership::Fresh));
        std::fs::write(root.join("notes.txt"), "mine").expect("foreign file");
        assert!(ownership(&root).is_err());
        std::fs::write(root.join(LEGACY_MARKER), format!("{LEGACY_IDENTITY}\n")).expect("marker");
        assert_eq!(ownership(&root), Ok(Ownership::Legacy));
        std::fs::write(root.join(LEGACY_MARKER), "someone-else").expect("marker");
        assert!(ownership(&root).is_err());
        let state = plain_state();
        state.write(&root).expect("state");
        assert_eq!(ownership(&root), Ok(Ownership::Owned(state)));
    }

    #[test]
    fn a_root_with_an_inno_uninstaller_belongs_to_the_windows_installer() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path();
        for name in [
            "unins000.exe",
            "unins000.dat",
            "unins001.exe",
            "uninstall.exe",
        ] {
            std::fs::write(root.join(name), "").expect("file");
        }
        assert_eq!(
            ownership(root),
            Ok(Ownership::Inno(root.join("unins001.exe")))
        );
        assert!(installable(root).is_err());
    }

    #[test]
    fn layout_paths_must_stay_inside_the_version() {
        let temp = tempfile::tempdir().expect("temp dir");
        let write = |service: &str| {
            let document = serde_json::json!({
                "schemaVersion": 1,
                "profile": "release",
                "serviceExecutable": service,
                "tuiExecutable": "bin/vulcan-tui",
            });
            std::fs::write(temp.path().join(LAYOUT_FILE), document.to_string()).expect("layout");
            Layout::read(temp.path())
        };
        assert_eq!(
            write("bin/vulcan-code-service").map(|layout| layout.service_executable),
            Ok(String::from("bin/vulcan-code-service"))
        );
        assert!(write("../bin/vulcan-code-service").is_err());
        assert!(write("/usr/bin/vulcan-code-service").is_err());
        assert!(write("bin/$(id)").is_err());
    }
}
