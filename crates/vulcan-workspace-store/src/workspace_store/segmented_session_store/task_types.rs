//! Child task metadata types for segmented session storage.
//! 分段会话存储的子任务元数据类型。

use serde::{Deserialize, Serialize};

use super::{SegmentMetadata, SegmentPolicy};

/// Agent profile source kind recorded for one subagent task session.
/// 为单个子代理任务会话记录的代理画像来源类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::workspace_store) enum SubagentAgentKind {
    /// The task inherits the main agent prompt and runtime baseline.
    /// 该任务继承主代理提示词与运行时基线。
    MainClone,
    /// The task uses a specific agent.md or persisted agent profile.
    /// 该任务使用特定 agent.md 或持久化代理画像。
    AgentMd,
    /// The task uses a dynamically constructed profile.
    /// 该任务使用动态构造的代理画像。
    Custom,
}

/// Agent profile metadata recorded for a child task session.
/// 为子任务会话记录的代理画像元数据。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::workspace_store) struct SubagentAgentProfile {
    /// Stable agent identifier when available.
    /// 可用时记录的稳定代理标识。
    pub agent_id: Option<String>,
    /// Human-readable agent name when available.
    /// 可用时记录的用户可读代理名称。
    pub agent_name: Option<String>,
    /// Normalized agent.md path or stable resource identifier when available.
    /// 可用时记录的规范化 agent.md 路径或稳定资源标识。
    pub agent_md_path: Option<String>,
    /// Prompt source label used to explain how the child agent was built.
    /// 用于说明子代理构建来源的提示词来源标签。
    pub prompt_source: Option<String>,
}

/// Spawn origin recorded in a child task metadata event.
/// 子任务元数据事件中记录的派生来源。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::workspace_store) struct SubagentSpawnedBy {
    /// Parent turn identifier when known.
    /// 已知时记录的父会话轮次标识。
    pub parent_turn_id: Option<String>,
    /// Parent event sequence that spawned the task when known.
    /// 已知时记录的派生该任务的父会话事件序号。
    pub parent_event_seq: Option<u64>,
    /// Tool call identifier that spawned the task when known.
    /// 已知时记录的派生该任务的工具调用标识。
    pub tool_call_id: Option<String>,
}

/// Lightweight task mirror stored in the parent session metadata.
/// 保存在父会话元数据中的轻量任务镜像。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::workspace_store) struct SubagentTaskMirror {
    /// Child task session identifier.
    /// 子任务会话标识。
    pub session_id: String,
    /// Child task directory relative to the parent session directory.
    /// 相对父会话目录的子任务目录。
    pub dir: String,
    /// Active child task segment path relative to the parent session directory.
    /// 相对父会话目录的活动子任务分段路径。
    pub active_segment: String,
    /// Child metadata file path relative to the parent session directory.
    /// 相对父会话目录的子任务元数据文件路径。
    pub metadata_file: String,
    /// Agent source kind for this child task.
    /// 当前子任务的代理来源类型。
    pub agent_kind: SubagentAgentKind,
    /// Agent profile details mirrored for list and navigation views.
    /// 为列表与导航视图镜像的代理画像详情。
    pub agent_profile: SubagentAgentProfile,
    /// Parent event sequence where the task starts when known.
    /// 已知时记录的子任务开始父事件序号。
    pub parent_start_seq: Option<u64>,
    /// Parent event sequence where the task ends when known.
    /// 已知时记录的子任务结束父事件序号。
    pub parent_end_seq: Option<u64>,
    /// Current task status label.
    /// 当前任务状态标签。
    pub status: String,
    /// Total child segment line count mirrored for quick inspection.
    /// 为快速检查镜像的子任务分段总行数。
    pub line_count: u64,
    /// Creation timestamp for this child task.
    /// 当前子任务的创建时间戳。
    pub created_at: String,
    /// Last update timestamp for this child task.
    /// 当前子任务的最后更新时间戳。
    pub updated_at: String,
}

/// Display-only marker describing where one forked session came from.
/// 描述分叉会话来源位置的纯展示标记。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::workspace_store) struct SegmentedSessionForkSource {
    /// Source project tag used to locate the original conversation.
    /// 用于定位原始会话的源项目标签。
    pub source_project_tag: String,
    /// Source session identifier for navigation.
    /// 用于导航的源会话标识。
    pub source_session_id: String,
    /// Source conversation title captured at fork time.
    /// 分叉时捕获的源会话标题。
    pub source_title: String,
    /// Last retained message id at the fork boundary.
    /// 分叉边界处最后保留的消息标识。
    pub anchor_message_id: Option<String>,
    /// Fork creation timestamp.
    /// 分叉创建时间戳。
    pub created_at: String,
}

