//! Everything outside the version directories: commands, menu entries and shortcuts, PATH, the
//! uninstall entry, the service settings and the user service. Each created thing is recorded so
//! that removal never guesses.
//! 版本目录之外的一切：命令、菜单项与快捷方式、PATH、卸载项、服务设置与用户服务。每项创建的内容都被
//! 记录，移除时从不猜测。

use std::path::{Path, PathBuf};
use std::process::Command;

use vulcan_install_core::apparmor;
use vulcan_install_core::names::Platform;

use crate::paths;
use crate::report::{Privileged, Purpose, Reporter, Stage};
use crate::root::{Description, Layout, Version};
use crate::state::{Item, SERVICE_PENDING_FILE, SETUP_DIR};

/// Line that marks the launcher scripts, menu entries and bundles this program writes; the same
/// line as `scripts/install.sh`, so its launchers are refreshed instead of being skipped.
/// 标记本程序写入的启动脚本、菜单项与应用程序包的行；与 `scripts/install.sh` 相同，因此它的启动器会被
/// 刷新而不是被跳过。
pub const LAUNCHER_MARKER: &str = vulcan_install_core::install_root::GENERATED_MARKER;

pub use vulcan_install_core::install_root::SETUP_PROGRAM;

/// The PATH forwarder inside a Windows version, part of the `tui` component
/// (`scripts/release-components.json`).
/// Windows 版本中的 PATH 转发器，属于 `tui` 组件（`scripts/release-components.json`）。
#[cfg(windows)]
const FORWARDER: &str = "bin/vulcan-tui-forwarder.exe";

/// The desktop launcher inside a Windows version, part of the `gui` component
/// (`scripts/release-components.json`).
/// Windows 版本中的桌面启动器，属于 `gui` 组件（`scripts/release-components.json`）。
#[cfg(windows)]
const DESKTOP_LAUNCHER: &str = "bin/vulcan-code-launcher.exe";

/// The desktop launcher at the top of a Windows root, where the Inno installer puts it too;
/// `vulcan-code-launcher` finds the root from it.
/// Windows 根目录顶层的桌面启动器，Inno 安装程序也放在此处；`vulcan-code-launcher` 据此找到根目录。
pub const ROOT_LAUNCHER: &str = "VulcanCode.exe";

/// File name of the PATH forwarder in [`CLI_DIR`] on Windows.
/// Windows 上 [`CLI_DIR`] 中 PATH 转发器的文件名。
pub const TUI_FORWARDER: &str = "vulcan-tui.exe";

/// Command of the terminal client on PATH.
/// PATH 上终端客户端的命令。
pub const TUI_COMMAND: &str = "vulcan-tui";

/// Command of the desktop app on PATH (Linux and macOS).
/// PATH 上桌面应用的命令（Linux 与 macOS）。
pub const DESKTOP_COMMAND: &str = "vulcan-code";

/// Command of the setup program on PATH.
/// PATH 上安装器的命令。
pub const SETUP_COMMAND: &str = "vulcan-code-setup";

/// Directory of the PATH forwarder below a Windows root; `vulcan-code-launcher` finds the root
/// from it.
/// Windows 根目录下 PATH 转发器所在的目录；`vulcan-code-launcher` 据此找到根目录。
pub const CLI_DIR: &str = "cli";

/// Name of the per-user uninstall entry on Windows; distinct from the Inno installer's entry.
/// Windows 上每用户卸载项的名称；与 Inno 安装器的卸载项不同。
#[cfg(windows)]
const UNINSTALL_ENTRY: &str = "VulcanCode.Setup";

/// Name of the application bundle, menu entry and shortcuts.
/// 应用程序包、菜单项与快捷方式的名称。
const APP_NAME: &str = "Vulcan Code";

/// File name of the Windows shortcuts, the same as the Inno installer's.
/// Windows 快捷方式的文件名，与 Inno 安装程序相同。
const WINDOWS_SHORTCUT: &str = "Vulcan Code.lnk";

/// Linux application id of the desktop app. The packaged app declares `desktopName` as this id
/// plus `.desktop` (`scripts/desktop-runtime.mjs`), and Electron then takes the window class
/// (`WM_CLASS "vulcan-code", "vulcan-code"` on X11) and the Wayland app id from it. The menu entry
/// is named after it and names it as `StartupWMClass`, so docks group the running window with the
/// entry and a pinned icon starts the entry's command.
/// 桌面应用的 Linux 应用 ID。打包应用把 `desktopName` 声明为此 ID 加 `.desktop`
/// （`scripts/desktop-runtime.mjs`），Electron 由此得出窗口类（X11 上为 `WM_CLASS "vulcan-code",
/// "vulcan-code"`）与 Wayland 应用 ID。菜单项以它命名并把它写为 `StartupWMClass`，使程序坞把运行中的
/// 窗口与菜单项归为一组，固定的图标启动菜单项的命令。
macro_rules! desktop_app_id {
    () => {
        "vulcan-code"
    };
}

/// File name of the Linux menu entry and desktop shortcut.
/// Linux 菜单项与桌面快捷方式的文件名。
const DESKTOP_ENTRY_FILE: &str = concat!(desktop_app_id!(), ".desktop");

/// Window class of the desktop app.
/// 桌面应用的窗口类。
const DESKTOP_WINDOW_CLASS: &str = desktop_app_id!();

/// Icon inside a version, part of the `core` component.
/// 版本中的图标，属于 `core` 组件。
const ICON: &str = "webui/vulcan-logo.png";

/// Service flag that registers the user service.
/// 注册用户服务的服务参数。
const INSTALL_USER_SERVICE: &str = "--install-user-service";

/// Service flag that removes the user service.
/// 移除用户服务的服务参数。
const UNINSTALL_USER_SERVICE: &str = "--uninstall-user-service";

/// Service flag that sets the port range in the service settings.
/// 在服务设置中设置端口范围的服务参数。
const SET_PORT_RANGE: &str = "--set-port-range";

/// Directory in which systemd keeps one file per lingering user.
/// systemd 为每个驻留用户保存一个文件的目录。
#[cfg(unix)]
const LINGER_DIR: &str = "/var/lib/systemd/linger";

/// What the integration is built from.
/// 构建集成所依据的内容。
pub struct Plan<'a> {
    /// The platform.
    /// 平台。
    pub platform: Platform,
    /// The install root.
    /// 安装根目录。
    pub root: &'a Path,
    /// The installed version, shown in the uninstall entry.
    /// 已安装的版本，显示在卸载项中。
    #[cfg_attr(not(windows), allow(dead_code))]
    pub version: &'a str,
    /// Whether the desktop app is installed.
    /// 是否安装桌面应用。
    pub gui: bool,
    /// Whether a menu entry or application bundle is wanted for the desktop app.
    /// 是否需要桌面应用的菜单项或应用程序包。
    pub menu: bool,
    /// Whether a desktop shortcut is wanted for the desktop app.
    /// 是否需要桌面应用的桌面快捷方式。
    pub desktop_shortcut: bool,
    /// Whether commands are wanted on PATH.
    /// 是否需要把命令加入 PATH。
    pub path: bool,
    /// Whether the Core service starts with the login session.
    /// Core 服务是否随登录会话启动。
    pub service: bool,
    /// Whether the service keeps running after logout (Linux).
    /// 服务是否在注销后继续运行（Linux）。
    pub linger: bool,
    /// The port range to set, when one was given.
    /// 要设置的端口范围（给出时）。
    pub port_range: Option<&'a str>,
}

