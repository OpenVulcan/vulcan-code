use super::*;

/// Describes runtime-native tools projected to WebUI clients.
/// 描述投影给 WebUI 客户端的运行时原生工具。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct HostToolsSettingsSummary {
    /// Runtime-registered native tool rows available for agent policy editing.
    /// 可用于智能体策略编辑的运行时已注册原生工具行。
    #[serde(default)]
    pub native_tools: Vec<NativeToolSummary>,
    /// Default shell shared by the integrated terminal and the command tools.
    /// 集成终端与命令工具共用的默认 Shell。
    #[serde(default)]
    pub command_shell: CommandShellSettingsSummary,
}

/// Describes the persisted default shell choice and the shells detected on this host.
/// 描述持久化的默认 Shell 选择及本机检测到的 Shell。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CommandShellSettingsSummary {
    /// Persisted shell identifier, or none when the host picks automatically.
    /// 持久化的 Shell 标识；由宿主自动选择时为空。
    #[serde(default)]
    pub configured_shell_id: Option<String>,
    /// Shell that a new terminal actually uses after the preference is resolved.
    /// 解析偏好后新终端实际使用的 Shell。
    #[serde(default)]
    pub resolved_shell_id: Option<String>,
    /// Shells detected on this host, in automatic priority order.
    /// 本机检测到的 Shell，按自动选择优先级排序。
    #[serde(default)]
    pub options: Vec<CommandShellOptionSummary>,
}

/// Describes one shell detected on the host.
/// 描述本机检测到的一个 Shell。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CommandShellOptionSummary {
    /// Stable shell identifier accepted by the save request.
    /// 保存请求接受的稳定 Shell 标识。
    pub id: String,
    /// Host display label for the shell.
    /// 该 Shell 的宿主显示名称。
    pub label: String,
}

/// Describes one runtime-registered native host tool.
/// 描述单个运行时已注册原生宿主工具。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct NativeToolSummary {
    /// Stable native tool identifier.
    /// 稳定原生工具标识。
    pub id: String,
    /// Human-readable native tool name.
    /// 人类可读原生工具名称。
    pub name: String,
    /// Human-readable native tool description.
    /// 人类可读原生工具描述。
    pub description: String,
    /// Whether this native tool is normally visible to AI agents.
    /// 该原生工具通常是否对 AI 代理可见。
    pub ai_visible: bool,
    /// Whether this native tool may run while Plan mode is active.
    /// 该原生工具是否允许在计划模式激活时运行。
    pub available_in_plan: bool,
}
