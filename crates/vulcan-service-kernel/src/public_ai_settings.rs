//! Public AI connection settings and runtime metadata services.
//! 公共 AI 连接设置与运行时元数据服务。

mod catalog_projection;
mod connections;
mod credential_repair;
mod custom_models;
mod custom_providers;
mod general_settings;
mod inference_settings;
mod inference_testing;
mod inference_validation;
mod media_candidates;
mod media_delivery_settings;
mod model_selection;
mod model_settings;
mod oauth;
pub(crate) mod openvulcan_account;
mod provider_synchronization;
mod provider_tasks;
mod provider_tools;
mod publication;
mod session_tools;
mod snapshot;

// Preserve original browser contract paths while their projection owners remain private.
// 保留原浏览器契约路径，同时投影实现模块保持私有。
#[cfg(test)]
use catalog_projection::build_public_ai_model_route_records;
#[expect(
    unused_imports,
    reason = "Preserve the original catalog projection type paths."
)]
pub(super) use catalog_projection::{
    PublicAiConnectionModelAccessPolicyRecord, PublicAiModelDirectoryKind, PublicAiModelRouteRecord,
};
use catalog_projection::{
    catalog_model_supports_text_conversation, effective_catalog_model_protocol_id,
};
// Preserve inferred settings request contracts at the original RPC boundary.
// 在原 RPC 边界保留通过类型推导消费的设置请求契约。
#[expect(
    unused_imports,
    reason = "Preserve the original general-settings request type paths."
)]
pub(super) use general_settings::{
    PublicAiContextCompactionWindowNodesSaveRequest,
    PublicAiContextWindowPolicyOverridesSaveRequest, PublicAiModelTakeoverNoticeAcknowledgeRequest,
    PublicAiNewSessionContextWindowDefaultSaveRequest, PublicAiProviderPrioritySaveRequest,
    PublicAiUsageRefreshSettingsRequest,
};
pub(super) use general_settings::{
    service_ack_public_ai_model_takeover_notices,
    service_save_public_ai_context_compaction_window_nodes,
    service_save_public_ai_context_window_policy_overrides,
    service_save_public_ai_new_session_context_window_default,
    service_save_public_ai_provider_priority, service_save_public_ai_usage_refresh_settings,
};
// Preserve nested browser snapshot contracts even when serialization infers their types.
// 即使序列化推导嵌套类型，也保留原浏览器快照契约。
pub(super) use media_delivery_settings::{
    service_reset_public_ai_media_delivery_policy, service_save_public_ai_media_delivery_policy,
};
use snapshot::build_public_ai_settings_snapshot;
#[expect(
    unused_imports,
    reason = "Preserve the original browser snapshot type paths."
)]
pub(super) use snapshot::{
    PublicAiConnectionSnapshot, PublicAiCustomModelConnectionAvailabilityRecord,
    PublicAiCustomModelProtocolPreferenceRecord, PublicAiCustomModelTemplateDecisionRecord,
    PublicAiModelAccessRecord, PublicAiModelCapabilityOverrideRecord, PublicAiModelOverrideRecord,
    PublicAiMultimodalUnderstandingCandidateRecord, PublicAiProviderToolTargetSnapshot,
    PublicAiSettingsSnapshot,
};