impl Plan<'_> {
    /// The command launchers of Linux and macOS: name and target relative to the root.
    /// Linux 与 macOS 的命令启动器：名称及相对根目录的目标。
    fn launchers(&self, layout: &Layout) -> Vec<(&'static str, String)> {
        let current = |entry: &str| format!("current/{entry}");
        let mut launchers = vec![(SETUP_COMMAND, format!("{SETUP_DIR}/{SETUP_PROGRAM}"))];
        launchers.push(("vulcan-code-service", current(&layout.service_executable)));
        if let Some(tui) = &layout.tui_executable {
            launchers.push((TUI_COMMAND, current(tui)));
        }
        if self.gui
            && let Some(desktop) = &layout.desktop_executable
        {
            launchers.push((DESKTOP_COMMAND, current(desktop)));
        }
        launchers
    }

    /// Every location outside the root that installing will write.
    /// 安装将在根目录之外写入的每个位置。
    pub fn locations(&self) -> Result<Vec<Location>, String> {
        let mut locations = Vec::new();
        if self.platform.is_windows() {
            if self.path {
                locations.push(Location::UserPath(self.root.join(CLI_DIR)));
                locations.push(Location::UserPath(self.root.join(SETUP_DIR)));
            }
            if self.gui && self.menu {
                locations.push(Location::MenuEntry(
                    paths::start_menu_dir()?.join(WINDOWS_SHORTCUT),
                ));
            }
            if self.gui
                && self.desktop_shortcut
                && let Some(desktop) = paths::desktop_dir()
            {
                locations.push(Location::DesktopShortcut(desktop.join(WINDOWS_SHORTCUT)));
            }
            locations.push(Location::UninstallEntry);
        } else {
            if self.path {
                locations.push(Location::Commands(paths::bin_dir()?));
            }
            if self.gui && self.menu {
                locations.push(if self.platform.is_macos() {
                    Location::AppBundle(app_bundle()?)
                } else {
                    Location::MenuEntry(desktop_entry()?)
                });
            }
            if self.gui
                && self.desktop_shortcut
                && !self.platform.is_macos()
                && let Some(desktop) = paths::desktop_dir()
            {
                locations.push(Location::DesktopShortcut(desktop.join(DESKTOP_ENTRY_FILE)));
            }
            if self.gui && !self.platform.is_macos() && apparmor::userns_restricted() {
                locations.push(Location::AppArmorProfile(PathBuf::from(
                    apparmor::PROFILE_PATH,
                )));
            }
        }
        if self.service {
            locations.push(Location::UserService);
        }
        if self.linger {
            locations.push(Location::Linger);
        }
        Ok(locations)
    }
}

/// A location outside the root that installing writes.
/// 安装在根目录之外写入的一个位置。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Location {
    /// Command launchers in this directory.
    /// 此目录中的命令启动器。
    Commands(PathBuf),
    /// The Linux menu entry or the Windows Start menu shortcut.
    /// Linux 菜单项或 Windows 开始菜单快捷方式。
    MenuEntry(PathBuf),
    /// The desktop shortcut.
    /// 桌面快捷方式。
    DesktopShortcut(PathBuf),
    /// The macOS application bundle.
    /// macOS 应用程序包。
    AppBundle(PathBuf),
    /// This directory on the Windows user PATH.
    /// Windows 用户 PATH 中的此目录。
    UserPath(PathBuf),
    /// The Windows uninstall entry.
    /// Windows 卸载项。
    UninstallEntry,
    /// The user service of this platform.
    /// 本平台的用户服务。
    UserService,
    /// Lingering of the user (Linux).
    /// 用户驻留（Linux）。
    Linger,
    /// The AppArmor profile of the desktop app, written with administrator rights (Linux).
    /// 桌面应用的 AppArmor 配置，以管理员权限写入（Linux）。
    AppArmorProfile(PathBuf),
}

impl std::fmt::Display for Location {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Commands(dir) => write!(formatter, "commands in {}", dir.display()),
            Self::MenuEntry(path) | Self::AppBundle(path) => {
                write!(formatter, "{}", path.display())
            }
            Self::DesktopShortcut(path) => write!(formatter, "desktop shortcut {}", path.display()),
            Self::UserPath(dir) => write!(formatter, "user PATH: {}", dir.display()),
            Self::UninstallEntry => {
                formatter.write_str("an entry in Settings > Apps for uninstalling")
            }
            Self::UserService => formatter.write_str(
                "the service, started with your login session as your own user (no administrator \
                 rights)",
            ),
            Self::Linger => formatter.write_str("lingering for your user (loginctl enable-linger)"),
            Self::AppArmorProfile(path) => write!(
                formatter,
                "the AppArmor profile {} that lets the desktop app start (asks for administrator \
                 rights)",
                path.display()
            ),
        }
    }
}

/// Collects what one integration run creates, remembering what an earlier run created.
/// 收集一次集成创建的内容，并记住更早一次创建的内容。
pub struct Recorder<'a> {
    /// Items of the earlier run.
    /// 更早一次的条目。
    previous: &'a [Item],
    /// Items of this run, in creation order.
    /// 本次的条目，按创建顺序排列。
    pub items: Vec<Item>,
    /// Notes for the summary.
    /// 用于总结的提示。
    pub notes: Vec<String>,
}

