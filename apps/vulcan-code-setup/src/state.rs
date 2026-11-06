//! What the setup program installed besides the version directories, kept in
//! `<root>/setup/installation.json` so that modifying and removing never guess.
//! 安装器在版本目录之外安装的内容，保存在 `<root>/setup/installation.json`，使修改与移除从不猜测。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use vulcan_install_core::fetch::Route;
use vulcan_install_core::names::{Channel, ComponentName};

/// The setup directory and its state file; the updater reads the same names to recognize a root
/// this program owns.
/// 安装器目录及其状态文件；更新器读取同样的名称以识别本程序拥有的根目录。
pub use vulcan_install_core::install_root::{SETUP_DIR, SETUP_STATE_FILE as STATE_FILE};

/// Marker file inside [`SETUP_DIR`] that a run which switched versions leaves until the service
/// is registered again, so a run repeated after a failure still registers it.
/// [`SETUP_DIR`] 中的标记文件：切换了版本的运行在服务重新注册之前留下它，使失败后重跑仍会注册服务。
pub const SERVICE_PENDING_FILE: &str = "service-pending";

/// Format version of [`STATE_FILE`]; formats 1 and 2 are upgraded when read, other versions are
/// refused. Format 3 added [`Item::AppArmorProfile`].
/// [`STATE_FILE`] 的格式版本；格式 1 与 2 在读取时升级，其他版本一律拒绝。格式 3 新增
/// [`Item::AppArmorProfile`]。
pub const STATE_SCHEMA: u32 = 3;

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
    /// A Windows shortcut this program wrote, or the one the desktop app creates for its
    /// notifications; removed when present.
    /// 本程序写入的 Windows 快捷方式，或桌面应用为其通知创建的快捷方式；存在时移除。
    Shortcut {
        /// Absolute path of the `.lnk` file.
        /// `.lnk` 文件的绝对路径。
        path: PathBuf,
    },
    /// The COM registration of the desktop app's notification activator, which the desktop app
    /// writes below `HKCU\Software\Classes\CLSID` (Windows).
    /// 桌面应用通知激活器的 COM 注册，由桌面应用写在 `HKCU\Software\Classes\CLSID` 下（Windows）。
    ToastActivator {
        /// The CLSID in braces.
        /// 带花括号的 CLSID。
        clsid: String,
    },
    /// The user service registered by the Core service: a systemd user unit, a launchd agent, or
    /// a sign-in entry.
    /// 由 Core 服务注册的用户服务：systemd 用户单元、launchd 代理或登录项。
    UserService,
    /// Lingering this program enabled for `user` (Linux).
    /// 本程序为 `user` 开启的驻留（Linux）。
    Linger {
        /// The user name.
        /// 用户名。
        user: String,
    },
    /// The AppArmor profile that lets the desktop app use user namespaces (Linux); written and
    /// removed with administrator rights.
    /// 允许桌面应用使用用户命名空间的 AppArmor 配置（Linux）；以管理员权限写入与移除。
    AppArmorProfile {
        /// Absolute path below `/etc/apparmor.d`.
        /// `/etc/apparmor.d` 下的绝对路径。
        path: PathBuf,
    },
}

/// Content of [`STATE_FILE`].
/// [`STATE_FILE`] 的内容。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetupState {
    /// Always [`STATE_SCHEMA`].
    /// 恒为 [`STATE_SCHEMA`]。
    pub schema: u32,
    /// The chosen channel.
    /// 所选通道。
    pub channel: Channel,
    /// The chosen download origins.
    /// 所选下载源。
    pub route: Route,
    /// Whether the desktop app was wanted.
    /// 是否需要桌面应用。
    pub gui: bool,
    /// Optional components chosen with `--with`.
    /// 以 `--with` 选择的可选组件。
    pub with: Vec<ComponentName>,
    /// Whether a menu entry or application bundle was wanted.
    /// 是否需要菜单项或应用程序包。
    pub menu: bool,
    /// Whether a desktop shortcut was wanted.
    /// 是否需要桌面快捷方式。
    pub desktop_shortcut: bool,
    /// Whether commands were wanted on PATH.
    /// 是否需要把命令加入 PATH。
    pub path: bool,
    /// Whether the Core service was wanted with the login session.
    /// 是否需要 Core 服务随登录会话启动。
    pub service: bool,
    /// Whether the service was wanted to keep running after logout (Linux).
    /// 是否需要服务在注销后继续运行（Linux）。
    pub linger: bool,
    /// Everything created, in creation order.
    /// 创建的全部内容，按创建顺序排列。
    pub items: Vec<Item>,
}

