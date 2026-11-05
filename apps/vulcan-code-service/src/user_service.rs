//! Per-user service registration that keeps the headless Core running in the background.
//! 为当前用户注册服务，使无头 Core 在后台持续运行。
//!
//! Linux registers a systemd user unit, macOS a launchd agent, and Windows a value under the
//! per-user `Run` key. Every registration belongs to the current user and needs no elevated rights;
//! nothing registers a system-wide service.
//! Linux 注册 systemd 用户单元，macOS 注册 launchd 代理，Windows 在每用户 `Run` 键下写入一个值。每种注册
//! 都属于当前用户，不需要提升权限；不注册任何系统级服务。

#[cfg(any(target_os = "macos", test))]
mod launch_agent;
#[cfg(windows)]
mod login_run;

/// Registers the headless Core as a per-user service, then starts it and waits for readiness.
/// 将无头 Core 注册为当前用户服务，随后启动并等待就绪。
pub const ARG_INSTALL_USER_SERVICE: &str = "--install-user-service";

/// Stops the per-user service and removes its registration.
/// 停止当前用户服务并移除其注册。
pub const ARG_UNINSTALL_USER_SERVICE: &str = "--uninstall-user-service";

/// One explicit per-user service management action.
/// 一个显式的当前用户服务管理操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserServiceAction {
    /// Write the registration, enable it at login, and start the service now.
    /// 写入注册、设为登录时启动，并立即启动服务。
    Install,
    /// Stop the service, disable it, and delete the registration.
    /// 停止服务、取消登录启动并删除注册。
    Uninstall,
}

/// Maps one argument to its management action.
/// 将一个参数映射为对应的管理操作。
fn action_for_argument(arg: &str) -> Option<UserServiceAction> {
    match arg {
        ARG_INSTALL_USER_SERVICE => Some(UserServiceAction::Install),
        ARG_UNINSTALL_USER_SERVICE => Some(UserServiceAction::Uninstall),
        _ => None,
    }
}

/// Parses a service management action before any launch-mode parsing or business startup.
/// 在解析启动模式或启动任何业务之前解析服务管理操作。
///
/// # Parameters
/// 参数
/// `args` are the full process arguments including the executable name.
/// `args` 是包含可执行文件名的完整进程参数。
///
/// # Returns
/// 返回值
/// `Ok(None)` when no management flag is present, the action when it is the only argument, or an
/// error when it is combined with anything else.
/// 无管理参数时返回 `Ok(None)`；管理参数是唯一参数时返回该操作；与其他参数混用时返回错误。
pub fn user_service_action(args: &[String]) -> Result<Option<UserServiceAction>, String> {
    let tail = args.get(1..).unwrap_or_default();
    if !tail.iter().any(|arg| action_for_argument(arg).is_some()) {
        return Ok(None);
    }
    match tail {
        [only] => Ok(action_for_argument(only)),
        _ => Err(format!(
            "{ARG_INSTALL_USER_SERVICE} and {ARG_UNINSTALL_USER_SERVICE} must be the only argument"
        )),
    }
}