impl<'a> Recorder<'a> {
    /// Starts a run after one that created `previous`.
    /// 在创建了 `previous` 的那次之后开始一次运行。
    pub fn new(previous: &'a [Item]) -> Self {
        Self {
            previous,
            items: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Records `item` once.
    /// 记录 `item`（只记录一次）。
    fn add(&mut self, item: Item) {
        if !self.items.contains(&item) {
            self.items.push(item);
        }
    }

    /// Creates `dir` and its parents, recording the directories this program created, now or in
    /// an earlier run.
    /// 创建 `dir` 及其父目录，并记录本程序（本次或更早一次）创建的目录。
    fn ensure_dir(&mut self, dir: &Path) -> Result<(), String> {
        let mut missing = Vec::new();
        for ancestor in dir.ancestors() {
            if ancestor.exists() {
                break;
            }
            missing.push(ancestor.to_path_buf());
        }
        std::fs::create_dir_all(dir)
            .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
        for created in missing.into_iter().rev() {
            self.add(Item::Directory { path: created });
        }
        let earlier = Item::Directory {
            path: dir.to_path_buf(),
        };
        if self.previous.contains(&earlier) {
            self.add(earlier);
        }
        Ok(())
    }

    /// Whether an earlier run created `item`.
    /// 更早一次运行是否创建了 `item`。
    fn had(&self, item: &Item) -> bool {
        self.previous.contains(item)
    }
}

/// Creates the integration of `plan` for `version`; `switched` tells whether the current version
/// changed in this run.
/// 为 `version` 创建 `plan` 的集成；`switched` 表示本次运行是否切换了当前版本。
pub fn integrate(
    plan: &Plan<'_>,
    version: &Version,
    switched: bool,
    recorder: &mut Recorder<'_>,
    reporter: &dyn Reporter,
) -> Result<(), String> {
    if plan.platform.is_windows() {
        integrate_windows(plan, version, recorder)?;
    } else {
        integrate_unix(plan, &version.layout, recorder, reporter)?;
    }
    // The ports are set before the service is registered, so a service started now uses them.
    // 端口先于服务注册设置，因此此时启动的服务即使用这些端口。
    if let Some(range) = plan.port_range {
        set_port_range(version, range, recorder)?;
    }
    // A run that switched versions and then failed leaves this marker, because its state still
    // lists the service of the earlier version.
    // 切换了版本后失败的运行留下此标记，因为其状态仍列出更早版本的服务。
    let pending = plan.root.join(SETUP_DIR).join(SERVICE_PENDING_FILE);
    if plan.service {
        if switched || pending.is_file() || !recorder.had(&Item::UserService) {
            reporter.stage(Stage::Service);
            let report = run_service(&version.service(), &[INSTALL_USER_SERVICE])?;
            recorder.notes.push(report);
        }
        recorder.add(Item::UserService);
    }
    // Registered now, or no longer wanted.
    // 已经注册，或不再需要。
    let _ = std::fs::remove_file(&pending);
    #[cfg(unix)]
    {
        if plan.linger {
            enable_linger(recorder)?;
        }
        if running_as_root() {
            recorder.notes.push(String::from(
                "Installed as root: the service and the agents it runs have root privileges.",
            ));
        }
    }
    Ok(())
}

/// Sets the port range through the service of `version`, which owns the service settings.
/// 通过 `version` 的服务设置端口范围；服务设置归该服务所有。
fn set_port_range(
    version: &Version,
    range: &str,
    recorder: &mut Recorder<'_>,
) -> Result<(), String> {
    if Description::read(&version.dir)?.settings_path.is_none() {
        return Err(format!(
            "the installed version in {} has no port settings; --port-range needs a newer version",
            version.dir.display()
        ));
    }
    let report = run_service(&version.service(), &[SET_PORT_RANGE, range])?;
    recorder.notes.push(report);
    Ok(())
}

/// The Windows integration: the PATH forwarder, the user PATH, the desktop launcher and its
/// shortcuts, and the uninstall entry.
/// Windows 集成：PATH 转发器、用户 PATH、桌面启动器及其快捷方式，以及卸载项。
#[cfg(windows)]
fn integrate_windows(
    plan: &Plan<'_>,
    version: &Version,
    recorder: &mut Recorder<'_>,
) -> Result<(), String> {
    let cli = plan.root.join(CLI_DIR);
    recorder.ensure_dir(&cli)?;
    let forwarder = cli.join(TUI_FORWARDER);
    // Windows has no current link; programs outside the versions are copied from the version.
    // Windows 没有 current 链接；版本之外的程序从版本目录复制。
    replace_running_file(&version.dir.join(FORWARDER), &forwarder)?;
    recorder.add(Item::File { path: forwarder });
    if plan.path {
        let mut added = false;
        for dir in [cli, plan.root.join(SETUP_DIR)] {
            added |= crate::windows::add_user_path(&dir)?;
            recorder.add(Item::UserPath { dir });
        }
        // A program cannot change the environment of the shell that started it; install.ps1
        // copies the new entries into its PowerShell session.
        // 程序无法修改启动它的 Shell 的环境；install.ps1 会把新条目复制到其 PowerShell 会话中。
        if added {
            recorder.notes.push(String::from(
                "vulcan-tui and vulcan-code-setup are on the user PATH; terminals opened from now on \
                 find them.",
            ));
        }
    }
    let launcher = plan.root.join(ROOT_LAUNCHER);
    if plan.gui {
        integrate_desktop_windows(plan, version, &launcher, recorder)?;
    }
    let setup = plan.root.join(SETUP_DIR).join(SETUP_PROGRAM);
    crate::windows::write_uninstall_entry(
        UNINSTALL_ENTRY,
        plan.version,
        plan.root,
        &format!("\"{}\" uninstall", setup.display()),
        plan.gui.then_some(launcher.as_path()),
    )?;
    recorder.add(Item::UninstallEntry {
        key: String::from(UNINSTALL_ENTRY),
    });
    Ok(())
}

/// The desktop launcher at the top of the root and its shortcuts, which carry the notification
/// identity of the version's layout as the Inno installer's shortcuts do.
/// 根目录顶层的桌面启动器及其快捷方式；快捷方式与 Inno 安装程序的一样携带版本布局中的通知身份。
#[cfg(windows)]
fn integrate_desktop_windows(
    plan: &Plan<'_>,
    version: &Version,
    launcher: &Path,
    recorder: &mut Recorder<'_>,
) -> Result<(), String> {
    replace_running_file(&version.dir.join(DESKTOP_LAUNCHER), launcher)?;
    recorder.add(Item::File {
        path: launcher.to_path_buf(),
    });
    let identity = version.layout.desktop_identity.as_ref();
    let mut shortcuts = Vec::new();
    if plan.menu {
        shortcuts.push(paths::start_menu_dir()?.join(WINDOWS_SHORTCUT));
    }
    if plan.desktop_shortcut {
        match paths::desktop_dir() {
            Some(desktop) => shortcuts.push(desktop.join(WINDOWS_SHORTCUT)),
            None => recorder.notes.push(String::from(
                "No desktop folder was found, so no desktop shortcut was created.",
            )),
        }
    }
    for path in shortcuts {
        let item = Item::Shortcut { path: path.clone() };
        if path.exists() && !recorder.had(&item) {
            recorder.notes.push(format!(
                "Skipped {}: a shortcut not created by the setup program already exists.",
                path.display()
            ));
            continue;
        }
        if let Some(parent) = path.parent() {
            recorder.ensure_dir(parent)?;
        }
        crate::windows::write_shortcut(
            &path,
            launcher,
            plan.root,
            identity.map(|identity| crate::windows::ShortcutIdentity {
                app_user_model_id: &identity.app_user_model_id,
                toast_activator_clsid: &identity.toast_activator_clsid,
            }),
        )?;
        recorder.add(item);
    }
    // The desktop app registers its notification activator and its own Start menu shortcut at its
    // first start; the Inno uninstaller removes both, and so does this program.
    // 桌面应用在首次启动时注册其通知激活器和自己的开始菜单快捷方式；Inno 卸载程序会移除两者，本程序同样如此。
    if let Some(identity) = identity {
        recorder.add(Item::Shortcut {
            path: paths::start_menu_dir()?.join(format!("{}.lnk", identity.name)),
        });
        recorder.add(Item::ToastActivator {
            clsid: identity.toast_activator_clsid.clone(),
        });
    }
    Ok(())
}

/// Windows integration is not built on other platforms.
/// 其他平台不构建 Windows 集成。
#[cfg(not(windows))]
fn integrate_windows(
    _plan: &Plan<'_>,
    _version: &Version,
    _recorder: &mut Recorder<'_>,
) -> Result<(), String> {
    Err(String::from("this setup program was not built for Windows"))
}

/// Where a running `target` is moved aside before it is replaced; removing `target` removes it too.
/// 正在运行的 `target` 被替换前移开到的位置；移除 `target` 时一并移除。
#[cfg(windows)]
fn aside_of(target: &Path) -> PathBuf {
    let mut aside = target.as_os_str().to_owned();
    aside.push(".old");
    PathBuf::from(aside)
}

/// Replaces `target` with a copy of `source`, moving a running `target` aside first.
/// 用 `source` 的副本替换 `target`；`target` 正在运行时先将其移开。
#[cfg(windows)]
fn replace_running_file(source: &Path, target: &Path) -> Result<(), String> {
    let fail = |error: std::io::Error| format!("cannot write {}: {error}", target.display());
    if !source.is_file() {
        return Err(format!("{} is missing", source.display()));
    }
    if target.exists() {
        let aside = aside_of(target);
        let _ = std::fs::remove_file(&aside);
        std::fs::rename(target, &aside).map_err(fail)?;
    }
    std::fs::copy(source, target).map_err(fail)?;
    Ok(())
}

/// The Linux and macOS integration: command launchers, and the menu entry, application bundle
/// or desktop shortcut of the desktop app.
/// Linux 与 macOS 集成：命令启动器，以及桌面应用的菜单项、应用程序包或桌面快捷方式。
fn integrate_unix(
    plan: &Plan<'_>,
    layout: &Layout,
    recorder: &mut Recorder<'_>,
    reporter: &dyn Reporter,
) -> Result<(), String> {
    if plan.path {
        let bin = paths::bin_dir()?;
        recorder.ensure_dir(&bin)?;
        for (name, target) in plan.launchers(layout) {
            let path = bin.join(name);
            let script = format!(
                "#!/bin/sh\n{LAUNCHER_MARKER}\nexec \"{}/{target}\" \"$@\"\n",
                plan.root.display()
            );
            if write_marked(&path, &script)? {
                recorder.add(Item::File { path });
            } else {
                recorder.notes.push(format!(
                    "Skipped {}: a file not created by the setup program already exists.",
                    path.display()
                ));
            }
        }
        let on_path = std::env::var_os("PATH")
            .is_some_and(|value| std::env::split_paths(&value).any(|dir| dir == bin));
        if !on_path {
            recorder.notes.push(format!(
                "Add {bin} to PATH to use the commands; for this shell: export PATH=\"{bin}:$PATH\"",
                bin = bin.display()
            ));
        }
    }
    let Some(desktop) = layout.desktop_executable.as_ref().filter(|_| plan.gui) else {
        return Ok(());
    };
    let program = plan.root.join("current").join(desktop);
    if plan.platform.is_macos() {
        if plan.menu {
            write_app_bundle(&program, recorder)?;
        }
        return Ok(());
    }
    let entry = format!(
        "[Desktop Entry]\n{LAUNCHER_MARKER}\nType=Application\nName={APP_NAME}\n\
         Comment=AI coding and work platform\nExec=\"{}\" %U\nIcon={}\nTerminal=false\n\
         StartupWMClass={DESKTOP_WINDOW_CLASS}\nCategories=Development;\n",
        program.display(),
        plan.root.join("current").join(ICON).display()
    );
    if plan.menu {
        let path = desktop_entry()?;
        if let Some(parent) = path.parent() {
            recorder.ensure_dir(parent)?;
        }
        if write_marked(&path, &entry)? {
            recorder.add(Item::File { path });
        }
    }
    if plan.desktop_shortcut {
        match paths::desktop_dir().filter(|dir| dir.is_dir()) {
            Some(dir) => {
                let path = dir.join(DESKTOP_ENTRY_FILE);
                if write_marked(&path, &entry)? {
                    // GNOME starts a desktop file only once it is trusted; other desktops ignore
                    // the attribute, and a missing `gio` only leaves the question to the user.
                    // GNOME 只启动受信任的桌面文件；其他桌面忽略该属性，缺少 `gio` 只会把确认留给用户。
                    let _ = Command::new("gio")
                        .arg("set")
                        .arg(&path)
                        .args(["metadata::trusted", "true"])
                        .output();
                    recorder.add(Item::File { path });
                } else {
                    recorder.notes.push(format!(
                        "Skipped {}: a file not created by the setup program already exists.",
                        path.display()
                    ));
                }
            }
            None => recorder.notes.push(String::from(
                "No desktop folder was found, so no desktop shortcut was created.",
            )),
        }
    }
    if apparmor::userns_restricted() {
        install_apparmor_profile(plan.root, desktop, recorder, reporter)?;
    }
    Ok(())
}

/// The AppArmor profile of `root` as an item, when it exists but `items` do not record it: the
/// in-app updater installs it for installations made before the setup program did.
/// `root` 的 AppArmor 配置作为记录项，前提是它存在而 `items` 未记录它：应用内更新器会为安装器尚未处理
/// 此事之前的安装装上它。
pub fn unrecorded_apparmor_profile(root: &Path, items: &[Item]) -> Option<Item> {
    let item = Item::AppArmorProfile {
        path: PathBuf::from(apparmor::PROFILE_PATH),
    };
    if items.contains(&item) {
        return None;
    }
    let root = std::fs::canonicalize(root).ok()?;
    let content = std::fs::read_to_string(apparmor::PROFILE_PATH).ok()?;
    apparmor::names_root(&content, &root).then_some(item)
}

/// Installs and loads the AppArmor profile with administrator rights, unless the same profile is
/// already there; a declined password leaves the command to run in the notes.
/// 以管理员权限安装并加载 AppArmor 配置，除非相同配置已存在；拒绝输入密码时把要运行的命令留在提示中。
fn install_apparmor_profile(
    root: &Path,
    desktop: &str,
    recorder: &mut Recorder<'_>,
    reporter: &dyn Reporter,
) -> Result<(), String> {
    let path = Path::new(apparmor::PROFILE_PATH);
    let item = Item::AppArmorProfile {
        path: path.to_path_buf(),
    };
    let resolved = std::fs::canonicalize(root)
        .map_err(|error| format!("cannot resolve {}: {error}", root.display()))?;
    let Some(profile) = apparmor::profile(&resolved, desktop) else {
        recorder.notes.push(format!(
            "The install path {} has characters an AppArmor profile cannot name, so the desktop \
             app may not start from the dock or the app menu on this system.",
            resolved.display()
        ));
        return Ok(());
    };
    if std::fs::read_to_string(path).is_ok_and(|existing| existing == profile) {
        recorder.add(item);
        return Ok(());
    }
    if path.exists() && !carries_marker(path) {
        recorder.notes.push(format!(
            "Skipped {}: a file not created by the setup program already exists.",
            path.display()
        ));
        return Ok(());
    }
    // The profile is prepared where this user can write and copied into place as root.
    // 配置先在本用户可写的位置准备，再以 root 复制到位。
    let staged = root.join(SETUP_DIR).join(apparmor::STAGED_PROFILE);
    write_file(&staged, &profile, false)?;
    let commands = apparmor::install_commands(&staged);
    let request = Privileged {
        purpose: Purpose::InstallAppArmorProfile,
        commands,
    };
    if reporter.privileged(&request) {
        let _ = std::fs::remove_file(&staged);
        recorder.add(item);
    } else {
        // An earlier profile of this program stays recorded, so uninstalling still removes it.
        // 本程序更早的配置保持记录，使卸载仍会移除它。
        if recorder.had(&item) && path.exists() {
            recorder.add(item);
        }
        recorder.notes.push(format!(
            "The desktop app needs an AppArmor profile to start from the dock or the app menu on \
             this system; to install it, run: {}",
            apparmor::shell_line(&request.commands, Some("sudo"))
        ));
    }
    Ok(())
}

/// Runs `commands` in order with administrator rights: directly as root, through `sudo` otherwise,
/// in the terminal of this program so that `sudo` can ask for the password.
/// 以管理员权限按顺序运行 `commands`：root 下直接运行，否则经 `sudo` 运行；使用本程序的终端，使 `sudo`
/// 可以询问密码。
pub fn run_privileged(commands: &[Vec<String>]) -> bool {
    let as_root = running_as_root();
    commands.iter().all(|command| {
        let Some((program, args)) = command.split_first() else {
            return true;
        };
        let mut process = if as_root {
            Command::new(program)
        } else {
            let mut sudo = Command::new("sudo");
            sudo.arg("--").arg(program);
            sudo
        };
        process
            .args(args)
            .status()
            .is_ok_and(|status| status.success())
    })
}

/// Path of the Linux menu entry.
/// Linux 菜单项的路径。
fn desktop_entry() -> Result<PathBuf, String> {
    Ok(paths::applications_dir()?.join(DESKTOP_ENTRY_FILE))
}

/// Path of the macOS application bundle.
/// macOS 应用程序包的路径。
fn app_bundle() -> Result<PathBuf, String> {
    Ok(paths::user_applications_dir()?.join(format!("{APP_NAME}.app")))
}

/// Executable of the application bundle, below `Contents/MacOS`.
/// 应用程序包的可执行文件，位于 `Contents/MacOS` 下。
const BUNDLE_EXECUTABLE: &str = "vulcan-code";

/// Icon file of the application bundle, below `Contents/Resources`.
/// 应用程序包的图标文件，位于 `Contents/Resources` 下。
const BUNDLE_ICON: &str = "AppIcon.icns";

/// The application icon of a macOS desktop program `.../Electron.app/Contents/MacOS/Electron`:
/// `scripts/desktop-runtime.mjs` copies it beside `gui/` in `Contents/Resources/app`, where the
/// desktop app reads it for the Dock (`app-icon.ts`).
/// macOS 桌面程序 `.../Electron.app/Contents/MacOS/Electron` 的应用图标：`scripts/desktop-runtime.mjs`
/// 将其复制到 `Contents/Resources/app` 中 `gui/` 旁，桌面应用从此处读取程序坞图标（`app-icon.ts`）。
fn desktop_icon(program: &Path) -> Option<PathBuf> {
    Some(program.parent()?.parent()?.join("Resources/app/icon.png"))
}

/// Writes the macOS application bundle that starts `program`.
/// 写入启动 `program` 的 macOS 应用程序包。
fn write_app_bundle(program: &Path, recorder: &mut Recorder<'_>) -> Result<(), String> {
    let bundle = app_bundle()?;
    let launcher = bundle.join("Contents/MacOS").join(BUNDLE_EXECUTABLE);
    if bundle.exists() && !carries_marker(&launcher) {
        recorder.notes.push(format!(
            "Skipped {}: an application not created by the setup program already exists.",
            bundle.display()
        ));
        return Ok(());
    }
    if let Some(parent) = bundle.parent() {
        recorder.ensure_dir(parent)?;
    }
    write_bundle_files(&bundle, program)?;
    // Finder and Spotlight cache bundle icons; a new modification time and a registration make
    // them read the new one. Both are best effort: the bundle works without them.
    // Finder 与 Spotlight 会缓存应用包图标；更新修改时间并重新注册使其读取新图标。两者都尽力而为：
    // 没有它们应用包照样可用。
    if let Ok(dir) = std::fs::File::open(&bundle) {
        let _ = dir.set_modified(std::time::SystemTime::now());
    }
    let _ = Command::new(
        "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/\
         Support/lsregister",
    )
    .arg("-f")
    .arg(&bundle)
    .output();
    recorder.add(Item::Bundle { path: bundle });
    Ok(())
}

/// Writes the files of the application `bundle` that starts `program`: the launcher script, the
/// property list and, when `program` carries the application icon, the bundle icon.
/// 写入启动 `program` 的应用程序包 `bundle` 中的文件：启动脚本、属性列表，以及 `program` 带有应用图标
/// 时的应用包图标。
fn write_bundle_files(bundle: &Path, program: &Path) -> Result<(), String> {
    let contents = bundle.join("Contents");
    let macos = contents.join("MacOS");
    std::fs::create_dir_all(&macos)
        .map_err(|error| format!("cannot create {}: {error}", macos.display()))?;
    // A version from before the icon was packaged has none; the bundle then keeps the generic one.
    // 打包图标之前的版本没有图标；此时应用包保留通用图标。
    let icon = desktop_icon(program)
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|png| icns_from_png(&png));
    let icon_key = match icon {
        Some(icns) => {
            let resources = contents.join("Resources");
            std::fs::create_dir_all(&resources)
                .map_err(|error| format!("cannot create {}: {error}", resources.display()))?;
            write_file(&resources.join(BUNDLE_ICON), &icns, false)?;
            format!("<key>CFBundleIconFile</key><string>{BUNDLE_ICON}</string>\n")
        }
        None => String::new(),
    };
    let plist = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\"><dict>\n\
         <key>CFBundleExecutable</key><string>{BUNDLE_EXECUTABLE}</string>\n\
         <key>CFBundleIdentifier</key><string>com.openvulcan.vulcancode.launcher</string>\n\
         <key>CFBundleName</key><string>{APP_NAME}</string>\n\
         <key>CFBundlePackageType</key><string>APPL</string>\n\
         {icon_key}</dict></plist>\n"
    );
    write_file(&contents.join("Info.plist"), &plist, false)?;
    let script = format!(
        "#!/bin/sh\n{LAUNCHER_MARKER}\nexec \"{}\" \"$@\"\n",
        program.display()
    );
    write_file(&macos.join(BUNDLE_EXECUTABLE), &script, true)
}

