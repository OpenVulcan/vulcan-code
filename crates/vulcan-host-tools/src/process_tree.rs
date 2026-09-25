//! Process-tree isolation and termination for host-owned child processes.
//! 宿主自有子进程的进程树隔离与终止能力。

use std::time::Duration;
use tokio::process::{Child, Command as TokioCommand};

#[cfg(windows)]
use crate::process_creation::{new_background_std_command, new_background_tokio_command};

/// Maximum time output readers may keep draining after the direct child exited or its tree was killed.
/// 直接子进程退出或其进程树被终止后，输出读取任务继续排空的最长时间。
///
/// Background descendants can inherit stdout/stderr and keep the pipes open indefinitely; readers stop
/// after this grace instead of blocking the caller forever.
/// 后台后代进程可能继承 stdout/stderr 并无限期保持管道打开；读取任务在此宽限期后停止，
/// 避免调用方永久阻塞。
pub const CHILD_OUTPUT_DRAIN_GRACE: Duration = Duration::from_millis(1_000);

/// Isolates one child so its complete descendant tree can be terminated together.
/// 隔离一个子进程，使其完整后代进程树可以被一起终止。
///
/// # Parameters
/// 参数
/// `command` is the child command being prepared before spawn.
/// `command` 是启动前正在配置的子命令。
///
/// Unix children lead a new process group whose id equals the child pid. Windows descendants are
/// reached through the parent-process tree walked by `taskkill /T`, so no creation change is needed.
/// Unix 子进程成为新进程组组长，组 ID 等于子进程 pid。Windows 后代通过 `taskkill /T` 遍历的父进程树
/// 触达，因此无需修改创建参数。
pub fn isolate_child_process_tree(command: &mut TokioCommand) {
    #[cfg(unix)]
    {
        command.process_group(0);
    }
    #[cfg(not(unix))]
    {
        let _ = command;
    }
}

/// Requests forced termination of one isolated child and all of its descendants.
/// 请求强制终止一个已隔离子进程及其全部后代进程。
///
/// # Parameters
/// 参数
/// `child` must have been spawned from a command prepared by `isolate_child_process_tree`.
/// `child` 必须由经过 `isolate_child_process_tree` 配置的命令启动。
///
/// # Returns
/// 返回值
/// Returns `Ok(())` once termination was requested, or an error describing why it could not be sent.
/// 终止请求发出后返回 `Ok(())`；无法发出时返回说明原因的错误。
pub async fn terminate_child_process_tree(child: &mut Child) -> Result<(), String> {
    #[cfg(unix)]
    {
        if let Some(process_id) = child.id() {
            return kill_unix_process_group(process_id);
        }
    }
    #[cfg(windows)]
    {
        if let Some(process_id) = child.id() {
            let mut taskkill = new_background_tokio_command("taskkill");
            taskkill.args(["/PID", &process_id.to_string(), "/T", "/F"]);
            if let Ok(status) = taskkill.status().await
                && status.success()
            {
                return Ok(());
            }
        }
    }
    child
        .start_kill()
        .map_err(|error| format!("Failed to terminate the child process: {error}"))
}

/// Sends `SIGKILL` to the process group led by one isolated child.
/// 向一个已隔离子进程所领导的进程组发送 `SIGKILL`。
///
/// # Returns
/// 返回值
/// Returns `Ok(())` when the signal was delivered or every group member had already exited.
/// 信号已送达或进程组成员已全部退出时返回 `Ok(())`。
#[cfg(unix)]
fn kill_unix_process_group(process_id: u32) -> Result<(), String> {
    let process_group_id = i32::try_from(process_id)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
        .ok_or_else(|| format!("Child process id {process_id} cannot address a process group."))?;
    match rustix::process::kill_process_group(process_group_id, rustix::process::Signal::KILL) {
        // ESRCH means every group member already exited before the signal was sent.
        // ESRCH 表示发送信号前进程组成员已经全部退出。
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(error) => Err(format!(
            "Failed to terminate the child process group {process_id}: {error}"
        )),
    }
}

