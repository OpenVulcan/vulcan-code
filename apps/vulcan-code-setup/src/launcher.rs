//! Dispatches the installed `vcode` entry using its own installation root.
//! 使用自身安装根目录分派已安装的 `vcode` 入口。
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(unix)]
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::Deserialize;
use vulcan_i18n::{SETUP, Translator, format_template};
use vulcan_install_core::install_root::InstallRoot;

use crate::root::{self, Current, Description, Version};

/// The private argument used by generated Unix entry scripts.
/// Unix 生成的入口脚本使用的私有参数。
pub const ENTRY_FLAG: &str = "--vcode-entry";

/// The graphical manager installed beside the static Unix console engine.
/// 安装在静态 Unix 终端引擎旁的图形管理器。
#[cfg(all(unix, not(target_os = "macos")))]
pub const GUI_MANAGER: &str = "manager";

/// Detects the installed entry, returning its exit code when used.
/// 检测已安装入口，命中时返回其退出码。
pub fn dispatch(arguments: &[String]) -> Option<i32> {
    let (root, args) = if let [marker, root, args @ ..] = arguments
        && marker == ENTRY_FLAG
    {
        (PathBuf::from(root), args)
    } else {
        #[cfg(windows)]
        {
            let executable = std::env::current_exe().ok()?;
            if executable.file_name()? != crate::integrate::ENTRY_EXE {
                return None;
            }
            (executable.parent()?.parent()?.to_path_buf(), arguments)
        }
        #[cfg(unix)]
        return None;
    };
    Some(match run(&root, args) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("vcode: {error}");
            1
        }
    })
}

/// Executes a public vcode action while staying bound to the entry's installation.
/// 执行公开的 vcode 操作，并始终绑定到入口所属安装。
fn run(root: &Path, args: &[String]) -> Result<i32, String> {
    match args {
        [arg] if arg == "gui" => {
            launch_desktop(root)?;
            Ok(0)
        }
        [arg] if arg == "tui" => {
            let version = installed_version(root)?;
            let executable = terminal_executable(&version)
                .ok_or("the terminal client is not installed; run `vcode manager`")?;
            child_status(Command::new(executable))
        }
        [arg] if matches!(arg.as_str(), "web" | "start") => {
            let url = web_url(root)?;
            println!("{url}");
            if arg == "web" {
                open_browser(&url)?;
            }
            Ok(0)
        }
        [arg] if arg == "stop" => {
            if crate::processes::stop_core(root)? {
                println!("Vulcan Code Core stopped.");
            } else {
                println!("Vulcan Code Core is already stopped.");
            }
            Ok(0)
        }
        [arg] if arg == "manager" => setup(root, &[String::from("--ui=tui")]),
        [arg, ui] if arg == "manager" && matches!(ui.as_str(), "gui" | "tui") => {
            setup(root, &[format!("--ui={ui}")])
        }
        [arg, ..]
            if matches!(arg.as_str(), "install" | "update" | "uninstall" | "status")
                && matches!(
                    crate::args::parse(args),
                    Ok(crate::args::Command::Install(_)
                        | crate::args::Command::Update(_)
                        | crate::args::Command::Uninstall(_)
                        | crate::args::Command::Status(_))
                ) =>
        {
            setup(root, args)
        }
        _ => {
            print_help();
            Ok(0)
        }
    }
}

/// Prints the available invocations and their behavior using the setup language catalog.
/// 从安装器语言目录输出可用指令及其行为。
fn print_help() {
    let mut language = Translator::new(&[&SETUP]);
    language.set_locale(vulcan_i18n::system_locale());
    let heading = language
        .text("setup.launcher.help.title")
        .unwrap_or_default();
    let version = format_template(
        language
            .text("setup.launcher.help.version")
            .unwrap_or_default(),
        &[("version", crate::selfupdate::SETUP_VERSION)],
    );
    println!("{heading}\n{version}\n");
    // Spellings are literal shell commands; only the descriptions are translated.
    // 指令是可直接输入的终端命令，只翻译说明。
    let commands = [
        ("vcode", "setup.launcher.help.help"),
        ("vcode gui", "setup.launcher.help.gui"),
        ("vcode tui", "setup.launcher.help.tui"),
        ("vcode web", "setup.launcher.help.web"),
        ("vcode start", "setup.launcher.help.start"),
        ("vcode stop", "setup.launcher.help.stop"),
        ("vcode manager [tui]", "setup.launcher.help.manager"),
        ("vcode manager gui", "setup.launcher.help.graphicalManager"),
        ("vcode install [options]", "setup.launcher.help.install"),
        ("vcode update", "setup.launcher.help.update"),
        ("vcode uninstall", "setup.launcher.help.uninstall"),
        ("vcode status", "setup.launcher.help.status"),
    ];
    for (command, key) in commands {
        let description = language.text(key).unwrap_or_default();
        println!("  {command:<26} {description}");
    }
}

