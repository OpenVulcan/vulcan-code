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

/// Full WebUI summary for the agent and expert catalog.
/// 智能体与专家目录的完整 WebUI 摘要。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCatalogSummary {
    /// System agent definition root shipped with the application.
    /// 随应用发布的系统智能体定义根目录。
    pub system_root: String,
    /// User-owned agent definition and override root.
    /// 用户拥有的智能体定义与覆盖根目录。
    pub user_root: String,
    /// Human-readable notice for the current catalog state.
    /// 当前目录状态的人类可读提示。
    pub notice: String,
    /// Public tool ids that child agents cannot execute directly.
    /// 智能体不能直接执行的公开工具标识。
    pub child_agent_forbidden_tool_ids: Vec<String>,
    /// Merged agent and expert definition rows.
    /// 合并后的智能体与专家定义行。
    pub definitions: Vec<SubagentDefinitionSummary>,
    /// Parsed category rows.
    /// 已解析的分类行。
    pub categories: Vec<SubagentCategorySummary>,
    /// Non-fatal loading issues.
    /// 非致命加载问题。
    pub errors: Vec<SubagentCatalogIssueSummary>,
}

/// Lightweight response for one agent enabled-state update.
/// 单个智能体启停状态更新的轻量响应。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentDefinitionEnabledUpdate {
    /// Stable agent id that was updated.
    /// 已更新的稳定智能体标识。
    pub agent_id: String,
    /// Updated enabled state.
    /// 更新后的启停状态。
    pub enabled: bool,
    /// Updated catalog notice after the local mutation.
    /// 局部变更后的目录提示。
    pub notice: String,
}

/// One agent catalog category projected for the WebUI.
/// 投影给 WebUI 的单个智能体目录分类。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCategorySummary {
    /// Stable category id.
    /// 稳定分类标识。
    pub category_id: String,
    /// Human-facing category name.
    /// 面向用户的分类名称。
    pub name: String,
    /// Optional category description.
    /// 可选分类说明。
    pub description: Option<String>,
    /// Localized display metadata keyed by locale tag.
    /// 按语言标签索引的本地化展示元数据。
    #[serde(default)]
    pub locales: std::collections::BTreeMap<String, SubagentCategoryLocaleSummary>,
    /// Human-facing category emoji.
    /// 面向用户的分类图标。
    pub emoji: String,
    /// Human-facing category color token or value.
    /// 面向用户的分类颜色标识或颜色值。
    pub color: String,
    /// Optional category order.
    /// 可选分类排序值。
    pub order: Option<i32>,
}

/// Localized WebUI metadata for one agent catalog category.
/// 单个智能体目录分类的本地化 WebUI 元数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCategoryLocaleSummary {
    /// Optional localized category name.
    /// 可选本地化分类名称。
    pub name: Option<String>,
    /// Optional localized category description.
    /// 可选本地化分类说明。
    pub description: Option<String>,
}

