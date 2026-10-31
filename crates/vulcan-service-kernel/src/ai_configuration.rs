//! Service-kernel side of the AI configuration tools `mcp_config` and `provider_config`.
//! AI 配置工具 `mcp_config` 与 `provider_config` 的服务内核侧实现。
//!
//! Every change goes through the same commit path as the settings pages: MCP servers through
//! `commit_mcp_configuration_change` and the server removal service, provider connections through
//! the public AI settings services with their own request types. The permission preview and the
//! change plan the same request against the live configuration, so the user approves what runs.
//! Secrets never pass through the model: API keys are copied from Vulcan environment variables
//! named by the call, and existing MCP secrets are kept, never shown.
//! 所有修改都走与设置页相同的提交路径：MCP 服务器经 `commit_mcp_configuration_change` 与移除服务，
//! 供应商连接经公共 AI 设置服务及其自身请求类型。权限预览与修改针对实时配置规划同一请求，因此用户
//! 批准的就是将要运行的内容。密钥绝不经过模型：API Key 从调用指定的 Vulcan 环境变量复制，既有 MCP
//! 密钥被保留且从不展示。

use super::*;

use std::sync::Weak;

use serde_json::json;
use vulcan_agent_runtime::agent_runtime::tool_permission::{
    ConfigurationFieldChange, ConfigurationPreview,
};
use vulcan_agent_runtime::model::SessionId;
use vulcan_agent_runtime::tooling::native::{
    ConfigurationOutcome, ConfigurationRequest, McpConfigRequest, NativeConfigurationHost,
    ProviderConfigRequest,
};
use vulcan_provider_adapters::{
    CredentialDirectoryVisibility, CredentialMode, CustomProviderAuthentication, ProviderKind,
    ProviderRegistry, ServiceEntry,
};
use vulcan_workspace_store::workspace_store::environment_variable_name_key;

use crate::session_runtime_registry_v1::SessionRuntimeRegistry;

/// Lists and changes configuration for the configuration tools.
/// 为配置工具列出并修改配置。
pub(crate) struct KernelConfigurationHost {
    /// Process session registry; resolves root sessions and their environment.
    /// 进程会话注册表；解析根会话及其环境。
    registry: Arc<SessionRuntimeRegistry>,
    /// Service state; weak, because the state owns the runtime that owns this host.
    /// 服务状态；使用弱引用，因为该状态拥有持有本宿主的运行时。
    state: Weak<Mutex<ServiceBridgeState>>,
    /// Publishes the workbench and settings after a change, like the settings pages do.
    /// 修改后像设置页一样发布工作台与设置。
    app: ServiceEventEmitter,
}

impl std::fmt::Debug for KernelConfigurationHost {
    /// Renders a label without exposing service state.
    /// 渲染不暴露服务状态的标签。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("KernelConfigurationHost")
            .finish_non_exhaustive()
    }
}

impl KernelConfigurationHost {
    /// Builds the host over the process session registry and the service state.
    /// 基于进程会话注册表与服务状态构建宿主。
    pub(crate) fn new(
        registry: Arc<SessionRuntimeRegistry>,
        state: Weak<Mutex<ServiceBridgeState>>,
        app: ServiceEventEmitter,
    ) -> Self {
        Self {
            registry,
            state,
            app,
        }
    }
}

#[async_trait::async_trait]
impl NativeConfigurationHost for KernelConfigurationHost {
    async fn run(
        &self,
        session_id: SessionId,
        request: ConfigurationRequest,
    ) -> Result<ConfigurationOutcome, String> {
        let state = self
            .state
            .upgrade()
            .ok_or_else(|| String::from("The service is shutting down."))?;
        if !request.is_list() {
            ensure_user_session(&self.registry, session_id)?;
        }
        match request {
            ConfigurationRequest::Mcp(McpConfigRequest::List) => list_mcp_servers(&state),
            ConfigurationRequest::Mcp(save @ McpConfigRequest::Save { .. }) => {
                save_mcp_server(&self.app, &state, &save).await
            }
            ConfigurationRequest::Mcp(McpConfigRequest::Remove { server_id }) => {
                let server_id = server_id.trim().to_string();
                service_remove_mcp_server_configuration(
                    self.app.clone(),
                    state.clone(),
                    server_id.clone(),
                )
                .await?;
                Ok(outcome(
                    format!("Removed MCP server `{server_id}`; no session loads it any more."),
                    json!({ "action": "remove", "serverId": server_id }),
                ))
            }
            ConfigurationRequest::Provider(ProviderConfigRequest::List) => list_providers(&state),
            ConfigurationRequest::Provider(request) => {
                change_provider(&self.registry, &self.app, &state, session_id, request).await
            }
        }
    }
}

