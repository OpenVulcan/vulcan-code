//! Provider checkpoint and runtime injection audit helpers.
//! Provider 检查点与运行时注入审计辅助函数。

use super::*;

/// Records one provider request checkpoint into the segmented session runtime audit log.
/// 将单个 Provider 请求检查点记录到分段会话运行时审计日志。
pub(crate) fn record_webui_provider_request_checkpoint(
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    snapshot: &ProviderRequestSnapshot,
) -> Result<(), String> {
    let event = vulcan_context::RuntimeLogEvent::ProviderRequestCheckpoint(
        vulcan_context::RuntimeProviderRequestCheckpointEvent {
            turn_id: None,
            checkpoint_id: format!(
                "provider-request-{}-{}",
                session_id,
                stable_provider_request_snapshot_hash(snapshot)
            ),
            provider_id: snapshot.provider_id.clone(),
            model_id: snapshot.model_id.clone(),
            provider_family: snapshot.provider_id.clone(),
            capability_fingerprint: None,
            prompt_fingerprint: Some(stable_provider_prompt_fingerprint(snapshot)),
            injection_fingerprint: None,
            request_fingerprint: Some(stable_provider_request_snapshot_hash(snapshot)),
            cache_policy_fingerprint: Some(stable_provider_cache_policy_fingerprint(snapshot)),
            cache_policy_diagnostic: provider_cache_policy_diagnostic(snapshot),
            converter_version: 1,
        },
    );
    vulcan_workspace_store::workspace_store::record_session_runtime_audit_event(
        &controller_runtime.data_root,
        session_id,
        event,
        &current_unix_millis_text(),
    )
    .map(|_| ())
}

/// Records one provider lowering repair into the segmented session runtime audit log.
/// 将单个 Provider 转换修复记录到分段会话运行时审计日志。
pub(crate) fn record_webui_provider_lowering_repair(
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    event: vulcan_context::RuntimeProviderLoweringRepairEvent,
) -> Result<(), String> {
    vulcan_workspace_store::workspace_store::record_session_runtime_audit_event(
        &controller_runtime.data_root,
        session_id,
        vulcan_context::RuntimeLogEvent::ProviderLoweringRepair(event),
        &current_unix_millis_text(),
    )
    .map(|_| ())
}

/// Records one context-injection merge event into the segmented session runtime audit log.
/// 将单个上下文注入合并事件记录到分段会话运行时审计日志。
pub(crate) fn record_webui_runtime_injection_merged(
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    event: vulcan_context::RuntimeInjectionMergedEvent,
    timestamp: &str,
) -> Result<(), String> {
    vulcan_workspace_store::workspace_store::record_session_runtime_injection_merged(
        &controller_runtime.data_root,
        session_id,
        event,
        timestamp,
    )
    .map(|_| ())
}

/// Records one context-injection cancellation event into the segmented session runtime audit log.
/// 将单个上下文注入取消事件记录到分段会话运行时审计日志。
pub(crate) fn record_webui_runtime_injection_cancelled(
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    event: vulcan_context::RuntimeInjectionCancelledEvent,
    timestamp: &str,
) -> Result<(), String> {
    vulcan_workspace_store::workspace_store::record_session_runtime_injection_cancelled(
        &controller_runtime.data_root,
        session_id,
        event,
        timestamp,
    )
    .map(|_| ())
}

