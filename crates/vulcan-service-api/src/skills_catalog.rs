use super::*;

/// Full WebUI summary for the embedded Lua Skills catalog.
/// 嵌入式 Lua Skills 目录的完整 WebUI 摘要。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillsCatalogSummary {
    /// Runtime root used by the embedded Lua Skills host.
    /// 嵌入式 Lua Skills 宿主使用的运行时根目录。
    pub runtime_root: String,
    /// System Lua library directory used by Lua Skills.
    /// Lua Skills 使用的系统 Lua 库目录。
    pub system_lua_lib: String,
    /// Total visible runtime entry count.
    /// 可见运行时入口总数。
    pub entry_count: usize,
    /// Human-readable notice for the current catalog state.
    /// 当前目录状态的人类可读提示。
    pub notice: String,
    /// Skill package rows visible to the host.
    /// 宿主当前可见的技能包行。
    pub skills: Vec<LuaSkillSummary>,
}

/// One Lua Skill package row projected for the WebUI.
/// 投影给 WebUI 的单个 Lua Skill 包行。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillSummary {
    /// Stable skill identifier.
    /// 稳定技能标识。
    pub skill_id: String,
    /// Human-facing display name.
    /// 面向用户的显示名称。
    pub display_name: String,
    /// Skill package version.
    /// 技能包版本。
    pub version: String,
    /// Runtime root layer name that owns this effective skill.
    /// 拥有该生效技能的运行时根层名称。
    pub root_name: String,
    /// Filesystem directory that contains this skill.
    /// 包含该技能的文件系统目录。
    pub skill_dir: String,
    /// Whether this skill is currently enabled.
    /// 该技能当前是否已启用。
    pub enabled: bool,
    /// Optional disabled reason from the lifecycle record.
    /// 生命周期记录中的可选停用原因。
    pub disabled_reason: Option<String>,
    /// Whether this skill has a managed install record.
    /// 该技能是否拥有受管安装记录。
    pub managed: bool,
    /// Optional install source type label.
    /// 可选安装来源类型标签。
    pub source_type: Option<String>,
    /// Optional install source locator.
    /// 可选安装来源定位值。
    pub source_locator: Option<String>,
    /// Number of visible runtime entries owned by this skill.
    /// 该技能拥有的可见运行时入口数量。
    pub entry_count: usize,
    /// Whether this skill belongs to the host-owned ROOT layer.
    /// 该技能是否属于宿主自有 ROOT 层。
    pub system_owned: bool,
    /// Tool rows exposed by this skill.
    /// 该技能暴露的工具行。
    pub tools: Vec<LuaSkillToolSummary>,
}

/// One Lua Skill runtime tool row with registration state.
/// 带注册状态的单个 Lua Skill 运行时工具行。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillToolSummary {
    /// Canonical host-visible tool name.
    /// 宿主可见的规范工具名称。
    pub canonical_name: String,
    /// Local tool name inside the skill package.
    /// 技能包内部的本地工具名称。
    pub local_name: String,
    /// Tool description from the runtime entry descriptor.
    /// 来自运行时入口描述符的工具说明。
    pub description: String,
    /// Persisted registration state key.
    /// 持久化注册状态键。
    pub registration_state: String,
    /// Human-readable registration state label.
    /// 人类可读的注册状态标签。
    pub registration_label: String,
    /// Short help text for the current registration state.
    /// 当前注册状态的简短说明。
    pub registration_help: String,
}

/// Full WebUI summary for prompt-based Agent Skills.
/// 基于提示词的 Agent Skills 完整 WebUI 摘要。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillsCatalogSummary {
    /// Whether Agent Skills are globally enabled.
    /// Agent Skills 是否已全局启用。
    pub global_enabled: bool,
    /// System skill root used by host-managed Agent Skills.
    /// 宿主管理 Agent Skills 使用的系统技能根目录。
    pub system_root: String,
    /// User skill root used by user-installed Agent Skills.
    /// 用户安装 Agent Skills 使用的用户技能根目录。
    pub user_root: String,
    /// Human-readable notice for the current catalog state.
    /// 当前目录状态的人类可读提示。
    pub notice: String,
    /// Parsed Agent Skill rows.
    /// 已解析的 Agent Skill 行。
    pub skills: Vec<AgentSkillSummary>,
    /// Non-fatal loading issues.
    /// 非致命加载问题。
    pub errors: Vec<AgentSkillLoadIssueSummary>,
}

/// One prompt-based Agent Skill row projected for the WebUI.
/// 投影给 WebUI 的单个提示词 Agent Skill 行。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillSummary {
    /// Stable skill name from frontmatter.
    /// 来自 frontmatter 的稳定技能名称。
    pub name: String,
    /// Human-facing display name.
    /// 面向用户的显示名称。
    pub display_name: String,
    /// Human-facing short description.
    /// 面向用户的简短说明。
    pub description: String,
    /// Source scope label.
    /// 来源作用域标签。
    pub scope: String,
    /// Whether this Agent Skill is enabled after global configuration is applied.
    /// 应用全局配置后该 Agent Skill 是否启用。
    pub enabled: bool,
    /// Filesystem path to the SKILL.md file.
    /// 指向 SKILL.md 文件的文件系统路径。
    pub skill_file: String,
    /// Directory that owns this Agent Skill and bundled resources.
    /// 拥有该 Agent Skill 与附带资源的目录。
    pub base_dir: String,
    /// Optional resolved icon path that the WebUI can render.
    /// WebUI 可渲染的可选解析图标路径。
    pub icon_path: Option<String>,
    /// Markdown body without YAML frontmatter.
    /// 去除 YAML frontmatter 后的 Markdown 正文。
    pub body: String,
}

/// One Agent Skill loading issue projected for diagnostics.
/// 投影给诊断界面的单个 Agent Skill 加载问题。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillLoadIssueSummary {
    /// Path related to the loading issue.
    /// 与加载问题相关的路径。
    pub path: String,
    /// Human-readable diagnostic message.
    /// 人类可读诊断信息。
    pub message: String,
}