/// Metadata document stored in each main segmented session directory.
/// 存储在每个主分段会话目录中的元数据文档。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::workspace_store) struct SegmentedSessionMetadata {
    /// Metadata schema version.
    /// 元数据结构版本。
    pub schema_version: u32,
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
    /// Stable project tag used by the directory layout.
    /// 目录布局使用的稳定项目标签。
    pub project_tag: String,
    /// Creation timestamp.
    /// 创建时间戳。
    pub created_at: String,
    /// Last update timestamp.
    /// 最后更新时间戳。
    pub updated_at: String,
    /// Archive timestamp when archived.
    /// 归档时记录的归档时间戳。
    pub archived_at: Option<String>,
    /// Project identifier associated with the session.
    /// 该会话关联的项目标识。
    pub project_id: Option<String>,
    /// Current display title.
    /// 当前展示标题。
    pub title: Option<String>,
    /// Source label for the current title.
    /// 当前标题的来源标签。
    pub title_source: Option<String>,
    /// Canonical source marker for this storage layout.
    /// 当前存储布局的规范来源标记。
    pub canonical_source: String,
    /// Active segment file name relative to the session directory.
    /// 相对会话目录的活动分段文件名。
    pub active_segment: String,
    /// Child task directory name relative to the session directory.
    /// 相对会话目录的子任务目录名。
    pub task_dir: String,
    /// Next global sequence number for the main session.
    /// 主会话下一条全局事件序号。
    pub next_global_seq: u64,
    /// Number of canonical transcript messages already appended after the latest rollback boundary.
    /// 最新回滚边界之后已经追加的规范转录消息数量。
    #[serde(default)]
    pub message_count: u64,
    /// Identifier of the last canonical transcript message appended after the latest rollback boundary.
    /// 最新回滚边界之后最后一条已追加规范转录消息的标识。
    #[serde(default)]
    pub last_message_id: Option<String>,
    /// Serialized fingerprint of the last canonical transcript message for tail-boundary validation.
    /// 用于尾部边界校验的最后一条规范转录消息序列化指纹。
    #[serde(default)]
    pub last_message_fingerprint: Option<String>,
    /// Latest rollback marker sequence for fast display-window and append checks.
    /// 最新回滚标记序号，用于快速显示窗口与追加检查。
    pub latest_rollback_seq: Option<u64>,
    /// Segment rollover policy.
    /// 分段滚动策略。
    pub segment_policy: SegmentPolicy,
    /// Ordered segment metadata entries.
    /// 按顺序记录的分段元数据集合。
    pub segments: Vec<SegmentMetadata>,
    /// Lightweight child task mirrors.
    /// 子任务轻量镜像集合。
    pub tasks: Vec<SubagentTaskMirror>,
    /// Display-only source marker for forked sessions.
    /// 分叉会话的纯展示来源标记。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork_source: Option<SegmentedSessionForkSource>,
}

/// Metadata document stored in each child task session directory.
/// 存储在每个子任务会话目录中的元数据文档。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::workspace_store) struct SegmentedTaskSessionMetadata {
    /// Metadata schema version.
    /// 元数据结构版本。
    pub schema_version: u32,
    /// Child task session identifier.
    /// 子任务会话标识。
    pub task_session_id: String,
    /// Parent main session identifier.
    /// 父主会话标识。
    pub parent_session_id: String,
    /// Parent project tag used by the directory layout.
    /// 目录布局中使用的父项目标签。
    pub parent_project_tag: String,
    /// Creation timestamp.
    /// 创建时间戳。
    pub created_at: String,
    /// Last update timestamp.
    /// 最后更新时间戳。
    pub updated_at: String,
    /// Current task status label.
    /// 当前任务状态标签。
    pub status: String,
    /// Agent source kind for this child task.
    /// 当前子任务的代理来源类型。
    pub agent_kind: SubagentAgentKind,
    /// Agent profile details for this child task.
    /// 当前子任务的代理画像详情。
    pub agent_profile: SubagentAgentProfile,
    /// Parent-side spawn information.
    /// 父侧派生信息。
    pub spawned_by: SubagentSpawnedBy,
    /// Active task segment file name relative to the task directory.
    /// 相对子任务目录的活动分段文件名。
    pub active_segment: String,
    /// Next task-local sequence number.
    /// 子任务局部下一条事件序号。
    pub next_local_seq: u64,
    /// Segment rollover policy.
    /// 分段滚动策略。
    pub segment_policy: SegmentPolicy,
    /// Ordered task segment metadata entries.
    /// 按顺序记录的子任务分段元数据集合。
    pub segments: Vec<SegmentMetadata>,
}