/// Describes the change one configuration call would make; errors deny the call unasked.
/// 描述一次配置调用将做出的变更；出错时直接拒绝而不询问。
pub(crate) async fn configuration_preview(
    registry: &SessionRuntimeRegistry,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    tool_name: &str,
    args: &serde_json::Value,
) -> Result<ConfigurationPreview, String> {
    let request = ConfigurationRequest::parse(tool_name, args)?;
    if request.is_list() {
        return Err(String::from("Listing configuration needs no approval."));
    }
    ensure_user_session(registry, session_id)?;
    match request {
        ConfigurationRequest::Mcp(McpConfigRequest::Remove { server_id }) => {
            let configuration = provider_configuration_from_state(state)?;
            let server_id = server_id.trim();
            let entry = configuration
                .mcp
                .servers
                .get(server_id)
                .ok_or_else(|| format!("MCP server `{server_id}` not found."))?;
            let notices = if entry.enabled {
                vec![String::from("removes_for_all_sessions")]
            } else {
                Vec::new()
            };
            Ok(ConfigurationPreview {
                target_kind: String::from("mcp_server"),
                target: server_id.to_string(),
                action: String::from("remove"),
                changes: field_changes(&mcp_entry_fields(entry), &[]),
                notices,
            })
        }
        ConfigurationRequest::Mcp(save) => {
            let configuration = provider_configuration_from_state(state)?;
            Ok(plan_mcp_save(&configuration, &save)?.preview())
        }
        ConfigurationRequest::Provider(request) => {
            Ok(plan_provider_change(registry, state, session_id, &request)
                .await?
                .preview)
        }
    }
}

/// Refuses configuration changes from spawned agents; only the user's own session may change it.
/// 拒绝派生代理的配置修改；只有用户自己的会话可以修改。
fn ensure_user_session(
    registry: &SessionRuntimeRegistry,
    session_id: SessionId,
) -> Result<(), String> {
    if crate::tool_permission_host::root_session_of(registry, session_id)? == session_id {
        Ok(())
    } else {
        Err(String::from(
            "Spawned agents cannot change configuration; only the user's own session can.",
        ))
    }
}

/// Builds one outcome whose text the model reads and whose payload clients render.
/// 构建一个结果：模型读取其文本，客户端渲染其载荷。
fn outcome(text: String, payload: serde_json::Value) -> ConfigurationOutcome {
    ConfigurationOutcome { text, payload }
}

/// Builds one listing outcome; the model reads the same JSON the clients get.
/// 构建一个列表结果；模型读取与客户端相同的 JSON。
fn listing(payload: serde_json::Value) -> Result<ConfigurationOutcome, String> {
    let text = serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?;
    Ok(outcome(text, payload))
}