/// Returns the terminal executable of `version` when the installed file exists.
/// 当安装文件存在时，返回 `version` 对应的终端可执行文件。
fn terminal_executable(version: &Version) -> Option<PathBuf> {
    let executable = version.dir.join(version.layout.tui_executable.as_ref()?);
    executable.is_file().then_some(executable)
}

/// Runs the installed engine, or its optional Unix GUI manager, with the bound root.
/// 使用绑定的根目录运行已安装引擎，或可选的 Unix 图形管理器。
fn setup(root: &Path, args: &[String]) -> Result<i32, String> {
    #[cfg(windows)]
    {
        let mut arguments = args.to_vec();
        if args != ["--version"] {
            arguments.extend([String::from("--prefix"), root.display().to_string()]);
        }
        // Reuse the running engine on Windows so uninstall can defer removal of its own image.
        // Windows 上复用当前引擎，使卸载能延迟移除自身映像。
        Ok(crate::run_engine(&arguments))
    }
    #[cfg(unix)]
    setup_unix(root, args)
}

/// Invokes the installed Unix console or graphical manager.
/// 调用已安装的 Unix 终端或图形管理器。
#[cfg(unix)]
fn setup_unix(root: &Path, args: &[String]) -> Result<i32, String> {
    let engine = InstallRoot::new(root).setup_program();
    // macOS can open the manager through its own engine, even if setup/manager was never installed.
    // 即使安装时没有写入 setup/manager，macOS 也能通过自身的安装引擎打开图形管理器。
    #[cfg(not(target_os = "macos"))]
    let engine = {
        let graphical = args.iter().any(|arg| arg == "--ui=gui")
            || args.windows(2).any(|pair| pair == ["--ui", "gui"]);
        if graphical {
            let manager = root.join(crate::state::SETUP_DIR).join(GUI_MANAGER);
            if !manager.is_file() {
                return Err(String::from(
                    "open the downloadable graphical manager to install the GUI entry",
                ));
            }
            manager
        } else {
            engine
        }
    };
    let mut command = Command::new(engine);
    command.args(args);
    if args != ["--version"] {
        command.arg("--prefix").arg(root);
    }
    child_status(command)
}

/// Waits for a foreground child and preserves its exit status.
/// 等待前台子进程并保留其退出状态。
fn child_status(mut child: Command) -> Result<i32, String> {
    child
        .status()
        .map(|status| status.code().unwrap_or(1))
        .map_err(|error| {
            format!(
                "cannot run {}: {error}",
                child.get_program().to_string_lossy()
            )
        })
}

/// Reads the validated active version of this installation.
/// 读取本安装中经过校验的当前版本。
fn installed_version(root_path: &Path) -> Result<Version, String> {
    let root = InstallRoot::new(root_path);
    match root::current(&root)? {
        Current::Version(version) => Version::read(&root.version_dir(&version)),
        Current::Legacy(version) => Version::read(
            &root
                .path()
                .join(vulcan_install_core::install_root::VERSIONS_DIR)
                .join(version),
        ),
        Current::None => Err(String::from(
            "Vulcan Code is not installed; run `vcode manager`",
        )),
    }
}

