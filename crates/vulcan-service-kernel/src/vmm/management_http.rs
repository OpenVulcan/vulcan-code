//! Backend-only proxy for the isolated VMM management HTTP API.
//! VMM 独立管理 HTTP API 的仅后端代理。

use std::time::Duration;

use reqwest::{Method, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use vulcan_service_api::{VmmMemoryActionRequest, VmmMemoryActionResponse, VmmMemoryActionResult};

use crate::SharedServiceBridgeState;

/// Maximum accepted management response size for one non-streaming bridge request.
/// 单次非流式桥接请求允许接收的最大管理响应大小。
const MAX_MANAGEMENT_RESPONSE_BYTES: usize = 1024 * 1024;

/// Secret-bearing external management target copied only for one backend request.
/// 仅为一次后端请求复制的含密外置管理目标。
struct ExternalVmmManagementTarget {
    /// Absolute external management API origin.
    /// 外置管理 API 的绝对源地址。
    base_url: String,
    /// External management Bearer token.
    /// 外置管理 Bearer Token。
    access_token: String,
    /// Bounded request timeout selected by the external configuration.
    /// 由外置配置选择的有界请求超时。
    timeout: Duration,
}

/// Resolves and validates one external-only management target from authoritative configuration.
/// 从权威配置解析并校验一个仅外置模式使用的管理目标。
///
/// # Parameters
/// 参数
/// `state` owns the currently published global VMM configuration.
/// `state` 持有当前已发布的全局 VMM 配置。
///
/// # Returns
/// 返回值
/// A validated secret-bearing target, or `None` when external management is unavailable.
/// 返回经过校验的含密目标；外置管理不可用时返回 `None`。
fn external_management_target_from_state(
    state: &SharedServiceBridgeState,
) -> Option<ExternalVmmManagementTarget> {
    let configuration = crate::provider_configuration_from_state(state).ok()?;
    let layer = &configuration.vmm;
    if layer.enabled != Some(true)
        || layer.backend_mode != Some(vulcan_vmm::VmmBackendMode::ExternalGrpc)
    {
        return None;
    }
    let base_url = layer
        .management_base_url
        .as_deref()?
        .trim()
        .trim_end_matches('/')
        .to_string();
    let access_token = layer.management_bearer_token.as_deref()?.trim().to_string();
    if base_url.is_empty() || access_token.is_empty() {
        return None;
    }
    let parsed = Url::parse(&base_url).ok()?;
    let loopback = parsed
        .host_str()
        .is_some_and(|host| matches!(host, "127.0.0.1" | "localhost" | "::1"));
    if parsed.query().is_some()
        || parsed.fragment().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || (parsed.scheme() != "https" && !(parsed.scheme() == "http" && loopback))
        || layer
            .management_timeout_ms
            .is_some_and(|value| !(1_000..=120_000).contains(&value))
    {
        return None;
    }
    Some(ExternalVmmManagementTarget {
        base_url,
        access_token,
        timeout: Duration::from_millis(layer.management_timeout_ms.unwrap_or(30_000)),
    })
}

/// Query payload accepted by bounded management list and detail actions.
/// 有界管理列表与详情动作接受的查询载荷。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagementQueryPayload {
    /// Optional session identifier.
    /// 可选会话标识。
    #[serde(default)]
    session_id: Option<String>,
    /// Optional turn identifier.
    /// 可选回合标识。
    #[serde(default)]
    turn_id: Option<String>,
    /// Optional operation identifier.
    /// 可选操作标识。
    #[serde(default)]
    operation_id: Option<String>,
    /// Optional recycle-batch identifier.
    /// 可选回收批次标识。
    #[serde(default)]
    batch_id: Option<String>,
    /// Optional filters forwarded only through the documented allowlist.
    /// 仅通过文档化白名单转发的可选筛选条件。
    #[serde(default)]
    filters: ManagementListFilters,
}