/// Field values in display order; equal fields are dropped from the change list.
/// 按展示顺序排列的字段值；相等的字段会从变更列表中去除。
type FieldValues = Vec<(&'static str, Option<String>)>;

/// The fields whose value differs between `before` and `after`, in `after`'s order first.
/// `before` 与 `after` 之间值不同的字段，先按 `after` 的顺序排列。
fn field_changes(
    before: &[(&'static str, Option<String>)],
    after: &[(&'static str, Option<String>)],
) -> Vec<ConfigurationFieldChange> {
    let lookup = |values: &[(&'static str, Option<String>)], field: &str| {
        values
            .iter()
            .find(|(name, _)| *name == field)
            .and_then(|(_, value)| value.clone())
    };
    let mut fields: Vec<&'static str> = after.iter().map(|(field, _)| *field).collect();
    for (field, _) in before {
        if !fields.contains(field) {
            fields.push(field);
        }
    }
    fields
        .into_iter()
        .filter_map(|field| {
            let before = lookup(before, field);
            let after = lookup(after, field);
            (before != after).then(|| ConfigurationFieldChange {
                field: field.to_string(),
                before,
                after,
            })
        })
        .collect()
}

/// Joins a list for display, or `None` when it is empty.
/// 将列表拼接为展示文本；为空时返回 `None`。
fn joined(values: impl IntoIterator<Item = String>) -> Option<String> {
    let values: Vec<String> = values.into_iter().collect();
    (!values.is_empty()).then(|| values.join(", "))
}

/// Stable label of one MCP transport.
/// 单个 MCP 传输的稳定标签。
fn transport_label(transport: &PersistedMcpTransportKind) -> &'static str {
    match transport {
        PersistedMcpTransportKind::Stdio => "stdio",
        PersistedMcpTransportKind::Http => "http",
        PersistedMcpTransportKind::Sse => "sse",
    }
}

/// The non-secret fields of one MCP server entry; literal values only appear as names.
/// 单个 MCP 服务器条目的非密钥字段；字面值只以名称出现。
fn mcp_entry_fields(entry: &PersistedMcpServerEntry) -> FieldValues {
    vec![
        ("name", Some(entry.name.clone())),
        ("enabled", Some(entry.enabled.to_string())),
        (
            "transport",
            Some(transport_label(&entry.transport).to_string()),
        ),
        (
            "command",
            (!entry.command.is_empty()).then(|| entry.command.clone()),
        ),
        (
            "args",
            (!entry.args.is_empty())
                .then(|| serde_json::to_string(&entry.args).unwrap_or_default()),
        ),
        ("cwd", entry.cwd.clone()),
        (
            "env_passthrough",
            joined(entry.env_passthrough.iter().cloned()),
        ),
        ("url", entry.url.clone()),
        (
            "headers_from_env",
            joined(
                entry
                    .headers_from_env
                    .iter()
                    .map(|(header, variable)| format!("{header}={variable}")),
            ),
        ),
    ]
}

/// Whether an entry holds secrets or literal values the tool cannot set or show.
/// 条目是否持有工具无法设置或展示的密钥或字面值。
fn mcp_entry_has_hidden_values(entry: &PersistedMcpServerEntry) -> bool {
    !entry.env.is_empty()
        || entry.bearer_token.is_some()
        || !entry.headers.is_empty()
        || entry.oauth.is_some()
}

/// One planned MCP server save, computed against one configuration.
/// 针对一份配置计算出的一次 MCP 服务器保存计划。
struct McpSavePlan {
    /// Server identifier.
    /// 服务器标识。
    server_id: String,
    /// Entry before the change; absent when creating.
    /// 变更前的条目；创建时缺省。
    before: Option<PersistedMcpServerEntry>,
    /// Entry after the change.
    /// 变更后的条目。
    after: PersistedMcpServerEntry,
    /// Whether a transport switch drops values the tool cannot carry over.
    /// 传输切换是否丢弃工具无法沿用的值。
    drops_hidden_values: bool,
}

impl McpSavePlan {
    /// The change shown to the user.
    /// 展示给用户的变更。
    fn preview(&self) -> ConfigurationPreview {
        let before = self
            .before
            .as_ref()
            .map(mcp_entry_fields)
            .unwrap_or_default();
        let was_enabled = self.before.as_ref().is_some_and(|entry| entry.enabled);
        let mut notices = Vec::new();
        if self.after.enabled && !was_enabled {
            notices.push(String::from("activates_for_all_sessions"));
        }
        if !self.after.enabled && was_enabled {
            notices.push(String::from("deactivates_for_all_sessions"));
        }
        if self.drops_hidden_values {
            notices.push(String::from("drops_secret_fields"));
        }
        ConfigurationPreview {
            target_kind: String::from("mcp_server"),
            target: self.server_id.clone(),
            action: String::from(if self.before.is_some() {
                "update"
            } else {
                "create"
            }),
            changes: field_changes(&before, &mcp_entry_fields(&self.after)),
            notices,
        }
    }
}

/// Plans one `mcp_config` save against `configuration`: omitted fields keep their value, secrets
/// and literal values are kept, and switching between stdio and remote clears the other family.
/// 针对 `configuration` 规划一次 `mcp_config` 保存：省略的字段保持原值，密钥与字面值被保留，
/// 在 stdio 与远端之间切换时清除另一类字段。
fn plan_mcp_save(
    configuration: &ProviderConfigurationFile,
    request: &McpConfigRequest,
) -> Result<McpSavePlan, String> {
    let McpConfigRequest::Save {
        server_id,
        name,
        enabled,
        transport,
        command,
        args,
        cwd,
        env_passthrough,
        url,
        headers_from_env,
    } = request
    else {
        return Err(String::from(
            "Only a save request can be planned as a save.",
        ));
    };
    let requested_name = name.as_deref().and_then(trim_to_option);
    let (server_id, before) = match server_id.as_deref().and_then(trim_to_option) {
        Some(server_id) => {
            let entry = configuration
                .mcp
                .servers
                .get(&server_id)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "MCP server `{server_id}` not found; omit `server_id` to create a server."
                    )
                })?;
            (server_id, Some(entry))
        }
        None => {
            let name = requested_name
                .as_deref()
                .ok_or_else(|| String::from("`name` is required when creating an MCP server."))?;
            let server_id = stable_identifier_from_label(name);
            if configuration.mcp.servers.contains_key(&server_id) {
                return Err(format!(
                    "MCP server `{server_id}` already exists; pass `server_id` to update it."
                ));
            }
            (server_id, None)
        }
    };
    let transport = match transport.as_deref() {
        Some(transport) => parse_mcp_transport(transport)?,
        None => before
            .as_ref()
            .map(|entry| entry.transport)
            .ok_or_else(|| String::from("`transport` is required when creating an MCP server."))?,
    };
    let is_stdio = |transport: &PersistedMcpTransportKind| {
        matches!(transport, PersistedMcpTransportKind::Stdio)
    };
    // Values of the same transport family carry over; a family switch starts that family empty.
    // 同一传输族的值会沿用；切换传输族时该族从空开始。
    let kept = before
        .as_ref()
        .filter(|entry| is_stdio(&entry.transport) == is_stdio(&transport));
    let drops_hidden_values = before
        .as_ref()
        .is_some_and(|entry| kept.is_none() && mcp_entry_has_hidden_values(entry));
    let name = requested_name
        .or_else(|| before.as_ref().map(|entry| entry.name.clone()))
        .ok_or_else(|| String::from("`name` must not be empty."))?;
    let enabled = enabled
        .or_else(|| before.as_ref().map(|entry| entry.enabled))
        .unwrap_or(false);
    let names = |values: &[String]| -> Vec<String> {
        values
            .iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    };
    let after = if is_stdio(&transport) {
        let command = command
            .as_deref()
            .map(str::trim)
            .map(ToOwned::to_owned)
            .or_else(|| kept.map(|entry| entry.command.clone()))
            .filter(|command| !command.is_empty())
            .ok_or_else(|| String::from("`command` is required for a stdio MCP server."))?;
        PersistedMcpServerEntry {
            name,
            enabled,
            transport,
            command,
            // Arguments keep their exact boundaries, like the settings page keeps them.
            // 参数保持精确边界，与设置页一致。
            args: args
                .clone()
                .or_else(|| kept.map(|entry| entry.args.clone()))
                .unwrap_or_default(),
            url: None,
            bearer_token: None,
            headers: BTreeMap::new(),
            headers_from_env: BTreeMap::new(),
            oauth: None,
            cwd: match cwd {
                Some(cwd) => trim_to_option(cwd),
                None => kept.and_then(|entry| entry.cwd.clone()),
            },
            env: kept.map(|entry| entry.env.clone()).unwrap_or_default(),
            env_passthrough: match env_passthrough {
                Some(values) => names(values),
                None => kept
                    .map(|entry| entry.env_passthrough.clone())
                    .unwrap_or_default(),
            },
        }
    } else {
        let url = url
            .as_deref()
            .map(str::trim)
            .map(ToOwned::to_owned)
            .or_else(|| kept.and_then(|entry| entry.url.clone()))
            .filter(|url| !url.is_empty())
            .ok_or_else(|| String::from("`url` is required for a remote MCP server."))?;
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(String::from(
                "The remote MCP server URL must start with `http://` or `https://`.",
            ));
        }
        let headers_from_env = match headers_from_env {
            Some(headers) => {
                let mut mapped = BTreeMap::new();
                for (header, variable) in headers {
                    let (header, variable) = (header.trim(), variable.trim());
                    if header.is_empty() || variable.is_empty() {
                        return Err(String::from(
                            "Each `headers_from_env` entry needs a header name and a variable name.",
                        ));
                    }
                    mapped.insert(header.to_string(), variable.to_string());
                }
                mapped
            }
            None => kept
                .map(|entry| entry.headers_from_env.clone())
                .unwrap_or_default(),
        };
        PersistedMcpServerEntry {
            name,
            enabled,
            transport,
            command: String::new(),
            args: Vec::new(),
            url: Some(url),
            bearer_token: kept.and_then(|entry| entry.bearer_token.clone()),
            headers: kept.map(|entry| entry.headers.clone()).unwrap_or_default(),
            headers_from_env,
            oauth: kept.and_then(|entry| entry.oauth.clone()),
            cwd: None,
            env: BTreeMap::new(),
            env_passthrough: Vec::new(),
        }
    };
    Ok(McpSavePlan {
        server_id,
        before,
        after,
        drops_hidden_values,
    })
}

/// Saves one MCP server through the shared commit path, planning against the configuration the
/// commit holds.
/// 通过共享提交路径保存一个 MCP 服务器，并针对提交持有的配置进行规划。
async fn save_mcp_server(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    request: &McpConfigRequest,
) -> Result<ConfigurationOutcome, String> {
    let mut saved = None;
    commit_mcp_configuration_change(
        app,
        state,
        |configuration| {
            let plan = plan_mcp_save(configuration, request)?;
            configuration
                .mcp
                .servers
                .insert(plan.server_id.clone(), plan.after.clone());
            saved = Some((plan.server_id, plan.before.is_none(), plan.after.enabled));
            Ok(())
        },
        "Failed to save the MCP server",
    )
    .await?;
    let (server_id, created, enabled) =
        saved.ok_or_else(|| String::from("The MCP server change was not planned."))?;
    let verb = if created { "Created" } else { "Updated" };
    let state_text = if enabled {
        "It is enabled for every session; its tools reach this session from the next turn."
    } else {
        "It is disabled; save it with `enabled: true` to load it."
    };
    Ok(outcome(
        format!("{verb} MCP server `{server_id}`. {state_text}"),
        json!({
            "action": if created { "create" } else { "update" },
            "serverId": server_id,
            "enabled": enabled,
        }),
    ))
}

/// Lists the configured MCP servers with their live status; secrets appear only as names.
/// 列出已配置的 MCP 服务器及其实时状态；密钥只以名称出现。
fn list_mcp_servers(state: &SharedServiceBridgeState) -> Result<ConfigurationOutcome, String> {
    let configuration = provider_configuration_from_state(state)?;
    let inspections = agent_mcp_runtime_from_state(state)?.server_inspections();
    let servers: Vec<serde_json::Value> = configuration
        .mcp
        .servers
        .iter()
        .map(|(server_id, entry)| {
            let inspection = inspections.iter().find(|inspection| {
                inspection.state.source == "user" && inspection.state.id == *server_id
            });
            json!({
                "serverId": server_id,
                "name": entry.name,
                "enabled": entry.enabled,
                "transport": transport_label(&entry.transport),
                "command": (!entry.command.is_empty()).then_some(&entry.command),
                "args": entry.args,
                "cwd": entry.cwd,
                "envPassthrough": entry.env_passthrough,
                "envNames": entry.env.keys().collect::<Vec<_>>(),
                "url": entry.url,
                "headersFromEnv": entry.headers_from_env,
                "headerNames": entry.headers.keys().collect::<Vec<_>>(),
                "hasBearerToken": entry.bearer_token.is_some(),
                "oauth": entry.oauth.is_some(),
                "status": inspection.map(|inspection| inspection.state.status),
                "toolCount": inspection.map(|inspection| inspection.state.tool_count),
            })
        })
        .collect();
    listing(json!({ "servers": servers }))
}

/// Stable label of one provider kind.
/// 单个供应商种类的稳定标签。
fn provider_kind_label(kind: &ProviderKind) -> &'static str {
    match kind {
        ProviderKind::System => "system",
        ProviderKind::Custom => "custom",
    }
}