/// Only the format version, read before the rest.
/// 只含格式版本，先于其余内容读取。
#[derive(Deserialize)]
struct Schema {
    /// The format version.
    /// 格式版本。
    schema: u32,
}

/// How format 1 recorded the use of Vulcan Code.
/// 格式 1 记录 Vulcan Code 使用方式的方法。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ModeV1 {
    /// The desktop app and the terminal client.
    /// 桌面应用与终端客户端。
    Desktop,
    /// The terminal client only.
    /// 只有终端客户端。
    Terminal,
    /// The systemd user unit and the terminal client.
    /// systemd 用户单元与终端客户端。
    Service,
}

/// Content of [`STATE_FILE`] in format 1 (setup program 0.1).
/// 格式 1（安装器 0.1）中 [`STATE_FILE`] 的内容。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StateV1 {
    /// Always 1.
    /// 恒为 1。
    #[allow(dead_code)]
    schema: u32,
    /// The chosen mode.
    /// 所选模式。
    mode: ModeV1,
    /// The chosen channel.
    /// 所选通道。
    channel: Channel,
    /// The chosen download origins.
    /// 所选下载源。
    route: Route,
    /// Optional components.
    /// 可选组件。
    with: Vec<ComponentName>,
    /// Whether the menu entry or application bundle was wanted.
    /// 是否需要菜单项或应用程序包。
    shortcut: bool,
    /// Whether commands were wanted on PATH.
    /// 是否需要把命令加入 PATH。
    path: bool,
    /// Everything created; format 1 items are a subset of the current ones.
    /// 创建的全部内容；格式 1 的条目是当前条目的子集。
    items: Vec<Item>,
}

impl StateV1 {
    /// The same installation in the current format: the mode becomes the desktop app and service
    /// choices, and the old shortcut choice was the menu entry.
    /// 以当前格式表示的同一安装：模式变为桌面应用与服务两项选择，旧的快捷方式选择即菜单项。
    fn upgrade(self) -> SetupState {
        SetupState {
            schema: STATE_SCHEMA,
            channel: self.channel,
            route: self.route,
            gui: self.mode == ModeV1::Desktop,
            with: self.with,
            menu: self.shortcut,
            desktop_shortcut: false,
            path: self.path,
            service: self.mode == ModeV1::Service,
            linger: false,
            items: self.items,
        }
    }
}

impl SetupState {
    /// Path of the state file of `root`.
    /// `root` 的状态文件路径。
    pub fn file(root: &Path) -> PathBuf {
        root.join(SETUP_DIR).join(STATE_FILE)
    }

