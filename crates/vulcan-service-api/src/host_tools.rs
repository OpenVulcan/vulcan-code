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
