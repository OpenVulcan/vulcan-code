//! Subagent model allowlists: the system default list in the global configuration and a session's
//! own replacement stored beside its V1 document, including optional session-model inheritance.
//! 子代理模型白名单：全局配置中的系统默认清单，以及保存在会话 V1 文档旁的会话自有替换清单。
//! 会话模型按显式开关额外获准。

use serde::{Deserialize, Serialize};
use vulcan_provider_config::provider_config::{
    MAX_SUBAGENT_MODEL_GRANTS, MAX_SUBAGENT_MODEL_NOTE_CHARS, SubagentModelGrant,
    SubagentModelPolicy, normalize_subagent_model_grants,
};
use vulcan_service_api::{
    SessionSubagentModelsSaveRequest, SessionSubagentModelsSnapshot, SubagentModelEntry,
    SubagentModelsSaveRequest, SubagentModelsSnapshot,
};
use vulcan_workspace_store::workspace_store::{
    load_session_subagent_models_with_upgrade, save_session_subagent_models,
};

use crate::SharedServiceBridgeState;
use crate::service_state_access::{
    mutate_provider_configuration, provider_configuration_from_state,
};
use crate::session_runtime_registry_v1::SessionHandle;

/// Current policy version; only the known previous shape can upgrade to it.
/// 当前策略版本；仅已知上一版形状可以升级到此版本。
const SESSION_SUBAGENT_MODELS_VERSION: u32 = 2;

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
    /// Explicit inclusion of the calling session model.
    /// 显式包含调用会话模型。
    include_session_model: bool,
}

/// Decodes exact policy versions; v1 always included the session model by contract.
/// 解码精确策略版本；v1 按契约始终包含会话模型。
/// `bytes` are preserved by the storage layer; returns a validated v2 document and optional upgrade source.
/// `bytes` 由存储层保留；返回已校验 v2 文档与可选升级来源。
fn decode_policy_document(
    bytes: &[u8],
) -> Result<(SessionSubagentModelsDocument, Option<u32>), String> {
    /// The header selects the exact shape before decoding mandatory fields.
    /// 版本头在解码必填字段前选择精确形状。
    #[derive(Deserialize)]
    struct Header {
        /// Persisted policy version.
        /// 持久化策略版本。
        version: u32,
    }
    /// The previous policy shape grants the session model unconditionally.
    /// 上一版策略形状无条件授权会话模型。
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Previous {
        /// Verified previous version.
        /// 已验证的上一版本。
        version: u32,
        /// Existing explicit grants, preserved during upgrade.
        /// 升级时保留的既有显式授权。
        models: Vec<SubagentModelGrant>,
    }
    // Invalid versions never reach publication or backup creation.
    // 无效版本绝不进入发布或备份创建。
    let header: Header = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    // Current documents require the explicit flag; only the known previous shape supplies it.
    // 当前文档必须具有显式开关；只有已知上一版形状能提供该开关的旧语义。
    let (mut document, upgraded) = match header.version {
        1 => {
            // Validate the complete previous shape before adding its established semantics.
            // 补入既有语义前校验完整上一版形状。
            let previous: Previous =
                serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
            (
                SessionSubagentModelsDocument {
                    version: SESSION_SUBAGENT_MODELS_VERSION,
                    models: previous.models,
                    include_session_model: true,
                },
                Some(previous.version),
            )
        }
        SESSION_SUBAGENT_MODELS_VERSION => (
            serde_json::from_slice(bytes).map_err(|error| error.to_string())?,
            None,
        ),
        version => {
            return Err(format!(
                "Session subagent model allowlist version {version} is not supported."
            ));
        }
    };
    document.models = normalize_subagent_model_grants(document.models)?;
    Ok((document, upgraded))
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
/// A damaged or unsupported file is an error; v1 upgrades preserve its known inclusion behavior.
/// 损坏或不支持的文件返回错误；v1 升级保留其已知包含行为。
pub(crate) fn session_subagent_model_policy(
    handle: &SessionHandle,
) -> Result<Option<SubagentModelPolicy>, String> {
    let locator = handle
        .storage_locator()
        .map_err(|error| error.to_string())?;
    load_session_subagent_models_with_upgrade(&locator, decode_policy_document)?
        .map(grants_from_document)
        .transpose()
}

/// Accepts only the exact file version.
/// 只接受精确文件版本。
fn grants_from_document(
    document: SessionSubagentModelsDocument,
) -> Result<SubagentModelPolicy, String> {
    if document.version != SESSION_SUBAGENT_MODELS_VERSION {
        return Err(format!(
            "Session subagent model allowlist version {} is not supported.",
            document.version
        ));
    }
    Ok(SubagentModelPolicy {
        include_session_model: document.include_session_model,
        models: normalize_subagent_model_grants(document.models)?,
    })
}

/// Builds the system snapshot from stored grants.
/// 由存储授权构建系统快照。
fn system_snapshot(
    grants: &[SubagentModelGrant],
    include_session_model: bool,
) -> SubagentModelsSnapshot {
    SubagentModelsSnapshot {
        include_session_model,
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
    Ok(system_snapshot(
        &configuration.subagent_models,
        !configuration.subagent_exclude_session_model,
    ))
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
            configuration.subagent_exclude_session_model = !request.include_session_model;
            Ok(())
        },
        |_, configuration| {
            system_snapshot(
                &configuration.subagent_models,
                !configuration.subagent_exclude_session_model,
            )
        },
    )
    .await
}