    /// Reads the state of `root`; `None` when it has none. Formats 1 and 2 are upgraded in place
    /// after their original bytes are kept beside it.
    /// 读取 `root` 的状态；没有状态时为 `None`。格式 1 与 2 先在旁边保留原字节，再原地升级。
    pub fn read(root: &Path) -> Result<Option<Self>, String> {
        let path = Self::file(root);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
        };
        let invalid =
            |error: serde_json::Error| format!("{} is not valid: {error}", path.display());
        let schema = serde_json::from_slice::<Schema>(&bytes)
            .map_err(invalid)?
            .schema;
        let state = match schema {
            STATE_SCHEMA => return serde_json::from_slice(&bytes).map(Some).map_err(invalid),
            1 => serde_json::from_slice::<StateV1>(&bytes)
                .map_err(invalid)?
                .upgrade(),
            // Format 2 has the same fields; format 3 only added an item kind.
            // 格式 2 字段相同；格式 3 只新增了一种条目。
            2 => SetupState {
                schema: STATE_SCHEMA,
                ..serde_json::from_slice::<SetupState>(&bytes).map_err(invalid)?
            },
            other => {
                return Err(format!(
                    "{} has format {other}, which this setup program does not know; use a newer \
                     one",
                    path.display()
                ));
            }
        };
        let mut backup = path.as_os_str().to_owned();
        backup.push(format!(".v{schema}"));
        let backup = PathBuf::from(backup);
        // An earlier backup of the same file is the original; it is never replaced.
        // 同一文件更早的备份才是原件，绝不替换。
        if !backup.exists() {
            std::fs::write(&backup, &bytes)
                .map_err(|error| format!("cannot write {}: {error}", backup.display()))?;
        }
        state.write(root)?;
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
pub(crate) mod tests {
    use super::*;

    /// A state with every choice off.
    /// 所有选择均关闭的状态。
    pub(crate) fn plain_state() -> SetupState {
        SetupState {
            schema: STATE_SCHEMA,
            channel: Channel::Stable,
            route: Route::Auto,
            gui: false,
            with: Vec::new(),
            menu: false,
            desktop_shortcut: false,
            path: false,
            service: false,
            linger: false,
            items: Vec::new(),
        }
    }

    #[test]
    fn state_round_trips_and_unknown_formats_are_refused() {
        let temp = tempfile::tempdir().expect("temp dir");
        assert_eq!(SetupState::read(temp.path()), Ok(None));
        let mut state = SetupState {
            gui: true,
            with: vec![ComponentName::new("browser").expect("name")],
            ..plain_state()
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

    #[test]
    fn format_1_is_upgraded_and_its_original_bytes_are_kept() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = SetupState::file(temp.path());
        std::fs::create_dir_all(path.parent().expect("parent")).expect("setup dir");
        let original = serde_json::json!({
            "schema": 1,
            "mode": "service",
            "channel": "preview",
            "route": "china",
            "with": ["browser"],
            "shortcut": true,
            "path": false,
            "items": [{"kind": "userService"}, {"kind": "file", "path": "/home/a/.local/bin/vulcan-tui"}],
        })
        .to_string();
        std::fs::write(&path, &original).expect("format 1");
        let state = SetupState::read(temp.path()).expect("read").expect("state");
        assert_eq!(state.schema, STATE_SCHEMA);
        assert!(state.service && !state.gui && state.menu && !state.path);
        assert_eq!(state.channel, Channel::Preview);
        assert_eq!(state.items.first(), Some(&Item::UserService));
        let mut backup = path.as_os_str().to_owned();
        backup.push(".v1");
        assert_eq!(std::fs::read_to_string(backup).expect("backup"), original);
        assert_eq!(SetupState::read(temp.path()), Ok(Some(state)));
    }

    #[test]
    fn format_2_is_upgraded_and_its_original_bytes_are_kept() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = SetupState::file(temp.path());
        std::fs::create_dir_all(path.parent().expect("parent")).expect("setup dir");
        let mut format_2 = serde_json::to_value(SetupState {
            gui: true,
            items: vec![Item::UserService],
            ..plain_state()
        })
        .expect("value");
        format_2["schema"] = serde_json::json!(2);
        let original = format_2.to_string();
        std::fs::write(&path, &original).expect("format 2");
        let state = SetupState::read(temp.path()).expect("read").expect("state");
        assert_eq!(state.schema, STATE_SCHEMA);
        assert!(state.gui);
        assert_eq!(state.items, vec![Item::UserService]);
        let mut backup = path.as_os_str().to_owned();
        backup.push(".v2");
        assert_eq!(std::fs::read_to_string(backup).expect("backup"), original);
        assert_eq!(SetupState::read(temp.path()), Ok(Some(state)));
    }
}