/// Runs one management action and returns the summary printed to the operator.
/// 执行一个管理操作，并返回打印给操作者的摘要。
pub fn run_user_service_action(action: UserServiceAction) -> Result<String, String> {
    #[cfg(target_os = "linux")]
    {
        match action {
            UserServiceAction::Install => systemd::install(),
            UserServiceAction::Uninstall => systemd::uninstall(),
        }
    }
    #[cfg(target_os = "macos")]
    {
        match action {
            UserServiceAction::Install => launch_agent::install(),
            UserServiceAction::Uninstall => launch_agent::uninstall(),
        }
    }
    #[cfg(windows)]
    {
        match action {
            UserServiceAction::Install => login_run::install(),
            UserServiceAction::Uninstall => login_run::uninstall(),
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = action;
        Err(String::from(
            "registering Vulcan Code as a user service is not supported on this system",
        ))
    }
}

/// Points an existing registration at this process when it belongs to the current version of the
/// same installation; Windows registrations name a version directory, so each update moves them.
/// Other platforms register the stable `current` path and have nothing to refresh.
/// 当本进程属于同一安装的当前版本时，让已有注册指向本进程；Windows 注册指向版本目录，因此每次更新都要
/// 移动它。其他平台注册稳定的 `current` 路径，无需刷新。
pub fn refresh_registration() {
    #[cfg(windows)]
    if let Err(error) = login_run::refresh() {
        tracing::warn!("[vulcan-code-service] could not refresh the login registration: {error}");
    }
}

/// Requires the executable to belong to a distribution whose runtime layout matches this build.
/// 要求可执行文件属于运行时布局与本构建配置一致的发行目录。
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn require_distribution() -> Result<(), String> {
    let (manifest, path) = crate::runtime_layout::read_runtime_layout_manifest_with_path()
        .map_err(|error| format!("only a built distribution can be registered: {error}"))?;
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    if manifest.profile != profile {
        return Err(format!(
            "runtime layout {} declares profile `{}` but this build is `{profile}`",
            path.display(),
            manifest.profile
        ));
    }
    Ok(())
}

/// Whether no other process owns the data root right now.
/// 当前是否没有其他进程拥有数据根。
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn data_root_is_free(data_root: &std::path::Path) -> Result<bool, String> {
    vulcan_service_host::instance::ServiceInstanceGuard::acquire(data_root)
        .map(|guard| guard.is_some())
        .map_err(|error| format!("failed to check the service lock: {error}"))
}

/// Fails when another process already owns the data root, so a started service would exit at once.
/// 若已有其他进程拥有数据根则失败，否则启动的服务会立即退出。
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn require_free_data_root(data_root: &std::path::Path) -> Result<(), String> {
    if data_root_is_free(data_root)? {
        Ok(())
    } else {
        Err(String::from(
            "another Vulcan Code service already owns this data directory; quit it (desktop tray, TUI-started Core, or a terminal service) and install again",
        ))
    }
}

/// The executable a registration runs: the path that follows updates where the platform has one.
/// 注册所运行的可执行文件：平台有随更新变化的路径时使用该路径。
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn registered_executable() -> Result<std::path::PathBuf, String> {
    let executable = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|error| format!("failed to resolve the service executable: {error}"))?;
    // An installed release runs from its version directory; Linux and macOS name the path that
    // follows updates.
    // 已安装的发行版从其版本目录运行；Linux 与 macOS 使用随更新变化的路径。
    Ok(vulcan_app_update::stable_executable(&executable).unwrap_or(executable))
}

/// Waits until the service whose process id `pid` reports (any process for `Ok(None)`) publishes a
/// ready discovery document; `pid` fails once the started service stopped.
/// 等待 `pid` 报告其进程号的服务（`Ok(None)` 时为任意进程）发布就绪的发现文档；已启动的服务停止后
/// `pid` 返回错误。
///
/// # Returns
/// 返回值
/// The browser-facing base URL of the ready service.
/// 返回已就绪服务面向浏览器的基础 URL。
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn wait_until_ready(
    discovery_path: &std::path::Path,
    mut pid: impl FnMut() -> Result<Option<u32>, String>,
) -> Result<String, String> {
    use std::time::{Duration, Instant};
    /// Bounds the wait for a started service to publish its ready discovery document.
    /// 限定等待已启动服务发布就绪发现文档的时长。
    const READY_TIMEOUT: Duration = Duration::from_secs(30);
    /// Interval between readiness checks of the discovery document.
    /// 两次检查发现文档就绪状态之间的间隔。
    const READY_POLL: Duration = Duration::from_millis(200);
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        let expected = pid()?;
        // A stale document from an earlier owner is ignored until the started process replaces it.
        // 早先所有者遗留的旧文档会被忽略，直到已启动的进程替换它。
        if let Ok(text) = std::fs::read_to_string(discovery_path)
            && let Ok(discovery) =
                serde_json::from_str::<vulcan_service_api::WebuiServiceDiscoverySnapshot>(&text)
            && expected.is_none_or(|pid| discovery.pid == pid)
            && discovery.status == "ready"
        {
            return Ok(discovery.base_url);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "the service did not become ready within {} s",
                READY_TIMEOUT.as_secs()
            ));
        }
        std::thread::sleep(READY_POLL);
    }
}

/// Unit file name; Debug and Release own separate data roots and therefore separate units.
/// 单元文件名；Debug 与 Release 拥有各自的数据根，因此使用各自的单元。
#[cfg(any(target_os = "linux", test))]
fn unit_name() -> &'static str {
    if cfg!(debug_assertions) {
        "vulcan-code-debug.service"
    } else {
        "vulcan-code.service"
    }
}

