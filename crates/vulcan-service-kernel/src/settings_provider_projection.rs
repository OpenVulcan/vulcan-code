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

/// Projects credential-scoped public AI catalogs into the shared model-picker shape.
/// 把凭据作用域公共 AI 目录投影为共享模型选择器结构。
///
/// # Parameters
/// 参数
/// `configuration` owns connection selection and `runtime` owns memory-only catalogs.
/// `configuration` 持有连接选择，`runtime` 持有仅内存目录。
///
/// # Returns
/// 返回值
/// One provider row per credential connection so duplicate model accounts remain explicit.
/// 每个凭据连接对应一行供应商，使重复模型账号保持明确。
pub(crate) fn public_ai_provider_rows(
    configuration: &vulcan_provider_adapters::PersistedAiConfiguration,
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
) -> Result<Vec<ProviderSummary>, String> {
    let active_selection = runtime.resolve_active_selection(configuration).ok();
    public_ai_provider_rows_for_selection(configuration, runtime, active_selection.as_ref())
}

/// Projects credential-scoped model rows using an explicit effective workbench selection.
/// 使用明确的工作台生效选择投影凭据作用域模型行。
///
/// # Parameters
/// 参数
/// `configuration` owns account preferences, `runtime` owns in-memory catalogs, and
/// `active_selection` is the selected session model after degradation.
/// `configuration` 持有账号偏好，`runtime` 持有内存目录，`active_selection`
/// 是完成降级后的当前会话模型。
///
/// # Returns
/// 返回值
/// One provider row per credential connection with exactly one effective active model row.
/// 返回每个凭据连接一行，并且仅标记一个生效模型行为当前项。
pub(crate) fn public_ai_provider_rows_for_selection(
    configuration: &vulcan_provider_adapters::PersistedAiConfiguration,
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    active_selection: Option<&vulcan_provider_adapters::ResolvedPublicAiSelection>,
) -> Result<Vec<ProviderSummary>, String> {
    let catalogs = vulcan_provider_adapters::RuntimeConnectionCatalogs::from_snapshot(
        runtime.catalog_snapshot()?,
    );
    let errors = runtime.catalog_error_snapshot()?;
    let providers = runtime.effective_providers(configuration)?;
    let mut rows = Vec::new();
    for connection in configuration.connection_graph.connections.values() {
        let provider = providers
            .provider(connection.provider_id.as_str())
            .ok_or_else(|| format!("Provider `{}` is not registered.", connection.provider_id))?;
        let models = catalogs
            .get(connection.id.as_str())
            .map(|catalog| {
                catalog
                    .models
                    .iter()
                    .map(|model| {
                        let settings_key =
                            vulcan_provider_adapters::PersistedAiConfiguration::connection_model_key(
                                connection.id.as_str(),
                                model.id.as_str(),
                            );
                        let secondary_route = configuration
                            .secondary_routes
                            .get(settings_key.as_str())
                            .and_then(|selection| {
                                provider_secondary_route_summary_from_public_selection(
                                    connection.provider_id.as_str(),
                                    model.id.as_str(),
                                    selection,
                                )
                            });
                        let active = active_selection.is_some_and(|selection| {
                            selection.route.provider.id == connection.provider_id
                                && selection.route.connection.id == connection.id
                                && selection.model.id == model.id
                        });
                        let preferred_connection = configuration
                            .connection_graph
                            .resolve_connection(
                                connection.provider_id.as_str(),
                                model.id.as_str(),
                                &catalogs,
                            )?
                            .id
                            == connection.id;
                        Ok(ProviderModelSummary {
                            id: model.id.clone(),
                            name: model.display_name.clone(),
                            active,
                            preferred_connection,
                            enabled: true,
                            status: None,
                            family: None,
                            context_limit: model
                                .capabilities
                                .context_limit
                                .and_then(|value| usize::try_from(value).ok()),
                            input_limit: None,
                            output_limit: model
                                .capabilities
                                .output_limit
                                .and_then(|value| usize::try_from(value).ok()),
                            reasoning: Some(!model.capabilities.reasoning_efforts.is_empty()),
                            attachment: Some(!model.capabilities.input_resources.is_empty()),
                            input_modalities: model
                                .capabilities
                                .input_resources
                                .iter()
                                .map(public_resource_kind_label)
                                .collect(),
                            output_modalities: model
                                .capabilities
                                .output_resources
                                .iter()
                                .map(public_resource_kind_label)
                                .collect(),
                            reasoning_profile_json: None,
                            supports_secondary_routes: connection.provider_id == "openrouter",
                            secondary_route,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()
            })
            .transpose()?
            .unwrap_or_default();
        let active = models.iter().any(|model| model.active);
        let status = errors
            .get(connection.id.as_str())
            .map(|error| format!("同步失败：{error}"))
            .unwrap_or_else(|| {
                if catalogs.get(connection.id.as_str()).is_some() {
                    String::from("目录可用")
                } else {
                    String::from("目录未同步")
                }
            });
        rows.push(ProviderSummary {
            id: connection.id.clone(),
            source_provider_id: Some(connection.provider_id.clone()),
            connection_id: Some(connection.id.clone()),
            account_name: Some(connection.display_name.clone()),
            name: format!("{} · {}", provider.display_name, connection.display_name),
            provider_display_name: Some(provider.display_name.clone()),
            status,
            configured: true,
            active,
            catalog_models: models.clone(),
            models,
            fields: Vec::new(),
        });
    }
    Ok(rows)
}

/// Resolves the active public model capability snapshot without exposing credential material.
/// 在不暴露凭据材料的前提下解析当前公共模型能力快照。
///
/// # Parameters
/// 参数
/// `runtime` is the optional process-local public AI runtime.
/// `runtime` 是可选的进程内公共 AI 运行时。
///
/// # Returns
/// 返回值
/// The active model capabilities when configuration and selection are both valid.
/// 当配置与选择均有效时返回当前模型能力。
pub(crate) fn active_public_ai_model_capabilities(
    runtime: Option<&vulcan_provider_adapters::PublicAiRuntime>,
) -> Option<vulcan_ai_contract::ModelCapabilities> {
    let runtime = runtime?;
    let configuration = runtime.configuration_snapshot().ok()?;
    runtime
        .resolve_active_selection(&configuration)
        .ok()
        .map(|selection| selection.model.capabilities)
}

/// Maps one public resource kind to the established WebUI modality label.
/// 把一个公共资源类别映射为既有 WebUI 模态标签。
///
/// # Parameters
/// 参数
/// `kind` is the exact registered public resource kind.
/// `kind` 是精确注册的公共资源类别。
///
/// # Returns
/// 返回值
/// A stable lowercase modality label.
/// 返回稳定的小写模态标签。
fn public_resource_kind_label(kind: &vulcan_ai_contract::ResourceKind) -> String {
    match kind {
        vulcan_ai_contract::ResourceKind::Image => String::from("image"),
        vulcan_ai_contract::ResourceKind::Audio => String::from("audio"),
        vulcan_ai_contract::ResourceKind::Video => String::from("video"),
        vulcan_ai_contract::ResourceKind::File => String::from("file"),
    }
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
