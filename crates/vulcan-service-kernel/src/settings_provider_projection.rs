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

/// Returns the process-local immutable selection owned by one exact session when coherent.
/// 当租约一致时返回一个精确会话拥有的进程内不可变选择。
///
/// # Parameters
/// 参数
/// `runtime` owns process-local leases, `session_id` selects one owner, and `runtime_state`
/// supplies the persisted target and optional non-secret lease identity.
/// `runtime` 持有进程内租约，`session_id` 选择一个所有者，`runtime_state` 提供持久化目标与
/// 可选的非秘密租约身份。
///
/// # Returns
/// 返回值
/// The frozen selection only when its target and persisted identity remain coherent.
/// 仅当目标与持久化身份仍一致时返回冻结选择。
pub(crate) fn session_process_public_ai_selection(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    session_id: crate::SessionId,
    runtime_state: Option<&vulcan_workbench_model::SessionRuntimeStateEntry>,
) -> Result<Option<vulcan_provider_adapters::ResolvedPublicAiSelection>, String> {
    let Some(runtime_state) = runtime_state else {
        return Ok(None);
    };
    let Some(selection) = runtime.session_selection_lease(session_id.to_string().as_str())? else {
        return Ok(None);
    };
    // targetMatches prevents a stale process entry from outranking an explicit persisted model change.
    // targetMatches 防止陈旧进程条目覆盖显式持久化模型变更。
    let target_matches = runtime_state.active_provider_id.as_deref()
        == Some(selection.route.provider.id.as_str())
        && runtime_state.active_connection_id.as_deref()
            == Some(selection.route.connection.id.as_str())
        && runtime_state.active_model_id.as_deref() == Some(selection.model.id.as_str());
    if !target_matches {
        return Ok(None);
    }
    if let Some(expected_identity) = runtime_state.provider_route_lease.as_ref() {
        // actualIdentity is derived by the same authoritative projection used during preparation.
        // actualIdentity 使用准备阶段的同一权威投影生成。
        let actual_identity =
            vulcan_agent_runtime::agent_runtime::provider::provider_route_lease_identity(
                &selection,
            );
        if !vulcan_agent_runtime::agent_runtime::provider::provider_route_lease_identity_matches(
            expected_identity,
            &actual_identity,
        ) {
            return Ok(None);
        }
    }
    Ok(Some(selection))
}

/// Resolves the model selection used by session-sensitive workbench projections.
/// 解析会话敏感工作台投影使用的模型选择。
///
/// # Parameters
/// 参数
/// `configuration` is the current catalog authority, `runtime` owns active leases, and the
/// optional session inputs identify the selected conversation.
/// `configuration` 是当前目录权威，`runtime` 持有活动租约，可选会话输入标识当前会话。
///
/// # Returns
/// 返回值
/// The immutable session lease first, then the current exact session target, provider/model
/// fallback, or global active selection.
/// 依次返回不可变会话租约、当前精确会话目标、供应商与模型回退或全局激活选择。
pub(crate) fn public_ai_workbench_selection(
    configuration: &vulcan_provider_adapters::PersistedAiConfiguration,
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    session_id: Option<crate::SessionId>,
    runtime_state: Option<&vulcan_workbench_model::SessionRuntimeStateEntry>,
) -> Result<Option<vulcan_provider_adapters::ResolvedPublicAiSelection>, String> {
    if let Some(session_id) = session_id
        && let Some(selection) =
            session_process_public_ai_selection(runtime, session_id, runtime_state)?
    {
        return Ok(Some(selection));
    }
    public_ai_workbench_selection_from_runtime_state(configuration, runtime, runtime_state)
}

/// Resolves one Workbench selection after an explicit route-lease revocation.
/// 在显式撤销路由租约后解析一份工作台选择。
///
/// # Parameters
/// 参数
/// `configuration` and `runtime` own the activated provider generation; `runtime_state` is the
/// already-reconciled selected session row whose former process-local lease must not win.
/// `configuration` 与 `runtime` 持有已激活供应商代际；`runtime_state` 是已完成协调的选中
/// 会话行，其原进程内租约不得继续胜出。
///
/// # Returns
/// 返回值
/// The exact reconciled session selection, configured global selection, or `None`.
/// 返回精确的已协调会话选择、已配置全局选择或 `None`。
pub(crate) fn public_ai_workbench_selection_after_route_revocation(
    configuration: &vulcan_provider_adapters::PersistedAiConfiguration,
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    runtime_state: Option<&vulcan_workbench_model::SessionRuntimeStateEntry>,
) -> Result<Option<vulcan_provider_adapters::ResolvedPublicAiSelection>, String> {
    public_ai_workbench_selection_from_runtime_state(configuration, runtime, runtime_state)
}

