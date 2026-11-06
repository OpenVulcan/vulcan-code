//! Restarting the service into an updated version or a full installer (plan section 8).
//! 将服务重启到更新版本或完整安装包（方案第八节）。
//!
//! The updater asks for a restart; the owner loop drains every session as for a normal quit, and
//! only after a successful drain starts the replacement. A replacement service waits for this
//! process to release the data-root lock ([`crate::launch::ARG_AWAIT_OWNER_EXIT`]). Under a
//! systemd user unit the unit is reinstalled from the new version instead, which stops this
//! process through its normal termination signal and starts the new one.
//!
//! 更新器请求重启；所有者循环像普通退出一样排空所有会话，只有排空成功后才启动替代进程。替代服务会等待
//! 本进程释放数据根锁（[`crate::launch::ARG_AWAIT_OWNER_EXIT`]）。在 systemd 用户单元下改为从新版本重新
//! 安装单元，由其正常终止信号停止本进程并启动新进程。

use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use vulcan_app_update::RestartPlan;

use crate::launch::ARG_AWAIT_OWNER_EXIT;

/// What starts once the drain succeeded.
/// 排空成功后启动的内容。
static PENDING: Mutex<Option<RestartPlan>> = Mutex::new(None);

/// Records `plan` and asks the owner loop to drain through `shutdown_requested`.
/// 记录 `plan`，并通过 `shutdown_requested` 请求所有者循环排空。
pub(crate) fn request(plan: RestartPlan, shutdown_requested: &AtomicBool) -> Result<(), String> {
    // Only the unit's own process restarts through systemd; any other Core drains and exits.
    // 只有单元自身的进程经 systemd 重启；其他 Core 一律排空后退出。
    #[cfg(target_os = "linux")]
    if let RestartPlan::Service { executable } = &plan
        && crate::user_service::running_in_user_unit()
    {
        return reinstall_user_unit(executable);
    }
    *PENDING
        .lock()
        .map_err(|_| String::from("restart state is poisoned"))? = Some(plan);
    shutdown_requested.store(true, Ordering::Release);
    Ok(())
}

/// Drops the recorded replacement after a failed drain, so a later ordinary quit does not start it.
/// 排空失败后丢弃已记录的替代进程，使之后的普通退出不会启动它。
pub(crate) fn discard_pending() {
    if let Ok(mut pending) = PENDING.lock()
        && let Some(plan) = pending.take()
    {
        tracing::warn!("[vulcan-code-service] drain failed; discarded {plan:?}");
    }
}

/// Starts the recorded replacement; called once the drain succeeded and before the process exits.
/// 启动已记录的替代进程；在排空成功后、进程退出前调用。
pub(crate) fn launch_pending() {
    let Some(plan) = PENDING.lock().ok().and_then(|mut pending| pending.take()) else {
        return;
    };
    let result = match &plan {
        RestartPlan::Service { executable } => {
            // The replacement keeps this process's launch mode.
            // 替代进程沿用本进程的启动模式。
            let mut arguments: Vec<String> = std::env::args()
                .skip(1)
                .filter(|arg| arg != ARG_AWAIT_OWNER_EXIT)
                .collect();
            arguments.push(String::from(ARG_AWAIT_OWNER_EXIT));
            spawn_detached(executable, &arguments)
        }
        RestartPlan::Installer { program, arguments } => spawn_detached(program, arguments),
    };
    match result {
        Ok(()) => tracing::info!("[vulcan-code-service] started {plan:?}"),
        Err(error) => tracing::error!("[vulcan-code-service] could not start {plan:?}: {error}"),
    }
}

/// Starts `program` outside this process's console and process group.
/// 在本进程的控制台与进程组之外启动 `program`。
fn spawn_detached(program: &std::path::Path, arguments: &[String]) -> Result<(), String> {
    let mut command = Command::new(program);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        /// Starts the child without a console of its own.
        /// 启动不带自身控制台的子进程。
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        /// Keeps console control events of this process away from the child.
        /// 使本进程的控制台控制事件不影响子进程。
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("{}: {error}", program.display()))
}

/// Reinstalls the systemd user unit from `executable` outside this unit, so stopping the unit does
/// not stop the reinstall.
/// 在本单元之外以 `executable` 重新安装 systemd 用户单元，使停止本单元不会中止重新安装。
#[cfg(target_os = "linux")]
fn reinstall_user_unit(executable: &std::path::Path) -> Result<(), String> {
    let status = Command::new("systemd-run")
        .args(["--user", "--collect", "--quiet", "--"])
        .arg(executable)
        .arg(crate::user_service::ARG_INSTALL_USER_SERVICE)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("failed to run systemd-run: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("systemd-run failed with {status}"))
    }
}