/// Documented management list filters.
/// 文档化的管理列表筛选条件。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagementListFilters {
    /// Stable user identifier represented as a decimal string.
    /// 以十进制字符串表示的稳定用户标识。
    #[serde(default)]
    user_id: Option<String>,
    /// Stable project identifier represented as a decimal string.
    /// 以十进制字符串表示的稳定项目标识。
    #[serde(default)]
    project_id: Option<String>,
    /// Stable session identifier represented as a decimal string.
    /// 以十进制字符串表示的稳定会话标识。
    #[serde(default)]
    session_id: Option<String>,
    /// Natural-language search text.
    /// 自然语言搜索文本。
    #[serde(default)]
    query: Option<String>,
    /// Exact lifecycle or extraction status.
    /// 准确生命周期或提炼状态。
    #[serde(default)]
    status: Option<String>,
    /// Inclusive creation lower bound.
    /// 创建时间包含式下界。
    #[serde(default)]
    created_from: Option<String>,
    /// Inclusive creation upper bound.
    /// 创建时间包含式上界。
    #[serde(default)]
    created_to: Option<String>,
    /// Inclusive update lower bound.
    /// 更新时间包含式下界。
    #[serde(default)]
    updated_from: Option<String>,
    /// Inclusive update upper bound.
    /// 更新时间包含式上界。
    #[serde(default)]
    updated_to: Option<String>,
    /// Optional derived-memory predicate.
    /// 可选派生记忆条件。
    #[serde(default)]
    has_memory: Option<bool>,
    /// Optional profile-reference predicate.
    /// 可选画像引用条件。
    #[serde(default)]
    has_profile: Option<bool>,
    /// Opaque continuation cursor.
    /// 不透明续传游标。
    #[serde(default)]
    cursor: Option<String>,
    /// Requested bounded page size.
    /// 请求的有界页大小。
    #[serde(default)]
    limit: Option<u32>,
    /// Requested UTF-8 chunk size.
    /// 请求的 UTF-8 分片大小。
    #[serde(default)]
    limit_bytes: Option<u32>,
    /// Requested UTF-8 content section.
    /// 请求的 UTF-8 内容区段。
    #[serde(default)]
    section: Option<String>,
    /// Requested timeline item index.
    /// 请求的时间线条目索引。
    #[serde(default)]
    index: Option<u32>,
    /// Requested UTF-8 byte offset.
    /// 请求的 UTF-8 字节偏移。
    #[serde(default)]
    offset: Option<u32>,
    /// Stable sort key.
    /// 稳定排序键。
    #[serde(default)]
    sort: Option<String>,
}

/// Removal preview payload projected to the VMM snake-case contract.
/// 投影到 VMM 下划线契约的移除预览载荷。
#[derive(Debug, Deserialize, Serialize)]
#[serde(
    rename_all(deserialize = "camelCase", serialize = "snake_case"),
    deny_unknown_fields
)]
struct ManagementRemovalPreviewPayload {
    /// Target resource kind.
    /// 目标资源类型。
    target_type: String,
    /// Exact decimal-string target identifiers.
    /// 准确的十进制字符串目标标识。
    target_ids: Vec<String>,
    /// Requested lifecycle action.
    /// 请求的生命周期动作。
    action: String,
    /// Human-client source marker.
    /// 人工客户端来源标记。
    source: String,
}

/// Preview execution payload projected to the VMM snake-case contract.
/// 投影到 VMM 下划线契约的预览执行载荷。
#[derive(Debug, Deserialize, Serialize)]
#[serde(
    rename_all(deserialize = "camelCase", serialize = "snake_case"),
    deny_unknown_fields
)]
struct ManagementExecutePayload {
    /// Previously issued preview token.
    /// 先前签发的预览令牌。
    preview_token: String,
    /// Optional permanent-delete confirmation phrase.
    /// 可选永久删除确认短语。
    #[serde(default)]
    confirm_text: String,
}

/// Restore payload projected to the VMM snake-case contract.
/// 投影到 VMM 下划线契约的恢复载荷。
#[derive(Debug, Deserialize, Serialize)]
#[serde(
    rename_all(deserialize = "camelCase", serialize = "snake_case"),
    deny_unknown_fields
)]
struct ManagementRestorePayload {
    /// Restorable recycle-batch identifier.
    /// 可恢复回收批次标识。
    batch_id: String,
}

