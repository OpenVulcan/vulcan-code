use super::*;

/// Summarizes one VMM memory bundle or node group.
/// 汇总单个 VMM 记忆集合或节点组。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MemorySummary {
    /// Stable memory identifier.
    /// 稳定记忆标识。
    pub id: String,
    /// Memory display title.
    /// 记忆显示标题。
    pub title: String,
    /// Memory category.
    /// 记忆分类。
    pub category: String,
    /// Memory state summary.
    /// 记忆状态摘要。
    pub state: String,
}

/// Full VMM memory page snapshot used by the WebUI.
/// WebUI 使用的完整 VMM 记忆页面快照。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmMemorySnapshot {
    /// Effective VMM configuration for the selected session.
    /// 当前选中会话的最终 VMM 配置。
    pub effective: VmmEffectiveSummary,
    /// Global VMM editable draft.
    /// 全局 VMM 可编辑草稿。
    pub global_draft: VmmConfigDraftSummary,
    /// Project VMM editable draft for the selected project.
    /// 当前选中项目的 VMM 可编辑草稿。
    pub project_draft: VmmConfigDraftSummary,
    /// Available project scopes for project-level VMM settings.
    /// 可用于项目级 VMM 设置的项目作用域。
    pub project_scopes: Vec<VmmProjectScopeSummary>,
    /// Current selected project scope id when available.
    /// 当前可用的选中项目作用域标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_project_id: Option<String>,
    /// Current selected conversation id when available.
    /// 当前可用的选中会话标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_conversation_id: Option<String>,
    /// Runtime side-effect summary for VMM hooks.
    /// VMM hook 运行时副作用摘要。
    pub runtime: VmmRuntimeSummary,
    /// Management sections available in the VMM page.
    /// VMM 页面可用的管理分区。
    pub sections: Vec<VmmManagementSectionSummary>,
    /// Human-readable notice for the current VMM page.
    /// 当前 VMM 页面的可读提示。
    pub notice: String,
}

/// Effective VMM configuration summary.
/// VMM 最终配置摘要。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmEffectiveSummary {
    /// Whether memory flow is enabled by configuration.
    /// 配置是否启用记忆链路。
    pub enabled: bool,
    /// Configured gRPC target.
    /// 已配置的 gRPC 目标。
    pub grpc_target: String,
    /// Whether gRPC target is present.
    /// 是否存在 gRPC 目标。
    pub grpc_configured: bool,
    /// Effective user id.
    /// 最终用户标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<u64>,
    /// Effective project id.
    /// 最终项目标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<u64>,
    /// Effective memory flow diagnostic key.
    /// 最终记忆链路诊断键。
    pub diagnostic: String,
    /// Effective automatic precheck diagnostic key.
    /// 最终自动前置召回诊断键。
    pub precheck_diagnostic: String,
    /// Effective completed-turn memory writeback diagnostic key.
    /// 最终已完成回合记忆写回诊断键。
    pub writeback_diagnostic: String,
    /// Whether VMM AI-only tools are exposed.
    /// 是否暴露 VMM AI 专属工具。
    pub ai_tools_enabled: bool,
    /// Whether automatic precheck recall is enabled.
    /// 是否启用自动前置召回。
    pub precheck_enabled: bool,
    /// Whether completed-turn memory writeback is enabled.
    /// 是否启用已完成回合记忆写回。
    pub memory_writeback_enabled: bool,
    /// Transient recall TTL in turns.
    /// 临时召回持续回合数。
    pub implicit_memory_turns: u32,
    /// Profile refresh interval in turns.
    /// 画像刷新间隔回合数。
    pub profile_refresh_turns: u32,
    /// Per-target profile node limit.
    /// 每目标画像节点上限。
    pub profile_node_limit: u32,
    /// Context injection tag.
    /// 上下文注入标签。
    pub injection_tag: String,
    /// Context injection description.
    /// 上下文注入说明。
    pub injection_description: String,
}