/// One agent or expert definition row projected for the WebUI.
/// 投影给 WebUI 的单个智能体或专家定义行。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentDefinitionSummary {
    /// Stable agent id, normally the Markdown file stem.
    /// 稳定智能体标识，通常为 Markdown 文件 stem。
    pub agent_id: String,
    /// Human-facing display name.
    /// 面向用户的显示名称。
    pub name: String,
    /// Optional human-facing description.
    /// 可选的人类可读说明。
    pub description: Option<String>,
    /// Localized display metadata keyed by locale tag.
    /// 按语言标签索引的本地化展示元数据。
    #[serde(default)]
    pub locales: std::collections::BTreeMap<String, SubagentDefinitionLocaleSummary>,
    /// Optional upstream or license source metadata.
    /// 可选上游或协议来源元数据。
    pub source_origin: Option<SubagentOriginSummary>,
    /// Human-facing emoji.
    /// 面向用户的图标。
    pub emoji: String,
    /// Human-facing color token or color value.
    /// 面向用户的颜色标识或颜色值。
    pub color: String,
    /// Optional short tone or persona hint.
    /// 可选简短语气或人格提示。
    pub vibe: Option<String>,
    /// Stable source key for this definition row.
    /// 当前定义行的稳定来源键。
    pub source_key: String,
    /// Optional canonical category id.
    /// 可选规范分类标识。
    pub category_id: Option<String>,
    /// Optional package id.
    /// 可选包标识。
    pub package_id: Option<String>,
    /// Source scope label.
    /// 来源作用域标签。
    pub scope: String,
    /// Stable source scope key used by UI logic.
    /// UI 逻辑使用的稳定来源作用域键。
    pub scope_key: String,
    /// Whether this definition is exposed to models and spawn_agent.
    /// 该定义是否暴露给模型与 spawn_agent。
    pub enabled: bool,
    /// Filesystem path to the Markdown definition file.
    /// 指向 Markdown 定义文件的文件系统路径。
    pub definition_file: String,
    /// Optional filesystem path to the sidecar metadata file.
    /// 指向 sidecar 元数据文件的可选文件系统路径。
    pub sidecar_file: Option<String>,
    /// Optional remote URL that supplied this user definition.
    /// 提供该用户定义的可选远程 URL。
    pub remote_url: Option<String>,
    /// Optional system definition id overridden by this row.
    /// 被当前行覆盖的可选系统定义标识。
    pub override_of: Option<String>,
    /// Whether the user layer can be removed to restore a system definition.
    /// 是否可以移除用户层以恢复系统定义。
    #[serde(default)]
    pub can_restore_default: bool,
    /// Optional source hash for audit and cache diagnostics.
    /// 用于审计与缓存诊断的可选来源哈希。
    pub source_hash: Option<String>,
    /// Optional source version string.
    /// 可选来源版本字符串。
    pub version: Option<String>,
    /// Optional last-updated timestamp string.
    /// 可选最近更新时间戳字符串。
    pub updated_at: Option<String>,
    /// Unified tool policy mode.
    /// 统一工具策略模式。
    pub tool_policy_mode: String,
    /// Unified tool policy selectors.
    /// 统一工具策略选择器。
    pub tool_policy_tools: Vec<String>,
    /// Number of explicitly allowed host tools.
    /// 显式允许的宿主工具数量。
    pub allowed_tool_count: usize,
    /// Number of explicitly disabled host tools.
    /// 显式禁用的宿主工具数量。
    pub disabled_tool_count: usize,
    /// Number of enabled prompt skills declared by this definition.
    /// 当前定义声明启用的提示词技能数量。
    pub enabled_prompt_skill_count: usize,
    /// Number of disabled prompt skills declared by this definition.
    /// 当前定义声明禁用的提示词技能数量。
    pub disabled_prompt_skill_count: usize,
    /// Unified prompt-skill policy mode.
    /// 统一提示词技能策略模式。
    pub prompt_skill_policy_mode: String,
    /// Unified prompt-skill policy selectors.
    /// 统一提示词技能策略选择器。
    pub prompt_skill_policy_skills: Vec<String>,
    /// Optional fixed provider/model selection for this agent.
    /// 当前智能体可选的固定供应商/模型选择。
    #[serde(default)]
    pub provider_model: SubagentProviderModelSummary,
    /// Editable base system prompt restored into the agent editor.
    /// 恢复到智能体编辑器中的可编辑基础系统提示词。
    pub system_prompt: Option<String>,
    /// Editable appended system prompt restored into the agent editor.
    /// 恢复到智能体编辑器中的可编辑追加系统提示词。
    pub system_prompt_append: Option<String>,
    /// Explicitly allowed host tool selectors.
    /// 显式允许的宿主工具选择器。
    pub allowed_tools: Vec<String>,
    /// Explicitly disabled host tool selectors.
    /// 显式禁用的宿主工具选择器。
    pub disabled_tools: Vec<String>,
    /// Explicitly allowed AI capability selectors.
    /// 显式允许的 AI 能力选择器。
    pub allowed_ai_capabilities: Vec<String>,
    /// Explicitly disabled AI capability selectors.
    /// 显式禁用的 AI 能力选择器。
    pub disabled_ai_capabilities: Vec<String>,
    /// LuaSkills-specific exposure override payload.
    /// LuaSkills 专属可见性覆盖载荷。
    pub luaskills_overrides: serde_json::Value,
    /// MCP-specific exposure override payload.
    /// MCP 专属可见性覆盖载荷。
    pub mcp_server_overrides: serde_json::Value,
    /// Explicitly enabled prompt skill names.
    /// 显式启用的提示词技能名称。
    pub enabled_prompt_skills: Vec<String>,
    /// Explicitly disabled prompt skill names.
    /// 显式禁用的提示词技能名称。
    pub disabled_prompt_skills: Vec<String>,
    /// Human-readable effective capability diagnostics.
    /// 人类可读的最终能力诊断信息。
    pub capability_diagnostics: Vec<String>,
}

/// Provider/model selection projected for one subagent definition.
/// 投影给单个子代理定义的供应商/模型选择。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentProviderModelSummary {
    /// Optional provider identifier.
    /// 可选供应商标识。
    #[serde(default)]
    pub provider_id: Option<String>,
    /// Optional model identifier.
    /// 可选模型标识。
    #[serde(default)]
    pub model_id: Option<String>,
    /// Effective display mode for this agent's model selection.
    /// 当前智能体模型选择的有效展示模式。
    #[serde(default)]
    pub mode: String,
    /// Stable provider/model selector when a fixed model is configured.
    /// 配置固定模型时使用的稳定 provider/model 选择器。
    #[serde(default)]
    pub selector: Option<String>,
    /// Whether the fixed model is currently selectable.
    /// 固定模型当前是否可被选择。
    #[serde(default)]
    pub available: Option<bool>,
    /// Human-readable reason when the fixed model is unavailable.
    /// 固定模型不可用时的人类可读原因。
    #[serde(default)]
    pub unavailable_reason: Option<String>,
}

