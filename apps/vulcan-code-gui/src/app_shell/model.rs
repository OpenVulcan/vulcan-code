//! Domain models for the AI coding desktop scaffold.
//! AI 编程桌面骨架的领域模型定义。

use std::fmt;
pub use vulcan_workbench_model::{
    ActivityEntry, ActivityLevel, PENDING_PROMPT_QUEUE_LIMIT, PendingPromptModelReference,
    PendingPromptPanelEntry, PendingPromptPanelEntryState, PendingPromptRuntimeOptions,
    PendingPromptWebSearchOverride, PendingSessionPromptEntry, PendingSessionPromptSource,
    SelectedSubagentTaskPanel, SessionCompactionEntry, SessionComposerDraftState, SessionId,
    SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind, SessionRunPhase,
    SessionRuntimeStateEntry, SessionState, SessionSummary, SessionTitleSource, SessionTurnStats,
    SessionTurnStatsStatus, SubagentTaskPanelState, SubagentTaskRunState, TaskProjectSummary,
    VmmProfileSyncState, VmmWritebackOutboxEntry, build_delegate_completion_followup_preview,
    build_delegate_completion_followup_prompt, build_session_record_message_id,
    collect_running_subagent_task_states_for_tree, merge_subagent_task_panel_cache,
    normalize_subagent_task_panel_state, pending_prompt_dispatch_index,
    select_subagent_navigation_panel_states, subagent_task_panel_is_openable,
    subagent_task_panel_state_has_recoverable_conversation,
    subagent_task_panel_states_from_task_session_store, task_session_arguments_from_tool_metadata,
    task_session_display_title_from_payload, task_session_display_title_from_tool_metadata,
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
