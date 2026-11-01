use super::*;

/// One model subagents may run besides the session model, with the user's guidance.
/// 子代理在会话模型之外可使用的一个模型，附带用户说明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubagentModelEntry {
    /// Stable provider identifier.
    /// 稳定 Provider 标识。
    pub provider_id: String,
    /// Provider-scoped model identifier.
    /// Provider 范围内的模型标识。
    pub model_id: String,
    /// Which tasks suit the model and what to watch for; shown to the agent verbatim.
    /// 适合哪些任务以及注意事项；原样展示给代理。
    #[serde(default)]
    pub note: String,
}

/// The system default subagent model allowlist.
/// 系统默认子代理模型白名单。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubagentModelsSnapshot {
    /// Whether the calling session model is additionally granted.
    /// 是否额外授权调用会话模型。
    pub include_session_model: bool,
    /// Granted models in display order.
    /// 按显示顺序排列的授权模型。
    pub models: Vec<SubagentModelEntry>,
    /// Longest accepted note, in Unicode scalar values.
    /// 允许的最长备注（按 Unicode 标量值计）。
    pub max_note_chars: usize,
    /// Largest accepted number of granted models.
    /// 允许的最大授权模型数。
    pub max_models: usize,
}

/// Replaces the system default subagent model allowlist.
/// 替换系统默认子代理模型白名单。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubagentModelsSaveRequest {
    /// Explicit inclusion choice saved with the complete model list.
    /// 与完整模型清单一起保存的显式包含选择。
    pub include_session_model: bool,
    /// Granted models in display order.
    /// 按显示顺序排列的授权模型。
    pub models: Vec<SubagentModelEntry>,
}

/// One session's subagent model allowlist next to the system list it would follow.
/// 一个会话的子代理模型白名单，以及其跟随的系统清单。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionSubagentModelsSnapshot {
    /// Session inclusion override; absent together with models means following the system.
    /// 会话包含覆盖；与模型同时为空表示跟随系统。
    pub include_session_model: Option<bool>,
    /// System inclusion choice used by following sessions.
    /// 跟随系统的会话所使用的系统包含选择。
    pub system_include_session_model: bool,
    /// Session identifier.
    /// 会话标识。
    pub session_id: String,
    /// The session's own list; `None` means the session follows the system list.
    /// 会话自有清单；`None` 表示会话跟随系统清单。
    pub models: Option<Vec<SubagentModelEntry>>,
    /// The system default list.
    /// 系统默认清单。
    pub system_models: Vec<SubagentModelEntry>,
    /// Longest accepted note, in Unicode scalar values.
    /// 允许的最长备注（按 Unicode 标量值计）。
    pub max_note_chars: usize,
    /// Largest accepted number of granted models.
    /// 允许的最大授权模型数。
    pub max_models: usize,
}

/// Saves one session's own list, or returns it to the system list with `None`.
/// 保存一个会话的自有清单；`None` 使其恢复跟随系统清单。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionSubagentModelsSaveRequest {
    /// Inclusion override; must be present exactly when models is present.
    /// 包含覆盖；必须与模型清单同时存在。
    pub include_session_model: Option<bool>,
    /// Session identifier.
    /// 会话标识。
    pub session_id: String,
    /// The session's own list, or `None` to follow the system list.
    /// 会话自有清单；`None` 表示跟随系统清单。
    pub models: Option<Vec<SubagentModelEntry>>,
}