/// Starts the installed desktop app from its validated runtime layout.
/// 从已校验的运行布局启动安装后的桌面应用。
pub fn launch_desktop(root: &Path) -> Result<(), String> {
    #[cfg(windows)]
    return crate::inno::relaunch(root);
    #[cfg(unix)]
    {
        let version = installed_version(root)?;
        let relative = version
            .layout
            .desktop_executable
            .ok_or("the desktop component is not installed")?;
        let executable = version.dir.join(relative);
        #[cfg(target_os = "macos")]
        {
            let bundle = executable
                .ancestors()
                .find(|path| path.extension().is_some_and(|ext| ext == "app"))
                .ok_or("the installed desktop executable is not in an application bundle")?;
            return open_with("/usr/bin/open", bundle.as_os_str());
        }
        #[cfg(not(target_os = "macos"))]
        return Command::new(executable)
            .current_dir(root)
            .env_remove("VULCAN_RUNTIME_LAYOUT")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(drop)
            .map_err(|error| format!("cannot start Vulcan Code: {error}"));
    }
}

/// The public identity fields of Core's WebUI discovery and health responses.
/// Core WebUI 发现文件和健康响应中的公开标识字段。
#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct WebIdentity {
    /// The readiness state.
    /// 就绪状态。
    status: String,
    /// The loopback port assigned by Core.
    /// Core 分配的回环端口。
    port: u16,
    /// The running Core process identifier.
    /// 正在运行的 Core 进程号。
    pid: u32,
    /// The startup identity that distinguishes reused processes.
    /// 区分进程号复用的启动标识。
    started_at: String,
}

/// Finds and verifies the active WebUI, starting this installation's Core when needed.
/// 查找并校验活动 WebUI，必要时启动本安装的 Core。
fn web_url(root: &Path) -> Result<String, String> {
    let version = installed_version(root)?;
    let discovery = Description::read(&version.dir)?
        .data_root
        .join("webui-service.json");
    let http: ureq::Agent = ureq::Agent::config_builder()
        .max_redirects(0)
        .proxy(None)
        .timeout_global(Some(Duration::from_millis(800)))
        .build()
        .into();
    if let Some(url) = live_url(&http, &discovery) {
        return Ok(url);
    }
    start_core(&version, core_launch_mode(desktop_session()))?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if let Some(url) = live_url(&http, &discovery) {
            return Ok(url);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(String::from(
        "Core did not publish a verified WebUI endpoint",
    ))
}

/// Starts the installed Core in `mode`, independently of the terminal that opened the WebUI.
/// 以 `mode` 启动已安装 Core，使它独立于打开 WebUI 的终端。
fn start_core(version: &Version, mode: &str) -> Result<(), String> {
    let args = [mode, "--no-debug-console"];
    #[cfg(windows)]
    let started = crate::windows::start_background(&version.service(), &args, &version.dir);
    #[cfg(unix)]
    let started = {
        use std::os::unix::process::CommandExt;

        Command::new(version.service())
            .args(args)
            .current_dir(&version.dir)
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(drop)
    };
    started.map_err(|error| format!("cannot start Core: {error}"))
}

/// Selects Core's tray mode for an interactive desktop and its headless mode otherwise.
/// 交互桌面启动带托盘的 Core，其他环境启动无头 Core。
fn core_launch_mode(desktop: bool) -> &'static str {
    if desktop { "--service" } else { "--headless" }
}

/// Reports whether the current launch context can use a desktop tray.
/// 判断当前启动环境是否可以使用桌面托盘。
fn desktop_session() -> bool {
    #[cfg(windows)]
    return crate::windows::interactive_desktop();
    #[cfg(target_os = "macos")]
    return Command::new("/bin/launchctl")
        .arg("managername")
        .output()
        .is_ok_and(|output| output.status.success() && output.stdout.trim_ascii() == b"Aqua");
    #[cfg(target_os = "linux")]
    return ["DISPLAY", "WAYLAND_DISPLAY"]
        .into_iter()
        .any(|key| std::env::var_os(key).is_some_and(|display| !display.is_empty()));
}

/// Returns a loopback URL only when health confirms the same discovery identity.
/// 仅在健康检查确认同一发现标识时返回回环网址。
fn live_url(http: &ureq::Agent, discovery: &Path) -> Option<String> {
    let expected: WebIdentity = serde_json::from_slice(&std::fs::read(discovery).ok()?).ok()?;
    if expected.port == 0 || expected.pid == 0 || expected.status != "ready" {
        return None;
    }
    let url = format!("http://127.0.0.1:{}", expected.port);
    let actual: WebIdentity = serde_json::from_slice(
        &http
            .get(format!("{url}/health"))
            .call()
            .ok()?
            .body_mut()
            .with_config()
            .limit(65536)
            .read_to_vec()
            .ok()?,
    )
    .ok()?;
    (actual == expected).then_some(url)
}