/// Executes one allowlisted VMM management HTTP action without exposing its token to WebUI state.
/// 执行一个白名单 VMM 管理 HTTP 动作，且不把令牌暴露给 WebUI 状态。
pub(super) async fn service_run_vmm_management_http_action(
    state: SharedServiceBridgeState,
    request: VmmMemoryActionRequest,
) -> Result<VmmMemoryActionResult, String> {
    let target = external_management_target_from_state(&state)
        .ok_or_else(|| String::from("The external VMM management service is not ready yet."))?;
    let client = reqwest::Client::builder()
        .timeout(target.timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| format!("Failed to create the VMM management proxy client: {error}"))?;
    let action = request.action.trim().to_string();
    let (method, path, query, body) = management_request_parts(&action, &request.payload)?;
    reject_active_local_session_targets(&state, &client, &target, &action, body.as_ref()).await?;
    let mut url = Url::parse(&format!(
        "{}/management/v1/{path}",
        target.base_url.trim_end_matches('/')
    ))
    .map_err(|error| format!("Failed to parse the VMM management address: {error}"))?;
    append_management_query(&mut url, &query);
    let mut outgoing = client
        .request(method, url)
        .bearer_auth(&target.access_token)
        .header("Accept", "application/json");
    if let Some(body) = body {
        outgoing = outgoing.json(&body);
    }
    if action == "management_execute_removal"
        || action == "management_restore_batch"
        || action == "management_purge"
    {
        outgoing = outgoing.header("Idempotency-Key", request.command_id.trim());
    }
    let response = outgoing
        .send()
        .await
        .map_err(|error| format!("VMM management request failed: {error}"))?;
    let (status, bytes) =
        read_bounded_management_response(response, "VMM management response").await?;
    let mut value = serde_json::from_slice::<Value>(&bytes)
        .map_err(|error| format_non_json_management_response(status, &bytes, &error.to_string()))?;
    if !status.is_success() {
        return Err(format_management_problem(status, &value));
    }
    project_management_session_rows(&state, &action, &mut value)?;
    Ok(VmmMemoryActionResult {
        action: request.action,
        summary: management_action_summary(&action),
        kind: String::from("management"),
        request: request.payload,
        response: value.clone(),
        data: VmmMemoryActionResponse::Management(value),
    })
}

/// Rejects destructive previews that target a local session with an active execution.
/// 拒绝以本地正在执行的会话为目标的破坏性预览。
async fn reject_active_local_session_targets(
    state: &SharedServiceBridgeState,
    client: &reqwest::Client,
    target: &ExternalVmmManagementTarget,
    action: &str,
    body: Option<&Value>,
) -> Result<(), String> {
    if action != "management_create_removal_preview"
        || body
            .and_then(|value| value.get("action"))
            .and_then(Value::as_str)
            != Some("recycle")
    {
        return Ok(());
    }
    let target_type = body
        .and_then(|value| value.get("target_type"))
        .and_then(Value::as_str)
        .ok_or_else(|| String::from("Invalid VMM recycle target type."))?;
    if !matches!(target_type, "session" | "turn") {
        return Ok(());
    }
    let ids = body
        .and_then(|value| value.get("target_ids"))
        .and_then(Value::as_array)
        .ok_or_else(|| String::from("Invalid VMM session recycle target."))?;
    for id in ids {
        let id = id
            .as_str()
            .ok_or_else(|| String::from("VMM recycle targets must be decimal strings."))?;
        let session_id = if target_type == "session" {
            id.to_string()
        } else {
            let turn = load_management_detail(client, target, "turns", id).await?;
            let session_id = turn
                .get("session_id")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    String::from("The VMM turn details are missing the session identifier.")
                })?
                .to_string();
            required_decimal_id(Some(&session_id), "sessionId")?;
            session_id
        };
        let session = load_management_detail(client, target, "sessions", &session_id).await?;
        let session_key = session
            .get("session_key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| String::from("The VMM session details are missing the backend session key, so the active state cannot be checked safely."))?;
        if local_session_is_active(state, session_key)? {
            return Err(String::from(
                "The current session is still generating or running tools and cannot be moved to the recycle bin.",
            ));
        }
    }
    Ok(())
}

