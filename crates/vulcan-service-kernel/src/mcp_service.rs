//! MCP service: the kernel backend behind `/mcp` and the settings RPCs that manage its clients.
//! MCP 服务：`/mcp` 背后的内核后端，以及管理其客户端的设置 RPC。
//!
//! External AI clients authenticate with per-client bearer tokens whose SHA-256 digests live in
//! the global configuration. Each client carries its own session and media grants; the protocol
//! surface itself lives in `vulcan-mcp-service`.
//! 外部 AI 客户端使用逐客户端 Bearer 令牌鉴权，令牌的 SHA-256 摘要保存在全局配置中。每个客户端
//! 拥有各自的会话与媒体授权；协议表面本身位于 `vulcan-mcp-service`。

mod backend;
mod transcript;

use std::collections::BTreeSet;
use std::sync::Arc;

use vulcan_mcp_service::{McpServer, issue_client_token};
use vulcan_provider_adapters::{AvailableProviderToolTarget, ManifestSourceLayer};
use vulcan_provider_config::provider_config::{
    MAX_MCP_CLIENT_MEDIA_TARGETS, MAX_MCP_CLIENT_NAME_CHARS, MAX_MCP_CLIENT_PROJECTS,
    MAX_MCP_SERVICE_CLIENTS, McpMediaTargetGrant, McpProjectScope, McpServiceClient,
    McpSessionGrant, PersistedMcpServiceConfiguration, normalize_mcp_service_configuration,
};
use vulcan_service_api::{
    McpMediaTargetCandidate, McpMediaTargetEntry, McpProjectScopeEntry,
    McpServiceClientSaveRequest, McpServiceClientSummary, McpServiceIssuedToken, McpServiceLimits,
    McpServiceSnapshot, McpSessionGrantEntry, ServiceEventEmitter,
};

use crate::SharedServiceBridgeState;
use crate::service_state_access::{
    mutate_provider_configuration, provider_configuration_from_state,
};

pub(crate) use backend::KernelMcpBackend;

/// Returns the process MCP server, creating it on first use.
/// 返回进程内的 MCP 服务器，首次使用时创建。
///
/// # Parameters
/// 参数
/// `app` and `state` are the kernel authorities the backend submits and reads through.
/// `app` 与 `state` 是后端提交与读取所经由的内核权威。
pub(crate) fn install_mcp_server(
    app: ServiceEventEmitter,
    state: &SharedServiceBridgeState,
) -> Result<Arc<McpServer>, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(server) = guard.mcp_server.as_ref() {
        return Ok(Arc::clone(server));
    }
    let backend = KernelMcpBackend::new(app, Arc::downgrade(state));
    let server = Arc::new(McpServer::new(Arc::new(backend), env!("CARGO_PKG_VERSION")));
    guard.mcp_server = Some(Arc::clone(&server));
    Ok(server)
}

/// Returns the settings snapshot.
/// 返回设置快照。
pub(crate) fn service_mcp_service_snapshot(
    state: SharedServiceBridgeState,
) -> Result<McpServiceSnapshot, String> {
    let configuration = provider_configuration_from_state(&state)?;
    snapshot(&state, &configuration.mcp_service)
}

/// Turns the endpoint on or off without touching any client.
/// 开启或关闭端点，不改动任何客户端。
pub(crate) async fn service_set_mcp_service_enabled(
    state: SharedServiceBridgeState,
    enabled: bool,
) -> Result<McpServiceSnapshot, String> {
    let saved = mutate_provider_configuration(
        &state,
        "Failed to save the MCP service setting",
        |configuration| {
            configuration.mcp_service.enabled = enabled;
            Ok(())
        },
        |_, configuration| configuration.mcp_service.clone(),
    )
    .await?;
    snapshot(&state, &saved)
}