/// Builds one session snapshot next to the system list.
/// 构建一个会话快照及其系统清单。
fn session_snapshot(
    state: &SharedServiceBridgeState,
    session_id: String,
    grants: Option<SubagentModelPolicy>,
) -> Result<SessionSubagentModelsSnapshot, String> {
    let configuration = provider_configuration_from_state(state)?;
    Ok(SessionSubagentModelsSnapshot {
        session_id,
        include_session_model: grants.as_ref().map(|policy| policy.include_session_model),
        system_include_session_model: !configuration.subagent_exclude_session_model,
        models: grants.map(|policy| policy.models.iter().map(entry_from_grant).collect()),
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
    let grants = session_subagent_model_policy(handle.as_ref())?;
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
    let grants = match (request.models, request.include_session_model) {
        (Some(models), Some(include_session_model)) => Some(SubagentModelPolicy {
            include_session_model,
            models: normalized_grants(models)?,
        }),
        (None, None) => None,
        _ => {
            return Err(String::from(
                "Session subagent models and inclusion must both be present or both be null.",
            ));
        }
    };
    let (id, _lifecycle, handle) =
        crate::permission_mode::session_access_writer(&state, &request.session_id).await?;
    let locator = handle
        .storage_locator()
        .map_err(|error| error.to_string())?;
    let document = grants.clone().map(|policy| SessionSubagentModelsDocument {
        version: SESSION_SUBAGENT_MODELS_VERSION,
        include_session_model: policy.include_session_model,
        models: policy.models,
    });
    save_session_subagent_models(&locator, document.as_ref())?;
    session_snapshot(&state, id.to_string(), grants)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Upgrades only the known previous semantics and rejects missing current fields or invalid grants.
    /// 仅升级已知上一版语义，并拒绝缺少当前字段或无效授权的文档。
    /// Inputs are synthetic bytes; no live settings are read or written.
    /// 输入为合成字节；不读写真实设置。
    #[test]
    fn subagent_policy_upgrade_preserves_inclusion_and_rejects_corruption() {
        // The previous empty list still permitted the calling session model.
        // 上一版空清单仍然允许调用会话模型。
        let (upgraded, source) = decode_policy_document(br#"{"version":1,"models":[]}"#).unwrap();
        assert_eq!(source, Some(1));
        assert_eq!(upgraded.version, SESSION_SUBAGENT_MODELS_VERSION);
        assert!(upgraded.include_session_model);
        // Current false is authoritative and must survive serialization and reload.
        // 当前 false 是权威值，必须在序列化及重新加载后保留。
        let current = serde_json::json!({ "version": SESSION_SUBAGENT_MODELS_VERSION, "models": [], "includeSessionModel": false });
        let (decoded, source) =
            decode_policy_document(&serde_json::to_vec(&current).unwrap()).unwrap();
        assert!(source.is_none());
        assert!(!grants_from_document(decoded).unwrap().include_session_model);
        for invalid in [
            serde_json::json!({"version": SESSION_SUBAGENT_MODELS_VERSION, "models": []}),
            serde_json::json!({"version": SESSION_SUBAGENT_MODELS_VERSION + 1, "models": [], "includeSessionModel": true}),
            serde_json::json!({"version": 0, "models": []}),
            serde_json::json!({"version": 1, "models": [], "includeSessionModel": false}),
            serde_json::json!({"version": 1, "models": [{"providerId":"", "modelId":"m"}]}),
        ] {
            assert!(decode_policy_document(&serde_json::to_vec(&invalid).unwrap()).is_err());
        }
    }

    fn document(value: serde_json::Value) -> SessionSubagentModelsDocument {
        serde_json::from_value(value).expect("document shape")
    }

    #[test]
    fn session_allowlist_accepts_only_the_current_version() {
        let models =
            serde_json::json!([{ "providerId": "openrouter", "modelId": "vendor/m", "note": "n" }]);
        let grants = grants_from_document(document(
            serde_json::json!({ "version": SESSION_SUBAGENT_MODELS_VERSION, "models": models, "includeSessionModel": true }),
        ))
        .expect("current version");
        assert_eq!(grants.models.len(), 1);
        assert_eq!(grants.models[0].selector(), "openrouter/vendor/m");
        assert!(
            grants_from_document(document(serde_json::json!({
                "version": SESSION_SUBAGENT_MODELS_VERSION + 1,
                "includeSessionModel": true,
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