/// Loads one bounded management detail for backend-only active-session protection.
/// 为仅后端可见的活跃会话保护读取一条有界管理详情。
async fn load_management_detail(
    client: &reqwest::Client,
    target: &ExternalVmmManagementTarget,
    resource: &str,
    id: &str,
) -> Result<Value, String> {
    let url = Url::parse(&format!(
        "{}/management/v1/{resource}/{id}",
        target.base_url.trim_end_matches('/')
    ))
    .map_err(|error| format!("Failed to parse the VMM management address: {error}"))?;
    let response = client
        .get(url)
        .bearer_auth(&target.access_token)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| format!("Failed to check active sessions: {error}"))?;
    let (status, bytes) =
        read_bounded_management_response(response, "VMM management detail response").await?;
    let detail = serde_json::from_slice::<Value>(&bytes)
        .map_err(|error| format_non_json_management_response(status, &bytes, &error.to_string()))?;
    if !status.is_success() {
        return Err(format_management_problem(status, &detail));
    }
    Ok(detail)
}

/// Reads one management response incrementally and rejects an oversized body before buffering it in full.
/// 增量读取单次管理响应，并在完整缓冲超限正文前拒绝该响应。
async fn read_bounded_management_response(
    mut response: reqwest::Response,
    label: &str,
) -> Result<(StatusCode, Vec<u8>), String> {
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|length| length > MAX_MANAGEMENT_RESPONSE_BYTES as u64)
    {
        return Err(format!("The {label} exceeds the 1 MiB safety limit."));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("Failed to read the {label}: {error}"))?
    {
        if body.len().saturating_add(chunk.len()) > MAX_MANAGEMENT_RESPONSE_BYTES {
            return Err(format!("The {label} exceeds the 1 MiB safety limit."));
        }
        body.extend_from_slice(&chunk);
    }
    Ok((status, body))
}

/// Returns whether one exact local session key owns a non-terminal execution phase.
/// 返回一个准确本地会话键是否持有未终结执行阶段。
fn local_session_is_active(
    state: &SharedServiceBridgeState,
    session_key: &str,
) -> Result<bool, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("Failed to read the local session state: {error}"))?;
    let Some(workspace) = guard.workspace_snapshot.as_ref() else {
        return Ok(false);
    };
    let local = workspace
        .sessions
        .iter()
        .chain(workspace.archived_sessions.iter())
        .find(|session| session.id.to_string() == session_key);
    let Some(local) = local else {
        return Ok(false);
    };
    Ok(workspace
        .session_run_phases
        .get(&local.id)
        .is_some_and(|phase| {
            matches!(
                phase,
                vulcan_workbench_model::SessionRunPhase::Preparing
                    | vulcan_workbench_model::SessionRunPhase::MemoryRecall
                    | vulcan_workbench_model::SessionRunPhase::Requesting
                    | vulcan_workbench_model::SessionRunPhase::Thinking
                    | vulcan_workbench_model::SessionRunPhase::ToolCalling
                    | vulcan_workbench_model::SessionRunPhase::ToolRunning
            )
        }))
}

