//! What the setup program installed besides the version directories, kept in
//! `<root>/setup/installation.json` so that modifying and removing never guess.
//! 安装器在版本目录之外安装的内容，保存在 `<root>/setup/installation.json`，使修改与移除从不猜测。

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use vulcan_install_core::fetch::Route;
use vulcan_install_core::names::{Channel, ComponentName};

/// Directory below the install root that holds the setup program and its state.
/// 安装根目录下保存安装器及其状态的目录。
pub const SETUP_DIR: &str = "setup";

/// State file below [`SETUP_DIR`].
/// [`SETUP_DIR`] 下的状态文件。
pub const STATE_FILE: &str = "installation.json";

/// Format version of [`STATE_FILE`]; other versions are refused.
/// [`STATE_FILE`] 的格式版本；其他版本一律拒绝。
pub const STATE_SCHEMA: u32 = 1;

/// How Vulcan Code is used on this machine.
/// Vulcan Code 在本机的使用方式。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    /// The desktop app and the terminal client.
    /// 桌面应用与终端客户端。
    Desktop,
    /// The terminal client only.
    /// 只有终端客户端。
    Terminal,
    /// The WebUI service as a systemd user unit, plus the terminal client (Linux only).
    /// 以 systemd 用户单元运行的 WebUI 服务，以及终端客户端（仅 Linux）。
    Service,
}

impl Mode {
    /// The word used on the command line and in the state file.
    /// 命令行与状态文件中使用的单词。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Terminal => "terminal",
            Self::Service => "service",
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for Mode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "desktop" => Ok(Self::Desktop),
            "terminal" => Ok(Self::Terminal),
            "service" => Ok(Self::Service),
            _ => Err(String::from("--mode accepts desktop, terminal or service")),
        }
    }
}

/// One thing created outside the version directories.
/// 在版本目录之外创建的一项内容。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Item {
    /// A file this program wrote, removed when it still carries the setup marker or is a copy of
    /// a release program.
    /// 本程序写入的文件；移除时它仍带有安装器标记或是发行程序的副本。
    File {
        /// Absolute path.
        /// 绝对路径。
        path: PathBuf,
    },
    /// A macOS application bundle this program wrote; removed with its content when its launcher
    /// script still carries the setup marker.
    /// 本程序写入的 macOS 应用程序包；其启动脚本仍带有安装器标记时连同内容一起移除。
    Bundle {
        /// Absolute path of the `.app` directory.
        /// `.app` 目录的绝对路径。
        path: PathBuf,
    },
    /// A directory this program created; removed when empty.
    /// 本程序创建的目录；为空时移除。
    Directory {
        /// Absolute path.
        /// 绝对路径。
        path: PathBuf,
    },
    /// A directory added to the user `PATH` in the Windows registry.
    /// 在 Windows 注册表中加入用户 `PATH` 的目录。
    UserPath {
        /// Absolute path.
        /// 绝对路径。
        dir: PathBuf,
    },
    /// The per-user uninstall entry under `HKCU\...\Uninstall`.
    /// `HKCU\...\Uninstall` 下的每用户卸载项。
    UninstallEntry {
        /// Key name below the uninstall key.
        /// 卸载键下的键名。
        key: String,
    },
    /// The systemd user unit registered by the service.
    /// 由服务注册的 systemd 用户单元。
    UserService,
}

/// Content of [`STATE_FILE`].
/// [`STATE_FILE`] 的内容。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetupState {
    /// Always [`STATE_SCHEMA`].
    /// 恒为 [`STATE_SCHEMA`]。
    pub schema: u32,
    /// The chosen mode.
    /// 所选模式。
    pub mode: Mode,
    /// The chosen channel.
    /// 所选通道。
    pub channel: Channel,
    /// The chosen download origins.
    /// 所选下载源。
    pub route: Route,
    /// Optional components chosen with `--with`.
    /// 以 `--with` 选择的可选组件。
    pub with: Vec<ComponentName>,
    /// Whether the desktop shortcut was wanted.
    /// 是否需要桌面快捷方式。
    pub shortcut: bool,
    /// Whether `vulcan-tui` was wanted on PATH.
    /// 是否需要把 `vulcan-tui` 加入 PATH。
    pub path: bool,
    /// Everything created, in creation order.
    /// 创建的全部内容，按创建顺序排列。
    pub items: Vec<Item>,
}

impl SetupState {
    /// Path of the state file of `root`.
    /// `root` 的状态文件路径。
    pub fn file(root: &Path) -> PathBuf {
        root.join(SETUP_DIR).join(STATE_FILE)
    }

    /// Reads the state of `root`; `None` when it has none.
    /// 读取 `root` 的状态；没有状态时为 `None`。
    pub fn read(root: &Path) -> Result<Option<Self>, String> {
        let path = Self::file(root);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
        };
        let state: Self = serde_json::from_slice(&bytes)
            .map_err(|error| format!("{} is not valid: {error}", path.display()))?;
        if state.schema != STATE_SCHEMA {
            return Err(format!(
                "{} has format {}, which this setup program does not know; use a newer one",
                path.display(),
                state.schema
            ));
        }
        Ok(Some(state))
    }

    /// Replaces the state of `root` atomically.
    /// 原子替换 `root` 的状态。
    pub fn write(&self, root: &Path) -> Result<(), String> {
        let path = Self::file(root);
        let fail = |error: std::io::Error| format!("cannot write {}: {error}", path.display());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(fail)?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        let mut partial = path.as_os_str().to_owned();
        partial.push(".partial");
        std::fs::write(&partial, bytes).map_err(fail)?;
        std::fs::rename(&partial, &path).map_err(fail)
    }

    /// Records `item` once.
    /// 记录 `item`（只记录一次）。
    pub fn add(&mut self, item: Item) {
        if !self.items.contains(&item) {
            self.items.push(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips_and_other_formats_are_refused() {
        let temp = tempfile::tempdir().expect("temp dir");
        assert_eq!(SetupState::read(temp.path()), Ok(None));
        let mut state = SetupState {
            schema: STATE_SCHEMA,
            mode: Mode::Terminal,
            channel: Channel::Stable,
            route: Route::Auto,
            with: vec![ComponentName::new("browser").expect("name")],
            shortcut: true,
            path: true,
            items: Vec::new(),
        };
        state.add(Item::UserService);
        state.add(Item::UserService);
        assert_eq!(state.items.len(), 1);
        state.write(temp.path()).expect("write");
        assert_eq!(SetupState::read(temp.path()), Ok(Some(state.clone())));
        state.schema = STATE_SCHEMA + 1;
        state.write(temp.path()).expect("write");
        assert!(SetupState::read(temp.path()).is_err());
    }
}
