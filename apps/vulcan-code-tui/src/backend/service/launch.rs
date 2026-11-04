// Starts the installed Core when the TUI finds no running service, as the desktop GUI does.
// 与桌面 GUI 一样，在 TUI 找不到运行中的服务时启动已安装的 Core。
//
// The TUI of an installed version lives in `<version>/bin/` beside `<version>/runtime-layout.json`, whose
// `serviceExecutable` names the Core of the same version. A TUI without that manifest (a workspace build)
// only connects. The Core is started once per TUI process, detached so it outlives the TUI; a concurrent
// launcher that loses the data-root lock exits on its own, so two clients starting together still end with
// one Core.
// 已安装版本的 TUI 位于 `<版本>/bin/`，旁边是 `<版本>/runtime-layout.json`，其 `serviceExecutable` 指明同一版本的
// Core。没有该清单的 TUI（工作区构建）只连接。每个 TUI 进程只启动一次 Core，并以分离方式启动，使其在 TUI 退出后
// 继续运行；未取得数据根锁的并发启动方会自行退出，因此两个客户端同时启动也只留下一个 Core。

use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Runtime layout manifest written beside the version's directories.
/// 写在版本目录旁的运行布局清单。
const LAYOUT_MANIFEST: &str = "runtime-layout.json";

/// How long a started Core may stay unreachable before connection failures are shown again; it matches the
/// desktop GUI's readiness wait (`READY_TIMEOUT_MS` in `apps/vulcan-code-desktop/src/gui/service-launcher.ts`).
/// 已启动的 Core 可以保持不可连接多久，之后重新显示连接失败；与桌面 GUI 的就绪等待一致
/// （`apps/vulcan-code-desktop/src/gui/service-launcher.ts` 中的 `READY_TIMEOUT_MS`）。
const STARTUP_WAIT: Duration = Duration::from_secs(120);

/// One installed Core this TUI may start.
/// 本 TUI 可以启动的一个已安装 Core。
pub(super) struct Launcher {
    /// Version root holding the runtime layout manifest; the Core runs with it as working directory.
    /// 存放运行布局清单的版本根；Core 以它作为工作目录运行。
    root: PathBuf,
    /// Absolute path of the Core executable inside `root`.
    /// `root` 内 Core 可执行文件的绝对路径。
    executable: PathBuf,
    /// The started Core and when it started, until it is seen running or seen exiting.
    /// 已启动的 Core 及其启动时刻，直到确认其运行或退出为止。
    child: Option<(Child, Instant)>,
    /// Whether this TUI already started its one Core.
    /// 本 TUI 是否已启动过它唯一的一次 Core。
    started: bool,
}

impl Launcher {
    /// The Core of the installed version this TUI belongs to, if it is one.
    /// 本 TUI 所属已安装版本的 Core；不属于已安装版本时为 `None`。
    pub(super) fn installed() -> Option<Self> {
        let executable = std::env::current_exe().ok()?;
        Self::beside(&executable)
    }

    /// The Core named by the runtime layout manifest of the version holding `tui`.
    /// 包含 `tui` 的版本的运行布局清单所指明的 Core。
    fn beside(tui: &Path) -> Option<Self> {
        let root = tui.parent()?.parent()?.to_path_buf();
        let text = std::fs::read_to_string(root.join(LAYOUT_MANIFEST)).ok()?;
        let manifest: serde_json::Value = serde_json::from_str(&text).ok()?;
        let relative = Path::new(manifest.get("serviceExecutable")?.as_str()?);
        // The manifest may only name a file inside its own root.
        // 清单只能指明其根目录之内的文件。
        if !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        {
            return None;
        }
        // Rebuilt from its components so the path uses this platform's separators.
        // 按组成部分重建，使路径使用本平台的分隔符。
        let executable = root.join(relative.components().collect::<PathBuf>());
        executable.is_file().then(|| Self {
            root,
            executable,
            child: None,
            started: false,
        })
    }