/// Wraps a square PNG in an Apple icon image (`.icns`), whose entries may hold PNG data as is;
/// `None` when `png` is not a PNG of a size an icon entry takes.
/// 将正方形 PNG 包装为 Apple 图标文件（`.icns`），其条目可以直接存放 PNG 数据；`png` 不是 PNG 或其尺寸
/// 没有对应的图标条目时为 `None`。
fn icns_from_png(png: &[u8]) -> Option<Vec<u8>> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if png.len() < 24 || !png.starts_with(SIGNATURE) || &png[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(png[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(png[20..24].try_into().ok()?);
    // Each pixel size serves its point size, and the point size of half of it on Retina screens.
    // 每种像素尺寸用于同等点数尺寸，以及 Retina 屏幕上一半的点数尺寸。
    let types: &[&[u8; 4]] = match (width, height) {
        (16, 16) => &[b"icp4"],
        (32, 32) => &[b"icp5", b"ic11"],
        (64, 64) => &[b"icp6", b"ic12"],
        (128, 128) => &[b"ic07"],
        (256, 256) => &[b"ic08", b"ic13"],
        (512, 512) => &[b"ic09", b"ic14"],
        (1024, 1024) => &[b"ic10"],
        _ => return None,
    };
    let entry = u32::try_from(png.len() + 8).ok()?;
    let total = u32::try_from(8 + types.len() * (png.len() + 8)).ok()?;
    let mut icns = Vec::with_capacity(total as usize);
    icns.extend_from_slice(b"icns");
    icns.extend_from_slice(&total.to_be_bytes());
    for kind in types {
        icns.extend_from_slice(*kind);
        icns.extend_from_slice(&entry.to_be_bytes());
        icns.extend_from_slice(png);
    }
    Some(icns)
}

/// Writes `content` to `path` unless a file without the setup marker is there; returns whether
/// it was written.
/// 将 `content` 写入 `path`，除非那里已有不带安装器标记的文件；返回是否已写入。
fn write_marked(path: &Path, content: &str) -> Result<bool, String> {
    if path.exists() && !carries_marker(path) {
        return Ok(false);
    }
    write_file(path, content, true)?;
    Ok(true)
}

/// Whether the file at `path` carries the setup marker.
/// `path` 处的文件是否带有安装器标记。
fn carries_marker(path: &Path) -> bool {
    std::fs::read(path).is_ok_and(|bytes| contains(&bytes, LAUNCHER_MARKER.as_bytes()))
}

/// Whether the file at `path` carries the setup marker and names a path inside `root`. Launchers,
/// menu entries and bundles sit at fixed places shared by every prefix, so one written for another
/// root belongs to that installation.
/// `path` 处的文件是否带有安装器标记并指向 `root` 内的路径。启动器、菜单项与应用程序包位于各前缀共享的
/// 固定位置，因此为其他根目录写入的文件属于那个安装。
fn marked_for(path: &Path, root: &Path) -> bool {
    let mut prefix = root.as_os_str().to_string_lossy().into_owned();
    prefix.push(std::path::MAIN_SEPARATOR);
    std::fs::read(path).is_ok_and(|bytes| {
        contains(&bytes, LAUNCHER_MARKER.as_bytes()) && contains(&bytes, prefix.as_bytes())
    })
}

/// Whether `bytes` contains `needle`.
/// `bytes` 是否包含 `needle`。
fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|window| window == needle)
}