/// Records one provider response checkpoint into the segmented session runtime audit log.
/// 将单个 Provider 响应检查点记录到分段会话运行时审计日志。
pub(crate) fn record_webui_provider_response_checkpoint(
    controller_runtime: &ControllerRuntimeHandle,
    report: &ExecutionRunReport,
) -> Result<(), String> {
    let Some(preparation) = report.preparation.as_ref() else {
        return Ok(());
    };
    let finish_reason = if report.error.is_some() {
        "failed"
    } else {
        "completed"
    };
    let native_state = latest_provider_native_state_envelope_from_conversation_state(
        report.conversation_state.as_ref(),
        &preparation.model_blueprint.provider_id,
        &preparation.model_blueprint.model_id,
    );
    let event = vulcan_context::RuntimeLogEvent::ProviderResponseCheckpoint(
        vulcan_context::RuntimeProviderResponseCheckpointEvent {
            turn_id: preparation
                .boundary_events
                .first()
                .and_then(|event| event.turn_id.clone()),
            checkpoint_id: format!(
                "provider-response-{}-{}",
                report.session_id,
                stable_provider_response_checkpoint_hash(
                    preparation,
                    report.provider_request_count,
                    report.last_successful_provider_usage.as_ref(),
                    report.error.as_ref(),
                )
            ),
            provider_id: preparation.model_blueprint.provider_id.clone(),
            model_id: preparation.model_blueprint.model_id.clone(),
            provider_family: preparation.model_blueprint.api_family_key().to_string(),
            finish_reason: Some(finish_reason.to_string()),
            native_state,
            usage: report.last_successful_provider_usage.clone(),
        },
    );
    vulcan_workspace_store::workspace_store::record_session_runtime_audit_event(
        &controller_runtime.data_root,
        report.session_id,
        event,
        &current_unix_millis_text(),
    )
    .map(|_| ())
}

