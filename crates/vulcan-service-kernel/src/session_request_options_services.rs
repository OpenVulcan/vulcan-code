//! Session request option services: the prompt-cache lifetime and service tier of one session.
//! 会话请求选项服务：单个会话的提示词缓存时长与服务档位。
//!
//! The options are stored per session and applied only by routes that offer them; other routes
//! ignore them, so they need no provider or model configuration.
//! 选项按会话保存，只由提供它们的路由应用；其他路由忽略它们，因此无需供应商或模型配置。

use serde::{Deserialize, Serialize};
use vulcan_session_core::v1::config::RequestOptionsConfig;
use vulcan_workbench_model::SessionRuntimeStateEntry;

use crate::SharedServiceBridgeState;
use crate::session_goal_lifecycle::{current_v1_goal_unix_millis, session_v1_handle_from_state};
use crate::session_request_config_v1::set_request_options_v1;
use crate::workspace_persistence_versions::parse_session_id;
use crate::workspace_session_queries::workspace_snapshot_contains_regular_session;

/// Requests the stored request options of one session.
/// 请求一个会话已保存的请求选项。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SessionRequestOptionsQuery {
    /// Exact persisted session identifier.
    /// 精确的已持久化会话标识。
    conversation_id: String,
}

/// Replaces the request options of one session.
/// 替换一个会话的请求选项。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SessionRequestOptionsUpdateRequest {
    /// Exact persisted session identifier.
    /// 精确的已持久化会话标识。
    conversation_id: String,
    /// Prompt-cache lifetime option id such as `1h`; absent or blank uses the route default.
    /// 提示词缓存时长选项标识，如 `1h`；缺省或空白时使用路由默认值。
    #[serde(default)]
    prompt_cache_ttl: Option<String>,
    /// Service tier such as `priority`; absent or blank uses the standard tier.
    /// 服务档位，如 `priority`；缺省或空白时使用标准档位。
    #[serde(default)]
    service_tier: Option<String>,
}

/// Request options stored for one session.
/// 为一个会话保存的请求选项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionRequestOptionsRecord {
    /// Session the options belong to.
    /// 选项所属会话。
    conversation_id: String,
    /// Chosen prompt-cache lifetime; null means the route default.
    /// 已选提示词缓存时长；null 表示路由默认值。
    prompt_cache_ttl: Option<String>,
    /// Chosen service tier; null means the standard tier.
    /// 已选服务档位；null 表示标准档位。
    service_tier: Option<String>,
}

/// Reads the request options stored for one session.
/// 读取一个会话已保存的请求选项。
/// Returns the stored options; draft or unknown sessions are errors.
/// 返回已保存选项；草稿或未知会话返回错误。
pub(crate) async fn service_get_session_request_options(
    state: SharedServiceBridgeState,
    request: SessionRequestOptionsQuery,
) -> Result<SessionRequestOptionsRecord, String> {
    let session_id = existing_session_id(&state, &request.conversation_id)?;
    let handle = session_v1_handle_from_state(&state, session_id).await?;
    let options = handle
        .read(|document| document.metadata.config.selection.request_options.clone())
        .map_err(|error| format!("Failed to read the V1 session request options: {error}"))?;
    Ok(record(request.conversation_id, options))
}

/// Saves the request options of one session and refreshes its runtime projection.
/// 保存一个会话的请求选项并刷新其运行时投影。
/// The next request of the session uses the saved options; returns them as stored.
/// 会话的下一次请求使用已保存选项；返回保存后的选项。
pub(crate) async fn service_set_session_request_options(
    state: SharedServiceBridgeState,
    request: SessionRequestOptionsUpdateRequest,
) -> Result<SessionRequestOptionsRecord, String> {
    let session_id = existing_session_id(&state, &request.conversation_id)?;
    let options = RequestOptionsConfig {
        prompt_cache_ttl: normalized_option(request.prompt_cache_ttl),
        service_tier: normalized_option(request.service_tier),
    };
    let updated_at_ms = current_v1_goal_unix_millis()?;
    let handle = session_v1_handle_from_state(&state, session_id).await?;
    set_request_options_v1(handle.as_ref(), options, updated_at_ms)?;
    let persisted = handle
        .read(|document| document.metadata.config.selection.request_options.clone())
        .map_err(|error| format!("Failed to read the V1 session request options: {error}"))?;
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let updated_at = updated_at_ms.to_string();
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: updated_at.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        runtime_state.request_options = persisted.clone();
        runtime_state.updated_at = updated_at;
    }
    Ok(record(request.conversation_id, persisted))
}

/// Resolves a persisted session that the workspace snapshot lists.
/// 解析工作区快照列出的已持久化会话。
fn existing_session_id(
    state: &SharedServiceBridgeState,
    conversation_id: &str,
) -> Result<crate::SessionId, String> {
    let session_id = parse_session_id(conversation_id).ok_or_else(|| {
        String::from(
            "The current draft session has not been created yet; its request options cannot be changed for now.",
        )
    })?;
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let workspace_snapshot = guard.workspace_snapshot.as_ref().ok_or_else(|| {
        String::from("service is still starting; workspace snapshot is not ready")
    })?;
    if !workspace_snapshot_contains_regular_session(workspace_snapshot, session_id) {
        return Err(format!("Session not found: {session_id}"));
    }
    Ok(session_id)
}

/// Trims one option value; blank means the default.
/// 修剪一个选项值；空白表示默认值。
fn normalized_option(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// Builds the wire record of stored options.
/// 构造已保存选项的线协议记录。
fn record(conversation_id: String, options: RequestOptionsConfig) -> SessionRequestOptionsRecord {
    SessionRequestOptionsRecord {
        conversation_id,
        prompt_cache_ttl: options.prompt_cache_ttl,
        service_tier: options.service_tier,
    }
}