/// Localized WebUI metadata for one agent definition.
/// 单个智能体定义的本地化 WebUI 元数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentDefinitionLocaleSummary {
    /// Optional localized agent name.
    /// 可选本地化智能体名称。
    pub name: Option<String>,
    /// Optional localized agent description.
    /// 可选本地化智能体说明。
    pub description: Option<String>,
}

/// Upstream or license source metadata projected for the WebUI.
/// 投影给 WebUI 的上游或协议来源元数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentOriginSummary {
    /// Source kind such as github, system, or other.
    /// 来源类型，例如 github、system 或 other。
    pub kind: Option<String>,
    /// Optional human-facing source label.
    /// 可选的人类可读来源标签。
    pub label: Option<String>,
    /// Optional icon key used by WebUI.
    /// WebUI 使用的可选图标键。
    pub icon: Option<String>,
    /// Optional source URL.
    /// 可选来源链接。
    pub url: Option<String>,
}

/// Tri-state text patch used by partial agent metadata saves.
/// 智能体元数据局部保存使用的三态文本补丁。
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub enum OptionalTextPatch {
    /// The request omitted this field and the previous value must be kept.
    /// 请求未提交该字段，必须保留旧值。
    #[default]
    Missing,
    /// The request explicitly sent null and the previous value must be cleared.
    /// 请求显式提交 null，必须清空旧值。
    Clear,
    /// The request supplied a replacement text value.
    /// 请求提交了用于替换的文本值。
    Set(String),
}

impl OptionalTextPatch {
    /// Returns true when the patch carries no requested change.
    /// 当补丁不携带变更请求时返回 true。
    pub fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }
}

impl Serialize for OptionalTextPatch {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Missing | Self::Clear => serializer.serialize_none(),
            Self::Set(value) => serializer.serialize_some(value),
        }
    }
}

impl<'de> Deserialize<'de> for OptionalTextPatch {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Option::<String>::deserialize(deserializer)
            .map(|value| value.map(Self::Set).unwrap_or(Self::Clear))
    }
}

/// Save request for one user-owned agent definition.
/// 单个用户拥有的智能体保存请求。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentDefinitionSaveRequest {
    /// Stable agent id, normally the Markdown file stem.
    /// 稳定智能体标识，通常为 Markdown 文件 stem。
    pub agent_id: String,
    /// Optional original agent id when saving a moved definition.
    /// 保存已转移定义时使用的可选原始智能体标识。
    #[serde(default)]
    pub original_agent_id: Option<String>,
    /// Optional user package id assigned to this definition.
    /// 分配给当前定义的可选用户包标识。
    #[serde(default)]
    pub package_id: Option<String>,
    /// Human-facing display name.
    /// 面向用户的显示名称。
    pub name: String,
    /// Optional human-facing description.
    /// 可选的人类可读说明。
    pub description: Option<String>,
    /// Localized display metadata keyed by locale tag.
    /// 按语言标签索引的本地化展示元数据。
    #[serde(default)]
    pub locales: std::collections::BTreeMap<String, SubagentDefinitionLocaleSummary>,
    /// Optional human-facing emoji override.
    /// 可选的人类可读图标覆盖。
    #[serde(default, skip_serializing_if = "OptionalTextPatch::is_missing")]
    pub emoji: OptionalTextPatch,
    /// Optional human-facing color token or value override.
    /// 可选的人类可读颜色标识或颜色值覆盖。
    #[serde(default, skip_serializing_if = "OptionalTextPatch::is_missing")]
    pub color: OptionalTextPatch,
    /// Optional short tone or persona hint override.
    /// 可选简短语气或人格提示覆盖。
    #[serde(default, skip_serializing_if = "OptionalTextPatch::is_missing")]
    pub vibe: OptionalTextPatch,
    /// Whether this definition is exposed to models and spawn_agent.
    /// 该定义是否暴露给模型与 spawn_agent。
    pub enabled: bool,
    /// Base system prompt saved for the agent.
    /// 为该智能体保存的基础系统提示词。
    pub system_prompt: Option<String>,
    /// Appended system prompt saved for the agent.
    /// 为该智能体保存的追加系统提示词。
    pub system_prompt_append: Option<String>,
    /// Explicitly allowed host tool selectors.
    /// 显式允许的宿主工具选择器。
    pub allowed_tools: Vec<String>,
    /// Explicitly disabled host tool selectors.
    /// 显式禁用的宿主工具选择器。
    pub disabled_tools: Vec<String>,
    /// Unified tool policy mode.
    /// 统一工具策略模式。
    #[serde(default)]
    pub tool_policy_mode: Option<String>,
    /// Unified tool policy selectors.
    /// 统一工具策略选择器。
    #[serde(default)]
    pub tool_policy_tools: Vec<String>,
    /// Explicitly allowed AI capability selectors.
    /// 显式允许的 AI 能力选择器。
    #[serde(default)]
    pub allowed_ai_capabilities: Vec<String>,
    /// Explicitly disabled AI capability selectors.
    /// 显式禁用的 AI 能力选择器。
    #[serde(default)]
    pub disabled_ai_capabilities: Vec<String>,
    /// LuaSkills-specific exposure override payload.
    /// LuaSkills 专属可见性覆盖载荷。
    #[serde(default)]
    pub luaskills_overrides: serde_json::Value,
    /// MCP-specific exposure override payload.
    /// MCP 专属可见性覆盖载荷。
    #[serde(default)]
    pub mcp_server_overrides: serde_json::Value,
    /// Explicitly enabled prompt skill names.
    /// 显式启用的提示词技能名称。
    pub enabled_prompt_skills: Vec<String>,
    /// Explicitly disabled prompt skill names.
    /// 显式禁用的提示词技能名称。
    pub disabled_prompt_skills: Vec<String>,
    /// Unified prompt-skill policy mode.
    /// 统一提示词技能策略模式。
    #[serde(default)]
    pub prompt_skill_policy_mode: Option<String>,
    /// Unified prompt-skill policy selectors.
    /// 统一提示词技能策略选择器。
    #[serde(default)]
    pub prompt_skill_policy_skills: Vec<String>,
    /// Optional fixed provider/model selection for this agent.
    /// 当前智能体可选的固定供应商/模型选择。
    #[serde(default)]
    pub provider_model: SubagentProviderModelSummary,
}

