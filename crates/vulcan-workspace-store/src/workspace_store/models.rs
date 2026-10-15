use super::*;

/// Stable in-memory snapshot hydrated from the host-owned workspace database.
/// 从宿主持有的工作台数据库中还原出的稳定内存快照。
#[derive(Debug, Clone)]
pub struct WorkspaceSnapshot {
    /// Visible project nodes shown in the task tree.
    /// 展示在任务树中的可见项目节点集合。
    pub task_projects: Vec<TaskProjectSummary>,
    /// Archived project nodes retained outside the visible tree for later restoration.
    /// 保留在可见树之外、供后续恢复使用的归档项目节点集合。
    pub archived_task_projects: Vec<TaskProjectSummary>,
    /// Visible sessions shown by the workbench.
    /// 展示在工作台中的可见会话集合。
    pub sessions: Vec<SessionSummary>,
    /// Archived sessions retained outside the visible workbench for later restoration.
    /// 保留在可见工作台之外、供后续恢复使用的归档会话集合。
    pub archived_sessions: Vec<SessionSummary>,
    /// Process-local execution adapters for active sessions; V1 loading always initializes this map empty and never persists it.
    /// 活动会话的进程内执行适配状态；V1 加载始终把该集合初始化为空，且绝不持久化它。
    pub session_runtime_state: BTreeMap<SessionId, SessionRuntimeStateEntry>,
    /// Process-local adapters retained only while an archived session remains in the current process; restart restoration must use V1 facts.
    /// 仅在当前进程内保留的归档会话适配状态；重启恢复必须使用 V1 事实。
    pub archived_session_runtime_state: BTreeMap<SessionId, SessionRuntimeStateEntry>,
    /// Fresh execution-phase defaults reconstructed from the loaded sessions.
    /// 根据已加载会话重建出的全新执行阶段默认值集合。
    pub session_run_phases: BTreeMap<SessionId, SessionRunPhase>,
    /// Expanded project identifiers restored for the task tree.
    /// 为任务树恢复出的项目展开标识集合。
    pub expanded_task_projects: BTreeSet<String>,
    /// Selected session identifier restored after startup.
    /// 启动完成后恢复出的当前选中会话标识。
    pub selected_session: SessionId,
    /// Session identifiers whose transcript/runtime payloads were actually loaded into memory.
    /// 已经真实加载到内存中的转录与运行时载荷会话标识集合。
    pub hydrated_session_ids: BTreeSet<SessionId>,
}

/// Raw project row decoded from the host_runtime SQLite query payload.
/// 从宿主运行时 SQLite 查询载荷中解码出的原始项目行。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct StoredProjectRow {
    /// Stable project identifier persisted in SQLite.
    /// 持久化到 SQLite 的稳定项目标识。
    pub(super) id: String,
    /// Project display name stored in SQLite.
    /// 存储在 SQLite 中的项目展示名称。
    pub(super) name: String,
    /// Absolute root path stored in SQLite.
    /// 存储在 SQLite 中的绝对根目录路径。
    pub(super) root_path: String,
    /// SQLite numeric pinned flag.
    /// SQLite 数值型置顶标记。
    pub(super) pinned: i64,
    /// Stable section order value.
    /// 稳定的分区顺序值。
    pub(super) sort_order: i64,
    /// Optional project icon pack stored in SQLite.
    /// 存储在 SQLite 中的可选项目图标包。
    pub(super) icon_pack: Option<String>,
    /// Optional project icon key stored in SQLite.
    /// 存储在 SQLite 中的可选项目图标键。
    pub(super) icon_key: Option<String>,
    /// SQLite numeric archived flag.
    /// SQLite 数值型归档标记。
    pub(super) archived: i64,
}

/// Raw application metadata row decoded from the host_runtime SQLite query payload.
/// 从宿主运行时 SQLite 查询载荷中解码出的原始应用元数据行。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct StoredAppMetadataRow {
    /// Persisted metadata value.
    /// 持久化元数据值。
    pub(super) value: String,
}
