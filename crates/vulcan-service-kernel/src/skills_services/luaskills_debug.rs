//! luaskills-debug status, the single Core-owned install task, and the `luaskills_debug` tool host.
//! luaskills-debug 状态、唯一的 Core 自有安装任务，以及 `luaskills_debug` 工具宿主。
//!
//! The settings page and the model tool start the same task, so at most one install runs and both
//! observe it by its stable task id.
//! 设置页与模型工具启动同一个任务，因此最多只有一个安装在运行，二者都按稳定任务标识观察它。

use super::*;
use serde_json::json;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;
use vulcan_agent_runtime::model::SessionId;
use vulcan_agent_runtime::tooling::native::{
    LuaSkillsDebugOutcome, LuaSkillsDebugRequest, NativeLuaSkillsDebugHost,
};
use vulcan_service_runtime::luaskills_debug_tool::{
    LuaSkillsDebugToolStatus, install_luaskills_debug, read_luaskills_debug_status,
};

use crate::session_runtime_registry_v1::SessionRuntimeRegistry;

/// Maximum diagnostic length retained for a failed install.
/// 失败安装保留的诊断最大长度。
const MAX_INSTALL_ERROR_CHARS: usize = 1024;

/// Interval at which the model tool checks a running install.
/// 模型工具检查进行中安装的间隔。
const INSTALL_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Publicly queryable state of the luaskills-debug install task.
/// luaskills-debug 安装任务的公开可查询状态。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LuaSkillsDebugInstallTask {
    /// Stable task id returned to callers and used for later queries.
    /// 返回给调用方并用于后续查询的稳定任务标识。
    pub(crate) task_id: String,
    /// Lifecycle state: running, succeeded, or failed.
    /// 生命周期状态：running、succeeded 或 failed。
    pub(crate) state: String,
    /// Stable installer phase key.
    /// 稳定的安装阶段键。
    pub(crate) phase: String,
    /// Bytes transferred for the active download.
    /// 当前下载已传输的字节数。
    pub(crate) completed_bytes: Option<u64>,
    /// Total bytes advertised for the active download.
    /// 当前下载声明的总字节数。
    pub(crate) total_bytes: Option<u64>,
    /// Overall completion percentage.
    /// 整体完成百分比。
    pub(crate) overall_percent: Option<u8>,
    /// Bounded English error, present only for failed tasks.
    /// 有界英文错误，仅失败任务存在。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

impl LuaSkillsDebugInstallTask {
    /// Returns whether the task has finished.
    /// 返回任务是否已结束。
    fn is_terminal(&self) -> bool {
        self.state != "running"
    }
}

/// luaskills-debug state as the settings page renders it.
/// 设置页渲染的 luaskills-debug 状态。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LuaSkillsDebugStatusView {
    /// `major.minor` series that matches the embedded LuaSkills runtime.
    /// 与内嵌 LuaSkills 运行时匹配的 `major.minor` 系列。
    pub(crate) series: String,
    /// Release platform key of this host; absent when luaskills-debug does not support it.
    /// 本宿主的发布平台键；luaskills-debug 不支持时缺省。
    pub(crate) platform: Option<String>,
    /// Folder that holds every installed version.
    /// 存放所有已安装版本的目录。
    pub(crate) install_root: Option<String>,
    /// Installed version of the selected installation.
    /// 所选安装的已安装版本。
    pub(crate) version: Option<String>,
    /// Whether the selected installation matches the series.
    /// 所选安装是否匹配系列。
    pub(crate) compatible: bool,
    /// luaskills-debug executable.
    /// luaskills-debug 可执行文件。
    pub(crate) executable: Option<String>,
    /// Extracted tool folder.
    /// 解压后的工具目录。
    pub(crate) package_root: Option<String>,
    /// Private debug runtime root.
    /// 私有调试运行时根。
    pub(crate) runtime_root: Option<String>,
    /// Whether the Lua runtime packages are set up.
    /// Lua 运行时包是否已设置。
    pub(crate) runtime_ready: bool,
    /// Latest install task, running or finished.
    /// 最近的安装任务（进行中或已结束）。
    pub(crate) task: Option<LuaSkillsDebugInstallTask>,
}