/// Replaces `path` atomically with `content`, executable when asked.
/// 以 `content` 原子替换 `path`，按需设为可执行。
fn write_file(path: &Path, content: impl AsRef<[u8]>, executable: bool) -> Result<(), String> {
    let fail = |error: std::io::Error| format!("cannot write {}: {error}", path.display());
    let mut partial = path.as_os_str().to_owned();
    partial.push(".partial");
    std::fs::write(&partial, content).map_err(fail)?;
    #[cfg(unix)]
    if executable {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o755)).map_err(fail)?;
    }
    #[cfg(not(unix))]
    let _ = executable;
    std::fs::rename(&partial, path).map_err(fail)
}

/// Runs the service `program` with `args` and returns what it reported.
///
/// The output goes to files, not pipes: on Windows the service that `--install-user-service`
/// starts inherits the output handles of its parent, so a pipe would stay open until that service
/// exits and reading it to the end would never finish. For the same reason the service program
/// receives no other handle of this program on Windows: a shell such as Windows PowerShell hands
/// this program extra inheritable copies of its own output, and a pipe the caller reads it
/// through (`| Tee-Object`, a CI log) would otherwise stay open in the service.
/// 以 `args` 运行服务程序 `program`，并返回其报告的内容。
///
/// 输出写入文件而不是管道：Windows 上 `--install-user-service` 启动的服务会继承其父进程的输出句柄，
/// 管道因此在该服务退出前一直保持打开，读到末尾永远无法完成。出于同样原因，Windows 上服务程序不
/// 得到本程序的任何其他句柄：Windows PowerShell 等 Shell 会把其输出的额外可继承副本交给本程序，
/// 否则调用方读取输出所用的管道（`| Tee-Object`、CI 日志）会一直被服务保持打开。
fn run_service(program: &Path, args: &[&str]) -> Result<String, String> {
    let stdout = Capture::create("out")?;
    let stderr = Capture::create("err")?;
    #[cfg(windows)]
    let status = crate::windows::run_with_only(program, args, &stdout.file, &stderr.file)
        .map(|code| (code == 0, format!("exit code {code}")));
    #[cfg(not(windows))]
    let status = Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(stdout.handle()?)
        .stderr(stderr.handle()?)
        .status()
        .map(|status| (status.success(), status.to_string()))
        .map_err(|error| error.to_string());
    let (success, status) =
        status.map_err(|error| format!("cannot run {}: {error}", program.display()))?;
    if success {
        Ok(stdout.text())
    } else {
        Err(format!(
            "{} {} failed ({status}): {}",
            program.display(),
            args.join(" "),
            stderr.text()
        ))
    }
}