use connections::{add_public_ai_connection_with_secret, apply_connection_update};
// Preserve connection request paths, including contracts inferred by RPC dispatch.
// 保留连接请求路径，包括 RPC 分发通过类型推导消费的契约。
#[expect(
    unused_imports,
    reason = "Preserve the original connection request type paths."
)]
pub(super) use connections::{
    PublicAiConnectionAddRequest, PublicAiConnectionEnabledSaveRequest,
    PublicAiConnectionRemoveRequest, PublicAiConnectionUpdateRequest,
    PublicAiDefaultConnectionSaveRequest,
};
pub(super) use connections::{
    service_add_public_ai_connection, service_query_public_ai_connection_usage,
    service_remove_public_ai_connection, service_save_public_ai_connection_enabled,
    service_save_public_ai_default_connection, service_sync_public_ai_connection,
    service_update_public_ai_connection,
};
// Preserve custom-model RPC type paths, including nested contracts inferred by dispatch.
// 保留自定义模型 RPC 类型路径，包括分发层推导的嵌套契约。
#[expect(
    unused_imports,
    reason = "Preserve the original custom-model RPC type paths."
)]
pub(super) use custom_models::{
    PublicAiCustomModelConnectionAvailabilitySaveRequest,
    PublicAiCustomModelProtocolPreferenceItem, PublicAiCustomModelProtocolPreferencesSaveRequest,
    PublicAiCustomModelTemplateDecisionBatchItem, PublicAiCustomModelTemplateDecisionSaveRequest,
    PublicAiCustomModelTemplateDecisionsBatchSaveRequest,
    PublicAiCustomModelTemplateSuggestionsBatchRequest,
    PublicAiCustomModelTemplateSuggestionsRecord, PublicAiCustomModelTemplateSuggestionsRequest,
    PublicAiCustomModelValidationRequest, PublicAiCustomModelValidationResult,
    PublicAiCustomModelsSaveRequest, PublicAiCustomProviderModelDefaultsSaveRequest,
};
pub(super) use custom_models::{
    service_public_ai_custom_model_template_suggestions,
    service_public_ai_custom_model_template_suggestions_batch,
    service_save_public_ai_custom_model_connection_availability,
    service_save_public_ai_custom_model_protocol_preferences,
    service_save_public_ai_custom_model_template_decision,
    service_save_public_ai_custom_model_template_decisions_batch,
    service_save_public_ai_custom_models, service_save_public_ai_custom_provider_model_defaults,
    service_validate_public_ai_custom_model,
};
// Preserve the existing custom-provider RPC contracts when dispatch infers their types.
// 分发层推导类型时仍保留既有自定义供应商 RPC 契约。
#[expect(
    unused_imports,
    reason = "Preserve the original custom-provider RPC type paths."
)]
pub(super) use custom_providers::{
    PublicAiCustomProviderProbeResult, PublicAiCustomProviderRemoveRequest,
    PublicAiCustomProviderSaveRequest,
};
pub(super) use custom_providers::{
    service_probe_public_ai_custom_provider, service_remove_public_ai_custom_provider,
    service_save_public_ai_custom_provider,
};
// Preserve internal-inference request paths when dispatch infers their contracts.
// 分发层推导契约时保留内部推理请求路径。
pub(super) use inference_settings::{
    KNOWLEDGE_CURATION_PURPOSE_ID, remove_knowledge_inference_profile,
    resolve_knowledge_internal_inference_candidate, service_save_public_ai_inference_route,
    service_save_public_ai_internal_model_enabled, service_save_public_ai_internal_model_overrides,
    sync_knowledge_inference_profile,
};
#[expect(
    unused_imports,
    reason = "Preserve the original internal-inference request type paths."
)]
pub(super) use inference_settings::{
    PublicAiInferenceRouteDiagnostic, PublicAiInferenceRouteSaveRequest,
    PublicAiInternalInferenceCandidate, PublicAiInternalModelEnabledSaveRequest,
    PublicAiInternalModelOverridesSaveRequest,
};
pub(super) use inference_testing::service_test_public_ai_inference_route;
// Preserve the original crate-visible RPC type paths even when dispatch infers their types.
// 即使分发层使用类型推导，也保留原有的 crate 可见 RPC 类型路径。
#[expect(
    unused_imports,
    reason = "Preserve the original RPC contract type paths."
)]
pub(super) use inference_testing::{PublicAiInferenceTestRequest, PublicAiInferenceTestResult};
// Preserve the original media candidate contract while snapshot assembly owns its use.
// 快照组装持有候选的使用逻辑，同时保留原媒体候选契约。
#[expect(
    unused_imports,
    reason = "Preserve the original media candidate type path."
)]
pub(super) use media_candidates::PublicAiMediaUnderstandingModelCandidate;
// Preserve the model-selection request path while dispatch infers its contract.
// 分发层推导契约时保留模型选择请求路径。
#[expect(
    unused_imports,
    reason = "Preserve the original model-selection request type path."
)]
pub(super) use model_selection::PublicAiModelSelectionRequest;
#[cfg(test)]
pub(crate) use model_selection::reconcile_missing_session_model_selections;
pub(super) use model_selection::service_select_public_ai_model;
use model_selection::{
    ensure_active_model_selection, first_enabled_model_selection,
    plan_reconciled_session_model_selections, preferred_replacement_model_selection,
    reconcile_disabled_connection_sessions, reconcile_explicitly_revoked_session_model_selections,
};
// Preserve model-settings RPC type paths while dispatch infers request contracts.
// 分发层推导请求契约时保留模型设置 RPC 类型路径。
#[expect(
    unused_imports,
    reason = "Preserve the original model-settings RPC type paths."
)]
pub(super) use model_settings::{
    PublicAiConnectionModelEnabledSaveRequest, PublicAiConnectionModelsEnabledSaveRequest,
    PublicAiModelCapabilityOverridesSaveRequest, PublicAiModelEnabledSaveRequest,
    PublicAiModelOverridesSaveRequest, PublicAiProviderCapabilityOverridesSaveRequest,
};
pub(super) use model_settings::{
    service_save_public_ai_connection_model_enabled,
    service_save_public_ai_connection_models_enabled,
    service_save_public_ai_model_capability_overrides, service_save_public_ai_model_enabled,
    service_save_public_ai_model_overrides, service_save_public_ai_provider_capability_overrides,
};
// Preserve the existing OAuth request paths while RPC dispatch infers request types.
// RPC 分发推导请求类型时仍保留现有 OAuth 请求路径。
#[expect(
    unused_imports,
    reason = "Preserve the original OAuth request type paths."
)]
pub(super) use oauth::{PublicAiOAuthCommitRequest, PublicAiOAuthStartRequest};
pub(super) use oauth::{
    service_cancel_public_ai_oauth, service_commit_public_ai_oauth_connection,
    service_public_ai_oauth_status, service_start_public_ai_oauth,
};

