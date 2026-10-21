//! Subagent model allowlists: the system default list in the global configuration and a session's
//! own replacement stored beside its V1 document. The session model is always granted on top.
//! 子代理模型白名单：全局配置中的系统默认清单，以及保存在会话 V1 文档旁的会话自有替换清单。
//! 会话模型始终额外获准。

use serde::{Deserialize, Serialize};
use vulcan_provider_config::provider_config::{
    MAX_SUBAGENT_MODEL_GRANTS, MAX_SUBAGENT_MODEL_NOTE_CHARS, SubagentModelGrant,
    normalize_subagent_model_grants,
};
use vulcan_service_api::{
    SessionSubagentModelsSaveRequest, SessionSubagentModelsSnapshot, SubagentModelEntry,
    SubagentModelsSaveRequest, SubagentModelsSnapshot,
};
use vulcan_workspace_store::workspace_store::{
    load_session_subagent_models, save_session_subagent_models,
};

use crate::SharedServiceBridgeState;
use crate::service_state_access::{
    mutate_provider_configuration, provider_configuration_from_state,
};
use crate::session_runtime_registry_v1::SessionHandle;

/// Exact version of the session allowlist file; other versions are rejected.
/// 会话白名单文件的精确版本；其他版本一律拒绝。
const SESSION_SUBAGENT_MODELS_VERSION: u32 = 1;

/// The session allowlist file stored beside the V1 document.
/// 保存在 V1 文档旁的会话白名单文件。
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionSubagentModelsDocument {
    /// Exact file version.
    /// 精确文件版本。
    version: u32,
    /// Granted models in display order.
    /// 按显示顺序排列的授权模型。
    models: Vec<SubagentModelGrant>,
}

/// Converts one wire entry into a stored grant.
/// 将一个线上条目转换为存储授权。
fn grant_from_entry(entry: SubagentModelEntry) -> SubagentModelGrant {
    SubagentModelGrant {
        provider_id: entry.provider_id,
        model_id: entry.model_id,
        note: entry.note,
    }
}

/// Converts one stored grant into a wire entry.
/// 将一个存储授权转换为线上条目。
fn entry_from_grant(grant: &SubagentModelGrant) -> SubagentModelEntry {
    SubagentModelEntry {
        provider_id: grant.provider_id.clone(),
        model_id: grant.model_id.clone(),
        note: grant.note.clone(),
    }
}

/// Validates a wire list before it is saved anywhere.
/// 在保存到任何位置前校验线上清单。
fn normalized_grants(entries: Vec<SubagentModelEntry>) -> Result<Vec<SubagentModelGrant>, String> {
    normalize_subagent_model_grants(entries.into_iter().map(grant_from_entry).collect())
}

/// Reads the session's own allowlist; `None` means the session follows the system list.
/// 读取会话自有白名单；`None` 表示会话跟随系统清单。
/// A damaged file or another version is an error, so a restricted session is never widened.
/// 文件损坏或版本不符时返回错误，受限会话绝不会被放宽。
pub(crate) fn session_subagent_model_grants(
    handle: &SessionHandle,
) -> Result<Option<Vec<SubagentModelGrant>>, String> {
    let locator = handle
        .storage_locator()
        .map_err(|error| error.to_string())?;
    load_session_subagent_models::<SessionSubagentModelsDocument>(&locator)?
        .map(grants_from_document)
        .transpose()
}

/// Accepts only the exact file version.
/// 只接受精确文件版本。
fn grants_from_document(
    document: SessionSubagentModelsDocument,
) -> Result<Vec<SubagentModelGrant>, String> {
    if document.version != SESSION_SUBAGENT_MODELS_VERSION {
        return Err(format!(
            "Session subagent model allowlist version {} is not supported.",
            document.version
        ));
    }
    Ok(document.models)
}

/// Builds the system snapshot from stored grants.
/// 由存储授权构建系统快照。
fn system_snapshot(grants: &[SubagentModelGrant]) -> SubagentModelsSnapshot {
    SubagentModelsSnapshot {
        models: grants.iter().map(entry_from_grant).collect(),
        max_note_chars: MAX_SUBAGENT_MODEL_NOTE_CHARS,
        max_models: MAX_SUBAGENT_MODEL_GRANTS,
    }
}

