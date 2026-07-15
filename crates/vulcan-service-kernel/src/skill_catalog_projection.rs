//! skill catalog projection helpers.
//! skill catalog projection 辅助逻辑。

use super::*;
use vulcan_plugin_runtime::{
    SystemPluginAvailability, SystemPluginCatalogSnapshot, SystemPluginPackageOrigin,
    SystemPluginRuntimeState, SystemPluginUiMountIcon, SystemPluginUiMountPoint,
    plugin_capability_key,
};
use vulcan_service_api::{
    AgentSkillCategoryLocaleSummary, AgentSkillCategorySummary, AgentSkillLocaleSummary,
    AgentSkillOriginSummary, AgentSkillRecordSummary, AgentSkillResourceSummary,
    AgentSkillStandardMetadataSummary, LuaSkillsManagedRuntimeProgressSummary,
    PluginExtensionSummary, PluginHookSummary, PluginToolSummary, PluginUiMountSummary,
    SubagentCategoryLocaleSummary, SubagentDefinitionLocaleSummary, SubagentOriginSummary,
    SubagentProviderModelSummary,
};

/// Combines LuaSkills packages and System Plugins into plugin summaries.
/// 将 LuaSkills 包与系统插件合并为插件摘要。
pub(crate) fn plugins_from_catalogs(
    catalog: &TauriLuaSkillsCatalog,
    provider_configuration: &ProviderConfigurationFile,
    system_plugin_catalog: Option<&SystemPluginCatalogSnapshot>,
) -> Vec<PluginSummary> {
    // plugins starts with ordinary Lua Skill rows before discovered System Plugins are appended.
    // plugins 先保存普通 Lua Skill 行，随后追加已发现的 System Plugin。
    let mut plugins = catalog
        .skills
        .iter()
        .map(|skill| PluginSummary {
            id: skill.skill_id.clone(),
            version: Some(skill.version.clone()),
            name: skill.display_name.clone(),
            kind: format!("LuaSkill / {}", skill.root_name),
            status: if skill.enabled {
                "已启用"
            } else {
                "已停用"
            }
            .to_string(),
            runtime_state: if skill.enabled {
                String::from("enabled")
            } else {
                String::from("disabled")
            },
            capability: format!("{} 个入口：{}", skill.entry_count, skill.capability),
            name_by_locale: BTreeMap::new(),
            capability_by_locale: BTreeMap::new(),
            enabled: skill.enabled,
            desired_enabled: skill.enabled,
            available: true,
            tool_count: skill.entry_count,
            tools: skill
                .tools
                .iter()
                .map(|tool| PluginToolSummary {
                    id: tool.canonical_name.clone(),
                    group: skill.skill_id.clone(),
                    description: tool.description.clone(),
                })
                .collect(),
            hook_count: 0,
            skill_count: 0,
            agent_count: 0,
            active_session_count: 0,
            diagnostics: skill.disabled_reason.iter().cloned().collect(),
            management_kind: String::from("luaSkill"),
            extension: None,
        })
        .collect::<Vec<_>>();
    let Some(plugin_catalog) = system_plugin_catalog else {
        return plugins;
    };
    for plugin in plugin_catalog.plugins.values() {
        let enabled = plugin.runtime_state == SystemPluginRuntimeState::Enabled;
        let available = matches!(
            plugin.availability,
            SystemPluginAvailability::Enabled | SystemPluginAvailability::Disabled
        );
        let (runtime_state, status) = match plugin.runtime_state {
            SystemPluginRuntimeState::Enabled => ("enabled", "已启用"),
            SystemPluginRuntimeState::Disabled => ("disabled", "已停用"),
            SystemPluginRuntimeState::Enabling => ("enabling", "正在启用"),
            SystemPluginRuntimeState::Draining => ("draining", "正在排空会话"),
            SystemPluginRuntimeState::DisabledWithCleanupWarning => {
                ("disabledWithCleanupWarning", "已停用（清理警告）")
            }
            SystemPluginRuntimeState::UnsupportedPlatform => {
                ("unsupportedPlatform", "当前平台不支持")
            }
            SystemPluginRuntimeState::Unavailable => ("unavailable", "不可用"),
        };
        let (name, capability) = plugin.localized_display(
            provider_configuration
                .ui_locale
                .as_deref()
                .unwrap_or_default(),
        );
        let name_by_locale = plugin
            .locales
            .iter()
            .map(|(locale, localized)| (locale.clone(), localized.name.clone()))
            .collect();
        let capability_by_locale = plugin
            .locales
            .iter()
            .map(|(locale, localized)| (locale.clone(), localized.capability.clone()))
            .collect();
        plugins.push(PluginSummary {
            id: plugin.id.clone(),
            version: plugin.version.clone(),
            name: name.to_owned(),
            kind: if plugin
                .manifest
                .as_ref()
                .is_some_and(|manifest| manifest.has_runtime())
            {
                String::from("Runtime Plugin")
            } else {
                String::from("Content Plugin")
            },
            status: status.to_owned(),
            runtime_state: runtime_state.to_owned(),
            capability: capability.to_owned(),
            name_by_locale,
            capability_by_locale,
            enabled,
            desired_enabled: plugin.desired_enabled,
            available,
            tool_count: plugin.tools.len(),
            tools: plugin
                .tools
                .iter()
                .map(|tool| PluginToolSummary {
                    id: tool.id.clone(),
                    group: tool.group.clone(),
                    description: tool.description.clone(),
                })
                .collect(),
            hook_count: plugin.hooks.len(),
            skill_count: plugin.skills.len(),
            agent_count: plugin.agents.len(),
            active_session_count: plugin.active_session_count,
            diagnostics: plugin.diagnostics.clone(),
            management_kind: String::from("systemPlugin"),
            extension: plugin
                .manifest
                .as_ref()
                .map(|manifest| PluginExtensionSummary {
                    origin: match plugin.origin {
                        SystemPluginPackageOrigin::BundledSystem => String::from("bundledSystem"),
                        SystemPluginPackageOrigin::UserInstalled => String::from("userInstalled"),
                    },
                    capabilities: manifest
                        .capabilities
                        .iter()
                        .map(|capability| plugin_capability_key(*capability).to_string())
                        .collect(),
                    hooks: manifest
                        .hooks
                        .iter()
                        .map(|hook| PluginHookSummary {
                            id: hook.id.clone(),
                            point: hook.point.clone(),
                        })
                        .collect(),
                    actions: manifest
                        .actions
                        .iter()
                        .map(|action| action.id.clone())
                        .collect(),
                    ui_mounts: manifest
                        .ui
                        .mounts
                        .iter()
                        .map(|mount| PluginUiMountSummary {
                            id: mount.id.clone(),
                            point: match mount.point {
                                SystemPluginUiMountPoint::SkillsPluginDetail => {
                                    String::from("skillsPluginDetail")
                                }
                                SystemPluginUiMountPoint::WorkspaceMain => {
                                    String::from("workspaceMain")
                                }
                                SystemPluginUiMountPoint::ConversationRightPanel => {
                                    String::from("conversationRightPanel")
                                }
                            },
                            label: mount
                                .label
                                .clone()
                                .unwrap_or_else(|| manifest.display.name.clone()),
                            label_by_locale: plugin
                                .locales
                                .iter()
                                .map(|(locale, localized)| {
                                    let label = mount
                                        .label_message_key
                                        .as_deref()
                                        .and_then(|key| localized.messages.get(key))
                                        .cloned()
                                        .unwrap_or_else(|| localized.name.clone());
                                    (locale.clone(), label)
                                })
                                .collect(),
                            icon: match mount.icon {
                                SystemPluginUiMountIcon::Plugin => String::from("plugin"),
                                SystemPluginUiMountIcon::Route => String::from("route"),
                                SystemPluginUiMountIcon::Chart => String::from("chart"),
                            },
                            export_name: mount.export.clone(),
                        })
                        .collect(),
                    skills: plugin
                        .skills
                        .iter()
                        .map(|skill| skill.name.clone())
                        .collect(),
                    agents: plugin.agents.iter().map(|agent| agent.id.clone()).collect(),
                    has_runtime: manifest.has_runtime(),
                }),
        });
    }
    plugins
}