pub(super) use provider_synchronization::{
    service_get_provider_manifest_status, service_preview_provider_manifest,
    service_rollback_provider_manifest, service_sync_provider_manifest,
    service_sync_provider_manifests,
};
pub(super) use provider_tasks::{service_cancel_provider_task, service_list_provider_tasks};
// Preserve provider-tool request paths when RPC dispatch infers their contracts.
// RPC 分发推导契约时保留供应商工具请求路径。
#[expect(
    unused_imports,
    reason = "Preserve the original provider-tool request type paths."
)]
pub(super) use provider_tools::{
    PublicAiMediaUnderstandingSelectionSaveRequest,
    PublicAiMultimodalUnderstandingCandidateSaveRequest,
    PublicAiProviderExtensionTargetSetSaveRequest,
    PublicAiProviderToolCapabilityEnabledSaveRequest, PublicAiProviderToolSelectionSaveRequest,
    PublicAiProviderToolTestRequest,
};
pub(super) use provider_tools::{
    service_save_public_ai_media_understanding_selection,
    service_save_public_ai_multimodal_understanding_candidate,
    service_save_public_ai_provider_extension_target_set,
    service_save_public_ai_provider_tool_capability_enabled,
    service_save_public_ai_provider_tool_selection, service_test_public_ai_provider_tool,
};
pub(crate) use publication::publish_public_ai_workbench;
use publication::{
    combine_results, compensated_operation_error,
    mutate_public_ai_configuration_and_publish_in_transaction,
    publish_public_ai_workbench_with_session_reconciliation_and_commit,
    restore_public_ai_configuration_activation_in_transaction,
    restore_public_ai_runtime_activation,
};
// Preserve session-tool RPC type paths, including nested conflict and snapshot contracts.
// 保留会话工具 RPC 类型路径，包括嵌套冲突与快照契约。
#[expect(
    unused_imports,
    reason = "Preserve the original session-tool RPC type paths."
)]
pub(super) use session_tools::{
    PublicAiSessionProviderToolConflict, PublicAiSessionProviderToolSelectionsSaveRequest,
    PublicAiSessionProviderToolSettingsSnapshot,
};
pub(crate) use session_tools::{
    provider_tool_capability_key, provider_tool_selection_config_from_parts_v1,
    public_ai_session_media_understanding_bridge_kinds,
    session_media_understanding_selections_to_runtime, validate_provider_tool_session_settings,
};
pub(super) use session_tools::{
    service_public_ai_session_provider_tool_selections,
    service_rebuild_public_ai_session_provider_tool_selections,
    service_save_public_ai_session_provider_tool_selections,
};
use std::sync::Arc;
use vulcan_provider_adapters::{PersistedAiConfiguration, PublicAiRuntime};