/// Resolves a Workbench selection from persisted session fields and the configured global value.
/// 从持久会话字段与已配置全局值解析工作台选择。
///
/// # Parameters
/// 参数
/// `configuration` and `runtime` own the exact activated generation; `runtime_state` optionally
/// supplies one session's persisted provider, connection, and model tuple.
/// `configuration` 与 `runtime` 持有精确已激活代际；`runtime_state` 可选提供一个会话持久化的
/// 供应商、连接与模型三元组。
///
/// # Returns
/// 返回值
/// The first exact persisted selection that resolves, then the configured global selection.
/// 返回首个可解析的精确持久选择，其次返回已配置全局选择。
fn public_ai_workbench_selection_from_runtime_state(
    configuration: &vulcan_provider_adapters::PersistedAiConfiguration,
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    runtime_state: Option<&vulcan_workbench_model::SessionRuntimeStateEntry>,
) -> Result<Option<vulcan_provider_adapters::ResolvedPublicAiSelection>, String> {
    // exactSelection follows the persisted account before any provider/model-only compatibility lookup.
    // exactSelection 在任何仅供应商与模型的兼容查找前遵循持久化账号。
    let exact_selection = runtime_state
        .and_then(|runtime_state| {
            runtime_state
                .active_provider_id
                .as_deref()
                .zip(runtime_state.active_connection_id.as_deref())
                .zip(runtime_state.active_model_id.as_deref())
        })
        .and_then(|((provider_id, connection_id), model_id)| {
            runtime
                .resolve_connection_model_selection(
                    configuration,
                    connection_id,
                    provider_id,
                    model_id,
                )
                .ok()
        });
    if exact_selection.is_some() {
        return Ok(exact_selection);
    }
    // providerModelSelection preserves the established compatibility behavior for old session state
    // that predates connection ownership.
    // providerModelSelection 为早于账号归属字段的旧会话状态保留既有兼容行为。
    let provider_model_selection = runtime_state
        .and_then(|runtime_state| {
            runtime_state
                .active_provider_id
                .as_deref()
                .zip(runtime_state.active_model_id.as_deref())
        })
        .and_then(|(provider_id, model_id)| {
            runtime
                .resolve_model_selection(configuration, provider_id, model_id)
                .ok()
        });
    Ok(provider_model_selection.or_else(|| runtime.resolve_active_selection(configuration).ok()))
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
    // runtimeConfiguration keeps every provider lacking executable authority out of the shared
    // picker without mutating persisted connections or repair bytes.
    // runtimeConfiguration 使全部缺少可执行权威的供应商不进入共享选择器，同时不修改持久化连接
    // 或修复源字节。
    let runtime_configuration = runtime.runtime_configuration_snapshot(configuration)?;
    let configuration = &runtime_configuration;
    // Rebuild the effective snapshot from immutable catalogs and persisted overlays so model rows
    // immediately reflect capability edits instead of a stale in-memory catalog copy.
    // 从不可变目录与持久化覆盖重新构建有效快照，确保模型行立即反映能力编辑而不是旧的内存目录副本。
    let mut catalog_snapshot = runtime.resolved_model_catalog_snapshot(configuration)?;
    catalog_snapshot.retain(|connection_id, _| {
        !configuration.connection_is_pending_validation(connection_id.as_str())
    });
    let catalogs =
        vulcan_provider_adapters::RuntimeConnectionCatalogs::from_snapshot(catalog_snapshot);
    let errors = runtime.catalog_error_snapshot()?;
    let providers = runtime.effective_providers(configuration)?;
    let mut rows = Vec::new();
    for connection in configuration.connection_graph.connections.values() {
        if configuration.connection_is_pending_validation(connection.id.as_str()) {
            continue;
        }
        let provider = providers
            .provider(connection.provider_id.as_str())
            .ok_or_else(|| format!("Provider `{}` is not registered.", connection.provider_id))?;
        let entry = provider
            .service_entries
            .iter()
            .find(|entry| entry.id == connection.service_entry_id)
            .ok_or_else(|| {
                format!(
                    "Service entry `{}` is not registered for provider `{}`.",
                    connection.service_entry_id, connection.provider_id
                )
            })?;
        // Secondary-route UI capability follows the manifest-declared request primitive.
        // 二级路由 UI 能力跟随 Manifest 声明的请求原语。
        let supports_secondary_routes = entry.request_projector
            == vulcan_provider_adapters::RequestProjectorKind::RoutedCompatible;
        let mut catalog_models = catalogs
            .get(connection.id.as_str())
            .map(|catalog| {
                catalog
                    .models
                    .iter()
                    .filter(|model| {
                        vulcan_provider_adapters::model_supports_text_conversation(
                            &model.capabilities,
                        )
                    })
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
                        let enabled = runtime.model_is_enabled_for_connection(
                            configuration,
                            connection.id.as_str(),
                            model.id.as_str(),
                        )?;
                        Ok(ProviderModelSummary {
                            id: model.id.clone(),
                            name: model.display_name.clone(),
                            active,
                            preferred_connection,
                            enabled,
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
                            reasoning: Some(
                                vulcan_provider_adapters::model_supports_reasoning(
                                    &model.capabilities,
                                ),
                            ),
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
                            reasoning_selection: reasoning_selection(
                                &ProviderConfigurationFile::default(),
                                Some(&model.capabilities),
                            ),
                            supports_secondary_routes,
                            secondary_route,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()
            })
            .transpose()?
            .unwrap_or_default();
        if let Some(selection) = active_selection.filter(|selection| {
            selection.route.provider.id == connection.provider_id
                && selection.route.connection.id == connection.id
                && !catalog_models
                    .iter()
                    .any(|model| model.id == selection.model.id)
                && vulcan_provider_adapters::model_supports_text_conversation(
                    &selection.model.capabilities,
                )
        }) {
            // leasedModel keeps a removed or renamed catalog row visible only for the session that
            // still owns its immutable process-local route. It is not added to current catalogs and
            // therefore cannot be selected by new sessions.
            // leasedModel 仅为仍持有进程内不可变路由的会话保留已移除或重命名的目录行；它不会
            // 写回当前目录，因此新会话无法选择。
            let capabilities = &selection.model.capabilities;
            let leased_supports_secondary_routes = selection.route.entry.request_projector
                == vulcan_provider_adapters::RequestProjectorKind::RoutedCompatible;
            catalog_models.push(ProviderModelSummary {
                id: selection.model.id.clone(),
                name: selection.model.display_name.clone(),
                active: true,
                preferred_connection: true,
                enabled: true,
                status: Some(String::from("会话路由已冻结；重新选择模型后使用最新目录")),
                family: None,
                context_limit: capabilities
                    .context_limit
                    .and_then(|value| usize::try_from(value).ok()),
                input_limit: None,
                output_limit: capabilities
                    .output_limit
                    .and_then(|value| usize::try_from(value).ok()),
                reasoning: Some(vulcan_provider_adapters::model_supports_reasoning(
                    capabilities,
                )),
                attachment: Some(!capabilities.input_resources.is_empty()),
                input_modalities: capabilities
                    .input_resources
                    .iter()
                    .map(public_resource_kind_label)
                    .collect(),
                output_modalities: capabilities
                    .output_resources
                    .iter()
                    .map(public_resource_kind_label)
                    .collect(),
                reasoning_profile_json: None,
                reasoning_selection: reasoning_selection(
                    &ProviderConfigurationFile::default(),
                    Some(capabilities),
                ),
                supports_secondary_routes: leased_supports_secondary_routes,
                secondary_route: provider_secondary_route_summary_from_public_selection(
                    selection.route.provider.id.as_str(),
                    selection.model.id.as_str(),
                    &selection.route.secondary_route,
                ),
            });
        }
        let models = catalog_models
            .iter()
            .filter(|model| model.enabled)
            .cloned()
            .collect::<Vec<_>>();
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
            catalog_models,
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