/// Returns the system default list.
/// 返回系统默认清单。
pub(crate) fn service_subagent_models(
    state: SharedServiceBridgeState,
) -> Result<SubagentModelsSnapshot, String> {
    let configuration = provider_configuration_from_state(&state)?;
    Ok(system_snapshot(&configuration.subagent_models))
}

/// Replaces the system default list; sessions with their own list keep it.
/// 替换系统默认清单；拥有自有清单的会话保留其清单。
pub(crate) async fn service_save_subagent_models(
    state: SharedServiceBridgeState,
    request: SubagentModelsSaveRequest,
) -> Result<SubagentModelsSnapshot, String> {
    let grants = normalized_grants(request.models)?;
    mutate_provider_configuration(
        &state,
        "Failed to save the subagent model allowlist",
        |configuration| {
            configuration.subagent_models = grants;
            Ok(())
        },
        |_, configuration| system_snapshot(&configuration.subagent_models),
    )
    .await
}

/// Builds one session snapshot next to the system list.
/// 构建一个会话快照及其系统清单。
fn session_snapshot(
    state: &SharedServiceBridgeState,
    session_id: String,
    grants: Option<Vec<SubagentModelGrant>>,
) -> Result<SessionSubagentModelsSnapshot, String> {
    let configuration = provider_configuration_from_state(state)?;
    Ok(SessionSubagentModelsSnapshot {
        session_id,
        models: grants.map(|grants| grants.iter().map(entry_from_grant).collect()),
        system_models: configuration
            .subagent_models
            .iter()
            .map(entry_from_grant)
            .collect(),
        max_note_chars: MAX_SUBAGENT_MODEL_NOTE_CHARS,
        max_models: MAX_SUBAGENT_MODEL_GRANTS,
    })
}

/// Returns one session's own list next to the system list.
/// 返回一个会话的自有清单及系统清单。
pub(crate) async fn service_session_subagent_models(
    state: SharedServiceBridgeState,
    session_id: String,
) -> Result<SessionSubagentModelsSnapshot, String> {
    let (id, _lifecycle, handle) =
        crate::permission_mode::session_access_writer(&state, &session_id).await?;
    let grants = session_subagent_model_grants(handle.as_ref())?;
    session_snapshot(&state, id.to_string(), grants)
}

/// Saves one session's own list, or returns the session to the system list with `None`.
/// 保存一个会话的自有清单；`None` 使会话恢复跟随系统清单。
/// The next turn reads the saved list; children already running keep their model.
/// 下一个回合读取已保存清单；已在运行的子代理保留其模型。
pub(crate) async fn service_save_session_subagent_models(
    state: SharedServiceBridgeState,
    request: SessionSubagentModelsSaveRequest,
) -> Result<SessionSubagentModelsSnapshot, String> {
    let grants = request.models.map(normalized_grants).transpose()?;
    let (id, _lifecycle, handle) =
        crate::permission_mode::session_access_writer(&state, &request.session_id).await?;
    let locator = handle
        .storage_locator()
        .map_err(|error| error.to_string())?;
    let document = grants.clone().map(|models| SessionSubagentModelsDocument {
        version: SESSION_SUBAGENT_MODELS_VERSION,
        models,
    });
    save_session_subagent_models(&locator, document.as_ref())?;
    session_snapshot(&state, id.to_string(), grants)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(value: serde_json::Value) -> SessionSubagentModelsDocument {
        serde_json::from_value(value).expect("document shape")
    }

    #[test]
    fn session_allowlist_accepts_only_the_current_version() {
        let models =
            serde_json::json!([{ "providerId": "openrouter", "modelId": "vendor/m", "note": "n" }]);
        let grants = grants_from_document(document(
            serde_json::json!({ "version": SESSION_SUBAGENT_MODELS_VERSION, "models": models }),
        ))
        .expect("current version");
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].selector(), "openrouter/vendor/m");
        assert!(
            grants_from_document(document(serde_json::json!({
                "version": SESSION_SUBAGENT_MODELS_VERSION + 1,
                "models": models,
            })))
            .is_err()
        );
        assert!(
            serde_json::from_value::<SessionSubagentModelsDocument>(serde_json::json!({
                "version": SESSION_SUBAGENT_MODELS_VERSION,
                "models": [],
                "extra": true,
            }))
            .is_err()
        );
    }
}
