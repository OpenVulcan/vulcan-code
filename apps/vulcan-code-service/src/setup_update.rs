//! Finishing an update through the setup program once the service stopped (plan section 8,
//! `docs/contracts/app-updates.md`).
//! 服务停止后经安装器完成更新（方案第八节，`docs/contracts/app-updates.md`）。
//!
//! Across install epochs the updater assembles the new version next to the running one, and the
//! setup program of a root it owns switches to it and refreshes what it installed outside the
//! version directory. That run needs the data root free, since it may register the user service, so
//! the stopping service starts this process instead: it waits for the old owner to exit, runs the
//! setup program, records the outcome, and starts the service of the current version again unless
//! the setup program already did.
//!
//! 跨安装纪元时，更新器在正在运行的版本旁组装新版本，由其拥有的根目录中的安装器切换到它并刷新其在版本目录之外
//! 安装的内容。该运行可能注册用户服务，需要数据根空闲，因此正在停止的服务改为启动本进程：等待旧所有者退出，
//! 运行安装器，记录结果，并在安装器尚未启动时重新启动当前版本的服务。

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::launch::{ARG_AWAIT_OWNER_EXIT, OWNER_EXIT_WAIT};

/// Starts this process as the helper; it must come first, followed by the version, the channel,
/// how the service starts again ([`START_DETACHED`] or [`START_USER_UNIT`]) and the arguments of
/// the stopped service.
/// 以辅助进程方式启动本进程；它必须位于首位，其后依次为版本、通道、服务如何重新启动（[`START_DETACHED`]
/// 或 [`START_USER_UNIT`]）以及已停止服务的参数。
pub const ARG_RUN_SETUP_UPDATE: &str = "--run-setup-update";

/// The service starts again as a detached process with the stopped service's arguments.
/// 服务以已停止服务的参数作为分离进程重新启动。
const START_DETACHED: &str = "detached";

/// The service ran in the systemd user unit, which is installed and started again.
/// 服务运行在 systemd 用户单元中，重新安装并启动该单元。
const START_USER_UNIT: &str = "user-unit";

/// How often the helper looks whether the old owner released the data root.
/// 辅助进程查看旧所有者是否已释放数据根的频率。
const OWNER_POLL: Duration = Duration::from_millis(250);

/// A helper run asked for on the command line.
/// 命令行请求的一次辅助运行。
#[derive(Debug, PartialEq, Eq)]
pub struct Request {
    /// The version assembled for the setup program.
    /// 为安装器组装的版本。
    version: String,
    /// The channel that offered it.
    /// 提供它的通道。
    channel: String,
    /// Whether the service starts again through the systemd user unit.
    /// 服务是否经 systemd 用户单元重新启动。
    user_unit: bool,
    /// The arguments of the stopped service.
    /// 已停止服务的参数。
    service_arguments: Vec<String>,
}

/// The arguments that start the helper for `version` from `channel`.
/// 为来自 `channel` 的 `version` 启动辅助进程的参数。
pub(crate) fn helper_arguments(
    version: &str,
    channel: &str,
    user_unit: bool,
    service_arguments: impl IntoIterator<Item = String>,
) -> Vec<String> {
    let start = if user_unit {
        START_USER_UNIT
    } else {
        START_DETACHED
    };
    [ARG_RUN_SETUP_UPDATE, version, channel, start]
        .into_iter()
        .map(String::from)
        .chain(service_arguments)
        .collect()
}

/// The helper run `args` (the full command line) asks for, if any.
/// `args`（完整命令行）请求的辅助运行（若有）。
pub fn requested(args: &[String]) -> Result<Option<Request>, String> {
    if args.get(1).map(String::as_str) != Some(ARG_RUN_SETUP_UPDATE) {
        return Ok(None);
    }
    let [version, channel, start, service_arguments @ ..] = &args[2..] else {
        return Err(format!(
            "{ARG_RUN_SETUP_UPDATE} needs a version, a channel and how the service starts"
        ));
    };
    let user_unit = match start.as_str() {
        START_DETACHED => false,
        START_USER_UNIT => true,
        other => return Err(format!("unknown service start `{other}`")),
    };
    Ok(Some(Request {
        version: version.clone(),
        channel: channel.clone(),
        user_unit,
        service_arguments: service_arguments
            .iter()
            .filter(|arg| *arg != ARG_AWAIT_OWNER_EXIT)
            .cloned()
            .collect(),
    }))
}

