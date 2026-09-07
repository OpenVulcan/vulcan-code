//! VMM profile protocol models.
//! VMM 画像协议模型。

use serde::{Deserialize, Serialize};

/// Profile target scope used by profile management calls.
/// 画像管理调用使用的画像目标作用域。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProfileTarget {
    /// No explicit target.
    /// 未指定目标。
    #[default]
    ProfileTargetUnspecified,
    /// User profile target.
    /// 用户画像目标。
    ProfileTargetUser,
    /// Project profile target.
    /// 项目画像目标。
    ProfileTargetProject,
    /// Team profile target.
    /// 团队画像目标。
    ProfileTargetTeam,
    /// Space profile target.
    /// 空间画像目标。
    ProfileTargetSpace,
    /// All supported profile targets for read-only profile node queries.
    /// 用于只读画像节点查询的全部支持目标。
    ProfileTargetAll,
}

/// Source kind of one durable VMM profile node.
/// 一条长期 VMM 画像节点的来源类型。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProfileNodeSourceKind {
    /// No explicit source kind.
    /// 未指定来源类型。
    #[default]
    ProfileNodeSourceKindUnspecified,
    /// Node extracted from a completed turn.
    /// 从已完成回合提取出的节点。
    ProfileNodeSourceKindTurnExtract,
    /// Node created from a manual profile instruction.
    /// 由手工画像指令创建的节点。
    ProfileNodeSourceKindManualInstruction,
    /// Node seeded by system data.
    /// 由系统种子数据创建的节点。
    ProfileNodeSourceKindSystemSeed,
    /// Node retained after user deletion.
    /// 用户删除后保留的节点。
    ProfileNodeSourceKindRetainedAfterUserDelete,
}

/// One active atomic profile node returned by VMM.
/// VMM 返回的一条 active 原子化画像节点。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileNodeEntry {
    /// Durable profile node identifier.
    /// 长期画像节点标识。
    pub profile_node_id: u64,
    /// Target scope for the node.
    /// 节点所属目标作用域。
    pub target: ProfileTarget,
    /// Bound user/project/team/space identifier.
    /// 绑定的 user/project/team/space 标识。
    pub bind_id: u64,
    /// Atomic profile content.
    /// 原子化画像内容。
    #[serde(default)]
    pub content: String,
    /// Priority label returned by VMM.
    /// VMM 返回的优先级标签。
    #[serde(default)]
    pub priority: String,
    /// Level label returned by VMM.
    /// VMM 返回的层级标签。
    #[serde(default)]
    pub level: String,
    /// Refresh weight returned by VMM.
    /// VMM 返回的刷新权重。
    pub refresh_weight: u32,
    /// Human-readable profile date.
    /// 人类可读画像日期。
    #[serde(default)]
    pub profile_date: String,
    /// Expiration timestamp when any.
    /// 可选到期时间戳。
    pub expires_timestamp: i64,
    /// Reason explaining the selected level.
    /// 解释所选层级的原因。
    #[serde(default)]
    pub level_reason: String,
    /// Source kind for this node.
    /// 该节点的来源类型。
    pub source_kind: ProfileNodeSourceKind,
    /// Source id such as turn id or instruction id.
    /// 来源标识，例如 turn id 或 instruction id。
    pub source_id: u64,
}

/// One retired profile node returned after manual instruction review.
/// 手工画像指令评审后返回的一条退役画像节点。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetiredProfileNodeEntry {
    /// Durable profile node identifier.
    /// 长期画像节点标识。
    pub profile_node_id: u64,
    /// Review reason attached to the retirement.
    /// 退役附带的评审原因。
    #[serde(default)]
    pub reason: String,
}

/// Request body for VMM GetProfileNodes.
/// VMM GetProfileNodes 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetProfileNodesRequest {
    /// Optional local source expectation, excluded from the external gRPC request.
    /// 可选本地来源预期，不进入外置 gRPC 请求。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_read_source: Option<crate::VmmMemoryReadSource>,
    /// Requested profile target.
    /// 请求的画像目标。
    pub target: ProfileTarget,
    /// Numeric VMM user identifier.
    /// VMM 数字用户标识。
    pub user_id: u64,
    /// Numeric VMM project identifier.
    /// VMM 数字项目标识。
    pub project_id: u64,
    /// Stable VulcanCode project identity used only by the process-local native backend.
    /// 仅供进程内原生后端使用的稳定 VulcanCode 项目身份。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host_project_id: String,
    /// Maximum node count to return.
    /// 最大返回节点数量。
    pub limit: u32,
}