/// One agent catalog loading issue projected for diagnostics.
/// 投影给诊断界面的单个智能体目录加载问题。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCatalogIssueSummary {
    /// Path related to the loading issue.
    /// 与加载问题相关的路径。
    pub path: String,
    /// Human-readable diagnostic message.
    /// 人类可读诊断信息。
    pub message: String,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Builds a minimal valid subagent save request payload for serde tests.
    /// 为 serde 测试构建最小有效智能体保存请求载荷。
    fn minimal_save_request_payload() -> serde_json::Value {
        json!({
            "agentId": "reviewer",
            "name": "Reviewer",
            "enabled": true,
            "systemPrompt": null,
            "systemPromptAppend": null,
            "allowedTools": [],
            "disabledTools": [],
            "enabledPromptSkills": [],
            "disabledPromptSkills": []
        })
    }

    /// Verifies omitted optional text fields are preserved as missing patches.
    /// 验证省略的可选文本字段会保留为缺失补丁。
    #[test]
    fn optional_text_patch_deserializes_missing_fields() {
        let request: SubagentDefinitionSaveRequest =
            serde_json::from_value(minimal_save_request_payload())
                .expect("request payload should deserialize");

        assert_eq!(request.emoji, OptionalTextPatch::Missing);
        assert_eq!(request.color, OptionalTextPatch::Missing);
        assert_eq!(request.vibe, OptionalTextPatch::Missing);
    }

    /// Verifies explicit null optional text fields become clear patches.
    /// 验证显式 null 的可选文本字段会成为清空补丁。
    #[test]
    fn optional_text_patch_deserializes_null_fields() {
        let mut payload = minimal_save_request_payload();
        payload["emoji"] = serde_json::Value::Null;
        payload["color"] = serde_json::Value::Null;
        payload["vibe"] = serde_json::Value::Null;

        let request: SubagentDefinitionSaveRequest =
            serde_json::from_value(payload).expect("request payload should deserialize");

        assert_eq!(request.emoji, OptionalTextPatch::Clear);
        assert_eq!(request.color, OptionalTextPatch::Clear);
        assert_eq!(request.vibe, OptionalTextPatch::Clear);
    }

    /// Verifies string optional text fields become set patches.
    /// 验证字符串可选文本字段会成为设置补丁。
    #[test]
    fn optional_text_patch_deserializes_string_fields() {
        let mut payload = minimal_save_request_payload();
        payload["emoji"] = json!("🧪");
        payload["color"] = json!("green");
        payload["vibe"] = json!("focused");

        let request: SubagentDefinitionSaveRequest =
            serde_json::from_value(payload).expect("request payload should deserialize");

        assert_eq!(request.emoji, OptionalTextPatch::Set(String::from("🧪")));
        assert_eq!(request.color, OptionalTextPatch::Set(String::from("green")));
        assert_eq!(
            request.vibe,
            OptionalTextPatch::Set(String::from("focused"))
        );
    }
}
