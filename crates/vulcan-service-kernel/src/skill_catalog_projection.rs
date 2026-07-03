//! skill catalog projection helpers.
//! skill catalog projection 辅助逻辑。

use super::*;
use vulcan_service_api::{
    SubagentCategoryLocaleSummary, SubagentDefinitionLocaleSummary, SubagentOriginSummary,
};

/// Converts LuaSkills catalog rows into plugin summaries.
/// 将 LuaSkills 目录行转换为插件摘要。
pub(crate) fn plugins_from_luaskills(catalog: &TauriLuaSkillsCatalog) -> Vec<PluginSummary> {
    catalog
        .skills
        .iter()
        .map(|skill| PluginSummary {
            id: skill.skill_id.clone(),
            name: skill.display_name.clone(),
            kind: format!("LuaSkill / {}", skill.root_name),
            status: if skill.enabled {
                "已启用"
            } else {
                "已停用"
            }
            .to_string(),
            capability: format!("{} 个入口：{}", skill.entry_count, skill.capability),
        })
        .collect()
}

/// Projects the retained LuaSkills catalog into the WebUI service API shape.
/// 将保留的 LuaSkills 目录投影为 WebUI service API 结构。
pub(crate) fn lua_skills_catalog_summary(
    catalog: &TauriLuaSkillsCatalog,
    provider_configuration: &ProviderConfigurationFile,
) -> LuaSkillsCatalogSummary {
    LuaSkillsCatalogSummary {
        runtime_root: normalized_workspace_path_text_from_path(&catalog.runtime_root),
        system_lua_lib: normalized_workspace_path_text_from_path(&catalog.system_lua_lib_dir),
        entry_count: catalog.entry_count,
        notice: if catalog.runtime_root.as_os_str().is_empty() {
            String::from("LuaSkills 技能目录尚未载入。")
        } else {
            format!(
                "可见技能 {} 个，运行时入口 {} 个。",
                catalog.skills.len(),
                catalog.entry_count
            )
        },
        skills: catalog
            .skills
            .iter()
            .map(|skill| LuaSkillSummary {
                skill_id: skill.skill_id.clone(),
                display_name: skill.display_name.clone(),
                version: skill.version.clone(),
                root_name: skill.root_name.clone(),
                skill_dir: normalized_workspace_path_text_from_path(&skill.skill_dir),
                enabled: skill.enabled,
                disabled_reason: skill.disabled_reason.clone(),
                managed: skill.managed,
                source_type: skill.source_type.clone(),
                source_locator: skill.source_locator.clone(),
                entry_count: skill.entry_count,
                system_owned: skill.system_owned,
                tools: skill
                    .tools
                    .iter()
                    .map(|tool| {
                        let state = provider_configuration
                            .luaskills
                            .tool_registration_state(&skill.skill_id, &tool.local_name);
                        LuaSkillToolSummary {
                            canonical_name: tool.canonical_name.clone(),
                            local_name: tool.local_name.clone(),
                            description: tool.description.clone(),
                            registration_state: luaskill_tool_registration_state_key(state)
                                .to_string(),
                            registration_label: state.to_string(),
                            registration_help: luaskill_tool_registration_state_help(state)
                                .to_string(),
                        }
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// Projects prompt-based Agent Skills into the WebUI service API shape.
/// 将基于提示词的 Agent Skills 投影为 WebUI service API 结构。
pub(crate) fn agent_skills_catalog_summary(
    provider_configuration: &ProviderConfigurationFile,
) -> AgentSkillsCatalogSummary {
    let runtime_config = vulcan_agent_runtime::skills::prompt::global_config_from_persisted(
        &provider_configuration.prompt_skills,
    );
    let catalog = vulcan_prompt_skills::load_prompt_skills(&runtime_config);
    let global_enabled = provider_configuration.prompt_skills.enabled;
    AgentSkillsCatalogSummary {
        global_enabled,
        system_root: normalized_workspace_path_text_from_path(&catalog.system_root),
        user_root: normalized_workspace_path_text_from_path(&catalog.user_root),
        notice: format!(
            "系统技能 {}，用户技能 {}，加载警告 {}。",
            catalog
                .skills
                .iter()
                .filter(|skill| skill.scope == vulcan_prompt_skills::PromptSkillScope::System)
                .count(),
            catalog
                .skills
                .iter()
                .filter(|skill| skill.scope == vulcan_prompt_skills::PromptSkillScope::User)
                .count(),
            catalog.errors.len()
        ),
        skills: catalog
            .skills
            .iter()
            .map(|skill| agent_skill_summary(skill, provider_configuration, global_enabled))
            .collect(),
        errors: catalog
            .errors
            .iter()
            .map(|error| AgentSkillLoadIssueSummary {
                path: normalized_workspace_path_text_from_path(&error.path),
                message: error.message.clone(),
            })
            .collect(),
    }
}

/// Projects agent and expert definitions into the WebUI service API shape.
/// 将智能体与专家定义投影为 WebUI service API 结构。
pub(crate) fn subagent_catalog_summary() -> SubagentCatalogSummary {
    let system_root = vulcan_runtime_assets::resolve_system_subagents_resource_directory();
    let user_root = vulcan_workspace_store::data_plane::application_data_root().join("agents");
    let catalog =
        vulcan_task_session::load_subagent_catalog_from_roots(system_root.clone(), user_root);
    subagent_catalog_summary_from_catalog(&catalog)
}

/// Projects one already loaded agent catalog into the WebUI service API shape.
/// 将一个已加载的智能体目录投影为 WebUI service API 结构。
pub(crate) fn subagent_catalog_summary_from_catalog(
    catalog: &vulcan_task_session::SubagentCatalog,
) -> SubagentCatalogSummary {
    let system_definition_ids = system_subagent_definition_ids_for_summary();
    let enabled_count = catalog
        .definitions
        .iter()
        .filter(|definition| definition.enabled)
        .count();
    SubagentCatalogSummary {
        system_root: normalized_workspace_path_text_from_path(&catalog.system_root),
        user_root: normalized_workspace_path_text_from_path(&catalog.user_root),
        notice: format!(
            "智能体定义 {} 个，已启用 {} 个，加载警告 {}。",
            catalog.definitions.len(),
            enabled_count,
            catalog.errors.len()
        ),
        definitions: catalog
            .definitions
            .iter()
            .map(|definition| subagent_definition_summary(definition, &system_definition_ids))
            .collect(),
        categories: catalog
            .categories
            .iter()
            .map(|category| SubagentCategorySummary {
                category_id: category.id.clone(),
                name: category.name.clone(),
                description: category.description.clone(),
                locales: category
                    .locales
                    .iter()
                    .map(|(locale, metadata)| {
                        (
                            locale.clone(),
                            SubagentCategoryLocaleSummary {
                                name: metadata.name.clone(),
                                description: metadata.description.clone(),
                            },
                        )
                    })
                    .collect(),
                emoji: category.emoji.clone(),
                color: category.color.clone(),
                order: category.order,
            })
            .collect(),
        errors: catalog
            .errors
            .iter()
            .map(|error| SubagentCatalogIssueSummary {
                path: normalized_workspace_path_text_from_path(&error.path),
                message: error.message.clone(),
            })
            .collect(),
    }
}

/// Projects one agent definition into a compact WebUI card row.
/// 将单个智能体定义投影为紧凑 WebUI 卡片行。
fn subagent_definition_summary(
    definition: &vulcan_task_session::SubagentDefinition,
    system_definition_ids: &std::collections::BTreeSet<String>,
) -> SubagentDefinitionSummary {
    SubagentDefinitionSummary {
        agent_id: definition.id.clone(),
        name: definition.name.clone(),
        description: definition.description.clone(),
        locales: definition
            .locales
            .iter()
            .map(|(locale, metadata)| {
                (
                    locale.clone(),
                    SubagentDefinitionLocaleSummary {
                        name: metadata.name.clone(),
                        description: metadata.description.clone(),
                    },
                )
            })
            .collect(),
        source_origin: definition
            .origin
            .as_ref()
            .map(|origin| SubagentOriginSummary {
                kind: origin.kind.clone(),
                label: origin.label.clone(),
                icon: origin.icon.clone(),
                url: origin.url.clone(),
            }),
        emoji: definition.emoji.clone(),
        color: definition.color.clone(),
        vibe: definition.vibe.clone(),
        source_key: definition.source.source_key.clone(),
        category_id: definition.source.category_id.clone(),
        package_id: definition.source.package_id.clone(),
        scope: subagent_definition_scope_label(definition.source.scope).to_string(),
        scope_key: subagent_definition_scope_key(definition.source.scope).to_string(),
        enabled: definition.enabled,
        definition_file: normalized_workspace_path_text_from_path(&definition.source.file),
        sidecar_file: definition
            .source
            .sidecar_file
            .as_ref()
            .map(|path| normalized_workspace_path_text_from_path(path)),
        remote_url: definition.source.remote_url.clone(),
        override_of: definition.source.override_of.clone(),
        can_restore_default: subagent_definition_can_restore_default(
            definition,
            system_definition_ids,
        ),
        source_hash: definition.source.source_hash.clone(),
        version: definition.source.version.clone(),
        updated_at: definition.source.updated_at.clone(),
        tool_policy_mode: capability_policy_mode_key(definition.tool_exposure.policy.mode)
            .to_string(),
        tool_policy_tools: definition.tool_exposure.policy.selectors.clone(),
        allowed_tool_count: definition.tool_exposure.allowed_tools.len(),
        disabled_tool_count: definition.tool_exposure.disabled_tools.len(),
        enabled_prompt_skill_count: definition.prompt_skills.enabled.len(),
        disabled_prompt_skill_count: definition.prompt_skills.disabled.len(),
        prompt_skill_policy_mode: capability_policy_mode_key(definition.prompt_skills.policy.mode)
            .to_string(),
        prompt_skill_policy_skills: definition.prompt_skills.policy.selectors.clone(),
        system_prompt: definition.system_prompt.clone(),
        system_prompt_append: definition.system_prompt_append.clone(),
        allowed_tools: definition.tool_exposure.allowed_tools.clone(),
        disabled_tools: definition.tool_exposure.disabled_tools.clone(),
        allowed_ai_capabilities: Vec::new(),
        disabled_ai_capabilities: Vec::new(),
        luaskills_overrides: definition.tool_exposure.luaskills_overrides.clone(),
        mcp_server_overrides: definition.tool_exposure.mcp_server_overrides.clone(),
        enabled_prompt_skills: definition.prompt_skills.enabled.clone(),
        disabled_prompt_skills: definition.prompt_skills.disabled.clone(),
        capability_diagnostics: subagent_capability_diagnostics(definition),
    }
}

/// Returns the stable API key for one capability policy mode.
/// 返回单个能力策略模式的稳定 API 键。
fn capability_policy_mode_key(mode: vulcan_session_core::CapabilityPolicyMode) -> &'static str {
    match mode {
        vulcan_session_core::CapabilityPolicyMode::All => "all",
        vulcan_session_core::CapabilityPolicyMode::AllowOnly => "allow_only",
        vulcan_session_core::CapabilityPolicyMode::DenyOnly => "deny_only",
    }
}

/// Returns whether one effective agent row can restore a shipped system definition.
/// 返回一个生效智能体行是否可以恢复为随应用发布的系统定义。
fn subagent_definition_can_restore_default(
    definition: &vulcan_task_session::SubagentDefinition,
    system_definition_ids: &std::collections::BTreeSet<String>,
) -> bool {
    let has_user_layer =
        definition.source.scope == vulcan_task_session::SubagentDefinitionScope::User;
    if !has_user_layer {
        return false;
    }
    let system_agent_id = definition
        .source
        .override_of
        .as_deref()
        .unwrap_or(definition.id.as_str());
    system_definition_ids.contains(system_agent_id)
}

/// Loads immutable system agent ids for one summary projection pass.
/// 为单次摘要投影加载不可变系统智能体标识集合。
fn system_subagent_definition_ids_for_summary() -> std::collections::BTreeSet<String> {
    vulcan_task_session::load_subagent_catalog_from_roots(
        vulcan_runtime_assets::resolve_system_subagents_resource_directory(),
        PathBuf::new(),
    )
    .definitions
    .into_iter()
    .filter(|definition| {
        definition.source.scope == vulcan_task_session::SubagentDefinitionScope::System
    })
    .map(|definition| definition.id)
    .collect()
}

/// Builds compact effective-capability diagnostics for one agent definition.
/// 为单个智能体定义构建紧凑的最终能力诊断。
fn subagent_capability_diagnostics(
    definition: &vulcan_task_session::SubagentDefinition,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if !definition.enabled {
        diagnostics.push(String::from(
            "当前智能体已禁用，不会暴露给模型，也不能被 spawn_agent 解析。",
        ));
    }
    if definition.tool_exposure.allowed_tools.is_empty()
        && definition.tool_exposure.disabled_tools.is_empty()
        && definition.tool_exposure.luaskills_overrides.is_null()
        && definition.tool_exposure.mcp_server_overrides.is_null()
        && definition.prompt_skills.enabled.is_empty()
        && definition.prompt_skills.disabled.is_empty()
    {
        diagnostics.push(String::from(
            "未声明额外限制；最终能力仍由系统策略、LuaSkills 状态、MCP 连接和运行时工具快照决定。",
        ));
    } else {
        diagnostics.push(String::from(
            "已声明能力策略；最终可见能力会继续受系统策略、LuaSkills 状态和 MCP 连接过滤。",
        ));
    }
    if !definition.tool_exposure.allowed_tools.is_empty() {
        diagnostics.push(format!(
            "配置允许工具 {} 个；不存在或被系统禁用的工具不会进入最终工具集。",
            definition.tool_exposure.allowed_tools.len()
        ));
    }
    if !definition.prompt_skills.enabled.is_empty() {
        diagnostics.push(format!(
            "配置启用 Prompt Skills {} 个；不存在或被全局禁用的技能不会进入最终提示词组合。",
            definition.prompt_skills.enabled.len()
        ));
    }
    diagnostics
}

/// Returns the Chinese scope label for one agent definition source.
/// 返回单个智能体定义来源的中文作用域标签。
fn subagent_definition_scope_label(
    scope: vulcan_task_session::SubagentDefinitionScope,
) -> &'static str {
    match scope {
        vulcan_task_session::SubagentDefinitionScope::System => "系统",
        vulcan_task_session::SubagentDefinitionScope::User => "用户",
        vulcan_task_session::SubagentDefinitionScope::Package => "包",
    }
}

/// Returns the stable API key for one subagent definition scope.
/// 返回单个智能体定义来源作用域的稳定 API 键。
fn subagent_definition_scope_key(
    scope: vulcan_task_session::SubagentDefinitionScope,
) -> &'static str {
    match scope {
        vulcan_task_session::SubagentDefinitionScope::System => "system",
        vulcan_task_session::SubagentDefinitionScope::User => "user",
        vulcan_task_session::SubagentDefinitionScope::Package => "package",
    }
}

/// Projects one prompt skill into a compact WebUI card row.
/// 将单个提示词技能投影为紧凑 WebUI 卡片行。
pub(crate) fn agent_skill_summary(
    skill: &vulcan_prompt_skills::PromptSkill,
    provider_configuration: &ProviderConfigurationFile,
    global_enabled: bool,
) -> AgentSkillSummary {
    let local_enabled = provider_configuration
        .prompt_skills
        .skill_overrides
        .get(&skill.name)
        .copied()
        .unwrap_or(provider_configuration.prompt_skills.default_skill_state)
        .is_enabled();
    AgentSkillSummary {
        name: skill.name.clone(),
        display_name: prompt_skill_display_name(skill).to_string(),
        description: prompt_skill_short_description(skill).to_string(),
        scope: skill.scope.zh_label().to_string(),
        enabled: global_enabled && local_enabled,
        skill_file: normalized_workspace_path_text_from_path(&skill.skill_file),
        base_dir: normalized_workspace_path_text_from_path(&skill.base_dir),
        icon_path: prompt_skill_icon_path(skill),
        body: skill.body.trim().to_string(),
    }
}

/// Returns the display name for one prompt skill.
/// 返回单个提示词技能的显示名称。
pub(crate) fn prompt_skill_display_name(skill: &vulcan_prompt_skills::PromptSkill) -> &str {
    skill
        .interface
        .as_ref()
        .and_then(|interface| interface.display_name.as_deref())
        .unwrap_or(skill.name.as_str())
}

/// Returns the short description for one prompt skill.
/// 返回单个提示词技能的简短说明。
pub(crate) fn prompt_skill_short_description(skill: &vulcan_prompt_skills::PromptSkill) -> &str {
    skill
        .interface
        .as_ref()
        .and_then(|interface| interface.short_description.as_deref())
        .filter(|description| !description.trim().is_empty())
        .or_else(|| (!skill.description.trim().is_empty()).then_some(skill.description.as_str()))
        .unwrap_or("该技能没有提供描述。")
}

/// Resolves an optional prompt skill icon path when it exists on disk.
/// 在图标文件存在时解析可选提示词技能图标路径。
pub(crate) fn prompt_skill_icon_path(skill: &vulcan_prompt_skills::PromptSkill) -> Option<String> {
    skill
        .interface
        .as_ref()
        .and_then(|interface| interface.icon_small.as_ref())
        .map(|path| skill.base_dir.join(path))
        .filter(|path| path.is_file())
        .map(|path| normalized_workspace_path_text_from_path(&path))
}

/// Returns a stable key for one persisted LuaSkills tool registration state.
/// 返回单个持久化 LuaSkills 工具注册状态的稳定键。
pub(crate) fn luaskill_tool_registration_state_key(
    state: PersistedLuaSkillToolRegistrationState,
) -> &'static str {
    match state {
        PersistedLuaSkillToolRegistrationState::Explicit => "explicit",
        PersistedLuaSkillToolRegistrationState::Implicit => "implicit",
        PersistedLuaSkillToolRegistrationState::Disabled => "disabled",
    }
}

/// Returns the help text shown for one LuaSkills tool registration state.
/// 返回单个 LuaSkills 工具注册状态展示的说明文本。
pub(crate) fn luaskill_tool_registration_state_help(
    state: PersistedLuaSkillToolRegistrationState,
) -> &'static str {
    match state {
        PersistedLuaSkillToolRegistrationState::Explicit => {
            "显式启用：工具会作为独立函数 Schema 暴露给模型。"
        }
        PersistedLuaSkillToolRegistrationState::Implicit => {
            "隐式启用：工具保留宿主调用能力，但不暴露独立 Schema。"
        }
        PersistedLuaSkillToolRegistrationState::Disabled => {
            "禁用：工具不会注册给模型，也不应被宿主调度调用。"
        }
    }
}