/// Whether this process runs inside the Vulcan Code user unit, read from its own cgroup.
///
/// `INVOCATION_ID` cannot answer this: desktop shells that run as systemd services (GNOME Shell,
/// plasmashell) pass it to every application they launch, so a desktop-started Core inherits it.
/// 本进程是否运行在 Vulcan Code 用户单元中，按其自身 cgroup 判断。
///
/// `INVOCATION_ID` 无法回答这个问题：以 systemd 服务运行的桌面外壳（GNOME Shell、plasmashell）会把它
/// 传给所启动的每个应用，因此由桌面端启动的 Core 也会继承它。
#[cfg(target_os = "linux")]
pub(crate) fn running_in_user_unit() -> bool {
    std::fs::read_to_string("/proc/self/cgroup")
        .is_ok_and(|cgroup| cgroup_names_unit(&cgroup, unit_name()))
}

/// Whether any hierarchy in `/proc/self/cgroup` text places the process directly in `unit`.
/// `/proc/self/cgroup` 文本中是否有任一层级把进程直接放在 `unit` 中。
#[cfg(any(target_os = "linux", test))]
fn cgroup_names_unit(cgroup: &str, unit: &str) -> bool {
    cgroup.lines().any(|line| {
        // Each line is `hierarchy-id:controllers:path`; the path may itself contain `:`.
        // 每行为 `层级编号:控制器:路径`；路径本身可能包含 `:`。
        line.splitn(3, ':')
            .nth(2)
            .and_then(|path| path.rsplit('/').next())
            .is_some_and(|leaf| leaf == unit)
    })
}

