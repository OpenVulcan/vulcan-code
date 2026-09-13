//! MCP server settings WebUI service handlers.
//! MCP 服务器设置 WebUI 服务处理器。

use super::*;

/// Returns complete catalogs from the published Agent MCP generation without network traffic.
/// 从已发布的 Agent MCP 代次返回完整目录，不发起网络请求。
pub(super) fn service_mcp_runtime_snapshot(
    state: SharedServiceBridgeState,
) -> Result<serde_json::Value, String> {
    let runtime = agent_mcp_runtime_from_state(&state)?;
    serde_json::to_value(runtime.server_inspections()).map_err(|error| error.to_string())
}

/// Reads optional metadata for one existing connection and discards retired generation results.
/// 读取单个现有连接的可选元数据，并丢弃已退休代次的结果。
pub(super) async fn service_mcp_server_capabilities(
    state: SharedServiceBridgeState,
    server_id: String,
) -> Result<serde_json::Value, String> {
    let runtime = agent_mcp_runtime_from_state(&state)?;
    let capabilities = runtime.server_capabilities(&server_id).await?;
    let current = agent_mcp_runtime_from_state(&state)?;
    if !runtime.shares_server_partition(&current, &server_id) {
        return Err(String::from("MCP 配置已经变化，请重新读取详情。"));
    }
    Ok(capabilities)
}

/// WebUI 设置页发送的 MCP 服务器配置保存请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct McpServerSaveRequest {
    /// Existing server identifier, or `None` when creating one.
    /// 已有服务器标识；创建新服务器时为 `None`。
    server_id: Option<String>,
    /// User-facing server name.
    /// 面向用户显示的服务器名称。
    name: String,
    /// Whether the server should be registered by the runtime.
    /// 该服务器是否应由运行时注册。
    enabled: bool,
    /// Transport key such as `stdio`, `http`, or `sse`.
    /// 传输键，例如 `stdio`、`http` 或 `sse`。
    transport: String,
    /// Stdio command.
    /// Stdio 启动命令。
    command: String,
    /// Stdio argument vector.
    /// Stdio 参数列表。
    args: Vec<String>,
    /// Remote endpoint URL.
    /// 远端端点 URL。
    url: String,
    /// Remote bearer token.
    /// 远端 Bearer Token。
    bearer_token: String,
    /// Explicit HTTP headers.
    /// 显式 HTTP 请求头。
    headers: Vec<KeyValueInput>,
    /// HTTP headers resolved from environment variables.
    /// 从环境变量解析的 HTTP 请求头。
    headers_from_env: Vec<KeyValueInput>,
    /// OAuth client identifier.
    /// OAuth 客户端标识。
    oauth_client_id: String,
    /// OAuth client secret.
    /// OAuth 客户端密钥。
    oauth_client_secret: String,
    /// OAuth scope list.
    /// OAuth scope 列表。
    oauth_scopes: Vec<String>,
    /// OAuth redirect URI.
    /// OAuth 回调 URI。
    oauth_redirect_uri: String,
    /// Stdio working directory.
    /// Stdio 工作目录。
    cwd: String,
    /// Environment variables injected into stdio servers.
    /// 注入 stdio 服务器的环境变量。
    env: Vec<KeyValueInput>,
    /// Environment variable names passed through to stdio servers.
    /// 透传给 stdio 服务器的环境变量名。
    env_passthrough: Vec<String>,
}

/// Generic string key/value input used by settings save requests.
/// 设置保存请求使用的通用字符串键值输入。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct KeyValueInput {
    /// Raw key.
    /// 原始键。
    key: String,
    /// Raw value.
    /// 原始值。
    value: String,
}