/// Projects the retained LuaSkills catalog into the WebUI service API shape.
/// 将保留的 LuaSkills 目录投影为 WebUI service API 结构。
pub(crate) fn lua_skills_catalog_summary(
    catalog: &TauriLuaSkillsCatalog,
    provider_configuration: &ProviderConfigurationFile,
) -> LuaSkillsCatalogSummary {
    LuaSkillsCatalogSummary {
        runtime_root: normalized_workspace_path_text_from_path(&catalog.runtime_root),
        managed_runtime_distribution_root: normalized_workspace_path_text_from_path(
            &catalog.managed_runtime_distribution_root,
        ),
        managed_runtime_environment_root: normalized_workspace_path_text_from_path(
            &catalog.managed_runtime_environment_root,
        ),
        managed_runtime_bootstrap: LuaSkillsManagedRuntimeBootstrapSummary {
            status: catalog.managed_runtime_report.status.as_key().to_string(),
            platform: catalog.managed_runtime_report.platform.clone(),
            install_attempted: catalog.managed_runtime_report.install_attempted,
            diagnostic: catalog.managed_runtime_report.diagnostic.clone(),
        },
        managed_python_runtime: LuaSkillsManagedRuntimeComponentSummary {
            version: catalog.managed_runtime_report.python.version.clone(),
            status: catalog
                .managed_runtime_report
                .python
                .status
                .as_key()
                .to_string(),
            diagnostic: catalog.managed_runtime_report.python.diagnostic.clone(),
        },
        managed_node_runtime: LuaSkillsManagedRuntimeComponentSummary {
            version: catalog.managed_runtime_report.node.version.clone(),
            status: catalog
                .managed_runtime_report
                .node
                .status
                .as_key()
                .to_string(),
            diagnostic: catalog.managed_runtime_report.node.diagnostic.clone(),
        },
        managed_runtime_progress: catalog.managed_runtime_progress.as_ref().map(|progress| {
            LuaSkillsManagedRuntimeProgressSummary {
                phase: progress.phase.clone(),
                component: progress.component.clone(),
                message: progress.message.clone(),
                completed_bytes: progress.completed_bytes,
                total_bytes: progress.total_bytes,
                overall_percent: progress.overall_percent,
            }
        }),
        system_lua_lib: normalized_workspace_path_text_from_path(&catalog.system_lua_lib_dir),
        runtime_settings_file: normalized_workspace_path_text_from_path(
            &catalog.runtime_settings_file,
        ),
        runtime_settings: catalog.runtime_settings.clone(),
        restart_required: catalog.restart_required,
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
    system_plugin_catalog: Option<&SystemPluginCatalogSnapshot>,
) -> AgentSkillsCatalogSummary {
    // Resolve runtime roots and per-skill states from persisted configuration.
    // 从持久化配置解析运行时根目录与单技能状态。
    let runtime_config = vulcan_agent_runtime::skills::prompt::global_config_from_persisted(
        &provider_configuration.prompt_skills,
    );
    // Load one complete source-aware Agent Skills catalog transaction.
    // 加载一次完整的来源感知 Agent Skills 目录事务。
    let mut catalog = vulcan_prompt_skills::load_prompt_skills(&runtime_config);
    if let Some(plugin_catalog) = system_plugin_catalog
        && let Err(error) =
            catalog.merge_plugin_skills(plugin_catalog.enabled_skills.values().cloned())
    {
        catalog
            .errors
            .push(vulcan_prompt_skills::PromptSkillLoadError {
                path: plugin_catalog.root.clone(),
                message: error,
            });
    }
    // Build the effective path set without inferring identity from display metadata.
    // 在不从展示元数据推测身份的情况下构建有效路径集合。
    let effective_paths = catalog
        .skills
        .iter()
        .map(|skill| skill.skill_file.clone())
        .collect::<std::collections::BTreeSet<_>>();
    // Build the system name set used to expose explicit user override relationships.
    // 构建用于暴露显式用户覆盖关系的系统名称集合。
    let system_names = catalog
        .records
        .iter()
        .filter(|skill| skill.scope == vulcan_prompt_skills::PromptSkillScope::System)
        .map(|skill| skill.name.clone())
        .collect::<std::collections::BTreeSet<_>>();
    // chinese determines the user-facing catalog notice language from persisted UI locale.
    // chinese 根据持久化界面语言决定面向用户的目录摘要语言。
    let chinese = provider_configuration
        .ui_locale
        .as_deref()
        .unwrap_or("zh-CN")
        .starts_with("zh");
    AgentSkillsCatalogSummary {
        system_root: normalized_workspace_path_text_from_path(&catalog.system_root),
        user_root: normalized_workspace_path_text_from_path(&catalog.user_root),
        notice: if chinese {
            format!(
                "系统记录 {}，用户记录 {}，插件记录 {}，有效技能 {}，目录问题 {}。",
                catalog
                    .records
                    .iter()
                    .filter(|skill| skill.scope == vulcan_prompt_skills::PromptSkillScope::System)
                    .count(),
                catalog
                    .records
                    .iter()
                    .filter(|skill| skill.scope == vulcan_prompt_skills::PromptSkillScope::User)
                    .count(),
                catalog
                    .records
                    .iter()
                    .filter(|skill| skill.scope == vulcan_prompt_skills::PromptSkillScope::Plugin)
                    .count(),
                catalog.skills.len(),
                catalog.errors.len()
            )
        } else {
            format!(
                "System records {}, user records {}, plugin records {}, effective skills {}, catalog issues {}.",
                catalog
                    .records
                    .iter()
                    .filter(|skill| skill.scope == vulcan_prompt_skills::PromptSkillScope::System)
                    .count(),
                catalog
                    .records
                    .iter()
                    .filter(|skill| skill.scope == vulcan_prompt_skills::PromptSkillScope::User)
                    .count(),
                catalog
                    .records
                    .iter()
                    .filter(|skill| skill.scope == vulcan_prompt_skills::PromptSkillScope::Plugin)
                    .count(),
                catalog.skills.len(),
                catalog.errors.len()
            )
        },
        records: catalog
            .records
            .iter()
            .map(|skill| {
                // Determine effective state by the exact parsed file identity.
                // 通过精确解析文件身份确定生效状态。
                let effective = effective_paths.contains(&skill.skill_file);
                // Link a user record to the exact system key it overrides.
                // 将用户记录链接到其覆盖的精确系统键。
                let override_of = (skill.scope == vulcan_prompt_skills::PromptSkillScope::User
                    && system_names.contains(&skill.name))
                .then(|| format!("system:{}", skill.name));
                agent_skill_record_summary(skill, provider_configuration, effective, override_of)
            })
            .collect(),
        categories: catalog
            .categories
            .iter()
            .map(|category| AgentSkillCategorySummary {
                category_id: category.category_id.clone(),
                name: category.name.clone(),
                description: category.description.clone(),
                emoji: category.emoji.clone(),
                color: category.color.clone(),
                order: category.order,
                builtin: category.builtin,
                locales: category
                    .locales
                    .iter()
                    .map(|(locale, metadata)| {
                        (
                            locale.clone(),
                            AgentSkillCategoryLocaleSummary {
                                name: metadata.name.clone(),
                                description: metadata.description.clone(),
                            },
                        )
                    })
                    .collect(),
            })
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
pub(crate) fn subagent_catalog_summary(
    provider_configuration: &ProviderConfigurationFile,
    system_plugin_catalog: Option<&SystemPluginCatalogSnapshot>,
) -> SubagentCatalogSummary {
    let system_root = vulcan_runtime_assets::resolve_system_subagents_resource_directory();
    let user_root = vulcan_workspace_store::data_plane::application_data_root().join("agents");
    let mut catalog =
        vulcan_task_session::load_subagent_catalog_from_roots(system_root.clone(), user_root);
    if let Some(plugin_catalog) = system_plugin_catalog
        && let Err(error) =
            catalog.merge_plugin_definitions(plugin_catalog.enabled_agents.values().cloned())
    {
        catalog
            .errors
            .push(vulcan_task_session::SubagentCatalogLoadError {
                path: plugin_catalog.root.clone(),
                message: error,
            });
    }
    subagent_catalog_summary_from_catalog(&catalog, provider_configuration)
}

/// Projects one already loaded agent catalog into the WebUI service API shape.
/// 将一个已加载的智能体目录投影为 WebUI service API 结构。
pub(crate) fn subagent_catalog_summary_from_catalog(
    catalog: &vulcan_task_session::SubagentCatalog,
    provider_configuration: &ProviderConfigurationFile,
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
        child_agent_forbidden_tool_ids: vulcan_session_core::child_agent_forbidden_tool_ids()
            .iter()
            .map(|tool_id| (*tool_id).to_string())
            .collect(),
        definitions: catalog
            .definitions
            .iter()
            .map(|definition| {
                subagent_definition_summary(
                    definition,
                    &system_definition_ids,
                    provider_configuration,
                )
            })
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
    provider_configuration: &ProviderConfigurationFile,
) -> SubagentDefinitionSummary {
    let plugin_owned =
        definition.source.scope == vulcan_task_session::SubagentDefinitionScope::Plugin;
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
        owning_plugin_id: definition.source.plugin_id().map(str::to_string),
        can_edit: !plugin_owned,
        can_delete: matches!(
            definition.source.scope,
            vulcan_task_session::SubagentDefinitionScope::User
                | vulcan_task_session::SubagentDefinitionScope::Package
        ) && definition.source.override_of.is_none(),
        can_toggle: !plugin_owned,
        can_copy: !plugin_owned,
        enabled: definition.enabled,
        definition_file: normalized_workspace_path_text_from_path(&definition.source.file),
        sidecar_file: definition
            .source
            .sidecar_file
            .as_ref()
            .map(|path| normalized_workspace_path_text_from_path(path)),
        remote_url: definition.source.remote_url.clone(),
        override_of: definition.source.override_of.clone(),
        can_restore_default: !plugin_owned
            && subagent_definition_can_restore_default(definition, system_definition_ids),
        source_hash: definition.source.source_hash.clone(),
        version: definition.source.version.clone(),
        updated_at: definition.source.updated_at.clone(),
        tool_policy_mode: capability_policy_mode_key(definition.tool_exposure.policy.mode)
            .to_string(),
        tool_policy_tools: definition.tool_exposure.policy.selectors.clone(),
        allowed_tool_count: capability_policy_allowed_selectors(&definition.tool_exposure.policy)
            .len(),
        disabled_tool_count: capability_policy_denied_selectors(&definition.tool_exposure.policy)
            .len(),
        enabled_prompt_skill_count: capability_policy_allowed_selectors(
            &definition.prompt_skills.policy,
        )
        .len(),
        disabled_prompt_skill_count: capability_policy_denied_selectors(
            &definition.prompt_skills.policy,
        )
        .len(),
        prompt_skill_policy_mode: capability_policy_mode_key(definition.prompt_skills.policy.mode)
            .to_string(),
        prompt_skill_policy_skills: definition.prompt_skills.policy.selectors.clone(),
        provider_model: subagent_provider_model_summary(definition, provider_configuration),
        system_prompt: definition.system_prompt.clone(),
        system_prompt_append: definition.system_prompt_append.clone(),
        allowed_tools: capability_policy_allowed_selectors(&definition.tool_exposure.policy),
        disabled_tools: capability_policy_denied_selectors(&definition.tool_exposure.policy),
        allowed_ai_capabilities: Vec::new(),
        disabled_ai_capabilities: Vec::new(),
        luaskills_overrides: definition.tool_exposure.luaskills_overrides.clone(),
        mcp_server_overrides: definition.tool_exposure.mcp_server_overrides.clone(),
        enabled_prompt_skills: capability_policy_allowed_selectors(
            &definition.prompt_skills.policy,
        ),
        disabled_prompt_skills: capability_policy_denied_selectors(
            &definition.prompt_skills.policy,
        ),
        capability_diagnostics: subagent_capability_diagnostics(definition),
    }
}

/// Returns the allowed selectors represented by one capability policy.
/// 返回单个能力策略表示的允许选择器。
fn capability_policy_allowed_selectors(
    policy: &vulcan_session_core::CapabilityPolicySnapshot,
) -> Vec<String> {
    match policy.mode {
        vulcan_session_core::CapabilityPolicyMode::AllowOnly => policy.selectors.clone(),
        vulcan_session_core::CapabilityPolicyMode::All
        | vulcan_session_core::CapabilityPolicyMode::DenyOnly => Vec::new(),
    }
}

/// Returns the denied selectors represented by one capability policy.
/// 返回单个能力策略表示的禁用选择器。
fn capability_policy_denied_selectors(
    policy: &vulcan_session_core::CapabilityPolicySnapshot,
) -> Vec<String> {
    match policy.mode {
        vulcan_session_core::CapabilityPolicyMode::DenyOnly => policy.selectors.clone(),
        vulcan_session_core::CapabilityPolicyMode::All
        | vulcan_session_core::CapabilityPolicyMode::AllowOnly => Vec::new(),
    }
}

/// Builds the WebUI fixed-model diagnostic projection for one subagent definition.
/// 为单个子代理定义构建 WebUI 固定模型诊断投影。
fn subagent_provider_model_summary(
    definition: &vulcan_task_session::SubagentDefinition,
    provider_configuration: &ProviderConfigurationFile,
) -> SubagentProviderModelSummary {
    let provider_id = definition.provider_model.provider_id.clone();
    let model_id = definition.provider_model.model_id.clone();
    let provider_id_text = provider_id.as_deref().filter(|value| !value.is_empty());
    let model_id_text = model_id.as_deref().filter(|value| !value.is_empty());
    let (Some(provider_id_text), Some(model_id_text)) = (provider_id_text, model_id_text) else {
        if provider_id_text.is_none() && model_id_text.is_none() {
            return SubagentProviderModelSummary {
                mode: String::from("session"),
                ..SubagentProviderModelSummary::default()
            };
        }
        let selector = subagent_provider_model_selector(provider_id_text, model_id_text);
        return SubagentProviderModelSummary {
            provider_id,
            model_id,
            mode: String::from("fixed"),
            selector,
            available: Some(false),
            unavailable_reason: Some(String::from(
                "固定模型配置不完整，请重新选择模型或恢复跟随会话模型。",
            )),
        };
    };
    let available =
        provider_config::effective_model_choices(provider_id_text, provider_configuration)
            .into_iter()
            .any(|choice| choice.id == model_id_text);
    let selector = format!("{provider_id_text}/{model_id_text}");
    SubagentProviderModelSummary {
        provider_id,
        model_id,
        mode: String::from("fixed"),
        selector: Some(selector),
        available: Some(available),
        unavailable_reason: if available {
            None
        } else {
            Some(String::from(
                "当前固定模型不在已启用模型列表中；未显式指定模型时 spawn_agent 会回退到会话模型。",
            ))
        },
    }
}

/// Builds a partial provider/model selector for diagnostics without inventing missing fields.
/// 构建用于诊断的部分供应商/模型选择器，且不臆造缺失字段。
fn subagent_provider_model_selector(
    provider_id: Option<&str>,
    model_id: Option<&str>,
) -> Option<String> {
    match (provider_id, model_id) {
        (Some(provider_id), Some(model_id)) => Some(format!("{provider_id}/{model_id}")),
        (Some(provider_id), None) => Some(format!("{provider_id}/")),
        (None, Some(model_id)) => Some(format!("/{model_id}")),
        (None, None) => None,
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
    let allowed_tools = capability_policy_allowed_selectors(&definition.tool_exposure.policy);
    let disabled_tools = capability_policy_denied_selectors(&definition.tool_exposure.policy);
    let enabled_prompt_skills =
        capability_policy_allowed_selectors(&definition.prompt_skills.policy);
    let disabled_prompt_skills =
        capability_policy_denied_selectors(&definition.prompt_skills.policy);
    if allowed_tools.is_empty()
        && disabled_tools.is_empty()
        && definition.tool_exposure.luaskills_overrides.is_null()
        && definition.tool_exposure.mcp_server_overrides.is_null()
        && enabled_prompt_skills.is_empty()
        && disabled_prompt_skills.is_empty()
    {
        diagnostics.push(String::from(
            "未声明额外限制；最终能力仍由系统策略、LuaSkills 状态、MCP 连接和运行时工具快照决定。",
        ));
    } else {
        diagnostics.push(String::from(
            "已声明能力策略；最终可见能力会继续受系统策略、LuaSkills 状态和 MCP 连接过滤。",
        ));
    }
    if !allowed_tools.is_empty() {
        diagnostics.push(format!(
            "配置允许工具 {} 个；不存在或被系统禁用的工具不会进入最终工具集。",
            allowed_tools.len()
        ));
    }
    if !enabled_prompt_skills.is_empty() {
        diagnostics.push(format!(
            "配置启用 Prompt Skills {} 个；不存在或已停用的技能不会进入最终提示词组合。",
            enabled_prompt_skills.len()
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
        vulcan_task_session::SubagentDefinitionScope::Plugin => "插件",
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
        vulcan_task_session::SubagentDefinitionScope::Plugin => "plugin",
    }
}

/// Projects one source-aware prompt skill record into the WebUI domain.
/// 将一条来源感知提示词技能记录投影到 WebUI 领域。
pub(crate) fn agent_skill_record_summary(
    skill: &vulcan_prompt_skills::PromptSkill,
    provider_configuration: &ProviderConfigurationFile,
    effective: bool,
    override_of: Option<String>,
) -> AgentSkillRecordSummary {
    // Resolve the persisted per-skill state for the standard skill name.
    // 为标准技能名称解析持久化单技能状态。
    let local_enabled = if skill.scope == vulcan_prompt_skills::PromptSkillScope::Plugin {
        true
    } else {
        provider_configuration
            .prompt_skills
            .skill_overrides
            .get(&skill.name)
            .copied()
            .unwrap_or(provider_configuration.prompt_skills.default_skill_state)
            .is_enabled()
    };
    let plugin_owned = skill.scope == vulcan_prompt_skills::PromptSkillScope::Plugin;
    AgentSkillRecordSummary {
        skill_key: format!("{}:{}", skill.scope.en_label(), skill.name),
        name: skill.name.clone(),
        display_name: prompt_skill_display_name(skill).to_string(),
        description: prompt_skill_short_description(skill).to_string(),
        locales: skill
            .interface
            .locales
            .iter()
            .map(|(locale, metadata)| {
                (
                    locale.clone(),
                    AgentSkillLocaleSummary {
                        display_name: metadata.display_name.clone(),
                        short_description: metadata.short_description.clone(),
                    },
                )
            })
            .collect(),
        scope_key: skill.scope.en_label().to_string(),
        owning_plugin_id: skill.plugin_id().map(str::to_string),
        can_edit: !plugin_owned,
        can_delete: skill.scope == vulcan_prompt_skills::PromptSkillScope::User
            && override_of.is_none(),
        can_toggle: !plugin_owned,
        can_copy: !plugin_owned,
        can_restore_default: !plugin_owned && override_of.is_some(),
        effective,
        override_of,
        enabled: effective && local_enabled,
        category_id: skill.catalog.category_id.clone(),
        tags: skill.catalog.tags.clone(),
        standard: AgentSkillStandardMetadataSummary {
            description: skill.description.clone(),
            license: skill.license.clone(),
            compatibility: skill.compatibility.clone(),
            metadata: skill.metadata.clone(),
        },
        origin: AgentSkillOriginSummary {
            kind: skill.origin.kind.clone(),
            label: skill.origin.label.clone(),
            url: skill.origin.url.clone(),
            version: skill.origin.version.clone(),
            revision: skill.origin.revision.clone(),
            managed: skill.origin.managed,
            integrity: skill.origin.integrity.clone(),
            installed_at: skill.origin.installed_at.clone(),
            updated_at: skill.origin.updated_at.clone(),
        },
        resources: AgentSkillResourceSummary {
            scripts: skill.resources.scripts,
            references: skill.resources.references,
            assets: skill.resources.assets,
        },
        skill_file: normalized_workspace_path_text_from_path(&skill.skill_file),
        base_dir: normalized_workspace_path_text_from_path(&skill.base_dir),
        emoji: skill.interface.emoji.clone(),
        color: skill.interface.color.clone(),
        body: skill.body.trim().to_string(),
    }
}

/// Returns the display name for one prompt skill.
/// 返回单个提示词技能的显示名称。
pub(crate) fn prompt_skill_display_name(skill: &vulcan_prompt_skills::PromptSkill) -> &str {
    skill
        .interface
        .display_name
        .as_deref()
        .unwrap_or(skill.name.as_str())
}

/// Returns the short description for one prompt skill.
/// 返回单个提示词技能的简短说明。
pub(crate) fn prompt_skill_short_description(skill: &vulcan_prompt_skills::PromptSkill) -> &str {
    skill
        .interface
        .short_description
        .as_deref()
        .filter(|description| !description.trim().is_empty())
        .or_else(|| (!skill.description.trim().is_empty()).then_some(skill.description.as_str()))
        .unwrap_or("该技能没有提供描述。")
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

#[cfg(test)]
mod tests {
    use super::{
        ProviderConfigurationFile, capability_policy_allowed_selectors,
        capability_policy_denied_selectors, empty_luaskills_catalog, plugins_from_catalogs,
    };
    use vulcan_session_core::{CapabilityPolicyMode, CapabilityPolicySnapshot};

    /// Verifies allow-only policy projection derives only allowed selectors.
    /// 验证仅允许策略投影只派生允许选择器。
    #[test]
    fn capability_policy_projection_derives_allowed_selectors() {
        let policy = CapabilityPolicySnapshot {
            mode: CapabilityPolicyMode::AllowOnly,
            selectors: vec![String::from("shell_command")],
        };

        assert_eq!(
            capability_policy_allowed_selectors(&policy),
            vec![String::from("shell_command")]
        );
        assert!(capability_policy_denied_selectors(&policy).is_empty());
    }

    /// Verifies deny-only policy projection derives only denied selectors.
    /// 验证仅禁用策略投影只派生禁用选择器。
    #[test]
    fn capability_policy_projection_derives_denied_selectors() {
        let policy = CapabilityPolicySnapshot {
            mode: CapabilityPolicyMode::DenyOnly,
            selectors: vec![String::from("ask")],
        };

        assert!(capability_policy_allowed_selectors(&policy).is_empty());
        assert_eq!(
            capability_policy_denied_selectors(&policy),
            vec![String::from("ask")]
        );
    }

    /// Verifies all policy projection exposes no selector list.
    /// 验证 all 策略投影不会暴露选择器列表。
    #[test]
    fn capability_policy_projection_hides_all_mode_selectors() {
        let policy = CapabilityPolicySnapshot {
            mode: CapabilityPolicyMode::All,
            selectors: vec![String::from("stale")],
        };

        assert!(capability_policy_allowed_selectors(&policy).is_empty());
        assert!(capability_policy_denied_selectors(&policy).is_empty());
    }

    /// Verifies plugin projection never scans package roots when no committed plugin snapshot exists.
    /// 验证不存在已提交插件快照时，插件投影绝不会自行扫描包目录。
    #[test]
    fn plugin_projection_requires_committed_catalog_snapshot() {
        let plugins = plugins_from_catalogs(
            &empty_luaskills_catalog(),
            &ProviderConfigurationFile::default(),
            None,
        );

        assert!(plugins.is_empty());
    }

    /// Verifies every bundled subagent sidecar satisfies the current strict catalog protocol.
    /// 验证每个内置子代理 sidecar 都满足当前严格目录协议。
    #[test]
    fn bundled_subagent_sidecars_load_without_catalog_errors() {
        // Loads the production resource directory through the same catalog entry used at runtime.
        // 通过运行时使用的同一目录入口加载生产资源目录。
        let catalog = vulcan_task_session::load_subagent_catalog_from_roots(
            vulcan_runtime_assets::resolve_system_subagents_resource_directory(),
            std::path::PathBuf::new(),
        );

        assert!(
            !catalog.definitions.is_empty(),
            "bundled subagent catalog must contain definitions"
        );
        assert!(
            catalog.errors.is_empty(),
            "bundled subagent catalog must load without errors: {:#?}",
            catalog.errors
        );
    }
}