/// A temporary file that receives one output stream of a service run; deleted when dropped.
/// 接收一次服务运行的一路输出的临时文件；释放时删除。
struct Capture {
    /// Path of the file.
    /// 文件路径。
    path: PathBuf,
    /// The open file, handed to the service run.
    /// 已打开的文件，交给服务运行使用。
    file: std::fs::File,
}

impl Capture {
    /// Creates a fresh file for the stream `name` in the temporary directory.
    /// 在临时目录中为输出流 `name` 新建文件。
    fn create(name: &str) -> Result<Self, String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let path = std::env::temp_dir().join(format!(
            "vulcan-code-setup-{}-{stamp}.{name}",
            std::process::id()
        ));
        let file = std::fs::File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
        Ok(Self { path, file })
    }

    /// A handle to the file for the child's standard stream.
    /// 供子进程标准流使用的文件句柄。
    #[cfg(not(windows))]
    fn handle(&self) -> Result<std::fs::File, String> {
        self.file
            .try_clone()
            .map_err(|error| format!("cannot use {}: {error}", self.path.display()))
    }

    /// What the run wrote, trimmed.
    /// 本次运行写入的内容，已去除首尾空白。
    fn text(&self) -> String {
        std::fs::read(&self.path)
            .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
            .unwrap_or_default()
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Keeps the user's service running after logout. Lingering that was on before is left alone and
/// not recorded, so removing the installation never turns off what the user turned on.
/// 使用户的服务在注销后继续运行。此前已开启的驻留保持不变且不记录，因此移除安装绝不会关闭用户自己开启的驻留。
#[cfg(unix)]
fn enable_linger(recorder: &mut Recorder<'_>) -> Result<(), String> {
    let user = unix::user_name()?;
    let item = Item::Linger { user: user.clone() };
    if Path::new(LINGER_DIR).join(&user).exists() {
        if recorder.had(&item) {
            recorder.add(item);
        }
        return Ok(());
    }
    let enabled = Command::new("loginctl")
        .args(["enable-linger", &user])
        .output()
        .is_ok_and(|output| output.status.success());
    if enabled {
        recorder.add(item);
    } else {
        recorder.notes.push(format!(
            "Lingering could not be turned on without administrator rights; to keep the service \
             running after you log out, run: sudo loginctl enable-linger {user}"
        ));
    }
    Ok(())
}

/// Queries about the user running the setup program on Linux and macOS.
/// Linux 与 macOS 上关于运行安装器之用户的查询。
#[cfg(unix)]
mod unix {
    use std::process::Command;

    /// The output of `id` with `flag`.
    /// 以 `flag` 运行 `id` 的输出。
    fn id(flag: &str) -> Option<String> {
        let output = Command::new("id").arg(flag).output().ok()?;
        let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (output.status.success() && !text.is_empty()).then_some(text)
    }

    /// The name of the user.
    /// 用户名。
    pub(super) fn user_name() -> Result<String, String> {
        id("-un").ok_or_else(|| String::from("cannot read the user name with `id -un`"))
    }

    /// Whether the user is root.
    /// 用户是否为 root。
    pub(super) fn is_root() -> bool {
        id("-u").as_deref() == Some("0")
    }
}

/// Whether the setup program runs as root, whose service and agents then have root privileges.
/// 安装器是否以 root 运行；此时其服务与代理拥有 root 权限。
pub fn running_as_root() -> bool {
    #[cfg(unix)]
    return unix::is_root();
    #[cfg(not(unix))]
    return false;
}

/// How to unregister the user service by hand, when the current version is missing; Windows
/// removes its sign-in value directly instead.
/// 当前版本缺失时手动注销用户服务的方法；Windows 改为直接移除其登录值。
#[cfg(not(windows))]
fn unregister_by_hand() -> &'static str {
    if cfg!(target_os = "macos") {
        "launchctl bootout gui/$(id -u)/com.openvulcan.vulcancode.service, then delete \
         ~/Library/LaunchAgents/com.openvulcan.vulcancode.service.plist"
    } else {
        "systemctl --user disable --now vulcan-code.service, then delete \
         ~/.config/systemd/user/vulcan-code.service"
    }
}

