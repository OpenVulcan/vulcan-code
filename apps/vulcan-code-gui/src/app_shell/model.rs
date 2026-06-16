//! Domain models for the AI coding desktop scaffold.
//! AI 编程桌面骨架的领域模型定义。

use crate::provider_config::{AiToolKind, PersistedAiToolConfiguration};
use std::{collections::BTreeSet, fmt};
pub use vulcan_workbench_model::{
    ActivityEntry, ActivityLevel, PENDING_PROMPT_QUEUE_LIMIT, PendingPromptModelReference,
    PendingPromptPanelEntry, PendingPromptPanelEntryState, PendingPromptRuntimeOptions,
    PendingPromptWebSearchOverride, PendingSessionPromptEntry, PendingSessionPromptSource,
    SelectedSubagentTaskPanel, SessionAddonNamespace, SessionAddonState, SessionCompactionEntry,
    SessionComposerDraftState, SessionId, SessionMessageEntry, SessionMessageKind,
    SessionMessageRecordKind, SessionRunPhase, SessionRuntimeControls, SessionRuntimeStateEntry,
    SessionState, SessionSummary, SessionTitleSource, SessionTurnStats, SessionTurnStatsStatus,
    SessionWorkMode, SubagentTaskPanelState, SubagentTaskRunState, TaskProjectSummary,
    VmmProfileSyncState, VmmWritebackOutboxEntry, build_session_record_message_id,
    collect_running_subagent_task_states_for_tree, merge_subagent_task_panel_cache,
    normalize_subagent_task_panel_state, pending_prompt_dispatch_index,
    select_subagent_navigation_panel_states, subagent_task_panel_is_openable,
    subagent_task_panel_state_detail_from_segmented_task_session_store,
    subagent_task_panel_state_has_recoverable_conversation,
    subagent_task_panel_states_from_segmented_task_session_store,
    task_session_arguments_from_tool_metadata, task_session_display_title_from_payload,
    task_session_display_title_from_tool_metadata,
    task_session_execution_status_from_tool_metadata, task_session_internal_id_from_payload,
    task_session_payload_from_tool_metadata, task_session_public_id_from_payload,
};

/// Execution modes for AI coding tasks.
/// AI 编码任务的执行模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Executes implementation tasks directly with the full coding tool surface.
    /// 使用完整编码工具面直接执行实现任务。
    Build,
    /// Generates one plan before implementation with a read-only and analysis-biased tool surface.
    /// 在实现前生成方案，并使用只读与分析偏向工具面。
    Plan,
}

impl ExecutionMode {
    /// All execution modes exposed in the radio group.
    /// 单选组件中暴露的全部执行模式。
    pub const ALL: [Self; 2] = [Self::Build, Self::Plan];

    /// Returns the stable key persisted in session addon state.
    /// 返回持久化到会话附属状态中的稳定键。
    pub fn as_key(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Plan => "plan",
        }
    }

    /// Parses one persisted session work-mode key.
    /// 解析一个已持久化的会话工作模式键。
    ///
    /// # Parameters
    /// 参数
    /// `key` is the persisted work-mode key.
    /// `key` 是已持久化的工作模式键。
    ///
    /// # Returns
    /// 返回值
    /// The matching execution mode when the key is known.
    /// 键已知时返回对应执行模式。
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "build" => Some(Self::Build),
            "plan" => Some(Self::Plan),
            _ => None,
        }
    }
}

impl fmt::Display for ExecutionMode {
    /// Renders a user-facing execution mode label.
    /// 渲染面向用户的执行模式标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Build => "Build",
            Self::Plan => "Plan",
        };

        formatter.write_str(label)
    }
}

/// Save state shown by the quick AI capability panel footer.
/// 快捷 AI 能力面板底部展示的保存状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiCapabilityPanelSaveState {
    /// No in-flight save is running.
    /// 当前没有正在执行的保存。
    Idle,
    /// A save task is running and the panel controls should be locked.
    /// 保存任务正在执行，面板控件应被锁定。
    Saving,
    /// The latest save has completed successfully.
    /// 最近一次保存已成功完成。
    Saved,
}

/// Snapshot used to compare and restore one quick AI capability panel draft.
/// 用于比较与恢复快捷 AI 能力面板草稿的一份快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiCapabilityPanelDraftSnapshot {
    /// Persistable AI capability configuration including current quick-toggle preferences.
    /// 包含当前快捷开关偏好的可持久化 AI 能力配置。
    pub configuration: PersistedAiToolConfiguration,
    /// Workbench quick-toggle disabled capability set captured with the configuration.
    /// 与配置一起捕获的工作台快捷开关关闭能力集合。
    pub disabled_capability_kinds: BTreeSet<AiToolKind>,
    /// Current workbench web-search override captured with quick-toggle state.
    /// 与快捷开关状态一起捕获的当前工作台网页搜索覆盖模式。
    pub web_search_override: PendingPromptWebSearchOverride,
    /// Current specific web-search provider override captured with quick-toggle state.
    /// 与快捷开关状态一起捕获的当前指定网页搜索供应商覆盖。
    pub web_search_provider_id: Option<String>,
}

/// Fingerprint for the save task started by the quick AI capability panel.
/// 快捷 AI 能力面板启动的保存任务指纹。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiCapabilityPanelPendingSave {
    /// Session that owns the in-flight quick-panel save.
    /// 拥有当前快捷面板保存任务的会话。
    pub session_id: SessionId,
    /// Request identifier attached to the matching persistence callback.
    /// 附着到匹配持久化回调上的请求标识。
    pub request_id: String,
    /// Stable hash of the saved AI capability configuration.
    /// 已保存 AI 能力配置的稳定哈希。
    pub hash: String,
}

/// Local AI capability edit fingerprint used to guard against stale hydration snapshots.
/// 用于防止旧水合快照覆盖本地 AI 能力编辑的本地指纹。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiToolConfigurationLocalGuard {
    /// Local runtime-state update timestamp captured when the edit was staged.
    /// 暂存编辑时捕获的本地运行态更新时间戳。
    pub updated_at: String,
    /// Stable hash of the staged AI capability configuration.
    /// 已暂存 AI 能力配置的稳定哈希。
    pub hash: String,
}

/// Navigation pane descriptors used by the pane grid.
/// 供分栏网格使用的面板描述类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneKind {
    /// Project navigation and sessions.
    /// 项目导航与会话区域。
    Navigator,
    /// Central prompt and editor workbench.
    /// 中央提示词与编辑器工作台。
    Workbench,
    /// Runtime controls and diagnostics.
    /// 运行控制与诊断区域。
    Inspector,
}

impl PaneKind {
    /// Returns the pane title shown in the title bar.
    /// 返回标题栏中展示的面板标题。
    pub const fn title(self) -> &'static str {
        match self {
            Self::Navigator => "Project Navigator",
            Self::Workbench => "Agent Workbench",
            Self::Inspector => "Runtime Inspector",
        }
    }

    /// Returns the pane subtitle shown in the title bar.
    /// 返回标题栏中展示的面板副标题。
    pub const fn subtitle(self) -> &'static str {
        match self {
            Self::Navigator => "workspace + sessions",
            Self::Workbench => "prompt + patch editor",
            Self::Inspector => "controls + telemetry",
        }
    }
}
