//! skill catalog projection helpers.
//! skill catalog projection 辅助逻辑。

use super::*;

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