/// Removes one recorded item; `version` is the current version, when it still exists, and
/// `reporter` runs what needs administrator rights.
/// 移除一个已记录的条目；`version` 为当前版本（若仍存在），`reporter` 运行需要管理员权限的内容。
pub fn remove(
    item: &Item,
    root: &Path,
    version: Option<&Version>,
    reporter: &dyn Reporter,
) -> Result<(), String> {
    let fail =
        |path: &Path, error: std::io::Error| format!("cannot remove {}: {error}", path.display());
    let remove_file = |path: &Path| match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(fail(path, error)),
    };
    match item {
        Item::File { path } => {
            if path.starts_with(root) || marked_for(path, root) {
                remove_file(path)?;
                #[cfg(windows)]
                remove_file(&aside_of(path))?;
            }
            Ok(())
        }
        Item::Shortcut { path } => remove_file(path),
        Item::Bundle { path } => {
            if marked_for(&path.join("Contents/MacOS").join(BUNDLE_EXECUTABLE), root) {
                std::fs::remove_dir_all(path).map_err(|error| fail(path, error))?;
            }
            Ok(())
        }
        Item::Directory { path } => {
            // Only an empty directory is removed; anything left in it belongs to the user.
            // 只移除空目录；其中剩下的内容属于用户。
            let _ = std::fs::remove_dir(path);
            Ok(())
        }
        Item::UserPath { dir } => remove_user_path(dir),
        Item::UninstallEntry { key } => remove_uninstall_entry(key),
        Item::ToastActivator { clsid } => remove_toast_activator(clsid),
        Item::UserService => match version {
            Some(version) => run_service(&version.service(), &[UNINSTALL_USER_SERVICE]).map(|_| ()),
            // Without a version to ask, the sign-in value that starts a program of this root is
            // removed directly.
            // 没有可调用的版本时，直接移除启动本根目录程序的登录值。
            #[cfg(windows)]
            None => crate::windows::remove_login_run(
                &root.join(vulcan_install_core::install_root::VERSIONS_DIR),
            ),
            #[cfg(not(windows))]
            None => Err(format!(
                "the current version is missing, so the user service cannot be unregistered; \
                 remove it by hand: {}",
                unregister_by_hand()
            )),
        },
        Item::Linger { user } => {
            let disabled = Command::new("loginctl")
                .args(["disable-linger", user])
                .output()
                .is_ok_and(|output| output.status.success());
            if disabled {
                Ok(())
            } else {
                Err(format!(
                    "lingering of {user} could not be turned off; run: sudo loginctl \
                     disable-linger {user}"
                ))
            }
        }
        Item::AppArmorProfile { path } => {
            // A profile that no longer carries the marker belongs to the person who replaced it.
            // 不再带有标记的配置属于替换它的人。
            if !carries_marker(path) {
                return Ok(());
            }
            let path = path.display().to_string();
            let request = Privileged {
                purpose: Purpose::RemoveAppArmorProfile,
                commands: apparmor::remove_commands(),
            };
            if reporter.privileged(&request) {
                Ok(())
            } else {
                Err(format!(
                    "the AppArmor profile {path} could not be removed; run: {}",
                    apparmor::shell_line(&request.commands, Some("sudo"))
                ))
            }
        }
    }
}

/// Removes `dir` from the user PATH.
/// 从用户 PATH 中移除 `dir`。
#[cfg(windows)]
fn remove_user_path(dir: &Path) -> Result<(), String> {
    crate::windows::remove_user_path(dir)
}

/// The user PATH is only changed on Windows.
/// 只有 Windows 会修改用户 PATH。
#[cfg(not(windows))]
fn remove_user_path(_dir: &Path) -> Result<(), String> {
    Ok(())
}