use crate::SharedServiceBridgeState;

/// Returns the current public AI settings snapshot.
/// 返回当前公共 AI 设置快照。
///
/// # Parameters
/// 参数
/// `state` owns the initialized shared public AI runtime.
/// `state` 持有已初始化的共享公共 AI 运行时。
///
/// # Returns
/// 返回值
/// The redacted current settings view, or an explicit runtime or projection failure.
/// 返回当前脱敏设置视图，或明确的运行时、投影失败。
pub(super) async fn service_public_ai_settings_snapshot(
    state: SharedServiceBridgeState,
) -> Result<PublicAiSettingsSnapshot, String> {
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
}

/// Clones the shared runtime and authoritative public AI configuration.
/// 克隆共享运行时与权威公共 AI 配置。
///
/// # Parameters
/// 参数
/// `state` contains the initialized agent runtime behind the service-state lock.
/// `state` 在服务状态锁内包含已初始化的智能体运行时。
///
/// # Returns
/// 返回值
/// The shared runtime and its current configuration snapshot, or a lock/startup/configuration error.
/// 返回共享运行时及其当前配置快照，或锁、启动、配置读取错误。
pub(crate) fn public_ai_runtime_and_configuration(
    state: &SharedServiceBridgeState,
) -> Result<(Arc<PublicAiRuntime>, PersistedAiConfiguration), String> {
    let guard = state
        .lock()
        .map_err(|error| format!("Failed to lock service state: {error}"))?;
    let agent_runtime = guard
        .agent_runtime
        .as_ref()
        .ok_or_else(|| String::from("Service is still starting; AI runtime is not ready."))?;
    let runtime = Arc::clone(&agent_runtime.public_ai_runtime);
    let configuration = runtime.configuration_snapshot()?;
    Ok((runtime, configuration))
}

/// Returns every currently available public AI provider/model selector.
/// 返回当前全部可用的公共 AI 供应商/模型选择器。
///
/// # Parameters
/// 参数
///
/// `state` owns the exact process-local catalogs and persisted connection graph.
/// `state` 持有精确的进程内目录与持久化连接图。
///
/// # Returns
/// 返回值
///
/// A stable deduplicated selector set derived without reading legacy provider catalogs.
/// 返回不读取旧供应商目录而派生的稳定去重选择器集合。
pub(crate) fn public_ai_available_model_selectors(
    state: &SharedServiceBridgeState,
) -> Result<std::collections::BTreeSet<String>, String> {
    let (runtime, configuration) = public_ai_runtime_and_configuration(state)?;
    runtime.available_model_selectors(&configuration)
}

/// Rejects empty browser string fields before mutation.
/// 在变更前拒绝浏览器传入的空字符串字段。
///
/// # Parameters
/// 参数
/// `field` names the rejected input and `value` is checked after trimming whitespace.
/// `field` 标识被拒绝的输入，`value` 去除首尾空白后参与校验。
///
/// # Returns
/// 返回值
/// Success for a nonempty value; otherwise an explicit field validation error.
/// 非空值返回成功；否则返回明确的字段校验错误。
fn validate_non_empty(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("Field `{field}` must not be empty."));
    }
    Ok(())
}
