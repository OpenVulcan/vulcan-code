use super::*;

/// One stored global or project environment variable as the manager shows it. Secret values are
/// never sent; `value` is then empty.
/// 管理器展示的一条已存储全局或项目环境变量。密钥值从不发送，此时 `value` 为空。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentVariableEntry {
    /// Stable variable identifier.
    /// 稳定变量标识。
    pub id: String,
    /// Variable name as entered.
    /// 输入时的变量名。
    pub name: String,
    /// `plain` or `secret`.
    /// `plain` 或 `secret`。
    pub kind: String,
    /// Plain value; empty for a secret.
    /// 普通值；密钥为空。
    pub value: String,
    /// What the variable is for.
    /// 变量用途。
    pub description: String,
    /// Whether the variable applies to every project.
    /// 变量是否适用于所有项目。
    pub global: bool,
    /// Projects the variable applies to when it is not global.
    /// 变量非全局时适用的项目。
    pub project_ids: Vec<String>,
    /// Creation time in Unix milliseconds.
    /// 创建时间（Unix 毫秒）。
    pub created_at_ms: i64,
    /// Last update time in Unix milliseconds.
    /// 最后更新时间（Unix 毫秒）。
    pub updated_at_ms: i64,
}

/// One variable kept by a session itself. Secret values are never sent; `value` is then empty.
/// 会话自身保存的一个变量。密钥值从不发送，此时 `value` 为空。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionEnvironmentVariableEntry {
    /// Variable name.
    /// 变量名。
    pub name: String,
    /// `plain` or `secret`.
    /// `plain` 或 `secret`。
    pub kind: String,
    /// Plain value; empty for a secret.
    /// 普通值；密钥为空。
    pub value: String,
    /// What the variable is for.
    /// 变量用途。
    pub description: String,
    /// Last update time in Unix milliseconds.
    /// 最后更新时间（Unix 毫秒）。
    pub updated_at_ms: i64,
}

/// Limits and naming rules shared by every environment-variable editor.
/// 所有环境变量编辑器共用的限制与命名规则。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentVariableRules {
    /// Recommended name prefix.
    /// 推荐的名称前缀。
    pub recommended_prefix: String,
    /// Names Vulcan sets itself; they cannot be stored.
    /// Vulcan 自行设置的名称；不能存储。
    pub reserved_names: Vec<String>,
    /// Longest name in characters.
    /// 名称最大字符数。
    pub name_max_chars: usize,
    /// Largest value in UTF-8 bytes.
    /// 值的最大 UTF-8 字节数。
    pub value_max_bytes: usize,
    /// Longest description in characters.
    /// 说明最大字符数。
    pub description_max_chars: usize,
}

/// Everything the environment-variable manager shows.
/// 环境变量管理器展示的全部内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentVariablesSnapshot {
    /// Global and project variables in name order.
    /// 按名称排序的全局与项目变量。
    pub variables: Vec<EnvironmentVariableEntry>,
    /// Naming rules and limits.
    /// 命名规则与限制。
    pub rules: EnvironmentVariableRules,
}

/// The variables one session keeps itself.
/// 一个会话自身保存的变量。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionEnvironmentSnapshot {
    /// Session identifier (the root session for a child agent).
    /// 会话标识（子代理则为其根会话）。
    pub session_id: String,
    /// Session variables in name order.
    /// 按名称排序的会话变量。
    pub variables: Vec<SessionEnvironmentVariableEntry>,
}

/// Creates or updates one global or project variable.
/// 创建或更新一条全局或项目变量。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentVariableSaveRequest {
    /// Existing variable to update; absent creates one.
    /// 要更新的已有变量；缺省时新建。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Variable name.
    /// 变量名。
    pub name: String,
    /// New value; absent keeps the stored value of an existing variable and is refused on create.
    /// 新值；缺省时保留已有变量的存储值，新建时拒绝。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// `plain` or `secret`.
    /// `plain` 或 `secret`。
    pub kind: String,
    /// What the variable is for.
    /// 变量用途。
    #[serde(default)]
    pub description: String,
    /// Whether the variable applies to every project.
    /// 变量是否适用于所有项目。
    pub global: bool,
    /// Projects the variable applies to when it is not global; at least one.
    /// 变量非全局时适用的项目；至少一个。
    #[serde(default)]
    pub project_ids: Vec<String>,
}

/// Facts about one name the user is entering, so the editor can warn before saving.
/// 用户正在输入的名称的相关事实，供编辑器在保存前提示。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentVariableNameCheck {
    /// Name as checked.
    /// 被检查的名称。
    pub name: String,
    /// Why the name cannot be stored, when it cannot.
    /// 名称无法存储时的原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invalid_reason: Option<String>,
    /// Whether the system environment of the Vulcan service already defines the name; the stored
    /// value then replaces it for processes Vulcan starts, without changing the system.
    /// Vulcan 服务的系统环境是否已定义该名称；此时存储值会在 Vulcan 启动的进程中替换它，而不改变系统。
    pub defined_by_system: bool,
    /// Whether the name starts with the recommended prefix.
    /// 名称是否以推荐前缀开头。
    pub has_recommended_prefix: bool,
}

/// Values the user entered in the Ask panel for one pending value request.
/// 用户在 Ask 面板中为一条待处理取值请求输入的值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentValuesSubmitRequest {
    /// Pending request identifier.
    /// 待处理请求标识。
    pub request_id: String,
    /// Whether the user declined; `values` must then be empty.
    /// 用户是否拒绝；此时 `values` 必须为空。
    #[serde(default)]
    pub declined: bool,
    /// Entered values; requested names left out stay unset.
    /// 输入的值；未包含的请求名称保持未设置。
    #[serde(default)]
    pub values: Vec<EnvironmentValueSubmission>,
}

/// One value entered in the Ask panel.
/// 在 Ask 面板中输入的一个值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentValueSubmission {
    /// Requested variable name.
    /// 请求的变量名。
    pub name: String,
    /// Entered value.
    /// 输入的值。
    pub value: String,
    /// `session`, `project` or `global`.
    /// `session`、`project` 或 `global`。
    pub scope: String,
}
