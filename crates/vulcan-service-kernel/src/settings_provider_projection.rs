//! settings provider projection helpers.
//! settings provider projection 辅助逻辑。

use super::*;

/// Creates first-version settings sections.
/// 创建第一版设置分组。
pub(crate) fn settings() -> Vec<SettingSectionSummary> {
    vec![
        setting_section(
            "runtime",
            "运行时",
            "Core Service、LuaSkills、VLDB、MCP 的启动与状态设置",
            &["启动模式", "运行时目录", "MCP 工具策略", "VLDB 控制器"],
        ),
        setting_section(
            "providers",
            "模型与 Provider",
            "模型供应商、默认模型、工具能力与推理配置",
            &["默认 Provider", "默认模型", "工具调用", "推理强度"],
        ),
        setting_section(
            "plugins",
            "插件",
            "LuaSkills、Rust lib 插件、WebUI 控制面板与权限",
            &["插件目录", "启用状态", "权限声明", "WebUI 面板"],
        ),
        setting_section(
            "remote",
            "远程访问",
            "浏览器、手机、远程客户端访问 Core Service 的设置",
            &["监听地址", "访问令牌", "远程开关", "审计日志"],
        ),
    ]
}

/// Creates one settings section summary.
/// 创建单个设置分组摘要。
pub(crate) fn setting_section(
    id: &str,
    label: &str,
    description: &str,
    items: &[&str],
) -> SettingSectionSummary {
    SettingSectionSummary {
        id: id.to_string(),
        label: label.to_string(),
        description: description.to_string(),
        items: items.iter().map(|item| item.to_string()).collect(),
    }
}

/// Creates provider summaries from the local catalog surface.
/// 从本地目录面创建 Provider 摘要。
pub(crate) fn providers(
    provider_configuration: &ProviderConfigurationFile,
) -> Vec<ProviderSummary> {
    let active_model_reference = provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)
        .map(|(provider_id, model_id)| (provider_id.to_string(), model_id.to_string()));
    let mut providers = provider_config::supported_providers()
        .iter()
        .map(|definition| {
            let configured = provider_configuration.provider.contains_key(&definition.id);
            let enabled_ids =
                provider_config::enabled_model_ids(&definition.id, provider_configuration);
            let catalog_models =
                provider_config::catalog_model_choices(&definition.id, provider_configuration)
                    .into_iter()
                    .map(|model_choice| {
                        let catalog_model_id = model_choice.id;
                        let reasoning_profile_json = model_choice
                            .reasoning_profile
                            .as_ref()
                            .and_then(|profile| serde_json::to_string_pretty(profile).ok());
                        let active = active_model_reference.as_ref().is_some_and(
                            |(active_provider_id, active_model_id)| {
                                active_provider_id == &definition.id
                                    && active_model_id == &catalog_model_id
                            },
                        );
                        let secondary_route = (definition.id == "openrouter")
                            .then(|| {
                                provider_config::openrouter_endpoint_route_for_model(
                                    provider_configuration,
                                    &catalog_model_id,
                                )
                            })
                            .flatten()
                            .map(|selection| {
                                provider_secondary_route_summary_from_openrouter_selection(
                                    &definition.id,
                                    &catalog_model_id,
                                    selection,
                                    active,
                                )
                            });
                        ProviderModelSummary {
                            enabled: configured && enabled_ids.contains(&catalog_model_id),
                            id: catalog_model_id.clone(),
                            name: model_choice.name,
                            active,
                            status: model_choice.status,
                            family: model_choice.family,
                            context_limit: model_choice.context_limit,
                            input_limit: model_choice.input_limit,
                            output_limit: model_choice.output_limit,
                            reasoning: model_choice.reasoning,
                            attachment: model_choice.attachment,
                            input_modalities: model_choice.input_modalities,
                            output_modalities: model_choice.output_modalities,
                            reasoning_profile_json,
                            supports_secondary_routes: definition.id == "openrouter",
                            secondary_route,
                        }
                    })
                    .collect::<Vec<_>>();
            let models = catalog_models
                .iter()
                .filter(|model| model.enabled)
                .cloned()
                .collect::<Vec<_>>();
            let active = models.iter().any(|model| model.active);
            ProviderSummary {
                id: definition.id.clone(),
                name: definition.name.clone(),
                status: if configured { "已配置" } else { "未配置" }.to_string(),
                configured,
                enabled_models: format!("{} 个已启用模型", models.len()),
                active,
                models,
                catalog_models,
                fields: provider_fields(definition, provider_configuration),
            }
        })
        .collect::<Vec<_>>();
    providers.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.id.cmp(&right.id))
    });
    providers
}

