//! Session-owned permission mode commands and durable receipts.
//! 会话自有权限模式命令与可靠回执。

use serde::{Deserialize, Serialize};
use vulcan_permission_judge::system::ReviewRules;
use vulcan_session_core::v1::config::{PermissionMode, UnattendedConfig};

/// Permission mode update shared by WebUI and TUI.
/// WebUI 与 TUI 共用的权限模式更新请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionPermissionModeUpdateRequest {
    /// Exact existing regular session.
    /// 精确的已有普通会话。
    pub session_id: String,
    /// Mode read by the next tool call of this session.
    /// 本会话下一次工具调用读取的模式。
    pub mode: PermissionMode,
}

/// Durable permission mode result captured by the unique session writer.
/// 唯一会话写入器捕获的可靠权限模式结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionPermissionModeUpdateResponse {
    /// Exact session owner of this response.
    /// 本响应的精确会话归属。
    pub session_id: String,
    /// Document revision whose durable publication completed before returning.
    /// 返回前已完成可靠发布的文档修订。
    pub revision: u64,
    /// Authoritative mode after the serialized operation.
    /// 串行操作后的权威模式。
    pub permission_mode: PermissionMode,
    /// Authoritative unattended mode after the operation; leaving auto-approve turns it off.
    /// 操作后的权威无人值守模式；离开自动审批时会被关闭。
    pub unattended: Option<UnattendedConfig>,
}

/// Unattended mode update shared by WebUI and TUI.
/// WebUI 与 TUI 共用的无人值守模式更新请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionUnattendedUpdateRequest {
    /// Exact existing regular session.
    /// 精确的已有普通会话。
    pub session_id: String,
    /// Mode with the deny policy the user confirmed, or `None` to turn it off.
    /// 带用户确认拒绝策略的模式；`None` 表示关闭。
    pub unattended: Option<UnattendedConfig>,
}

/// Durable unattended mode result captured by the unique session writer.
/// 唯一会话写入器捕获的可靠无人值守模式结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionUnattendedUpdateResponse {
    /// Exact session owner of this response.
    /// 本响应的精确会话归属。
    pub session_id: String,
    /// Document revision whose durable publication completed before returning.
    /// 返回前已完成可靠发布的文档修订。
    pub revision: u64,
    /// Authoritative unattended mode after the serialized operation.
    /// 串行操作后的权威无人值守模式。
    pub unattended: Option<UnattendedConfig>,
}

/// Deny policy that prefills the dialog turning unattended mode on.
/// 预填开启无人值守模式对话框的拒绝策略。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnattendedDefaultResponse {
    /// Project default when the project sets one, else the system default, else empty.
    /// 项目设置时为项目默认值，否则为系统默认值，再否则为空。
    pub deny_policy: String,
    /// Where `deny_policy` came from: `project`, `system`, or `none`.
    /// `deny_policy` 的来源：`project`、`system` 或 `none`。
    pub source: String,
    /// Longest accepted policy, in Unicode scalar values.
    /// 允许的最长策略（按 Unicode 标量值计）。
    pub max_chars: usize,
}

/// Asks for the deny policy that prefills the unattended dialog of one session.
/// 请求预填某会话无人值守对话框的拒绝策略。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnattendedDefaultRequest {
    /// Exact existing regular session; its project supplies the project default.
    /// 精确的已有普通会话；其项目提供项目默认值。
    pub session_id: String,
}

/// System default deny policy for unattended mode.
/// 无人值守模式的系统默认拒绝策略。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnattendedSystemDefaultSnapshot {
    /// Saved system default; empty means none.
    /// 已保存的系统默认值；为空表示没有。
    pub deny_policy: String,
    /// Longest accepted policy, in Unicode scalar values.
    /// 允许的最长策略（按 Unicode 标量值计）。
    pub max_chars: usize,
}

/// Saves the system default deny policy for unattended mode.
/// 保存无人值守模式的系统默认拒绝策略。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnattendedSystemDefaultSaveRequest {
    /// New system default; empty means none.
    /// 新的系统默认值；为空表示没有。
    pub deny_policy: String,
}

/// One project's default deny policy for unattended mode.
/// 一个项目的无人值守模式默认拒绝策略。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectUnattendedDefaultSnapshot {
    /// Project identity.
    /// 项目标识。
    pub project_id: String,
    /// Project default; absent means the system default applies.
    /// 项目默认值；缺省表示采用系统默认值。
    pub deny_policy: Option<String>,
    /// System default the project inherits while it sets none.
    /// 项目未设置时继承的系统默认值。
    pub system_deny_policy: String,
    /// Longest accepted policy, in Unicode scalar values.
    /// 允许的最长策略（按 Unicode 标量值计）。
    pub max_chars: usize,
}

/// Saves one project's default deny policy for unattended mode.
/// 保存一个项目的无人值守模式默认拒绝策略。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectUnattendedDefaultSaveRequest {
    /// Project identity.
    /// 项目标识。
    pub project_id: String,
    /// Project default; `None` returns the project to the system default.
    /// 项目默认值；`None` 使项目恢复采用系统默认值。
    pub deny_policy: Option<String>,
}

/// System default rules of the permission reviewer.
/// 权限复核器的系统默认规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewRulesSystemDefaultSnapshot {
    /// Saved system rules; empty texts mean none.
    /// 已保存的系统规则；空文本表示没有。
    pub rules: ReviewRules,
    /// Longest accepted text of one rule group, in Unicode scalar values.
    /// 单组规则允许的最长文本（按 Unicode 标量值计）。
    pub max_chars: usize,
}

/// Saves the system default rules of the permission reviewer.
/// 保存权限复核器的系统默认规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewRulesSystemDefaultSaveRequest {
    /// New system rules; empty texts mean none.
    /// 新的系统规则；空文本表示没有。
    pub rules: ReviewRules,
}

/// One project's rules for the permission reviewer.
/// 一个项目的权限复核器规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectReviewRulesSnapshot {
    /// Project identity.
    /// 项目标识。
    pub project_id: String,
    /// Project rules; absent means the system default applies.
    /// 项目规则；缺省表示采用系统默认值。
    pub rules: Option<ReviewRules>,
    /// System rules the project inherits while it sets none.
    /// 项目未设置时继承的系统规则。
    pub system_rules: ReviewRules,
    /// Longest accepted text of one rule group, in Unicode scalar values.
    /// 单组规则允许的最长文本（按 Unicode 标量值计）。
    pub max_chars: usize,
}

/// Saves one project's rules for the permission reviewer.
/// 保存一个项目的权限复核器规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectReviewRulesSaveRequest {
    /// Project identity.
    /// 项目标识。
    pub project_id: String,
    /// Project rules; `None` returns the project to the system default.
    /// 项目规则；`None` 使项目恢复采用系统默认值。
    pub rules: Option<ReviewRules>,
}