/// Returns the process-local slot holding the latest install task.
/// 返回保存最近安装任务的进程内槽位。
fn install_task_slot() -> &'static Mutex<Option<LuaSkillsDebugInstallTask>> {
    static SLOT: OnceLock<Mutex<Option<LuaSkillsDebugInstallTask>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// Returns a copy of the latest install task.
/// 返回最近安装任务的副本。
fn latest_install_task() -> Result<Option<LuaSkillsDebugInstallTask>, String> {
    install_task_slot()
        .lock()
        .map(|slot| slot.clone())
        .map_err(|_| String::from("The luaskills-debug install task lock is poisoned."))
}

/// Applies one change to the task with `task_id` when it is still the latest task.
/// 当 `task_id` 对应的任务仍是最近任务时对其应用一次变更。
fn update_install_task(task_id: &str, change: impl FnOnce(&mut LuaSkillsDebugInstallTask)) {
    // The slot only holds plain data, so a poisoned lock still carries a consistent value.
    // 该槽位只保存普通数据，因此锁中毒后其值仍然一致。
    let mut slot = install_task_slot()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(task) = slot.as_mut()
        && task.task_id == task_id
    {
        change(task);
    }
}

/// Projects the installer status and the latest task into the settings-page view.
/// 将安装器状态与最近任务投影为设置页视图。
fn status_view(
    status: LuaSkillsDebugToolStatus,
    task: Option<LuaSkillsDebugInstallTask>,
) -> LuaSkillsDebugStatusView {
    let compatible = status.compatible();
    let path_text = |path: &std::path::Path| path.display().to_string();
    LuaSkillsDebugStatusView {
        series: status.series,
        platform: status.platform,
        install_root: status.install_root.as_deref().map(path_text),
        version: status.installed.as_ref().map(|item| item.version.clone()),
        compatible,
        executable: status
            .installed
            .as_ref()
            .map(|item| path_text(&item.executable)),
        package_root: status
            .installed
            .as_ref()
            .map(|item| path_text(&item.package_root)),
        runtime_root: status
            .installed
            .as_ref()
            .map(|item| path_text(&item.runtime_root)),
        runtime_ready: status
            .installed
            .as_ref()
            .is_some_and(|item| item.runtime_ready),
        task,
    }
}

/// Reads the luaskills-debug state and the latest install task without touching the network.
/// 在不访问网络的情况下读取 luaskills-debug 状态与最近安装任务。
pub(crate) async fn service_luaskills_debug_status() -> Result<LuaSkillsDebugStatusView, String> {
    let status = tokio::task::spawn_blocking(read_luaskills_debug_status)
        .await
        .map_err(|error| format!("The luaskills-debug status task failed: {error}"))??;
    Ok(status_view(status, latest_install_task()?))
}

/// Starts the luaskills-debug install, or returns the install that is already running.
/// 启动 luaskills-debug 安装，或返回已在运行的安装。
///
/// The task is detached, so it keeps running when the caller disappears; query it by id with
/// [`service_luaskills_debug_install_status`].
/// 任务是分离的，调用方消失后仍继续运行；通过 [`service_luaskills_debug_install_status`] 按标识查询。
pub(crate) fn service_start_luaskills_debug_install(
    state: SharedServiceBridgeState,
) -> Result<LuaSkillsDebugInstallTask, String> {
    let handle = service_luaskills_handle(&state)?;
    let task = {
        let mut slot = install_task_slot()
            .lock()
            .map_err(|_| String::from("The luaskills-debug install task lock is poisoned."))?;
        if let Some(task) = slot.as_ref()
            && !task.is_terminal()
        {
            return Ok(task.clone());
        }
        let task = LuaSkillsDebugInstallTask {
            task_id: Uuid::new_v4().to_string(),
            state: String::from("running"),
            phase: String::from("queued"),
            completed_bytes: None,
            total_bytes: None,
            overall_percent: Some(0),
            error: None,
        };
        *slot = Some(task.clone());
        task
    };
    let task_id = task.task_id.clone();
    spawn_service_task(async move {
        let worker_task_id = task_id.clone();
        let result = tokio::task::spawn_blocking(move || {
            install_luaskills_debug(&handle, &mut |progress| {
                update_install_task(&worker_task_id, |task| {
                    task.phase = progress.phase.clone();
                    task.completed_bytes = progress.completed_bytes;
                    task.total_bytes = progress.total_bytes;
                    task.overall_percent = progress.overall_percent;
                });
            })
        })
        .await
        .map_err(|error| format!("The luaskills-debug install task failed: {error}"))
        .and_then(|result| result);
        update_install_task(&task_id, |task| match result {
            Ok(_) => {
                task.state = String::from("succeeded");
                task.phase = String::from("completed");
                task.overall_percent = Some(100);
            }
            Err(error) => {
                task.state = String::from("failed");
                task.error = Some(error.chars().take(MAX_INSTALL_ERROR_CHARS).collect());
            }
        });
    });
    Ok(task)
}