/// Replaces backend-only session keys with local titles and active-runtime protection metadata.
/// 用本地标题与活跃运行保护元数据替换仅后端可见的会话键。
fn project_management_session_rows(
    state: &SharedServiceBridgeState,
    action: &str,
    value: &mut Value,
) -> Result<(), String> {
    if !matches!(
        action,
        "management_list_sessions" | "management_get_session"
    ) {
        return Ok(());
    }
    let guard = state
        .lock()
        .map_err(|error| format!("Failed to read the local session projection: {error}"))?;
    let Some(workspace) = guard.workspace_snapshot.as_ref() else {
        strip_management_session_keys(value);
        return Ok(());
    };
    let project_one = |row: &mut Value| {
        let Some(object) = row.as_object_mut() else {
            return;
        };
        let session_key = object
            .remove("session_key")
            .and_then(|value| value.as_str().map(str::to_string));
        let Some(session_key) = session_key else {
            return;
        };
        let local = workspace
            .sessions
            .iter()
            .chain(workspace.archived_sessions.iter())
            .find(|session| session.id.to_string() == session_key);
        let Some(local) = local else {
            return;
        };
        object.insert("display_title".into(), Value::String(local.title.clone()));
        object.insert(
            "local_session_id".into(),
            Value::String(local.id.to_string()),
        );
        let active = workspace
            .session_run_phases
            .get(&local.id)
            .is_some_and(|phase| {
                matches!(
                    phase,
                    vulcan_workbench_model::SessionRunPhase::Preparing
                        | vulcan_workbench_model::SessionRunPhase::MemoryRecall
                        | vulcan_workbench_model::SessionRunPhase::Requesting
                        | vulcan_workbench_model::SessionRunPhase::Thinking
                        | vulcan_workbench_model::SessionRunPhase::ToolCalling
                        | vulcan_workbench_model::SessionRunPhase::ToolRunning
                )
            });
        object.insert("active_runtime".into(), Value::Bool(active));
    };
    if let Some(items) = value.get_mut("items").and_then(Value::as_array_mut) {
        for row in items {
            project_one(row);
        }
    } else {
        project_one(value);
    }
    Ok(())
}

/// Removes backend-only session keys when no local workspace projection is available.
/// 在没有本地工作区投影时移除仅后端可见的会话键。
fn strip_management_session_keys(value: &mut Value) {
    if let Some(items) = value.get_mut("items").and_then(Value::as_array_mut) {
        for row in items {
            if let Some(object) = row.as_object_mut() {
                object.remove("session_key");
            }
        }
    } else if let Some(object) = value.as_object_mut() {
        object.remove("session_key");
    }
}

/// Builds the exact HTTP method, path, query, and body for one allowlisted action.
/// 为一个白名单动作构建准确 HTTP 方法、路径、查询和请求体。
fn management_request_parts(
    action: &str,
    payload: &Value,
) -> Result<(Method, String, ManagementListFilters, Option<Value>), String> {
    match action {
        "management_capabilities" => Ok((
            Method::GET,
            String::from("capabilities"),
            ManagementListFilters::default(),
            None,
        )),
        "management_list_users" => Ok((
            Method::GET,
            String::from("users"),
            ManagementListFilters::default(),
            None,
        )),
        "management_list_projects" => Ok((
            Method::GET,
            String::from("projects"),
            ManagementListFilters::default(),
            None,
        )),
        "management_list_sessions" => {
            let payload: ManagementQueryPayload = decode_management_payload(payload)?;
            Ok((Method::GET, String::from("sessions"), payload.filters, None))
        }
        "management_get_session" => management_detail_parts(payload, "sessions", None),
        "management_list_session_turns" => {
            management_detail_parts(payload, "sessions", Some("turns"))
        }
        "management_list_turns" => {
            let payload: ManagementQueryPayload = decode_management_payload(payload)?;
            Ok((Method::GET, String::from("turns"), payload.filters, None))
        }
        "management_get_turn" => management_turn_parts(payload, false),
        "management_get_turn_content" => management_turn_parts(payload, true),
        "management_create_removal_preview" => {
            let decoded: ManagementRemovalPreviewPayload = decode_management_payload(payload)?;
            validate_management_removal_preview(&decoded)?;
            management_json_value_parts(decoded, "removal-previews")
        }
        "management_execute_removal" => {
            management_json_parts::<ManagementExecutePayload>(payload, "removals")
        }
        "management_restore_batch" => {
            management_json_parts::<ManagementRestorePayload>(payload, "restores")
        }
        "management_purge" => management_json_parts::<ManagementExecutePayload>(payload, "purges"),
        "management_list_recycle_batches" => {
            let payload: ManagementQueryPayload = decode_management_payload(payload)?;
            Ok((
                Method::GET,
                String::from("recycle-batches"),
                payload.filters,
                None,
            ))
        }
        "management_get_recycle_batch" => management_batch_parts(payload),
        "management_get_operation" => management_operation_parts(payload),
        _ => Err(format!("Unsupported VMM management action: {action}")),
    }
}

