//! Restarting the service into an updated version, a full installer or the setup program (plan
//! section 8).
//! 将服务重启到更新版本、完整安装包或安装器（方案第八节）。
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
    // The replacement keeps this process's launch mode.
    // 替代进程沿用本进程的启动模式。
    let own_arguments = || {
        std::env::args()
            .skip(1)
            .filter(|arg| arg != ARG_AWAIT_OWNER_EXIT)
    };
    let result = match &plan {
        RestartPlan::Service { executable } => {
            let mut arguments: Vec<String> = own_arguments().collect();
            arguments.push(String::from(ARG_AWAIT_OWNER_EXIT));
            spawn_detached(executable, &arguments)
        }
        RestartPlan::Installer { program, arguments } => spawn_detached(program, arguments),
        RestartPlan::Setup { version, channel } => std::env::current_exe()
            .map_err(|error| format!("failed to resolve the service executable: {error}"))
            .and_then(|executable| {
                spawn_setup_helper(&executable, version, channel, own_arguments().collect())
            }),
    };
    match result {
        Ok(()) => tracing::info!("[vulcan-code-service] started {plan:?}"),
        Err(error) => tracing::error!("[vulcan-code-service] could not start {plan:?}: {error}"),
    }
}

/// Starts this program as the helper that finishes an update through the setup program
/// ([`crate::setup_update`]). Under the systemd user unit it runs outside the unit, since stopping
/// the unit would stop it too, and starts the unit again afterwards.
/// 以经安装器完成更新的辅助进程方式启动本程序（[`crate::setup_update`]）。在 systemd 用户单元中它运行于
/// 单元之外（否则停止单元也会停止它），之后再重新启动该单元。
fn spawn_setup_helper(
    executable: &std::path::Path,
    version: &str,
    channel: &str,
    service_arguments: Vec<String>,
) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    if crate::user_service::running_in_user_unit() {
        let arguments =
            crate::setup_update::helper_arguments(version, channel, true, service_arguments);
        let status = Command::new("systemd-run")
            .args(["--user", "--collect", "--quiet", "--"])
            .arg(executable)
            .args(&arguments)
            .env_remove(crate::runtime_layout::RUNTIME_LAYOUT_ENV)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|error| format!("failed to run systemd-run: {error}"))?;
        return if status.success() {
            Ok(())
        } else {
            Err(format!("systemd-run failed with {status}"))
        };
    }
    let arguments =
        crate::setup_update::helper_arguments(version, channel, false, service_arguments);
    spawn_detached(executable, &arguments)
}

/// Starts `program` outside this process's console and process group.
/// 在本进程的控制台与进程组之外启动 `program`。
fn spawn_detached(program: &std::path::Path, arguments: &[String]) -> Result<(), String> {
    replacement_command(program, arguments)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("{}: {error}", program.display()))
}

/// The command that starts `program` with `arguments`, detached from this process.
/// 以 `arguments` 启动 `program` 且与本进程分离的命令。
///
/// The replacement belongs to another version: the layout override this process inherited from
/// its launcher names this version's manifest, so it is dropped and the replacement (or the app
/// an installer starts) reads the manifest beside its own executable.
/// 替代进程属于另一个版本：本进程从启动者继承的布局覆盖指向本版本的清单，因此将其移除，使替代进程
/// （或安装包启动的应用）读取其自身可执行文件旁的清单。
pub(crate) fn replacement_command(program: &std::path::Path, arguments: &[String]) -> Command {
    let mut command = Command::new(program);
    command
        .args(arguments)
        .env_remove(crate::runtime_layout::RUNTIME_LAYOUT_ENV)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_replacement_does_not_inherit_the_layout_override() {
        // command is what a restart into another version would start.
        // command 是重启到另一版本时会启动的命令。
        let command = replacement_command(
            std::path::Path::new("vulcan-code-service"),
            &[String::from(ARG_AWAIT_OWNER_EXIT)],
        );
        let layout = command.get_envs().find(|(key, _)| {
            *key == std::ffi::OsStr::new(crate::runtime_layout::RUNTIME_LAYOUT_ENV)
        });
        assert_eq!(
            layout,
            Some((
                std::ffi::OsStr::new(crate::runtime_layout::RUNTIME_LAYOUT_ENV),
                None
            ))
        );
    }
}