/// Returns the install task with `task_id`.
/// 返回标识为 `task_id` 的安装任务。
///
/// # Errors
/// Returns an English error when `task_id` is not the latest install task.
/// `task_id` 不是最近安装任务时返回英文错误。
pub(crate) fn service_luaskills_debug_install_status(
    task_id: String,
) -> Result<LuaSkillsDebugInstallTask, String> {
    latest_install_task()?
        .filter(|task| task.task_id == task_id)
        .ok_or_else(|| format!("luaskills-debug install task `{task_id}` was not found."))
}

/// Reads and installs luaskills-debug for the `luaskills_debug` tool.
/// 为 `luaskills_debug` 工具读取并安装 luaskills-debug。
pub(crate) struct KernelLuaSkillsDebugHost {
    /// Process session registry; tells root sessions from spawned agents.
    /// 进程会话注册表；区分根会话与派生代理。
    registry: Arc<SessionRuntimeRegistry>,
    /// Service state; weak, because the state owns the runtime that owns this host.
    /// 服务状态；使用弱引用，因为该状态拥有持有本宿主的运行时。
    state: Weak<Mutex<ServiceBridgeState>>,
}

impl std::fmt::Debug for KernelLuaSkillsDebugHost {
    /// Renders a label without exposing service state.
    /// 渲染不暴露服务状态的标签。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("KernelLuaSkillsDebugHost")
            .finish_non_exhaustive()
    }
}

impl KernelLuaSkillsDebugHost {
    /// Builds the host over the process session registry and the service state.
    /// 基于进程会话注册表与服务状态构建宿主。
    pub(crate) fn new(
        registry: Arc<SessionRuntimeRegistry>,
        state: Weak<Mutex<ServiceBridgeState>>,
    ) -> Self {
        Self { registry, state }
    }
}

#[async_trait::async_trait]
impl NativeLuaSkillsDebugHost for KernelLuaSkillsDebugHost {
    async fn run(
        &self,
        session_id: SessionId,
        request: LuaSkillsDebugRequest,
    ) -> Result<LuaSkillsDebugOutcome, String> {
        if request == LuaSkillsDebugRequest::Install {
            if crate::tool_permission_host::root_session_of(&self.registry, session_id)?
                != session_id
            {
                return Err(String::from(
                    "Spawned agents cannot install luaskills-debug; only the user's own session can.",
                ));
            }
            let state = self
                .state
                .upgrade()
                .ok_or_else(|| String::from("The service is shutting down."))?;
            let task_id = service_start_luaskills_debug_install(state)?.task_id;
            loop {
                let task = service_luaskills_debug_install_status(task_id.clone())?;
                if task.is_terminal() {
                    if let Some(error) = task.error {
                        return Err(format!("luaskills-debug install failed: {error}"));
                    }
                    break;
                }
                tokio::time::sleep(INSTALL_WAIT_POLL_INTERVAL).await;
            }
        }
        let view = service_luaskills_debug_status().await?;
        let payload = json!({
            "series": view.series,
            "platform": view.platform,
            "installed": view.version.is_some(),
            "version": view.version,
            "compatible": view.compatible,
            "executable": view.executable,
            "package_root": view.package_root,
            "runtime_root": view.runtime_root,
            "runtime_ready": view.runtime_ready,
        });
        let text = serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?;
        Ok(LuaSkillsDebugOutcome { text, payload })
    }
}