/// Builds one session detail or session-scoped turn-list request.
/// 构建一次会话详情或会话范围回合列表请求。
fn management_detail_parts(
    payload: &Value,
    resource: &str,
    suffix: Option<&str>,
) -> Result<(Method, String, ManagementListFilters, Option<Value>), String> {
    let payload: ManagementQueryPayload = decode_management_payload(payload)?;
    let session_id = required_decimal_id(payload.session_id.as_deref(), "sessionId")?;
    let path = suffix.map_or_else(
        || format!("{resource}/{session_id}"),
        |suffix| format!("{resource}/{session_id}/{suffix}"),
    );
    Ok((Method::GET, path, payload.filters, None))
}

/// Builds one turn detail or UTF-8 content request.
/// 构建一次回合详情或 UTF-8 内容请求。
fn management_turn_parts(
    payload: &Value,
    content: bool,
) -> Result<(Method, String, ManagementListFilters, Option<Value>), String> {
    let payload: ManagementQueryPayload = decode_management_payload(payload)?;
    let turn_id = required_decimal_id(payload.turn_id.as_deref(), "turnId")?;
    let path = if content {
        format!("turns/{turn_id}/content")
    } else {
        format!("turns/{turn_id}")
    };
    Ok((Method::GET, path, payload.filters, None))
}

/// Builds one recycle-batch detail request.
/// 构建一次回收批次详情请求。
fn management_batch_parts(
    payload: &Value,
) -> Result<(Method, String, ManagementListFilters, Option<Value>), String> {
    let payload: ManagementQueryPayload = decode_management_payload(payload)?;
    let batch_id = required_decimal_id(payload.batch_id.as_deref(), "batchId")?;
    Ok((
        Method::GET,
        format!("recycle-batches/{batch_id}"),
        ManagementListFilters::default(),
        None,
    ))
}

/// Builds one operation detail request.
/// 构建一次操作详情请求。
fn management_operation_parts(
    payload: &Value,
) -> Result<(Method, String, ManagementListFilters, Option<Value>), String> {
    let payload: ManagementQueryPayload = decode_management_payload(payload)?;
    let operation_id = payload
        .operation_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("operationId must not be empty."))?;
    if operation_id.len() > 128
        || !operation_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(String::from(
            "operationId may only contain ASCII letters, digits, underscores and hyphens, and at most 128 characters.",
        ));
    }
    Ok((
        Method::GET,
        format!("operations/{operation_id}"),
        ManagementListFilters::default(),
        None,
    ))
}

/// Serializes one strict POST payload into the VMM snake-case body.
/// 把一个严格 POST 载荷序列化为 VMM 下划线请求体。
fn management_json_parts<T>(
    payload: &Value,
    path: &str,
) -> Result<(Method, String, ManagementListFilters, Option<Value>), String>
where
    T: for<'de> Deserialize<'de> + Serialize,
{
    let decoded: T = decode_management_payload(payload)?;
    let body = serde_json::to_value(decoded)
        .map_err(|error| format!("Failed to build the VMM management request: {error}"))?;
    Ok((
        Method::POST,
        path.to_string(),
        ManagementListFilters::default(),
        Some(body),
    ))
}

/// Serializes one already-validated strict POST payload.
/// 序列化一个已经校验的严格 POST 载荷。
fn management_json_value_parts<T>(
    decoded: T,
    path: &str,
) -> Result<(Method, String, ManagementListFilters, Option<Value>), String>
where
    T: Serialize,
{
    let body = serde_json::to_value(decoded)
        .map_err(|error| format!("Failed to build the VMM management request: {error}"))?;
    Ok((
        Method::POST,
        path.to_string(),
        ManagementListFilters::default(),
        Some(body),
    ))
}