/// Saves one MCP server configuration from the WebUI settings page.
/// 从 WebUI 设置页保存单个 MCP 服务器配置。
pub(super) async fn service_save_mcp_server_configuration(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: McpServerSaveRequest,
) -> Result<WorkbenchSnapshot, String> {
    // configuration_transaction serializes clone, asynchronous runtime preparation, persistence, and runtime replacement.
    // configuration_transaction 串行化克隆、异步运行时准备、持久化与运行时替换。
    let configuration_transaction = begin_provider_configuration_transaction().await;
    let create_new_server = request
        .server_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none();
    let (server_id, entry) = mcp_server_entry_from_request(request)?;
    let next_configuration = {
        let mut configuration = provider_configuration_from_state(&state)?;
        apply_mcp_server_entry_to_configuration(
            &mut configuration,
            server_id,
            entry,
            create_new_server,
        )?;
        configuration
    };
    let current_mcp_runtime = agent_mcp_runtime_from_state(&state)?;
    let next_mcp_runtime = replace_user_mcp_runtime_partition(
        &current_mcp_runtime,
        runtime_mcp_server_configs(&next_configuration)?,
    )
    .await
    .map_err(|error| format!("MCP Runtime 重建失败，配置未保存：{error}"))?;
    if let Err(error) =
        configuration_transaction.persist(&next_configuration, "保存 MCP 服务器失败")
    {
        next_mcp_runtime.shutdown_user_partition();
        return Err(error);
    }
    let workbench = store_provider_and_mcp_configuration_after_change(
        &state,
        next_configuration,
        next_mcp_runtime,
    )?;
    // Release the global configuration transaction only after the rebuilt runtime is stored.
    // 仅在重建运行时完成回存后释放全局配置事务。
    drop(configuration_transaction);
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Removes one MCP server configuration from the WebUI settings page.
/// 从 WebUI 设置页移除单个 MCP 服务器配置。
pub(super) async fn service_remove_mcp_server_configuration(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    server_id: String,
) -> Result<WorkbenchSnapshot, String> {
    // configuration_transaction serializes clone, asynchronous runtime preparation, persistence, and runtime replacement.
    // configuration_transaction 串行化克隆、异步运行时准备、持久化与运行时替换。
    let configuration_transaction = begin_provider_configuration_transaction().await;
    let mut next_configuration = provider_configuration_from_state(&state)?;
    next_configuration
        .mcp
        .servers
        .remove(server_id.trim())
        .ok_or_else(|| format!("未找到 MCP 服务器 `{}`。", server_id.trim()))?;
    let current_mcp_runtime = agent_mcp_runtime_from_state(&state)?;
    let next_mcp_runtime = replace_user_mcp_runtime_partition(
        &current_mcp_runtime,
        runtime_mcp_server_configs(&next_configuration)?,
    )
    .await
    .map_err(|error| format!("MCP Runtime 重建失败，配置未保存：{error}"))?;
    if let Err(error) =
        configuration_transaction.persist(&next_configuration, "移除 MCP 服务器失败")
    {
        next_mcp_runtime.shutdown_user_partition();
        return Err(error);
    }
    let workbench = store_provider_and_mcp_configuration_after_change(
        &state,
        next_configuration,
        next_mcp_runtime,
    )?;
    // Release the global configuration transaction only after the rebuilt runtime is stored.
    // 仅在重建运行时完成回存后释放全局配置事务。
    drop(configuration_transaction);
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Clones the currently published Agent MCP access generation before asynchronous replacement.
/// 在异步替换前克隆当前已发布的 Agent MCP 访问代次。
fn agent_mcp_runtime_from_state(
    state: &SharedServiceBridgeState,
) -> Result<McpRuntimeAccess, String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .agent_runtime
        .as_ref()
        .map(|runtime| runtime.extension_generation.mcp_runtime())
        .ok_or_else(|| String::from("Agent MCP Runtime 尚未初始化。"))
}

/// Applies one parsed MCP server entry to the provider configuration without silent overwrites.
/// 将单个已解析 MCP 服务器条目应用到 Provider 配置，并禁止静默覆盖。
///
/// # Parameters
/// 参数
/// `configuration` is the mutable provider configuration being prepared for persistence.
/// `configuration` 是准备持久化的可变 Provider 配置。
/// `server_id` is the stable MCP server identifier derived from the request.
/// `server_id` 是从请求解析出的稳定 MCP 服务器标识。
/// `entry` is the validated persisted MCP server entry.
/// `entry` 是已经校验过的持久化 MCP 服务器条目。
/// `create_new_server` is true when the request came from the create flow.
/// `create_new_server` 在请求来自新增流程时为 true。
///
/// # Returns
/// 返回值
/// `Ok(())` after insertion or replacement is legal; otherwise a user-facing error.
/// 插入或替换合法时返回 `Ok(())`；否则返回面向用户的错误。
pub(super) fn apply_mcp_server_entry_to_configuration(
    configuration: &mut ProviderConfigurationFile,
    server_id: String,
    entry: PersistedMcpServerEntry,
    create_new_server: bool,
) -> Result<(), String> {
    let existing = configuration.mcp.servers.contains_key(&server_id);
    if create_new_server && existing {
        return Err(format!(
            "MCP 服务器 `{server_id}` 已存在。请更换名称，或编辑现有服务器。"
        ));
    }
    if !create_new_server && !existing {
        return Err(format!("未找到 MCP 服务器 `{server_id}`，无法保存修改。"));
    }
    configuration.mcp.servers.insert(server_id, entry);
    Ok(())
}

/// Parses one MCP transport key from the WebUI request payload.
/// 从 WebUI 请求载荷解析单个 MCP 传输键。
pub(super) fn parse_mcp_transport(value: &str) -> Result<PersistedMcpTransportKind, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "stdio" => Ok(PersistedMcpTransportKind::Stdio),
        "http" => Ok(PersistedMcpTransportKind::Http),
        "sse" => Ok(PersistedMcpTransportKind::Sse),
        other => Err(format!("不支持的 MCP 传输类型：{other}")),
    }
}