/// Runs the helper: waits for the old owner, runs the setup program, records the outcome and
/// starts the service again; returns a summary for the log.
/// 运行辅助进程：等待旧所有者，运行安装器，记录结果并重新启动服务；返回用于日志的摘要。
pub fn run(request: &Request) -> Result<String, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("failed to resolve the service executable: {error}"))?;
    let data_root = crate::service_runtime::authoritative_data_root()?;
    if !wait_until_free(&data_root, OWNER_EXIT_WAIT)? {
        let error = String::from("the previous service did not exit");
        vulcan_app_update::finish_setup_run(&executable, &request.version, Err(error.clone()))?;
        return Err(error);
    }
    let outcome = run_setup_program(&executable, &request.channel);
    let summary = match &outcome {
        Ok(status) => format!("the setup program exited with {status}"),
        Err(error) => format!("the setup program could not run: {error}"),
    };
    vulcan_app_update::finish_setup_run(&executable, &request.version, outcome)?;
    // Registering the user service starts it, so the setup program may have started it already.
    // 注册用户服务会启动它，因此安装器可能已经启动了服务。
    if data_root_is_free(&data_root)? {
        let service = vulcan_app_update::current_executable(&executable).unwrap_or(executable);
        start_service(&service, request)?;
    }
    Ok(summary)
}

/// Waits up to `limit` until no process owns `data_root`.
/// 最多等待 `limit`，直到没有进程拥有 `data_root`。
fn wait_until_free(data_root: &Path, limit: Duration) -> Result<bool, String> {
    let deadline = Instant::now() + limit;
    loop {
        if data_root_is_free(data_root)? {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(OWNER_POLL);
    }
}

/// Whether no process owns `data_root` right now.
/// 当前是否没有进程拥有 `data_root`。
fn data_root_is_free(data_root: &Path) -> Result<bool, String> {
    vulcan_service_host::instance::ServiceInstanceGuard::acquire(data_root)
        .map(|guard| guard.is_some())
        .map_err(|error| format!("failed to check the service lock: {error}"))
}

/// Runs the setup program of this root unattended, its output going to the setup log.
/// 无人值守地运行本根目录的安装器，其输出写入安装器日志。
fn run_setup_program(executable: &Path, channel: &str) -> Result<std::process::ExitStatus, String> {
    let (program, arguments) = vulcan_app_update::setup_command(executable, channel)?;
    let log = vulcan_app_update::setup_log(executable)
        .ok_or_else(|| String::from("this installation has no updates directory"))?;
    let output = open_log(&log)?;
    let errors = output
        .try_clone()
        .map_err(|error| format!("{}: {error}", log.display()))?;
    let mut command = Command::new(&program);
    command
        .args(&arguments)
        .env_remove(crate::runtime_layout::RUNTIME_LAYOUT_ENV)
        .stdin(Stdio::null())
        .stdout(output)
        .stderr(errors);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        /// Keeps the console program from opening a window of its own.
        /// 使控制台程序不打开自己的窗口。
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
        .status()
        .map_err(|error| format!("{}: {error}", program.display()))
}

/// Opens the setup log afresh.
/// 重新打开安装器日志。
fn open_log(log: &Path) -> Result<std::fs::File, String> {
    if let Some(parent) = log.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    std::fs::File::create(log).map_err(|error| format!("{}: {error}", log.display()))
}

/// Starts the service at `service` the way the stopped one ran.
/// 按已停止服务的运行方式启动位于 `service` 的服务。
fn start_service(service: &Path, request: &Request) -> Result<(), String> {
    if request.user_unit {
        let status = Command::new(service)
            .arg(crate::user_service::ARG_INSTALL_USER_SERVICE)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|error| format!("{}: {error}", service.display()))?;
        return if status.success() {
            Ok(())
        } else {
            Err(format!("installing the user service exited with {status}"))
        };
    }
    let mut arguments = request.service_arguments.clone();
    arguments.push(String::from(ARG_AWAIT_OWNER_EXIT));
    crate::restart::replacement_command(service, &arguments)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("{}: {error}", service.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A full command line for `tail`.
    /// 以 `tail` 结尾的完整命令行。
    fn command_line(tail: Vec<String>) -> Vec<String> {
        std::iter::once(String::from("vulcan-code-service"))
            .chain(tail)
            .collect()
    }

    #[test]
    fn helper_arguments_round_trip_without_the_owner_wait() {
        for user_unit in [false, true] {
            let arguments = helper_arguments(
                "0.2.0",
                "preview",
                user_unit,
                [
                    String::from("--gui-launch"),
                    String::from(ARG_AWAIT_OWNER_EXIT),
                ],
            );
            let request = requested(&command_line(arguments))
                .expect("parsed")
                .expect("requested");
            assert_eq!(
                request,
                Request {
                    version: String::from("0.2.0"),
                    channel: String::from("preview"),
                    user_unit,
                    service_arguments: vec![String::from("--gui-launch")],
                }
            );
        }
    }

    #[test]
    fn only_a_leading_flag_asks_for_the_helper() {
        assert_eq!(
            requested(&command_line(vec![String::from("--service")])).expect("parsed"),
            None
        );
        assert!(requested(&command_line(vec![String::from(ARG_RUN_SETUP_UPDATE)])).is_err());
        assert!(
            requested(&command_line(
                ["--run-setup-update", "0.2.0", "stable", "elsewhere"]
                    .map(String::from)
                    .to_vec()
            ))
            .is_err()
        );
    }
}