/// Drop guard that terminates an isolated process tree when its owner is cancelled before disarming it.
/// 在所有者解除前被取消时终止已隔离进程树的析构守卫。
///
/// `kill_on_drop` only reaches the direct child; this guard also reaches descendants that would
/// otherwise survive a cancelled tool call.
/// `kill_on_drop` 只能触达直接子进程；该守卫还会触达工具调用被取消后原本会残留的后代进程。
#[derive(Debug)]
pub struct ChildProcessTreeDropGuard {
    /// Process id of the isolated tree root, cleared once the owner no longer wants termination.
    /// 已隔离进程树根进程 ID；所有者不再需要终止时清空。
    process_id: Option<u32>,
}

impl ChildProcessTreeDropGuard {
    /// Arms a guard for one child spawned from a command prepared by `isolate_child_process_tree`.
    /// 为一个由 `isolate_child_process_tree` 配置命令启动的子进程创建守卫。
    pub fn arm(child: &Child) -> Self {
        Self {
            process_id: child.id(),
        }
    }

    /// Disarms the guard after the owner reaped the child or terminated the tree explicitly.
    /// 所有者已回收子进程或显式终止进程树后解除守卫。
    pub fn disarm(&mut self) {
        self.process_id = None;
    }
}

impl Drop for ChildProcessTreeDropGuard {
    fn drop(&mut self) {
        let Some(process_id) = self.process_id.take() else {
            return;
        };
        #[cfg(unix)]
        {
            // The guard is armed only while the owned child is unreaped, so the group id cannot have
            // been recycled by an unrelated process.
            // 守卫只在自有子进程尚未回收时处于启用状态，因此进程组 ID 不会被无关进程复用。
            let _ = kill_unix_process_group(process_id);
        }
        #[cfg(windows)]
        {
            // Drop cannot await, so taskkill is started without waiting for its completion.
            // Drop 中无法等待，因此启动 taskkill 后不等待其完成。
            let _ = new_background_std_command("taskkill")
                .args(["/PID", &process_id.to_string(), "/T", "/F"])
                .spawn();
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = process_id;
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::Stdio;
    use tokio::io::AsyncReadExt;

    /// Verifies terminating the tree also stops a grandchild that keeps the output pipe open.
    /// 验证终止进程树时也会停止持有输出管道的孙进程。
    #[tokio::test]
    async fn terminating_tree_closes_pipes_held_by_grandchildren() {
        let mut command = TokioCommand::new("sh");
        command.args(["-c", "sleep 600 & echo started; wait"]);
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.kill_on_drop(true);
        isolate_child_process_tree(&mut command);
        let mut child = command.spawn().expect("spawn process-tree test shell");
        let mut stdout = child
            .stdout
            .take()
            .expect("capture process-tree test stdout");
        let mut first_line = [0_u8; 8];
        stdout
            .read_exact(&mut first_line)
            .await
            .expect("read process-tree test marker");
        assert_eq!(&first_line, b"started\n");

        terminate_child_process_tree(&mut child)
            .await
            .expect("terminate process-tree test shell");
        let mut rest = Vec::new();
        tokio::time::timeout(Duration::from_secs(10), stdout.read_to_end(&mut rest))
            .await
            .expect("grandchild must release stdout after tree termination")
            .expect("read remaining process-tree test output");
        child.wait().await.expect("reap process-tree test shell");
    }

    /// Verifies dropping an armed guard terminates descendants of a cancelled owner.
    /// 验证释放已启用的守卫会终止被取消所有者的后代进程。
    #[tokio::test]
    async fn dropping_armed_guard_terminates_process_tree() {
        let mut command = TokioCommand::new("sh");
        command.args(["-c", "sleep 600 & echo started; wait"]);
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.kill_on_drop(true);
        isolate_child_process_tree(&mut command);
        let mut child = command
            .spawn()
            .expect("spawn process-tree guard test shell");
        let mut stdout = child.stdout.take().expect("capture guard test stdout");
        let mut first_line = [0_u8; 8];
        stdout
            .read_exact(&mut first_line)
            .await
            .expect("read guard test marker");

        drop(ChildProcessTreeDropGuard::arm(&child));
        let mut rest = Vec::new();
        tokio::time::timeout(Duration::from_secs(10), stdout.read_to_end(&mut rest))
            .await
            .expect("grandchild must release stdout after guard drop")
            .expect("read remaining guard test output");
        child
            .wait()
            .await
            .expect("reap process-tree guard test shell");
    }
}