/// Builds a stable fingerprint for one provider request snapshot without persisting raw prompt text.
/// 为单个 Provider 请求快照构建稳定指纹，同时不持久化原始提示词正文。
pub(crate) fn stable_provider_request_snapshot_hash(snapshot: &ProviderRequestSnapshot) -> String {
    let serialized = serde_json::to_string(snapshot).unwrap_or_else(|_| {
        format!(
            "{}:{}:{}:{}",
            snapshot.provider_id,
            snapshot.model_id,
            snapshot.history.len(),
            snapshot.tool_schemas.len()
        )
    });
    let mut hasher = DefaultHasher::new();
    serialized.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Builds a stable prompt fingerprint for provider request audit records.
/// 为 Provider 请求审计记录构建稳定提示词指纹。
pub(crate) fn stable_provider_prompt_fingerprint(snapshot: &ProviderRequestSnapshot) -> String {
    let mut hasher = DefaultHasher::new();
    snapshot.provider_id.hash(&mut hasher);
    snapshot.model_id.hash(&mut hasher);
    snapshot.system_prompt.hash(&mut hasher);
    snapshot.prompt.hash(&mut hasher);
    snapshot.history.len().hash(&mut hasher);
    snapshot.tool_schemas.len().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Builds a stable cache-policy fingerprint for one request without storing prompt bodies.
/// 为单个请求构建稳定缓存策略指纹，同时不存储提示词正文。
pub(crate) fn stable_provider_cache_policy_fingerprint(
    snapshot: &ProviderRequestSnapshot,
) -> String {
    let mut hasher = DefaultHasher::new();
    snapshot.provider_id.hash(&mut hasher);
    snapshot.model_id.hash(&mut hasher);
    (!snapshot.system_prompt.trim().is_empty()).hash(&mut hasher);
    snapshot.history.len().hash(&mut hasher);
    snapshot.tool_schemas.len().hash(&mut hasher);
    (!snapshot.prompt.trim().is_empty()).hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Returns the cache-policy diagnostic for providers that cannot represent cache hints.
/// 返回无法表达缓存提示的 Provider 对应缓存策略诊断。
pub(crate) fn provider_cache_policy_diagnostic(
    snapshot: &ProviderRequestSnapshot,
) -> Option<String> {
    let has_cacheable_boundary = !snapshot.system_prompt.trim().is_empty()
        || !snapshot.prompt.trim().is_empty()
        || !snapshot.tool_schemas.is_empty()
        || !snapshot.history.is_empty();
    if has_cacheable_boundary
        && !snapshot
            .provider_id
            .to_ascii_lowercase()
            .contains("anthropic")
    {
        Some(String::from("cache_hints_unsupported"))
    } else {
        None
    }
}

/// Builds a stable hash for a provider response checkpoint without storing response bodies.
/// 为 Provider 响应检查点构建稳定哈希，且不存储响应正文。
pub(crate) fn stable_provider_response_checkpoint_hash(
    preparation: &ExecutionPreparation,
    provider_request_count: usize,
    usage: Option<&ProviderUsageSnapshot>,
    error: Option<&String>,
) -> String {
    let mut hasher = DefaultHasher::new();
    preparation.model_blueprint.provider_id.hash(&mut hasher);
    preparation.model_blueprint.model_id.hash(&mut hasher);
    preparation
        .model_blueprint
        .api_family_key()
        .hash(&mut hasher);
    preparation
        .boundary_events
        .first()
        .and_then(|event| event.turn_id.as_deref())
        .hash(&mut hasher);
    provider_request_count.hash(&mut hasher);
    error.is_some().hash(&mut hasher);
    if let Some(error) = error {
        error.hash(&mut hasher);
    }
    if let Some(usage) = usage
        && let Ok(serialized_usage) = serde_json::to_string(usage)
    {
        serialized_usage.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

/// Extracts the latest scoped provider-native state envelope from one terminal conversation state.
/// 从单个终态会话状态中提取最新的带作用域 Provider 原生状态 envelope。
pub(crate) fn latest_provider_native_state_envelope_from_conversation_state(
    state: Option<&vulcan_agent_runtime::conversation_protocol::SessionConversationState>,
    provider_id: &str,
    model_id: &str,
) -> Option<vulcan_context::RuntimeProviderNativeStateEnvelope> {
    state?
        .messages
        .iter()
        .rev()
        .filter_map(|message| {
            match &message.payload {
            vulcan_agent_runtime::conversation_protocol::SessionConversationPayload::Assistant {
                provider_message_state: Some(provider_message_state),
                ..
            } => serde_json::from_value::<vulcan_context::RuntimeProviderNativeStateEnvelope>(
                provider_message_state.clone(),
            )
            .ok(),
            _ => None,
        }
        })
        .find(|envelope| envelope.provider_id == provider_id && envelope.model_id == model_id)
}

/// Builds a stable fingerprint for persistent injection blocks without storing their content.
/// 为长期注入块构建稳定指纹，同时不保存注入正文。
pub(crate) fn persistent_injection_fingerprint(state: &ContextInjectionState) -> Option<String> {
    let view = state.resolve_view();
    if view.persistent_blocks.is_empty() {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    view.persistent_blocks.hash(&mut hasher);
    Some(format!("{:016x}", hasher.finish()))
}

/// Returns stable non-content source identifiers for persistent injection items.
/// 返回长期注入内容项的稳定非正文来源标识。
pub(crate) fn persistent_injection_source_node_ids(state: &ContextInjectionState) -> Vec<String> {
    state
        .groups
        .values()
        .filter(|group| group.kind == InjectionKind::Persistent)
        .flat_map(|group| {
            group
                .items
                .values()
                .map(move |item| format!("{}:{}", group.tag, item.content_hash))
        })
        .collect()
}

/// Builds one runtime audit event for legacy VMM injection cleanup.
/// 为旧版 VMM 注入清理构建一条运行时审计事件。
pub(crate) fn legacy_vmm_injection_cancelled_event(
    previous_persistent_fingerprint: Option<String>,
    reason: &str,
) -> vulcan_context::RuntimeInjectionCancelledEvent {
    vulcan_context::RuntimeInjectionCancelledEvent {
        turn_id: None,
        cancellation_id: format!("{reason}-{}", current_unix_millis_text()),
        source_node_ids: vec![String::from("persistent:vmm_profile")],
        previous_persistent_fingerprint,
        previous_transient_fingerprint: None,
        reason: Some(reason.to_string()),
    }
}