/// Creates one client and returns its token, which is shown only this once.
/// 创建一个客户端并返回其令牌，令牌只展示这一次。
pub(crate) async fn service_create_mcp_client(
    state: SharedServiceBridgeState,
    request: McpServiceClientSaveRequest,
) -> Result<McpServiceIssuedToken, String> {
    if request.client_id.is_some() {
        return Err(String::from("A new MCP client must not carry a client id."));
    }
    let client_id = uuid::Uuid::new_v4().to_string();
    let issued = issue_client_token();
    let current = provider_configuration_from_state(&state)?.mcp_service;
    validate_new_grants(&state, &request, None)?;
    let client = McpServiceClient {
        id: client_id.clone(),
        name: request.name,
        token_sha256: issued.token_sha256,
        token_hint: issued.token_hint,
        created_at_ms: now_unix_millis(),
        enabled: request.enabled,
        sessions: grant_from_entry(request.sessions),
        media_targets: request
            .media_targets
            .into_iter()
            .map(target_grant_from_entry)
            .collect(),
    };
    let mut next = current;
    next.clients.push(client);
    let normalized = normalize_mcp_service_configuration(next)?;
    let saved = save_configuration(&state, normalized).await?;
    Ok(McpServiceIssuedToken {
        snapshot: snapshot(&state, &saved)?,
        client_id,
        token: issued.token,
    })
}

/// Replaces one client's name, switch, and grants; its token is unchanged.
/// 替换一个客户端的名称、开关与授权；其令牌保持不变。
pub(crate) async fn service_update_mcp_client(
    state: SharedServiceBridgeState,
    request: McpServiceClientSaveRequest,
) -> Result<McpServiceSnapshot, String> {
    let client_id = request
        .client_id
        .clone()
        .ok_or_else(|| String::from("Updating an MCP client requires its client id."))?;
    let mut next = provider_configuration_from_state(&state)?.mcp_service;
    let position = client_position(&next, &client_id)?;
    validate_new_grants(&state, &request, Some(&next.clients[position]))?;
    let client = &mut next.clients[position];
    client.name = request.name;
    client.enabled = request.enabled;
    client.sessions = grant_from_entry(request.sessions);
    client.media_targets = request
        .media_targets
        .into_iter()
        .map(target_grant_from_entry)
        .collect();
    let normalized = normalize_mcp_service_configuration(next)?;
    let saved = save_configuration(&state, normalized).await?;
    snapshot(&state, &saved)
}

/// Issues a new token for one client; the previous token stops working immediately.
/// 为一个客户端签发新令牌；旧令牌立即失效。
pub(crate) async fn service_rotate_mcp_client_token(
    state: SharedServiceBridgeState,
    client_id: String,
) -> Result<McpServiceIssuedToken, String> {
    let issued = issue_client_token();
    let mut next = provider_configuration_from_state(&state)?.mcp_service;
    let position = client_position(&next, &client_id)?;
    next.clients[position].token_sha256 = issued.token_sha256;
    next.clients[position].token_hint = issued.token_hint;
    let normalized = normalize_mcp_service_configuration(next)?;
    let saved = save_configuration(&state, normalized).await?;
    Ok(McpServiceIssuedToken {
        snapshot: snapshot(&state, &saved)?,
        client_id,
        token: issued.token,
    })
}

/// Deletes one client; its token stops working and its media jobs are cancelled.
/// 删除一个客户端；其令牌失效，其媒体任务被取消。
pub(crate) async fn service_delete_mcp_client(
    state: SharedServiceBridgeState,
    client_id: String,
) -> Result<McpServiceSnapshot, String> {
    let mut next = provider_configuration_from_state(&state)?.mcp_service;
    let position = client_position(&next, &client_id)?;
    next.clients.remove(position);
    let saved = save_configuration(&state, next).await?;
    snapshot(&state, &saved)
}

/// Saves the normalized MCP configuration and drops runtime state of removed clients.
/// 保存归一化的 MCP 配置，并丢弃已移除客户端的运行时状态。
async fn save_configuration(
    state: &SharedServiceBridgeState,
    next: PersistedMcpServiceConfiguration,
) -> Result<PersistedMcpServiceConfiguration, String> {
    let saved = mutate_provider_configuration(
        state,
        "Failed to save the MCP service clients",
        |configuration| {
            configuration.mcp_service = next;
            Ok(())
        },
        |guard, configuration| {
            if let Some(server) = guard.mcp_server.as_ref() {
                let live = configuration
                    .mcp_service
                    .clients
                    .iter()
                    .map(|client| client.id.as_str())
                    .collect::<Vec<_>>();
                server.retain_clients(&live);
            }
            configuration.mcp_service.clone()
        },
    )
    .await?;
    Ok(saved)
}