/// Service entries of one provider that accept an API key, each with its API-key profiles.
/// 一个供应商中接受 API Key 的服务入口，各自附带其 API Key 配置档。
fn api_key_entries(entries: &[ServiceEntry]) -> Vec<(&ServiceEntry, Vec<(&str, &str)>)> {
    entries
        .iter()
        .filter_map(|entry| {
            let profiles: Vec<(&str, &str)> = entry
                .credential_profiles
                .iter()
                .filter(|profile| profile.credential_mode == CredentialMode::Token)
                .map(|profile| (profile.id.as_str(), profile.display_name.as_str()))
                .collect();
            (!profiles.is_empty()).then_some((entry, profiles))
        })
        .collect()
}

/// Lists the providers that take an API key and every configured connection.
/// 列出接受 API Key 的供应商以及全部已配置连接。
fn list_providers(state: &SharedServiceBridgeState) -> Result<ConfigurationOutcome, String> {
    let (runtime, configuration) = public_ai_runtime_and_configuration(state)?;
    let providers = runtime.effective_providers(&configuration)?;
    let defaults = &configuration
        .connection_graph
        .account_preferences
        .provider_defaults;
    let provider_rows: Vec<serde_json::Value> = providers
        .providers()
        .into_iter()
        .filter(|provider| {
            provider.credential_directory_visibility != CredentialDirectoryVisibility::CoreAccount
        })
        .filter_map(|provider| {
            let entries = api_key_entries(&provider.service_entries);
            (!entries.is_empty()).then(|| {
                json!({
                    "providerId": provider.id,
                    "name": provider.display_name,
                    "kind": provider_kind_label(&provider.kind),
                    "serviceEntries": entries.iter().map(|(entry, profiles)| json!({
                        "serviceEntryId": entry.id,
                        "name": entry.display_name,
                        "baseUrl": entry.base_url,
                        "credentialProfiles": profiles.iter().map(|(id, name)| json!({
                            "credentialProfileId": id,
                            "name": name,
                        })).collect::<Vec<_>>(),
                    })).collect::<Vec<_>>(),
                })
            })
        })
        .collect();
    let connections: Vec<serde_json::Value> = configuration
        .connection_graph
        .connections
        .values()
        .map(|connection| {
            json!({
                "connectionId": connection.id,
                "providerId": connection.provider_id,
                "name": connection.display_name,
                "serviceEntryId": connection.service_entry_id,
                "credentialProfileId": connection.credential_profile_id,
                "browserSignIn": connection.credential_mode == CredentialMode::Auth,
                "enabled": configuration.connection_is_enabled(&connection.id),
                "pendingValidation": configuration.connection_is_pending_validation(&connection.id),
                "default": defaults.get(&connection.provider_id) == Some(&connection.id),
            })
        })
        .collect();
    listing(json!({ "providers": provider_rows, "connections": connections }))
}