/// Validates a bounded removal preview before any HTTP request is dispatched.
/// 在分派任何 HTTP 请求前校验有界移除预览。
fn validate_management_removal_preview(
    payload: &ManagementRemovalPreviewPayload,
) -> Result<(), String> {
    if payload.target_ids.is_empty() || payload.target_ids.len() > 100 {
        return Err(String::from(
            "Each VMM management operation must select 1 to 100 targets.",
        ));
    }
    for id in &payload.target_ids {
        required_decimal_id(Some(id), "targetIds")?;
    }
    if !matches!(
        payload.target_type.as_str(),
        "session" | "turn" | "recycle_batch"
    ) {
        return Err(String::from("Invalid VMM management target type."));
    }
    if !matches!(
        payload.action.as_str(),
        "archive" | "unarchive" | "recycle" | "restart" | "purge"
    ) {
        return Err(String::from("Invalid VMM management operation type."));
    }
    if payload.source.trim().is_empty() || payload.source.len() > 128 {
        return Err(String::from(
            "The VMM management request source must not be empty and is limited to 128 characters.",
        ));
    }
    Ok(())
}

/// Decodes one strict action payload.
/// 解码一个严格动作载荷。
fn decode_management_payload<T>(payload: &Value) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(payload.clone())
        .map_err(|error| format!("Invalid VMM management parameters: {error}"))
}

/// Validates one positive decimal-string identifier before placing it into a path.
/// 在把标识放入路径前校验其为正十进制字符串。
fn required_decimal_id<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str, String> {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{field} must not be empty."))?;
    let parsed = value
        .parse::<u64>()
        .map_err(|_| format!("{field} must be a decimal identifier."))?;
    if parsed == 0 {
        return Err(format!("{field} must be greater than 0."));
    }
    Ok(value)
}

/// Appends only documented management query parameters.
/// 仅追加文档化的管理查询参数。
fn append_management_query(url: &mut Url, filters: &ManagementListFilters) {
    let mut pairs = url.query_pairs_mut();
    for (key, value) in [
        ("user_id", filters.user_id.as_deref()),
        ("project_id", filters.project_id.as_deref()),
        ("session_id", filters.session_id.as_deref()),
        ("query", filters.query.as_deref()),
        ("status", filters.status.as_deref()),
        ("created_from", filters.created_from.as_deref()),
        ("created_to", filters.created_to.as_deref()),
        ("updated_from", filters.updated_from.as_deref()),
        ("updated_to", filters.updated_to.as_deref()),
        ("cursor", filters.cursor.as_deref()),
        ("sort", filters.sort.as_deref()),
        ("section", filters.section.as_deref()),
    ] {
        if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
            pairs.append_pair(key, value);
        }
    }
    if let Some(value) = filters.has_memory {
        pairs.append_pair("has_memory", if value { "true" } else { "false" });
    }
    if let Some(value) = filters.has_profile {
        pairs.append_pair("has_profile", if value { "true" } else { "false" });
    }
    if let Some(value) = filters.limit_bytes.or(filters.limit) {
        pairs.append_pair("limit", &value.to_string());
    }
    if let Some(value) = filters.index {
        pairs.append_pair("index", &value.to_string());
    }
    if let Some(value) = filters.offset {
        pairs.append_pair("offset", &value.to_string());
    }
}

/// Formats one safe RFC 9457 management problem without leaking credentials.
/// 格式化一个安全的 RFC 9457 管理问题且不泄露凭据。
fn format_management_problem(status: StatusCode, value: &Value) -> String {
    let code = value
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or("management_error");
    let detail = value
        .get("detail")
        .and_then(Value::as_str)
        .unwrap_or("VMM management request failed.");
    let request_id = value
        .get("request_id")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    format!(
        "VMM management request failed: HTTP {status} | {code} | {detail} | request_id={request_id}"
    )
}

/// Formats a bounded non-JSON response without exposing markup or arbitrary binary data.
/// 格式化有界的非 JSON 响应，且不暴露标记文本或任意二进制数据。
fn format_non_json_management_response(
    status: StatusCode,
    bytes: &[u8],
    parse_error: &str,
) -> String {
    if status.is_success() {
        return format!("The VMM management service returned invalid JSON: {parse_error}");
    }
    let preview = String::from_utf8_lossy(bytes)
        .chars()
        .filter(|character| !character.is_control() || character.is_whitespace())
        .take(256)
        .collect::<String>();
    let preview = preview.trim();
    if preview.is_empty() {
        format!(
            "VMM management request failed: HTTP {status}; the server returned no structured error."
        )
    } else {
        format!("VMM management request failed: HTTP {status} | {preview}")
    }
}