/// Finds one client by id.
/// 按标识查找一个客户端。
fn client_position(
    configuration: &PersistedMcpServiceConfiguration,
    client_id: &str,
) -> Result<usize, String> {
    configuration
        .clients
        .iter()
        .position(|client| client.id == client_id)
        .ok_or_else(|| format!("MCP client `{client_id}` was not found."))
}

/// Rejects grants added by this request that point at unknown projects or unavailable media
/// targets. Grants the client already held stay valid even after their target disappears, so an
/// unrelated edit never fails on stale data.
/// 拒绝本次请求新增的、指向未知项目或不可用媒体目标的授权。客户端原有的授权即使其目标已消失也
/// 仍然有效，因此无关编辑绝不会因过期数据而失败。
fn validate_new_grants(
    state: &SharedServiceBridgeState,
    request: &McpServiceClientSaveRequest,
    previous: Option<&McpServiceClient>,
) -> Result<(), String> {
    if let McpProjectScopeEntry::Selected { project_ids } = &request.sessions.projects {
        let held = previous
            .map(|client| match &client.sessions.projects {
                McpProjectScope::All => BTreeSet::new(),
                McpProjectScope::Selected { project_ids } => {
                    project_ids.iter().cloned().collect::<BTreeSet<_>>()
                }
            })
            .unwrap_or_default();
        let known = {
            let guard = state
                .lock()
                .map_err(|error| format!("failed to lock service state: {error}"))?;
            guard
                .workbench
                .projects
                .iter()
                .map(|project| project.id.clone())
                .collect::<BTreeSet<_>>()
        };
        if let Some(unknown) = project_ids
            .iter()
            .map(|project_id| project_id.trim())
            .find(|project_id| !held.contains(*project_id) && !known.contains(*project_id))
        {
            return Err(format!("Project `{unknown}` was not found."));
        }
    }
    let held_targets = previous
        .map(|client| client.media_targets.clone())
        .unwrap_or_default();
    let added = request
        .media_targets
        .iter()
        .map(|entry| target_grant_from_entry(entry.clone()))
        .filter(|grant| !held_targets.contains(grant))
        .collect::<Vec<_>>();
    if added.is_empty() {
        return Ok(());
    }
    let candidates = media_candidates(state)?;
    for grant in added {
        if !candidates.iter().any(|candidate| {
            candidate.connection_id == grant.connection_id && candidate.target_id == grant.target_id
        }) {
            return Err(format!(
                "Media target `{}` is not available through account `{}`.",
                grant.target_id, grant.connection_id
            ));
        }
    }
    Ok(())
}

/// Builds the settings snapshot with endpoint, candidates, limits, and in-memory usage.
/// 构建包含端点、候选、上限与内存用量的设置快照。
fn snapshot(
    state: &SharedServiceBridgeState,
    configuration: &PersistedMcpServiceConfiguration,
) -> Result<McpServiceSnapshot, String> {
    let (server, endpoint_url) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let endpoint_url = guard
            .webui_service_discovery
            .as_ref()
            .and_then(|discovery| {
                discovery
                    .lock()
                    .ok()
                    .map(|snapshot| snapshot.base_url.clone())
            })
            .filter(|base_url| !base_url.is_empty())
            .map(|base_url| format!("{}/mcp", base_url.trim_end_matches('/')));
        (guard.mcp_server.clone(), endpoint_url)
    };
    // Candidates are best effort: a runtime that is still starting shows an empty list.
    // 候选为尽力而为：仍在启动的运行时显示空列表。
    let media_candidates = media_candidates(state).unwrap_or_default();
    let clients = configuration
        .clients
        .iter()
        .map(|client| {
            let usage = server
                .as_ref()
                .map(|server| server.client_usage(&client.id))
                .unwrap_or_default();
            McpServiceClientSummary {
                id: client.id.clone(),
                name: client.name.clone(),
                token_hint: client.token_hint.clone(),
                created_at_ms: client.created_at_ms,
                enabled: client.enabled,
                sessions: entry_from_grant(&client.sessions),
                media_targets: client
                    .media_targets
                    .iter()
                    .map(|grant| McpMediaTargetEntry {
                        connection_id: grant.connection_id.clone(),
                        target_id: grant.target_id.clone(),
                    })
                    .collect(),
                last_used_at_ms: usage.last_used_at_ms,
                call_count: usage.call_count,
            }
        })
        .collect();
    Ok(McpServiceSnapshot {
        enabled: configuration.enabled,
        endpoint_url,
        clients,
        media_candidates,
        limits: McpServiceLimits {
            max_clients: MAX_MCP_SERVICE_CLIENTS,
            max_name_chars: MAX_MCP_CLIENT_NAME_CHARS,
            max_projects: MAX_MCP_CLIENT_PROJECTS,
            max_media_targets: MAX_MCP_CLIENT_MEDIA_TARGETS,
        },
    })
}