/// One planned provider change: what the user approves and what the settings service receives.
/// 一次已规划的供应商变更：用户批准的内容以及设置服务收到的内容。
struct ProviderPlan {
    /// The change shown to the user.
    /// 展示给用户的变更。
    preview: ConfigurationPreview,
    /// The settings call that applies it.
    /// 应用它的设置调用。
    call: ProviderCall,
}

/// One call into the public AI settings services.
/// 对公共 AI 设置服务的一次调用。
enum ProviderCall {
    /// Adds an API-key connection; `request` is the settings page's add-connection request.
    /// 添加 API Key 连接；`request` 是设置页的添加连接请求。
    AddConnection {
        /// Settings request, holding the copied API key.
        /// 设置请求，持有复制来的 API Key。
        request: PublicAiConnectionAddRequest,
        /// Provider receiving the connection.
        /// 接收该连接的供应商。
        provider_id: String,
        /// Variable the key was copied from.
        /// 复制密钥的来源变量。
        api_key_env: String,
    },
    /// Creates a custom provider with its first connection.
    /// 创建自定义供应商及其首个连接。
    AddCustomProvider {
        /// Settings request, holding the copied API key.
        /// 设置请求，持有复制来的 API Key。
        request: PublicAiCustomProviderSaveRequest,
        /// Variable the key was copied from.
        /// 复制密钥的来源变量。
        api_key_env: String,
    },
    /// Enables or disables one connection.
    /// 启用或停用一个连接。
    SetEnabled {
        /// Connection to change.
        /// 要修改的连接。
        connection_id: String,
        /// New state.
        /// 新状态。
        enabled: bool,
    },
    /// Makes one connection its provider's default.
    /// 将一个连接设为其供应商的默认连接。
    SetDefault {
        /// Provider of the connection.
        /// 连接所属的供应商。
        provider_id: String,
        /// Connection to make the default.
        /// 要设为默认的连接。
        connection_id: String,
    },
}

/// Reads the API key from the session's Vulcan environment variable `name`.
/// 从会话的 Vulcan 环境变量 `name` 读取 API Key。
async fn api_key_from_environment(
    registry: &SessionRuntimeRegistry,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    name: &str,
) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(String::from(
            "`api_key_env` must name the Vulcan environment variable that holds the API key.",
        ));
    }
    let key = environment_variable_name_key(name);
    let variables =
        crate::environment_variables::session_effective_environment(registry, state, session_id)
            .await?;
    let value = variables
        .into_iter()
        .find(|variable| environment_variable_name_key(&variable.name) == key)
        .map(|variable| variable.value)
        .ok_or_else(|| {
            format!(
                "The Vulcan environment variable `{name}` is not set for this session. Ask the user to store the API key in it (the `env` tool can request the value), then retry."
            )
        })?;
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(format!(
            "The Vulcan environment variable `{name}` is empty; it must hold the API key."
        ));
    }
    Ok(value)
}