/// Returns one compact user-facing action summary.
/// 返回一个紧凑的用户可见动作摘要。
fn management_action_summary(action: &str) -> String {
    match action {
        "management_create_removal_preview" => String::from("Generated the data impact preview."),
        "management_execute_removal" => String::from("Committed the memory data change."),
        "management_restore_batch" => String::from("Committed the recycled data restore."),
        "management_purge" => String::from("Committed the permanent deletion."),
        _ => String::from("Read the VMM management data."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Verifies arbitrary paths and unknown payload fields cannot cross the backend proxy.
    /// 验证任意路径与未知载荷字段无法穿过后端代理。
    #[test]
    fn management_proxy_rejects_unknown_actions_and_fields() {
        assert!(
            management_request_parts(
                "management_fetch_url",
                &json!({"url":"http://example.invalid"})
            )
            .is_err()
        );
        assert!(
            management_request_parts(
                "management_list_sessions",
                &json!({"filters":{},"accessToken":"secret"})
            )
            .is_err()
        );
    }

    /// Verifies a large decimal identifier remains an exact string in the generated path.
    /// 验证大十进制标识在生成路径中保持为准确字符串。
    #[test]
    fn management_proxy_preserves_string_identifier_precision() {
        let (_, path, _, _) = management_request_parts(
            "management_get_turn",
            &json!({"turnId":"9007199254740993","filters":{}}),
        )
        .expect("turn request should be valid");
        assert_eq!(path, "turns/9007199254740993");
    }

    /// Verifies operation identifiers cannot escape their allowlisted path segment.
    /// 验证操作标识不能逃逸其白名单路径段。
    #[test]
    fn management_proxy_rejects_unsafe_operation_identifiers() {
        assert!(
            management_request_parts(
                "management_get_operation",
                &json!({"operationId":"../capabilities","filters":{}}),
            )
            .is_err()
        );
        assert!(
            management_request_parts(
                "management_get_operation",
                &json!({"operationId":"operation_01-abc","filters":{}}),
            )
            .is_ok()
        );
    }

    /// Verifies server-derived identifiers are revalidated before they can be interpolated into a follow-up path.
    /// 验证服务端返回的标识在插入后续路径前会被重新校验。
    #[test]
    fn management_proxy_rejects_invalid_server_derived_identifier() {
        assert!(required_decimal_id(Some("../sessions"), "sessionId").is_err());
        assert!(required_decimal_id(Some("0"), "sessionId").is_err());
        assert_eq!(
            required_decimal_id(Some("9007199254740993"), "sessionId")
                .expect("large decimal string should remain valid"),
            "9007199254740993"
        );
    }

    /// Verifies non-JSON upstream failures retain a bounded actionable diagnostic.
    /// 验证非 JSON 上游失败会保留有界且可操作的诊断信息。
    #[test]
    fn management_proxy_formats_non_json_failure() {
        let message = format_non_json_management_response(
            StatusCode::BAD_GATEWAY,
            b"upstream unavailable",
            "expected value",
        );
        assert!(message.contains("HTTP 502"));
        assert!(message.contains("upstream unavailable"));
    }

    /// Verifies destructive previews are bounded and use only closed target and action enums.
    /// 验证破坏性预览有数量边界且只接受封闭的目标与操作枚举。
    #[test]
    fn management_proxy_validates_removal_preview_contract() {
        assert!(
            management_request_parts(
                "management_create_removal_preview",
                &json!({"targetType":"session","targetIds":[],"action":"recycle","source":"test"}),
            )
            .is_err()
        );
        assert!(management_request_parts(
            "management_create_removal_preview",
            &json!({"targetType":"session","targetIds":["1"],"action":"recycle","source":"test"}),
        )
        .is_ok());
        assert!(
            management_request_parts(
                "management_create_removal_preview",
                &json!({"targetType":"session","targetIds":["1"],"action":"drop","source":"test"}),
            )
            .is_err()
        );
    }
}