    /// Starts the Core once; later calls do nothing and return `Ok(false)`.
    /// 只启动一次 Core；之后的调用不做任何事并返回 `Ok(false)`。
    pub(super) fn start(&mut self) -> Result<bool, String> {
        if self.started {
            return Ok(false);
        }
        self.started = true;
        let mut command = Command::new(&self.executable);
        command
            .arg(launch_mode())
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        detach(&mut command);
        let child = command.spawn().map_err(|error| error.to_string())?;
        self.child = Some((child, Instant::now()));
        Ok(true)
    }

    /// Reports once that the started Core exited before a client connected; `None` while it runs.
    /// 已启动的 Core 在客户端连上之前退出时报告一次；仍在运行时为 `None`。
    pub(super) fn exited(&mut self) -> Option<String> {
        let status = self.child.as_mut()?.0.try_wait().ok()??;
        self.child = None;
        Some(status.to_string())
    }

    /// Whether the started Core is still within its startup wait and has not exited.
    /// 已启动的 Core 是否仍在启动等待期内且未退出。
    pub(super) fn starting(&self) -> bool {
        self.child
            .as_ref()
            .is_some_and(|(_, started)| started.elapsed() < STARTUP_WAIT)
    }

    /// Forgets the started Core once a client connected; it keeps running on its own.
    /// 客户端连上后不再跟踪已启动的 Core；它自行继续运行。
    pub(super) fn connected(&mut self) {
        self.child = None;
    }
}

/// The launch mode for this session: a tray where a desktop session can show one, otherwise headless.
/// 本次会话的启动模式：能显示托盘的桌面会话用托盘，否则无头运行。
fn launch_mode() -> &'static str {
    let graphical = !cfg!(target_os = "linux")
        || std::env::var_os("DISPLAY").is_some()
        || std::env::var_os("WAYLAND_DISPLAY").is_some();
    if graphical { "--service" } else { "--headless" }
}

/// Detaches the Core from this console and process group so it outlives the TUI and its terminal.
/// 让 Core 脱离本控制台与进程组，使其在 TUI 及其终端退出后继续运行。
#[cfg(windows)]
fn detach(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

/// Detaches the Core from this console and process group so it outlives the TUI and its terminal.
/// 让 Core 脱离本控制台与进程组，使其在 TUI 及其终端退出后继续运行。
#[cfg(unix)]
fn detach(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lays out a version root with a TUI and the given manifest, returning the TUI path.
    /// 布置一个含 TUI 与给定清单的版本根，返回 TUI 路径。
    fn version(root: &Path, manifest: &str) -> PathBuf {
        std::fs::create_dir_all(root.join("bin")).expect("bin");
        std::fs::write(root.join("bin/service.exe"), b"").expect("service");
        std::fs::write(root.join(LAYOUT_MANIFEST), manifest).expect("manifest");
        root.join("bin/tui.exe")
    }

    #[test]
    fn finds_the_core_named_by_the_version_manifest() {
        let directory =
            std::env::temp_dir().join(format!("vulcan-tui-launch-{}", std::process::id()));
        let tui = version(&directory, r#"{"serviceExecutable":"bin/service.exe"}"#);
        let launcher = Launcher::beside(&tui).expect("installed core");
        assert_eq!(launcher.executable, directory.join("bin/service.exe"));
        for escaping in [
            r#"{"serviceExecutable":"../service.exe"}"#,
            r#"{"serviceExecutable":"bin/missing.exe"}"#,
            r#"{}"#,
        ] {
            version(&directory, escaping);
            assert!(Launcher::beside(&tui).is_none(), "accepted {escaping}");
        }
        std::fs::remove_dir_all(&directory).expect("cleanup");
    }

    #[test]
    fn a_workspace_build_without_a_manifest_only_connects() {
        let directory =
            std::env::temp_dir().join(format!("vulcan-tui-bare-{}", std::process::id()));
        std::fs::create_dir_all(directory.join("bin")).expect("bin");
        assert!(Launcher::beside(&directory.join("bin/tui.exe")).is_none());
        std::fs::remove_dir_all(&directory).expect("cleanup");
    }
}