/// Refuses providers whose accounts are managed by account sign-in.
/// 拒绝其账号由账号登录管理的供应商。
fn ensure_generic_provider(providers: &ProviderRegistry, provider_id: &str) -> Result<(), String> {
    if providers.provider(provider_id).is_some_and(|provider| {
        provider.credential_directory_visibility == CredentialDirectoryVisibility::CoreAccount
    }) {
        return Err(String::from(
            "Core account connections are managed through account sign-in, not this tool.",
        ));
    }
    Ok(())
}

/// Plans one provider change against the live configuration.
/// 针对实时配置规划一次供应商变更。
async fn plan_provider_change(
    registry: &SessionRuntimeRegistry,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    request: &ProviderConfigRequest,
) -> Result<ProviderPlan, String> {
    let (runtime, configuration) = public_ai_runtime_and_configuration(state)?;
    let providers = runtime.effective_providers(&configuration)?;
    let preview =
        |target_kind: &str, target: &str, action: &str, changes, notices| ConfigurationPreview {
            target_kind: target_kind.to_string(),
            target: target.to_string(),
            action: action.to_string(),
            changes,
            notices,
        };
    let connection_of = |connection_id: &str| {
        configuration
            .connection_graph
            .connections
            .get(connection_id.trim())
            .ok_or_else(|| {
                format!(
                    "Connection `{}` not found; list the providers to see the connections.",
                    connection_id.trim()
                )
            })
    };
    match request {
        ProviderConfigRequest::List => Err(String::from("Listing needs no plan.")),
        ProviderConfigRequest::AddConnection {
            provider_id,
            service_entry_id,
            credential_profile_id,
            display_name,
            api_key_env,
        } => {
            let provider_id = provider_id.trim();
            let provider = providers.provider(provider_id).ok_or_else(|| {
                format!("Provider `{provider_id}` is not registered; list the providers first.")
            })?;
            ensure_generic_provider(&providers, provider_id)?;
            let entries = api_key_entries(&provider.service_entries);
            let choose = |requested: Option<&str>, available: Vec<&str>, what: &str| match requested
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                Some(requested) if available.contains(&requested) => Ok(requested.to_string()),
                Some(requested) => Err(format!(
                    "`{requested}` is not an API-key {what} of `{provider_id}`; choose one of: {}.",
                    available.join(", ")
                )),
                None if available.len() == 1 => Ok(available[0].to_string()),
                None if available.is_empty() => Err(format!(
                    "Provider `{provider_id}` takes no API key; add it in the settings page instead."
                )),
                None => Err(format!(
                    "Provider `{provider_id}` has several API-key {what}s; pass one of: {}.",
                    available.join(", ")
                )),
            };
            let entry_id = choose(
                service_entry_id.as_deref(),
                entries.iter().map(|(entry, _)| entry.id.as_str()).collect(),
                "service entry",
            )?;
            let (entry, profiles) = entries
                .iter()
                .find(|(entry, _)| entry.id == entry_id)
                .ok_or_else(|| format!("Service entry `{entry_id}` disappeared."))?;
            let profile_id = choose(
                credential_profile_id.as_deref(),
                profiles.iter().map(|(id, _)| *id).collect(),
                "credential profile",
            )?;
            let display_name = display_name
                .as_deref()
                .and_then(trim_to_option)
                .unwrap_or_else(|| provider.display_name.clone());
            let secret = api_key_from_environment(registry, state, session_id, api_key_env).await?;
            let changes = vec![
                ConfigurationFieldChange {
                    field: String::from("provider"),
                    before: None,
                    after: Some(provider.display_name.clone()),
                },
                ConfigurationFieldChange {
                    field: String::from("service_entry"),
                    before: None,
                    after: Some(entry.display_name.clone()),
                },
                ConfigurationFieldChange {
                    field: String::from("credential_profile"),
                    before: None,
                    after: Some(profile_id.clone()),
                },
                ConfigurationFieldChange {
                    field: String::from("api_key_env"),
                    before: None,
                    after: Some(api_key_env.trim().to_string()),
                },
            ];
            Ok(ProviderPlan {
                preview: preview(
                    "provider_connection",
                    &display_name,
                    "create",
                    changes,
                    vec![
                        String::from("copies_api_key"),
                        String::from("validates_with_provider"),
                    ],
                ),
                call: ProviderCall::AddConnection {
                    request: add_connection_request(
                        provider_id,
                        &entry_id,
                        &profile_id,
                        &display_name,
                        &secret,
                    )?,
                    provider_id: provider_id.to_string(),
                    api_key_env: api_key_env.trim().to_string(),
                },
            })
        }
        ProviderConfigRequest::AddCustomProvider {
            display_name,
            base_url,
            protocols,
            model_discovery_protocol,
            authentication,
            api_key_env,
        } => {
            let display_name = trim_to_option(display_name)
                .ok_or_else(|| String::from("`display_name` must not be empty."))?;
            let base_url = base_url.trim();
            if protocols.is_empty() {
                return Err(String::from("`protocols` must name at least one protocol."));
            }
            let authentication =
                authentication.unwrap_or(CustomProviderAuthentication::BearerAuthorization);
            let label = |value: serde_json::Value| value.as_str().unwrap_or_default().to_string();
            let protocol_labels = joined(protocols.iter().map(|protocol| label(json!(protocol))));
            let secret = api_key_from_environment(registry, state, session_id, api_key_env).await?;
            let changes = vec![
                ConfigurationFieldChange {
                    field: String::from("base_url"),
                    before: None,
                    after: Some(base_url.to_string()),
                },
                ConfigurationFieldChange {
                    field: String::from("protocols"),
                    before: None,
                    after: protocol_labels,
                },
                ConfigurationFieldChange {
                    field: String::from("model_discovery_protocol"),
                    before: None,
                    after: Some(label(json!(model_discovery_protocol))),
                },
                ConfigurationFieldChange {
                    field: String::from("authentication"),
                    before: None,
                    after: Some(label(json!(authentication))),
                },
                ConfigurationFieldChange {
                    field: String::from("api_key_env"),
                    before: None,
                    after: Some(api_key_env.trim().to_string()),
                },
            ];
            Ok(ProviderPlan {
                preview: preview(
                    "custom_provider",
                    &display_name,
                    "create",
                    changes,
                    vec![
                        String::from("copies_api_key"),
                        String::from("validates_with_provider"),
                    ],
                ),
                call: ProviderCall::AddCustomProvider {
                    request: custom_provider_request(
                        &display_name,
                        base_url,
                        authentication,
                        protocols,
                        *model_discovery_protocol,
                        &secret,
                    )?,
                    api_key_env: api_key_env.trim().to_string(),
                },
            })
        }
        ProviderConfigRequest::SetConnectionEnabled {
            connection_id,
            enabled,
        } => {
            let connection = connection_of(connection_id)?;
            ensure_generic_provider(&providers, &connection.provider_id)?;
            let current = configuration.connection_is_enabled(&connection.id);
            if current == *enabled {
                return Err(format!(
                    "Connection `{}` is already {}.",
                    connection.id,
                    if current { "enabled" } else { "disabled" }
                ));
            }
            Ok(ProviderPlan {
                preview: preview(
                    "provider_connection",
                    &connection.display_name,
                    if *enabled { "enable" } else { "disable" },
                    vec![ConfigurationFieldChange {
                        field: String::from("enabled"),
                        before: Some(current.to_string()),
                        after: Some(enabled.to_string()),
                    }],
                    vec![String::from(if *enabled {
                        "activates_for_all_sessions"
                    } else {
                        "deactivates_for_all_sessions"
                    })],
                ),
                call: ProviderCall::SetEnabled {
                    connection_id: connection.id.clone(),
                    enabled: *enabled,
                },
            })
        }
        ProviderConfigRequest::SetDefaultConnection { connection_id } => {
            let connection = connection_of(connection_id)?;
            ensure_generic_provider(&providers, &connection.provider_id)?;
            let current = configuration
                .connection_graph
                .account_preferences
                .provider_defaults
                .get(&connection.provider_id);
            if current == Some(&connection.id) {
                return Err(format!(
                    "Connection `{}` is already the default of `{}`.",
                    connection.id, connection.provider_id
                ));
            }
            let current_name = current.map(|current| {
                configuration
                    .connection_graph
                    .connections
                    .get(current)
                    .map_or_else(|| current.clone(), |current| current.display_name.clone())
            });
            let provider_name = providers.provider(&connection.provider_id).map_or_else(
                || connection.provider_id.clone(),
                |provider| provider.display_name.clone(),
            );
            Ok(ProviderPlan {
                preview: preview(
                    "provider",
                    &provider_name,
                    "set_default",
                    vec![ConfigurationFieldChange {
                        field: String::from("default_connection"),
                        before: current_name,
                        after: Some(connection.display_name.clone()),
                    }],
                    Vec::new(),
                ),
                call: ProviderCall::SetDefault {
                    provider_id: connection.provider_id.clone(),
                    connection_id: connection.id.clone(),
                },
            })
        }
    }
}