/// Lists the system image and video model targets that may be granted, one row per account.
/// 列出可以授权的系统图片与视频模型目标，每个账号一行。
fn media_candidates(
    state: &SharedServiceBridgeState,
) -> Result<Vec<McpMediaTargetCandidate>, String> {
    let (runtime, configuration) = crate::public_ai_runtime_and_configuration(state)?;
    let mut candidates = runtime
        .available_provider_tool_targets(&configuration)?
        .iter()
        .filter_map(|target| {
            let capability = grantable_capability(target)?;
            Some(McpMediaTargetCandidate {
                connection_id: target.connection_id.clone(),
                target_id: target.target.target_id.clone(),
                capability,
                model: vulcan_agent_runtime::model_extension_public_model_id(target),
                display_name: target.target.display_name.clone(),
                account_name: target.connection_display_name.clone(),
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        (&left.model, &left.capability, &left.account_name).cmp(&(
            &right.model,
            &right.capability,
            &right.account_name,
        ))
    });
    Ok(candidates)
}

/// Returns the capability name when a target is a reviewed system media model extension.
/// 当目标是已审核的系统媒体模型扩展时返回其能力名称。
pub(super) fn grantable_capability(target: &AvailableProviderToolTarget) -> Option<String> {
    if target.manifest_source_layer == ManifestSourceLayer::Custom
        || target.target.contract.extension_profile.is_none()
    {
        return None;
    }
    let capability = serde_json::to_value(target.target.contract.capability)
        .ok()?
        .as_str()?
        .to_string();
    vulcan_mcp_service::MCP_MEDIA_CAPABILITIES
        .contains(&capability.as_str())
        .then_some(capability)
}

/// Converts the wire grant into the stored grant.
/// 将线上授权转换为存储授权。
fn grant_from_entry(entry: McpSessionGrantEntry) -> McpSessionGrant {
    McpSessionGrant {
        read: entry.read,
        send: entry.send,
        create: entry.create,
        stop: entry.stop,
        answer_questions: entry.answer_questions,
        projects: match entry.projects {
            McpProjectScopeEntry::All => McpProjectScope::All,
            McpProjectScopeEntry::Selected { project_ids } => {
                McpProjectScope::Selected { project_ids }
            }
        },
    }
}

/// Converts the stored grant into the wire grant.
/// 将存储授权转换为线上授权。
fn entry_from_grant(grant: &McpSessionGrant) -> McpSessionGrantEntry {
    McpSessionGrantEntry {
        read: grant.read,
        send: grant.send,
        create: grant.create,
        stop: grant.stop,
        answer_questions: grant.answer_questions,
        projects: match &grant.projects {
            McpProjectScope::All => McpProjectScopeEntry::All,
            McpProjectScope::Selected { project_ids } => McpProjectScopeEntry::Selected {
                project_ids: project_ids.clone(),
            },
        },
    }
}

/// Converts one wire media grant into the stored grant.
/// 将一个线上媒体授权转换为存储授权。
fn target_grant_from_entry(entry: McpMediaTargetEntry) -> McpMediaTargetGrant {
    McpMediaTargetGrant {
        connection_id: entry.connection_id,
        target_id: entry.target_id,
    }
}

/// Current Unix time in milliseconds.
/// 当前 Unix 毫秒时间。
fn now_unix_millis() -> u64 {
    u64::try_from(crate::current_unix_millis_value()).unwrap_or_default()
}