/// Creates editable provider field summaries for one provider definition.
/// 为单个供应商定义创建可编辑字段摘要。
pub(crate) fn provider_fields(
    definition: &provider_config::SupportedProviderDefinition,
    provider_configuration: &ProviderConfigurationFile,
) -> Vec<ProviderFieldSummary> {
    let options = provider_configuration
        .provider
        .get(&definition.id)
        .map(|entry| entry.options.clone())
        .unwrap_or_default();
    definition
        .input_fields()
        .into_iter()
        .map(|field| ProviderFieldSummary {
            key: provider_field_request_key(field).to_string(),
            label: definition.field_label(field).to_string(),
            placeholder: definition.field_placeholder(field),
            value: options.get(field),
            secret: field.is_secret(),
            required: true,
        })
        .collect()
}

/// Creates MCP server summaries from the shared provider configuration.
/// 从共享 Provider 配置创建 MCP 服务器摘要。
pub(crate) fn mcp_servers(
    provider_configuration: &ProviderConfigurationFile,
) -> Vec<McpServerSummary> {
    provider_configuration
        .mcp
        .servers
        .iter()
        .map(|(server_id, entry)| McpServerSummary {
            id: server_id.clone(),
            name: entry.name.clone(),
            enabled: entry.enabled,
            transport: entry.transport.label().to_string(),
            command: entry.command.clone(),
            args: entry.args.clone(),
            url: entry.url.clone().unwrap_or_default(),
            bearer_token: entry.bearer_token.clone().unwrap_or_default(),
            headers: key_value_summaries(&entry.headers),
            headers_from_env: key_value_summaries(&entry.headers_from_env),
            oauth_client_id: entry
                .oauth
                .as_ref()
                .and_then(|oauth| oauth.client_id.clone())
                .unwrap_or_default(),
            oauth_client_secret: entry
                .oauth
                .as_ref()
                .and_then(|oauth| oauth.client_secret.clone())
                .unwrap_or_default(),
            oauth_scopes: entry
                .oauth
                .as_ref()
                .map(|oauth| oauth.scopes.clone())
                .unwrap_or_default(),
            oauth_redirect_uri: entry
                .oauth
                .as_ref()
                .and_then(|oauth| oauth.redirect_uri.clone())
                .unwrap_or_default(),
            cwd: entry.cwd.clone().unwrap_or_default(),
            env: key_value_summaries(&entry.env),
            env_passthrough: entry.env_passthrough.clone(),
        })
        .collect()
}

/// Creates MCP runtime tool summaries from the cached MCP runtime catalog.
/// 从缓存的 MCP 运行时目录创建 MCP 工具摘要。
pub(crate) fn mcp_tools(mcp_catalog: &TauriMcpRuntimeCatalog) -> Vec<McpToolSummary> {
    mcp_catalog
        .tools
        .iter()
        .map(|tool| McpToolSummary {
            id: tool.id.clone(),
            server_id: tool.server_id.clone(),
            name: tool.name.clone(),
            description: tool.description.clone(),
            origin_tool_id: tool.origin_tool_id.clone(),
        })
        .collect()
}

/// Converts one ordered map into WebUI key/value summaries.
/// 将单个有序映射转换为 WebUI 键值摘要。
pub(crate) fn key_value_summaries(values: &BTreeMap<String, String>) -> Vec<KeyValueSummary> {
    values
        .iter()
        .map(|(key, value)| KeyValueSummary {
            key: key.clone(),
            value: value.clone(),
        })
        .collect()
}

/// Creates VMM memory summaries from restored workspace state.
/// 从已恢复工作区状态创建 VMM 记忆摘要。
pub(crate) fn memories(
    workspace: &vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
) -> Vec<MemorySummary> {
    vec![
        MemorySummary {
            id: String::from("projects"),
            title: String::from("项目绑定"),
            category: String::from("Project"),
            state: format!("已恢复 {} 个项目", workspace.task_projects.len()),
        },
        MemorySummary {
            id: String::from("sessions"),
            title: String::from("会话绑定"),
            category: String::from("Conversation"),
            state: format!(
                "已恢复 {} 个会话运行态",
                workspace.session_runtime_state.len()
            ),
        },
    ]
}