/// The settings page's add-connection request for one API-key connection.
/// 设置页为一个 API Key 连接发出的添加连接请求。
fn add_connection_request(
    provider_id: &str,
    service_entry_id: &str,
    credential_profile_id: &str,
    display_name: &str,
    secret: &str,
) -> Result<PublicAiConnectionAddRequest, String> {
    settings_request(json!({
        "providerId": provider_id,
        "serviceEntryId": service_entry_id,
        "credentialProfileId": credential_profile_id,
        "credentialMode": CredentialMode::Token,
        "displayName": display_name,
        "secret": secret,
        "managementKey": null,
        "serviceOriginOverride": null,
    }))
}

/// The settings page's save request creating one custom provider with its first connection.
/// 设置页创建一个自定义供应商及其首个连接的保存请求。
/// `display_name`, `base_url`, `authentication`, and `secret` describe the shared connection.
/// `display_name`、`base_url`、`authentication` 与 `secret` 描述共享连接。
/// `protocols` enables conversation routes; `model_discovery_protocol` independently chooses one catalog.
/// `protocols` 启用会话路由；`model_discovery_protocol` 独立选择唯一目录。
/// Returns the exact settings request, or a decoding error before any configuration is changed.
/// 返回精确设置请求，或在任何配置变更前返回解码错误。
fn custom_provider_request(
    display_name: &str,
    base_url: &str,
    authentication: CustomProviderAuthentication,
    protocols: &[vulcan_provider_adapters::CustomProviderProtocol],
    model_discovery_protocol: vulcan_provider_adapters::CustomProviderModelDiscoveryProtocol,
    secret: &str,
) -> Result<PublicAiCustomProviderSaveRequest, String> {
    settings_request(json!({
        "providerId": null,
        "displayName": display_name,
        "baseUrl": base_url,
        "authentication": authentication,
        "credentialDisplayName": display_name,
        "secret": secret,
        "protocols": protocols,
        "modelDiscoveryProtocol": model_discovery_protocol,
    }))
}

