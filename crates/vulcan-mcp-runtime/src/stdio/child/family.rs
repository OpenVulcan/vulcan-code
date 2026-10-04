//! Platform-owned process scope retained by the native supervisor through confirmed exit.
//! 原生监督器持有至确认退出的平台进程范围。

use std::{
    io,
    process::{Child, Command},
};

/// Provides one lifecycle interface without treating root exit as Windows family exit.
/// 提供统一生命周期接口，不把根进程退出当作 Windows 进程族退出。
pub(super) trait ChildScope: Sized {
    /// Prepares containment before spawning; command receives the platform's required launch flags.
    /// 启动前准备纳管；command 接收平台要求的启动标志。
    /// Returns the scope owner or an error before any child has been created.
    /// 返回范围所有者，或在尚未创建子进程前返回错误。
    fn prepare(command: &mut Command) -> io::Result<Self>;

    /// Attaches the exact owned child before plugin code may run, retaining ownership on failure.
    /// 插件代码运行前绑定精确自有子进程，失败时保留所有权。
    /// Child is the process just spawned; returns startup readiness or an explicit error.
    /// child 是刚启动的进程；返回启动就绪或明确错误。
    fn attach(&mut self, child: &Child) -> io::Result<()>;

    /// Polls the owned root and platform scope without inferring completion from a stop request.
    /// 轮询自有根进程及平台范围，不从停止请求推断完成。
    /// Child remains owned; true means every process covered by this platform implementation has exited.
    /// child 保持自有；true 表示此平台实现覆盖的全部进程已经退出。
    fn exit_confirmed(&self, child: &mut Child) -> io::Result<bool>;

    /// Requests scope termination using retained OS handles; success alone never proves exit.
    /// 使用保留操作系统句柄请求终止范围；仅成功本身绝不证明退出。
    /// Child is retained for reaping and failed-start cleanup; returns only the termination request result.
    /// child 保留用于回收及失败启动清理；仅返回终止请求结果。
    fn terminate(&self, child: &mut Child) -> io::Result<()>;
}

/// Owns a private non-inheritable Windows Job until all assigned descendants exit.
/// 持有私有不可继承 Windows Job，直至全部已绑定后代退出。
#[cfg(windows)]
pub(super) struct ChildFamily {
    /// This live job prevents descendants from escaping via ordinary process creation.
    /// 此有效 Job 阻止后代通过普通进程创建脱离。
    job: vulcan_process_lifecycle::WindowsKillOnCloseJob,
    /// Assignment is recorded before resumption, including any partial startup failure.
    /// 恢复前记录绑定，包括任何部分启动失败。
    assigned: bool,
}

#[cfg(windows)]
impl ChildScope for ChildFamily {
    /// Creates the job first, preserving the host's sole no-window flag and adding suspension.
    /// 先创建 Job，保留宿主唯一无窗口标志并添加挂起。
    /// Command is the actual launch command; returns an unassigned owner or the native preparation error.
    /// command 为实际启动命令；返回未绑定所有者或原生准备错误。
    fn prepare(command: &mut Command) -> io::Result<Self> {
        // Creating the job before spawning ensures all fallible configuration precedes process creation.
        // 在启动前创建 Job，确保全部可能失败的配置先于进程创建。
        let job = vulcan_process_lifecycle::WindowsKillOnCloseJob::create()?;
        vulcan_process_lifecycle::configure_suspended_child(
            command,
            vulcan_host_tools::process_creation::WINDOWS_CREATE_NO_WINDOW,
        );
        Ok(Self {
            job,
            assigned: false,
        })
    }

    /// Assigns before resuming; a resume failure still leaves the child inside the retained job.
    /// 恢复前绑定；恢复失败仍使子进程留在保留 Job 内。
    /// Child is the suspended process; returns readiness only after native resumption succeeds.
    /// child 为挂起进程；仅在原生恢复成功后返回就绪。
    fn attach(&mut self, child: &Child) -> io::Result<()> {
        self.job.assign_child(child)?;
        self.assigned = true;
        vulcan_process_lifecycle::resume_suspended_child(child)
    }

    /// Requires a reaped root and zero active job members, so detached descendants remain owned.
    /// 要求根进程已回收且活动 Job 成员归零，使后台后代保持被持有。
    /// Child supplies the exact root handle; query failures explicitly prevent successful completion.
    /// child 提供精确根句柄；查询失败明确阻止完成成功。
    fn exit_confirmed(&self, child: &mut Child) -> io::Result<bool> {
        if child.try_wait()?.is_none() {
            return Ok(false);
        }
        if self.assigned {
            return Ok(self.job.active_process_count()? == 0);
        }
        // Only a failed assignment leaves an unassigned root; it never ran plugin code.
        // 仅绑定失败会留下未绑定根进程；它从未运行插件代码。
        Ok(true)
    }

    /// Terminates the retained job, or the still-suspended root if assignment itself failed.
    /// 终止保留 Job；如果绑定本身失败，则终止仍挂起的根进程。
    /// Child remains available for actual reaping; an assignment error never permits uncontained startup.
    /// child 保持可供实际回收；绑定错误绝不允许未纳管启动。
    fn terminate(&self, child: &mut Child) -> io::Result<()> {
        if self.assigned {
            self.job.terminate(1)
        } else {
            child.kill()
        }
    }
}

/// Retains the existing direct-child boundary until Unix process-family support is implemented.
/// 在 Unix 进程族支持完成前，保留现有直接子进程边界。
/// This unfinished platform boundary is a release gate, not a complete family-isolation claim.
/// 此未完成平台边界属于发布门槛，不代表完整进程族隔离。
#[cfg(not(windows))]
pub(super) struct ChildFamily;

#[cfg(not(windows))]
impl ChildScope for ChildFamily {
    /// Preserves the existing Unix command; no process-family containment is added here yet.
    /// 保留既有 Unix 命令；此处尚未添加进程族纳管。
    /// The unchanged command receives a direct-child scope with no additional setup failure.
    /// 未变命令接收直接子进程范围，不产生额外准备失败。
    fn prepare(_command: &mut Command) -> io::Result<Self> {
        Ok(Self)
    }

    /// Accepts the already owned direct child without claiming descendant containment.
    /// 接受已持有的直接子进程，不声称后代已纳管。
    /// Child is unused on this current Unix path; returns successful direct-child attachment.
    /// 当前 Unix 路径不使用 child；返回直接子进程绑定成功。
    fn attach(&mut self, _child: &Child) -> io::Result<()> {
        Ok(())
    }

    /// Polls only the owned direct child on the unfinished Unix implementation.
    /// 在尚未完成的 Unix 实现中，仅轮询持有的直接子进程。
    /// Child supplies the real process; true proves only this process was reaped.
    /// child 提供实际进程；true 仅证明此进程已回收。
    fn exit_confirmed(&self, child: &mut Child) -> io::Result<bool> {
        child.try_wait().map(|status| status.is_some())
    }

    /// Requests termination of the owned direct Unix child.
    /// 请求终止持有的 Unix 直接子进程。
    /// Child supplies the exact handle; returns the OS request result without an exit claim.
    /// child 提供精确句柄；返回操作系统请求结果，不声明退出。
    fn terminate(&self, child: &mut Child) -> io::Result<()> {
        child.kill()
    }
}