/// Editable VMM configuration draft summary.
/// 可编辑 VMM 配置草稿摘要。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmConfigDraftSummary {
    /// Whether this layer has explicit overrides.
    /// 当前层是否存在显式覆盖。
    pub has_override: bool,
    /// Whether memory is enabled on this layer.
    /// 当前层是否启用记忆。
    pub enabled: bool,
    /// Editable gRPC target.
    /// 可编辑 gRPC 目标。
    pub grpc_target: String,
    /// Editable gRPC API key.
    /// 可编辑 gRPC API key。
    pub grpc_api_key: String,
    /// Editable user id text.
    /// 可编辑用户标识文本。
    pub user_id: String,
    /// Editable project id text.
    /// 可编辑项目标识文本。
    pub project_id: String,
    /// Editable receive timeout text.
    /// 可编辑请求超时文本。
    pub receive_timeout_ms: String,
    /// Editable handshake timeout text.
    /// 可编辑握手超时文本。
    pub handshake_timeout_ms: String,
    /// Editable max receive bytes text.
    /// 可编辑最大接收字节数文本。
    pub max_receive_message_bytes: String,
    /// Editable gRPC keepalive interval text.
    /// 可编辑 gRPC keepalive 间隔文本。
    pub grpc_keepalive_time_ms: String,
    /// Editable gRPC keepalive timeout text.
    /// 可编辑 gRPC keepalive 超时文本。
    pub grpc_keepalive_timeout_ms: String,
    /// Whether idle keepalive is enabled.
    /// 是否启用空闲 keepalive。
    pub grpc_keepalive_permit_without_calls: bool,
    /// Editable transient recall TTL text.
    /// 可编辑临时召回 TTL 文本。
    pub implicit_memory_turns: String,
    /// Editable profile refresh interval text.
    /// 可编辑画像刷新间隔文本。
    pub profile_refresh_turns: String,
    /// Editable per-target profile node limit text.
    /// 可编辑每目标画像节点上限文本。
    pub profile_node_limit: String,
    /// Whether VMM AI-only tools are exposed.
    /// 是否暴露 VMM AI 专属工具。
    pub ai_tools_enabled: bool,
    /// Whether automatic precheck recall is enabled.
    /// 是否启用自动前置召回。
    pub precheck_enabled: bool,
    /// Precheck policy mode for this layer.
    /// 当前层的前置召回策略模式。
    pub precheck_mode: String,
    /// Whether completed-turn memory writeback is enabled.
    /// 是否启用已完成回合记忆写回。
    pub memory_writeback_enabled: bool,
    /// Completed-turn writeback policy mode for this layer.
    /// 当前层的已完成回合写回策略模式。
    pub memory_writeback_mode: String,
    /// Editable injection tag.
    /// 可编辑注入标签。
    pub injection_tag: String,
    /// Editable injection description.
    /// 可编辑注入说明。
    pub injection_description: String,
}

/// Project scope row available to VMM project settings.
/// VMM 项目设置可用的项目作用域行。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmProjectScopeSummary {
    /// Stable task project id.
    /// 稳定任务项目标识。
    pub project_id: String,
    /// Project display name.
    /// 项目显示名称。
    pub name: String,
    /// Absolute workspace path.
    /// 绝对工作区路径。
    pub path: String,
    /// Whether this project has explicit VMM overrides.
    /// 该项目是否存在显式 VMM 覆盖。
    pub has_override: bool,
}

/// Runtime state summary for VMM hooks.
/// VMM hook 运行态摘要。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmRuntimeSummary {
    /// Pending VMM writeback outbox count.
    /// 待处理 VMM 写回 outbox 数量。
    pub outbox_count: usize,
    /// Profile sync node count.
    /// 画像同步节点数量。
    pub profile_sync_count: usize,
    /// Completed turn count recorded by profile sync.
    /// 画像同步记录的已完成回合数。
    pub completed_turns: u32,
    /// Latest profile update timestamp.
    /// 最近画像更新时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Profile base content preview.
    /// 画像基点内容预览。
    pub profile_preview: String,
}

/// VMM management section descriptor.
/// VMM 管理分区描述。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmManagementSectionSummary {
    /// Stable section id.
    /// 稳定分区标识。
    pub id: String,
    /// OpenCode-style section key.
    /// OpenCode 风格分区键。
    pub key: String,
    /// User-facing section label.
    /// 面向用户的分区标签。
    pub label: String,
    /// Compact section description.
    /// 紧凑分区说明。
    pub description: String,
}

/// Request body for one WebUI VMM management action.
/// WebUI 单次 VMM 管理动作的请求体。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmMemoryActionRequest {
    /// Stable action id.
    /// 稳定动作标识。
    pub action: String,
    /// Scope key, either global or project.
    /// 作用域键，可为 global 或 project。
    pub scope: String,
    /// Optional task project id used by project scope.
    /// 项目作用域使用的可选任务项目标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Exact persisted conversation id required by session-scoped actions.
    /// 会话级动作必须提供的准确已持久化会话标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    /// Action-specific JSON payload.
    /// 动作专用 JSON 载荷。
    #[serde(default)]
    pub payload: serde_json::Value,
}

/// Result body returned by one WebUI VMM management action.
/// WebUI 单次 VMM 管理动作返回的结果体。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmMemoryActionResult {
    /// Stable action id.
    /// 稳定动作标识。
    pub action: String,
    /// Compact user-facing summary.
    /// 紧凑用户可见摘要。
    pub summary: String,
    /// Structured response kind.
    /// 结构化响应类型。
    pub kind: String,
    /// Raw request preview JSON.
    /// 原始请求预览 JSON。
    pub request: serde_json::Value,
    /// Raw response preview JSON.
    /// 原始响应预览 JSON。
    pub response: serde_json::Value,
}

/// Request body for saving a WebUI VMM configuration draft.
/// 保存 WebUI VMM 配置草稿的请求体。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmConfigSaveRequest {
    /// Scope key, either global or project.
    /// 作用域键，可为 global 或 project。
    pub scope: String,
    /// Optional task project id used by project scope.
    /// 项目作用域使用的可选任务项目标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Draft data to persist.
    /// 需要持久化的草稿数据。
    pub draft: VmmConfigDraftSummary,
}