/// The settings page's request enabling or disabling one connection.
/// 设置页启用或停用一个连接的请求。
fn connection_enabled_request(
    connection_id: &str,
    enabled: bool,
) -> Result<PublicAiConnectionEnabledSaveRequest, String> {
    settings_request(json!({ "connectionId": connection_id, "enabled": enabled }))
}

/// The settings page's request making one connection its provider's default.
/// 设置页将一个连接设为其供应商默认连接的请求。
fn default_connection_request(
    provider_id: &str,
    connection_id: &str,
) -> Result<PublicAiDefaultConnectionSaveRequest, String> {
    settings_request(json!({ "providerId": provider_id, "connectionId": connection_id }))
}

/// Parses one settings request the way the settings RPC parses it.
/// 以设置 RPC 的方式解析一个设置请求。
fn settings_request<T: serde::de::DeserializeOwned>(
    request: serde_json::Value,
) -> Result<T, String> {
    serde_json::from_value(request)
        .map_err(|error| format!("The settings request could not be built: {error}"))
}

/// Applies one provider change through the public AI settings services.
/// 通过公共 AI 设置服务应用一次供应商变更。
async fn change_provider(
    registry: &SessionRuntimeRegistry,
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    request: ProviderConfigRequest,
) -> Result<ConfigurationOutcome, String> {
    let plan = plan_provider_change(registry, state, session_id, &request).await?;
    let connection_ids = |state: &SharedServiceBridgeState| -> Result<BTreeSet<String>, String> {
        Ok(public_ai_runtime_and_configuration(state)?
            .1
            .connection_graph
            .connections
            .into_keys()
            .collect())
    };
    match plan.call {
        ProviderCall::AddConnection {
            request,
            provider_id,
            api_key_env,
        } => {
            let before = connection_ids(state)?;
            service_add_public_ai_connection(app.clone(), state.clone(), request).await?;
            let added: Vec<String> = connection_ids(state)?
                .difference(&before)
                .cloned()
                .collect();
            Ok(outcome(
                format!(
                    "Added connection {} to provider `{provider_id}`. The API key was copied from `{api_key_env}`; changing that variable later does not change the stored key.",
                    added
                        .iter()
                        .map(|id| format!("`{id}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                json!({ "action": "add_connection", "providerId": provider_id, "connectionIds": added }),
            ))
        }
        ProviderCall::AddCustomProvider {
            request,
            api_key_env,
        } => {
            let custom_providers =
                |state: &SharedServiceBridgeState| -> Result<BTreeSet<String>, String> {
                    Ok(public_ai_runtime_and_configuration(state)?
                        .1
                        .custom_providers
                        .into_keys()
                        .collect())
                };
            let before = custom_providers(state)?;
            service_save_public_ai_custom_provider(app.clone(), state.clone(), request).await?;
            let added: Vec<String> = custom_providers(state)?
                .difference(&before)
                .cloned()
                .collect();
            Ok(outcome(
                format!(
                    "Added custom provider {} with its first connection. The API key was copied from `{api_key_env}`; changing that variable later does not change the stored key.",
                    added
                        .iter()
                        .map(|id| format!("`{id}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                json!({ "action": "add_custom_provider", "providerIds": added }),
            ))
        }
        ProviderCall::SetEnabled {
            connection_id,
            enabled,
        } => {
            service_save_public_ai_connection_enabled(
                app.clone(),
                state.clone(),
                connection_enabled_request(&connection_id, enabled)?,
            )
            .await?;
            Ok(outcome(
                format!(
                    "Connection `{connection_id}` is now {}.",
                    if enabled { "enabled" } else { "disabled" }
                ),
                json!({ "action": "set_connection_enabled", "connectionId": connection_id, "enabled": enabled }),
            ))
        }
        ProviderCall::SetDefault {
            provider_id,
            connection_id,
        } => {
            service_save_public_ai_default_connection(
                app.clone(),
                state.clone(),
                default_connection_request(&provider_id, &connection_id)?,
            )
            .await?;
            Ok(outcome(
                format!("Connection `{connection_id}` is now the default of `{provider_id}`."),
                json!({ "action": "set_default_connection", "providerId": provider_id, "connectionId": connection_id }),
            ))
        }
    }
}

#[cfg(test)]
mod tests;