/// Response body for VMM GetProfileNodes.
/// VMM GetProfileNodes 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetProfileNodesResponse {
    /// Active profile nodes returned by VMM.
    /// VMM 返回的 active 画像节点集合。
    #[serde(default)]
    pub nodes: Vec<ProfileNodeEntry>,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}

/// Profile bundle shape requested from VMM.
/// 向 VMM 请求的画像 bundle 形态。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProfileBundleMode {
    /// Let the server choose the default bundle shape.
    /// 由服务端选择默认 bundle 形态。
    #[default]
    ProfileBundleModeUnspecified,
    /// Request one combined prompt text.
    /// 请求一段组合提示词文本。
    ProfileBundleModeFull,
    /// Request split TEAM/SPACE/PROJECT/USER texts.
    /// 请求拆分后的 TEAM/SPACE/PROJECT/USER 文本。
    ProfileBundleModeSplit,
}

/// Request body for VMM GetProfileBundle.
/// VMM GetProfileBundle 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetProfileBundleRequest {
    /// Numeric VMM user identifier.
    /// VMM 数字用户标识。
    pub user_id: u64,
    /// Numeric VMM project identifier.
    /// VMM 数字项目标识。
    pub project_id: u64,
    /// Stable VulcanCode project identity used only by the process-local native backend.
    /// 仅供进程内原生后端使用的稳定 VulcanCode 项目身份。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host_project_id: String,
    /// Requested bundle shape.
    /// 请求的 bundle 形态。
    pub mode: ProfileBundleMode,
    /// Whether explanation text should be included.
    /// 是否包含解释文本。
    pub include_explanation: Option<bool>,
}

/// Response body for VMM GetProfileBundle.
/// VMM GetProfileBundle 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetProfileBundleResponse {
    /// Actual bundle shape returned by VMM.
    /// VMM 实际返回的 bundle 形态。
    pub mode: ProfileBundleMode,
    /// Whether explanation text is included.
    /// 是否包含解释文本。
    pub include_explanation: bool,
    /// Optional explanation text.
    /// 可选解释文本。
    #[serde(default)]
    pub explanation_text: String,
    /// Environment priority text returned by VMM.
    /// VMM 返回的环境优先级文本。
    #[serde(default)]
    pub environment_priority_text: String,
    /// Combined prompt bundle text.
    /// 组合提示词 bundle 文本。
    #[serde(default)]
    pub combined_text: String,
    /// Team profile section text.
    /// 团队画像段落文本。
    #[serde(default)]
    pub team_profile: String,
    /// Space profile section text.
    /// 空间画像段落文本。
    #[serde(default)]
    pub space_profile: String,
    /// Project profile section text.
    /// 项目画像段落文本。
    #[serde(default)]
    pub project_profile: String,
    /// User profile section text.
    /// 用户画像段落文本。
    #[serde(default)]
    pub user_profile: String,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}

/// Request body for VMM ApplyProfileInstruction.
/// VMM ApplyProfileInstruction 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyProfileInstructionRequest {
    /// Requested profile target.
    /// 请求的画像目标。
    pub target: ProfileTarget,
    /// Numeric VMM user identifier.
    /// VMM 数字用户标识。
    pub user_id: u64,
    /// Numeric VMM project identifier.
    /// VMM 数字项目标识。
    pub project_id: u64,
    /// Stable VulcanCode project identity used only by the process-local native backend.
    /// 仅供进程内原生后端使用的稳定 VulcanCode 项目身份。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host_project_id: String,
    /// Natural-language manual profile instruction.
    /// 自然语言手工画像指令。
    pub instruction: String,
}

/// Response body for VMM ApplyProfileInstruction.
/// VMM ApplyProfileInstruction 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyProfileInstructionResponse {
    /// Durable instruction identifier created by VMM.
    /// VMM 创建的长期指令标识。
    pub instruction_id: u64,
    /// Accepted new profile nodes.
    /// 被接纳的新画像节点。
    #[serde(default)]
    pub accepted_nodes: Vec<ProfileNodeEntry>,
    /// Retired old profile nodes.
    /// 被退役的旧画像节点。
    #[serde(default)]
    pub retired_nodes: Vec<RetiredProfileNodeEntry>,
    /// Review reason returned by VMM.
    /// VMM 返回的评审原因。
    #[serde(default)]
    pub review_reason: String,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}