/// Opens a verified WebUI URL in the current user's browser.
/// 在当前用户的浏览器中打开已校验的 WebUI 网址。
fn open_browser(url: &str) -> Result<(), String> {
    #[cfg(windows)]
    return crate::windows::open_browser(url);
    #[cfg(target_os = "macos")]
    return open_with("/usr/bin/open", url.as_ref());
    #[cfg(target_os = "linux")]
    return open_with("xdg-open", url.as_ref());
}

/// Invokes the platform's application opener with one exact argument.
/// 以单个精确参数调用平台应用打开器。
#[cfg(unix)]
fn open_with(opener: &str, target: &std::ffi::OsStr) -> Result<(), String> {
    let output = Command::new(opener)
        .arg(target)
        .env_remove("VULCAN_RUNTIME_LAYOUT")
        .output()
        .map_err(|error| format!("cannot run {opener}: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{opener} reported {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Verifies that a macOS installation with only the engine can dispatch the GUI request.
    /// 验证 macOS 安装仅有安装引擎时也能正确分派图形管理请求。
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_manager_gui_uses_the_installed_engine() {
        use std::os::unix::fs::PermissionsExt;

        // Use a synthetic engine so the test checks dispatch without opening a real window.
        // 使用合成引擎验证命令分派，无需打开真实窗口。
        let fixture = tempfile::tempdir().expect("isolated installation");
        let engine = InstallRoot::new(fixture.path()).setup_program();
        std::fs::create_dir_all(fixture.path().join(crate::state::SETUP_DIR)).expect("setup");
        std::fs::write(
            &engine,
            "#!/bin/sh\n[ \"$#\" = 3 ] && [ \"$1\" = --ui=gui ] && [ \"$2\" = --prefix ] && [ \"$3\" = \"$(dirname \"$(dirname \"$0\")\")\" ]\n",
        )
        .expect("write engine");
        std::fs::set_permissions(engine, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        assert_eq!(
            run(
                fixture.path(),
                &[String::from("manager"), String::from("gui")]
            ),
            Ok(0)
        );
    }

    /// Checks the mode passed to Core when opening the WebUI or starting it directly.
    /// 检查打开 WebUI 或直接启动时传给 Core 的运行模式。
    #[test]
    fn web_start_selects_tray_only_for_a_desktop_session() {
        assert_eq!(core_launch_mode(true), "--service");
        assert_eq!(core_launch_mode(false), "--headless");
    }

    /// Exercises discovery with an actual loopback health response, including mismatched identity.
    /// 使用真实回环健康响应验证发现流程，并覆盖标识不一致的情况。
    #[test]
    fn web_entry_accepts_only_verified_core_identity() {
        let fixture = tempfile::tempdir().expect("isolated discovery directory");
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("isolated health listener");
        let port = listener.local_addr().expect("assigned port").port();
        let path = fixture.path().join("discovery.json");
        let identity = |started| serde_json::json!({"status":"ready", "port":port, "pid":42, "startedAt":started});
        std::fs::write(&path, identity("current").to_string()).expect("write discovery");
        let responses = [identity("stale"), identity("current")];
        let responder = std::thread::spawn(move || {
            for body in responses {
                let (mut socket, _) = listener.accept().expect("health request");
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .expect("bounded request read");
                let mut request = [0u8; 2048];
                let _ = socket.read(&mut request).expect("request bytes");
                let body = body.to_string();
                write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .expect("health response");
            }
        });
        let http: ureq::Agent = ureq::Agent::config_builder()
            .proxy(None)
            .timeout_global(Some(Duration::from_secs(2)))
            .build()
            .into();
        assert_eq!(live_url(&http, &path), None);
        assert_eq!(
            live_url(&http, &path),
            Some(format!("http://127.0.0.1:{port}"))
        );
        responder.join().expect("health listener ended");
    }
}