/// Removes the uninstall entry `key`.
/// 移除卸载项 `key`。
#[cfg(windows)]
fn remove_uninstall_entry(key: &str) -> Result<(), String> {
    crate::windows::delete_uninstall_entry(key)
}

/// Uninstall entries only exist on Windows.
/// 只有 Windows 有卸载项。
#[cfg(not(windows))]
fn remove_uninstall_entry(_key: &str) -> Result<(), String> {
    Ok(())
}

/// Removes the COM registration of the notification activator `clsid`.
/// 移除通知激活器 `clsid` 的 COM 注册。
#[cfg(windows)]
fn remove_toast_activator(clsid: &str) -> Result<(), String> {
    crate::windows::delete_toast_activator(clsid)
}

/// Notification activators only exist on Windows.
/// 只有 Windows 有通知激活器。
#[cfg(not(windows))]
fn remove_toast_activator(_clsid: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Plain;

    fn platform(name: &str) -> Platform {
        Platform::ALL
            .into_iter()
            .find(|platform| platform.as_str() == name)
            .expect("platform")
    }

    /// A PNG header of `size` pixels square, followed by a few body bytes.
    fn png(size: u32) -> Vec<u8> {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        png.extend_from_slice(&size.to_be_bytes());
        png.extend_from_slice(&size.to_be_bytes());
        png.extend_from_slice(b"body");
        png
    }

    #[test]
    fn icns_wraps_the_png_in_entries_of_its_size() {
        let image = png(512);
        let icns = icns_from_png(&image).expect("icns");
        assert_eq!(&icns[..4], b"icns");
        assert_eq!(
            u32::from_be_bytes(icns[4..8].try_into().expect("length")) as usize,
            icns.len()
        );
        let mut entries = Vec::new();
        let mut offset = 8;
        while offset < icns.len() {
            let length = u32::from_be_bytes(icns[offset + 4..offset + 8].try_into().expect("entry"))
                as usize;
            assert_eq!(&icns[offset + 8..offset + length], image.as_slice());
            entries.push(icns[offset..offset + 4].to_vec());
            offset += length;
        }
        assert_eq!(offset, icns.len());
        assert_eq!(entries, [b"ic09".to_vec(), b"ic14".to_vec()]);
        assert_eq!(icns_from_png(&png(500)), None);
        assert_eq!(icns_from_png(b"not a png at all, just text"), None);
    }

    #[test]
    fn the_bundle_carries_the_icon_of_its_program() {
        let temp = tempfile::tempdir().expect("temp dir");
        let app = temp.path().join("Electron.app/Contents");
        let program = app.join("MacOS/Electron");
        let bundle = temp.path().join("Vulcan Code.app");
        let plist = bundle.join("Contents/Info.plist");
        let icon = bundle.join("Contents/Resources").join(BUNDLE_ICON);

        write_bundle_files(&bundle, &program).expect("bundle without icon");
        assert!(carries_marker(
            &bundle.join("Contents/MacOS").join(BUNDLE_EXECUTABLE)
        ));
        assert!(!icon.exists());
        assert!(
            !std::fs::read_to_string(&plist)
                .expect("plist")
                .contains("CFBundleIconFile")
        );

        std::fs::create_dir_all(app.join("Resources/app")).expect("app dir");
        std::fs::write(app.join("Resources/app/icon.png"), png(512)).expect("icon");
        write_bundle_files(&bundle, &program).expect("bundle with icon");
        assert_eq!(
            std::fs::read(&icon).expect("icns"),
            icns_from_png(&png(512)).expect("icns")
        );
        assert!(
            std::fs::read_to_string(&plist)
                .expect("plist")
                .contains(&format!("<string>{BUNDLE_ICON}</string>"))
        );
    }

    #[test]
    fn foreign_files_are_neither_overwritten_nor_removed() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path().join("root");
        let foreign = temp.path().join("vulcan-tui");
        std::fs::write(&foreign, "#!/bin/sh\necho mine\n").expect("foreign");
        assert_eq!(write_marked(&foreign, "replaced"), Ok(false));
        remove(
            &Item::File {
                path: foreign.clone(),
            },
            &root,
            None,
            &Plain::new(true),
        )
        .expect("remove");
        assert!(foreign.exists());

        let ours = temp.path().join("vulcan-code");
        let launcher = |root: &Path| {
            format!(
                "#!/bin/sh\n{LAUNCHER_MARKER}\nexec \"{}\" \"$@\"\n",
                root.join("current").join("vulcan-code").display()
            )
        };
        // A launcher another prefix wrote at the shared place stays with that installation.
        // 其他前缀写在共享位置的启动器仍归那个安装所有。
        let other = temp.path().join("other");
        assert_eq!(write_marked(&ours, &launcher(&other)), Ok(true));
        remove(
            &Item::File { path: ours.clone() },
            &root,
            None,
            &Plain::new(true),
        )
        .expect("remove");
        assert!(ours.exists());

        assert_eq!(write_marked(&ours, &launcher(&root)), Ok(true));
        assert_eq!(write_marked(&ours, &launcher(&root)), Ok(true));
        remove(
            &Item::File { path: ours.clone() },
            &root,
            None,
            &Plain::new(true),
        )
        .expect("remove");
        assert!(!ours.exists());
    }

    #[test]
    fn directories_are_recorded_only_when_this_program_created_them() {
        let temp = tempfile::tempdir().expect("temp dir");
        let existing = temp.path().join("existing");
        std::fs::create_dir(&existing).expect("existing");
        let earlier = vec![Item::Directory {
            path: existing.clone(),
        }];
        let mut recorder = Recorder::new(&earlier);
        let nested = temp.path().join("a").join("b");
        recorder.ensure_dir(&nested).expect("nested");
        recorder.ensure_dir(&existing).expect("existing");
        assert_eq!(
            recorder.items,
            vec![
                Item::Directory {
                    path: temp.path().join("a")
                },
                Item::Directory { path: nested },
                Item::Directory { path: existing },
            ]
        );
    }

    #[test]
    fn locations_follow_the_choices_on_every_platform() {
        let root = PathBuf::from("/opt/vulcan");
        let plan = |name: &str, gui: bool| Plan {
            platform: platform(name),
            root: &root,
            version: "1.0.0",
            gui,
            menu: true,
            desktop_shortcut: false,
            path: true,
            service: true,
            linger: false,
            port_range: None,
        };
        let windows = plan("windows-x64", true).locations().expect("windows");
        assert!(windows.contains(&Location::UserPath(root.join(SETUP_DIR))));
        assert!(
            windows
                .iter()
                .any(|location| matches!(location, Location::MenuEntry(_)))
        );
        assert!(windows.contains(&Location::UserService));
        let terminal = plan("linux-x64", false).locations().expect("linux");
        assert!(
            !terminal
                .iter()
                .any(|location| matches!(location, Location::MenuEntry(_)))
        );
        assert!(terminal.contains(&Location::UserService));
        let macos = plan("macos-arm64", true).locations().expect("macos");
        assert!(
            macos
                .iter()
                .any(|location| matches!(location, Location::AppBundle(_)))
        );
    }
}
