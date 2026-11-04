//! Per-user service registration that keeps the headless Core running in the background.
//! 为当前用户注册服务，使无头 Core 在后台持续运行。
//!
//! Linux registers a systemd user unit; other platforms report that registration is unsupported.
//! Linux 注册 systemd 用户单元；其他平台报告暂不支持注册。

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
    #[cfg(not(target_os = "linux"))]
    {
        let _ = action;
        Err(String::from(
            "registering Vulcan Code as a user service is supported only on Linux with systemd",
        ))
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
    use std::time::{Duration, Instant};

    use super::{render_unit, unit_name};

    /// Bounds the wait for a started unit to publish its ready discovery document.
    /// 限定等待已启动单元发布就绪发现文档的时长。
    const READY_TIMEOUT: Duration = Duration::from_secs(30);

    /// Interval between readiness checks of the discovery document.
    /// 两次检查发现文档就绪状态之间的间隔。
    const READY_POLL: Duration = Duration::from_millis(200);

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
        systemctl_checked(&["show-environment"]).map(|_| ()).map_err(|error| {
            format!("no systemd user manager is reachable ({error}); enable systemd for this session first")
        })
    }

    /// Requires the executable to belong to a distribution whose runtime layout matches this build.
    /// 要求可执行文件属于运行时布局与本构建配置一致的发行目录。
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

    /// Fails when another process already owns the data root, so the unit would exit at once.
    /// 若已有其他进程拥有数据根则失败，否则单元启动后会立即退出。
    fn require_free_data_root(data_root: &std::path::Path) -> Result<(), String> {
        let guard = vulcan_service_host::instance::ServiceInstanceGuard::acquire(data_root)
            .map_err(|error| format!("failed to check the service lock: {error}"))?;
        if guard.is_none() {
            return Err(String::from(
                "another Vulcan Code service already owns this data directory; quit it (desktop tray, TUI-started Core, or a terminal service) and install again",
            ));
        }
        Ok(())
    }

    /// Waits until the unit's main process publishes a ready discovery document.
    /// 等待单元主进程发布就绪的发现文档。
    ///
    /// # Returns
    /// 返回值
    /// The browser-facing base URL of the ready service.
    /// 返回已就绪服务面向浏览器的基础 URL。
    fn wait_until_ready(discovery_path: &std::path::Path) -> Result<String, String> {
        let deadline = Instant::now() + READY_TIMEOUT;
        loop {
            let main_pid = String::from_utf8_lossy(
                &systemctl_checked(&["show", "--property=MainPID", "--value", unit_name()])?.stdout,
            )
            .trim()
            .parse::<u32>()
            .unwrap_or(0);
            if main_pid == 0 {
                return Err(format!(
                    "{} stopped during startup; inspect `journalctl --user -u {}`",
                    unit_name(),
                    unit_name()
                ));
            }
            // A stale document from an earlier owner is ignored until this unit's process replaces it.
            // 早先所有者遗留的旧文档会被忽略，直到本单元的进程替换它。
            if let Ok(text) = std::fs::read_to_string(discovery_path)
                && let Ok(discovery) =
                    serde_json::from_str::<vulcan_service_api::WebuiServiceDiscoverySnapshot>(&text)
                && discovery.pid == main_pid
                && discovery.status == "ready"
            {
                return Ok(discovery.base_url);
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "{} did not become ready within {} s; inspect `journalctl --user -u {}`",
                    unit_name(),
                    READY_TIMEOUT.as_secs(),
                    unit_name()
                ));
            }
            std::thread::sleep(READY_POLL);
        }
    }

    /// Writes the unit, enables it at login, starts it, and waits for readiness.
    /// 写入单元、设为登录时启动、立即启动并等待就绪。
    pub(super) fn install() -> Result<String, String> {
        require_user_manager()?;
        require_distribution()?;
        let executable = std::env::current_exe()
            .and_then(|path| path.canonicalize())
            .map_err(|error| format!("failed to resolve the service executable: {error}"))?;
        // An installed release runs from its version directory; the unit names the path that
        // follows updates.
        // 已安装的发行版从其版本目录运行；单元使用随更新变化的路径。
        let executable = vulcan_app_update::stable_executable(&executable).unwrap_or(executable);
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