/// Quotes one value for a systemd unit line, escaping specifier and variable expansion.
/// 为 systemd 单元行引用一个值，并转义说明符与变量展开。
#[cfg(any(target_os = "linux", test))]
fn systemd_quoted(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for character in value.chars() {
        match character {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '%' => quoted.push_str("%%"),
            '$' => quoted.push_str("$$"),
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

/// Renders the user unit that runs `executable` headless and restarts it only after a failure.
/// 渲染以无头模式运行 `executable`、仅在故障后重启的用户单元。
///
/// # Parameters
/// 参数
/// `executable` is the canonical distribution Core; `path` is the installing shell's `PATH`, kept
/// so commands run by the agent find the same tools as that terminal.
/// `executable` 是规范化的发行版 Core；`path` 是安装时 shell 的 `PATH`，使智能体执行的命令能找到
/// 与该终端相同的工具。
#[cfg(any(target_os = "linux", test))]
fn render_unit(executable: &std::path::Path, path: Option<&str>) -> String {
    let executable = executable.to_string_lossy();
    let mut unit = format!(
        "[Unit]\n\
         Description=Vulcan Code service\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={} --headless --no-debug-console\n\
         Restart=on-failure\n\
         RestartSec=5\n",
        systemd_quoted(&executable)
    );
    if let Some(path) = path {
        unit.push_str(&format!(
            "Environment={}\n",
            systemd_quoted(&format!("PATH={path}"))
        ));
    }
    unit.push_str("\n[Install]\nWantedBy=default.target\n");
    unit
}

#[cfg(target_os = "linux")]
mod systemd {
    use std::path::PathBuf;
    use std::process::{Command, Output};

    use super::{
        registered_executable, render_unit, require_distribution, require_free_data_root, unit_name,
    };

    /// Directory that exists only while systemd is the running service manager.
    /// 仅当 systemd 是正在运行的服务管理器时才存在的目录。
    const SYSTEMD_RUNTIME_DIR: &str = "/run/systemd/system";

    /// Runs `systemctl --user` with `args` and returns its raw output.
    /// 以 `args` 运行 `systemctl --user` 并返回原始输出。
    fn systemctl(args: &[&str]) -> Result<Output, String> {
        Command::new("systemctl")
            .arg("--user")
            .args(args)
            .output()
            .map_err(|error| format!("failed to run systemctl --user: {error}"))
    }

    /// Runs `systemctl --user` with `args`, turning a non-zero exit into its stderr text.
    /// 以 `args` 运行 `systemctl --user`，非零退出时返回其标准错误文本。
    fn systemctl_checked(args: &[&str]) -> Result<Output, String> {
        let output = systemctl(args)?;
        if output.status.success() {
            Ok(output)
        } else {
            Err(format!(
                "systemctl --user {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }

    /// Resolves the systemd user unit directory below `XDG_CONFIG_HOME` or `~/.config`.
    /// 解析位于 `XDG_CONFIG_HOME` 或 `~/.config` 下的 systemd 用户单元目录。
    fn unit_path() -> Result<PathBuf, String> {
        let config = match std::env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
            Some(config) => PathBuf::from(config),
            None => vulcan_runtime_assets::user_home_dir()
                .ok_or_else(|| String::from("failed to resolve user home directory"))?
                .join(".config"),
        };
        Ok(config.join("systemd").join("user").join(unit_name()))
    }

    /// Requires a reachable systemd user manager before touching any registration.
    /// 修改任何注册前，要求 systemd 用户管理器可用。
    fn require_user_manager() -> Result<(), String> {
        if !std::path::Path::new(SYSTEMD_RUNTIME_DIR).is_dir() {
            return Err(String::from(
                "this system does not run systemd (for example inside a container), so no user service can be registered; run `vulcan-code-service --headless` under the system's own process manager instead",
            ));
        }
        systemctl_checked(&["show-environment"]).map(|_| ()).map_err(|error| {
            let user = std::env::var("USER").unwrap_or_else(|_| String::from("<user>"));
            format!(
                "systemd runs, but this login has no systemd user manager ({error}); this is common after `su` or `sudo -i`. Log in as {user} directly (SSH or the console) or open a full session with `machinectl shell {user}@`, then install again; `loginctl enable-linger {user}` also keeps a user manager running"
            )
        })
    }

    /// Waits until the unit's main process publishes a ready discovery document.
    /// 等待单元主进程发布就绪的发现文档。
    ///
    /// # Returns
    /// 返回值
    /// The browser-facing base URL of the ready service.
    /// 返回已就绪服务面向浏览器的基础 URL。
    fn wait_until_ready(discovery_path: &std::path::Path) -> Result<String, String> {
        super::wait_until_ready(discovery_path, || {
            let main_pid = String::from_utf8_lossy(
                &systemctl_checked(&["show", "--property=MainPID", "--value", unit_name()])?.stdout,
            )
            .trim()
            .parse::<u32>()
            .unwrap_or(0);
            if main_pid == 0 {
                return Err(format!("{} stopped during startup", unit_name()));
            }
            Ok(Some(main_pid))
        })
        .map_err(|error| format!("{error}; inspect `journalctl --user -u {}`", unit_name()))
    }

    /// Writes the unit, enables it at login, starts it, and waits for readiness.
    /// 写入单元、设为登录时启动、立即启动并等待就绪。
    pub(super) fn install() -> Result<String, String> {
        require_user_manager()?;
        require_distribution()?;
        let executable = registered_executable()?;
        let unit_path = unit_path()?;
        // Reinstalling replaces a running registration, so release its lock before checking it.
        // 重新安装会替换正在运行的注册，因此检查锁之前先让其释放。
        if unit_path.exists() {
            systemctl_checked(&["stop", unit_name()])?;
        }
        let data_root = crate::service_runtime::authoritative_data_root()?;
        require_free_data_root(&data_root)?;
        let directory = unit_path
            .parent()
            .ok_or_else(|| String::from("systemd user unit path has no parent"))?;
        std::fs::create_dir_all(directory)
            .map_err(|error| format!("failed to create {}: {error}", directory.display()))?;
        let path = std::env::var("PATH").ok();
        // A same-directory temporary file keeps systemd from ever reading a partial unit.
        // 同目录临时文件确保 systemd 不会读到不完整的单元。
        let staged = unit_path.with_extension("service.tmp");
        std::fs::write(&staged, render_unit(&executable, path.as_deref()))
            .map_err(|error| format!("failed to write {}: {error}", staged.display()))?;
        std::fs::rename(&staged, &unit_path)
            .map_err(|error| format!("failed to install {}: {error}", unit_path.display()))?;
        systemctl_checked(&["daemon-reload"])?;
        systemctl_checked(&["enable", "--now", unit_name()])?;
        let discovery_path =
            crate::launch::service_launch_description(executable, data_root).discovery_path;
        let base_url = wait_until_ready(&discovery_path)?;
        Ok(format!(
            "Installed and started {unit}: {unit_path}\nWebUI: {base_url}\nIt starts with your login session; to keep it running after logout, run `loginctl enable-linger`.\nStatus: systemctl --user status {unit}",
            unit = unit_name(),
            unit_path = unit_path.display(),
        ))
    }

    /// Stops and disables the unit, then deletes its file.
    /// 停止并取消启用单元，随后删除其文件。
    pub(super) fn uninstall() -> Result<String, String> {
        let unit_path = unit_path()?;
        if !unit_path.exists() {
            return Ok(format!(
                "{} is not installed; nothing to remove",
                unit_name()
            ));
        }
        require_user_manager()?;
        // `disable --now` stops gracefully: the Core drains its sessions on SIGTERM.
        // `disable --now` 优雅停止：Core 收到 SIGTERM 后排空会话。
        systemctl_checked(&["disable", "--now", unit_name()])?;
        std::fs::remove_file(&unit_path)
            .map_err(|error| format!("failed to remove {}: {error}", unit_path.display()))?;
        systemctl_checked(&["daemon-reload"])?;
        Ok(format!(
            "Stopped and removed {}: {}",
            unit_name(),
            unit_path.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds process arguments from one executable name and tail flags.
    /// 从可执行文件名与尾部参数构建进程参数。
    fn args(tail: &[&str]) -> Vec<String> {
        std::iter::once("vulcan-code-service")
            .chain(tail.iter().copied())
            .map(String::from)
            .collect()
    }

    /// Verifies management flags are recognized alone and rejected beside other arguments.
    /// 验证管理参数单独使用时被识别，与其他参数混用时被拒绝。
    #[test]
    fn management_flags_must_stand_alone() {
        assert_eq!(user_service_action(&args(&["--headless"])), Ok(None));
        assert_eq!(user_service_action(&args(&[])), Ok(None));
        assert_eq!(
            user_service_action(&args(&[ARG_INSTALL_USER_SERVICE])),
            Ok(Some(UserServiceAction::Install))
        );
        assert_eq!(
            user_service_action(&args(&[ARG_UNINSTALL_USER_SERVICE])),
            Ok(Some(UserServiceAction::Uninstall))
        );
        assert!(user_service_action(&args(&[ARG_INSTALL_USER_SERVICE, "--headless"])).is_err());
        assert!(
            user_service_action(&args(&[
                ARG_INSTALL_USER_SERVICE,
                ARG_UNINSTALL_USER_SERVICE
            ]))
            .is_err()
        );
    }

    /// Verifies the unit runs the exact executable headless with expansion-safe quoting.
    /// 验证单元以无头模式运行精确的可执行文件，并使用不会被展开的引用。
    #[test]
    fn unit_runs_the_exact_executable_headless() {
        let unit = render_unit(
            std::path::Path::new("/opt/vulcan 1/bin/vulcan-code-service"),
            Some("/usr/bin:/home/a$b/100%"),
        );
        let lines: Vec<&str> = unit.lines().collect();
        assert!(lines.contains(
            &"ExecStart=\"/opt/vulcan 1/bin/vulcan-code-service\" --headless --no-debug-console"
        ));
        assert!(lines.contains(&"Environment=\"PATH=/usr/bin:/home/a$$b/100%%\""));
        assert!(lines.contains(&"Restart=on-failure"));
        assert!(lines.contains(&"WantedBy=default.target"));
        assert!(unit_name().ends_with(".service"));
    }

    /// Verifies only a process placed in the unit's own cgroup counts as running in the unit.
    /// 验证只有位于单元自身 cgroup 的进程才算运行在单元中。
    #[test]
    fn only_the_unit_cgroup_counts_as_the_unit() {
        let unit = "vulcan-code.service";
        let in_unit =
            "0::/user.slice/user-1000.slice/user@1000.service/app.slice/vulcan-code.service\n";
        assert!(cgroup_names_unit(in_unit, unit));
        let hybrid = "12:cpu,cpuacct:/\n1:name=systemd:/user.slice/user-1000.slice/user@1000.service/app.slice/vulcan-code.service\n0::/\n";
        assert!(cgroup_names_unit(hybrid, unit));
        // A desktop-launched Core sits in the launcher's scope or the shell's service.
        // 由桌面端启动的 Core 位于启动器的 scope 或桌面外壳的服务中。
        let desktop = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-gnome-vulcan\\x2dcode-4242.scope\n";
        assert!(!cgroup_names_unit(desktop, unit));
        let shell = "0::/user.slice/user-1000.slice/user@1000.service/session.slice/org.gnome.Shell@wayland.service\n";
        assert!(!cgroup_names_unit(shell, unit));
        // The transient unit that reinstalls the service is not the service.
        // 重新安装服务的临时单元不是该服务本身。
        let transient =
            "0::/user.slice/user-1000.slice/user@1000.service/app.slice/run-u42.service\n";
        assert!(!cgroup_names_unit(transient, unit));
        assert!(!cgroup_names_unit("", unit));
    }

    /// Verifies a missing PATH leaves the unit to the user manager's environment.
    /// 验证缺少 PATH 时单元沿用用户管理器的环境。
    #[test]
    fn unit_without_path_sets_no_environment() {
        let unit = render_unit(
            std::path::Path::new("/opt/vulcan/bin/vulcan-code-service"),
            None,
        );
        assert!(!unit.contains("Environment="));
    }
}