/// Converts WebUI key/value inputs into a validated map.
/// 将 WebUI 键值列表转换为经过校验的映射。
pub(super) fn key_value_inputs_to_map(
    inputs: Vec<KeyValueInput>,
    subject_label: &str,
    value_label: &str,
    preserve_value: bool,
) -> Result<BTreeMap<String, String>, String> {
    let mut map = BTreeMap::new();
    for input in inputs {
        let key = input.key.trim();
        let value = input.value.trim();
        if key.is_empty() && value.is_empty() {
            continue;
        }
        if key.is_empty() {
            return Err(format!("{subject_label} 的键不能为空。"));
        }
        if value.is_empty() {
            return Err(format!("{subject_label} 的{value_label}不能为空。"));
        }
        // Environment values are process arguments too; meaningful surrounding whitespace must survive saving.
        // 环境变量值同样是进程输入；有意义的前后空白必须在保存后保留。
        let saved_value = if preserve_value {
            input.value.clone()
        } else {
            value.to_string()
        };
        if map.insert(key.to_string(), saved_value).is_some() {
            return Err(format!("{subject_label} `{key}` 重复，请保留一条即可。"));
        }
    }
    Ok(map)
}

/// Converts optional text into an owned option after trimming.
/// 裁剪可选文本并转换为自有可选值。
pub(super) fn trim_to_option(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Builds one persisted MCP server entry from a WebUI save request.
/// 从 WebUI 保存请求构建单个持久化 MCP 服务器条目。
pub(super) fn mcp_server_entry_from_request(
    request: McpServerSaveRequest,
) -> Result<(String, PersistedMcpServerEntry), String> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err(String::from("请先填写 MCP 服务器名称。"));
    }
    let server_id = request
        .server_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| stable_identifier_from_label(name));
    let transport = parse_mcp_transport(&request.transport)?;
    let oauth = mcp_oauth_config_from_request(&request);
    match transport {
        PersistedMcpTransportKind::Stdio => {
            let command = request.command.trim();
            if command.is_empty() {
                return Err(String::from("请先填写 MCP 服务器启动命令。"));
            }
            Ok((
                server_id,
                PersistedMcpServerEntry {
                    name: name.to_string(),
                    enabled: request.enabled,
                    transport,
                    command: command.to_string(),
                    // Preserve exact argv boundaries, including intentional whitespace and empty arguments.
                    // 保留精确 argv 边界，包括有意传入的空白与空参数。
                    args: request.args,
                    url: None,
                    bearer_token: None,
                    headers: BTreeMap::new(),
                    headers_from_env: BTreeMap::new(),
                    oauth: None,
                    cwd: trim_to_option(&request.cwd),
                    env: key_value_inputs_to_map(request.env, "环境变量", "值", true)?,
                    env_passthrough: request
                        .env_passthrough
                        .into_iter()
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect(),
                },
            ))
        }
        PersistedMcpTransportKind::Http | PersistedMcpTransportKind::Sse => {
            let url = request.url.trim();
            if url.is_empty() {
                return Err(String::from("请先填写 MCP 远端服务器 URL。"));
            }
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Err(String::from(
                    "MCP 远端服务器 URL 必须以 `http://` 或 `https://` 开头。",
                ));
            }
            Ok((
                server_id,
                PersistedMcpServerEntry {
                    name: name.to_string(),
                    enabled: request.enabled,
                    transport,
                    command: String::new(),
                    args: Vec::new(),
                    url: Some(url.to_string()),
                    bearer_token: trim_to_option(&request.bearer_token),
                    headers: key_value_inputs_to_map(request.headers, "请求头", "值", false)?,
                    headers_from_env: key_value_inputs_to_map(
                        request.headers_from_env,
                        "来自环境变量的请求头",
                        "环境变量名",
                        false,
                    )?,
                    oauth,
                    cwd: None,
                    env: BTreeMap::new(),
                    env_passthrough: Vec::new(),
                },
            ))
        }
    }
}

/// Builds an optional MCP OAuth block from a WebUI save request.
/// 从 WebUI 保存请求构建可选 MCP OAuth 配置块。
pub(super) fn mcp_oauth_config_from_request(
    request: &McpServerSaveRequest,
) -> Option<PersistedMcpOAuthConfig> {
    let client_id = trim_to_option(&request.oauth_client_id);
    let client_secret = trim_to_option(&request.oauth_client_secret);
    let redirect_uri = trim_to_option(&request.oauth_redirect_uri);
    let scopes = request
        .oauth_scopes
        .iter()
        .map(|scope| scope.trim().to_string())
        .filter(|scope| !scope.is_empty())
        .collect::<Vec<_>>();
    if client_id.is_none() && client_secret.is_none() && redirect_uri.is_none() && scopes.is_empty()
    {
        None
    } else {
        Some(PersistedMcpOAuthConfig {
            client_id,
            client_secret,
            scopes,
            redirect_uri,
        })
    }
}

/// Builds one stable identifier from a human-readable label.
/// 从人类可读标签构建单个稳定标识。
pub(super) fn stable_identifier_from_label(label: &str) -> String {
    let mut normalized = label
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while normalized.contains("--") {
        normalized = normalized.replace("--", "-");
    }
    let trimmed = normalized.trim_matches('-');
    if trimmed.is_empty() {
        format!("mcp-{}", current_unix_millis_value())
    } else {
        trimmed.to_string()
    }
}
