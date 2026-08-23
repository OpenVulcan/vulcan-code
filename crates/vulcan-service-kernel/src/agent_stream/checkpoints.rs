//! Provider checkpoint and runtime injection audit helpers.
//! Provider 检查点与运行时注入审计辅助函数。

use super::*;
use std::path::Path;

/// Records one provider request checkpoint into the segmented session runtime audit log.
/// 将单个 Provider 请求检查点记录到分段会话运行时审计日志。
pub(crate) fn record_webui_provider_request_checkpoint(
    session_directory: &Path,
    session_id: SessionId,
    provider_family: &str,
    snapshot: &ProviderRequestSnapshot,
) -> Result<(), String> {
    // event retains the exact API family resolved before the provider request was lowered.
    // event 保留 Provider 请求转换前解析出的精确 API 协议族。
    let event = provider_request_checkpoint_event(session_id, provider_family, snapshot);
    vulcan_workspace_store::workspace_store::record_session_runtime_audit_event(
        session_directory,
        session_id,
        event,
        &current_unix_millis_text(),
    )
    .map(|_| ())
}

/// Builds one provider request checkpoint from exact stream and snapshot facts.
/// 使用精确的运行时流与请求快照事实构建单个 Provider 请求检查点。
///
/// # Parameters
/// 参数
/// `session_id` identifies the session that owns the physical request.
/// `session_id` 标识拥有该真实请求的会话。
/// `provider_family` is the canonical API family resolved during execution preparation.
/// `provider_family` 是执行准备期间解析出的规范 API 协议族。
/// `snapshot` is the provider-neutral request captured before wire lowering.
/// `snapshot` 是线协议转换前捕获的 Provider 中立请求。
///
/// # Returns
/// 返回值
/// Returns the runtime-only audit event ready for segmented-log persistence.
/// 返回可写入分段日志的仅运行时审计事件。
fn provider_request_checkpoint_event(
    session_id: SessionId,
    provider_family: &str,
    snapshot: &ProviderRequestSnapshot,
) -> vulcan_context::RuntimeLogEvent {
    // request_fingerprint is computed once so the checkpoint identity and audit field cannot diverge.
    // request_fingerprint 仅计算一次，确保检查点身份与审计字段不会产生分歧。
    let request_fingerprint = stable_provider_request_snapshot_hash(snapshot);
    vulcan_context::RuntimeLogEvent::ProviderRequestCheckpoint(
        vulcan_context::RuntimeProviderRequestCheckpointEvent {
            turn_id: None,
            checkpoint_id: format!("provider-request-{}-{}", session_id, request_fingerprint),
            provider_id: snapshot.provider_id.clone(),
            model_id: snapshot.model_id.clone(),
            provider_family: provider_family.to_string(),
            capability_fingerprint: None,
            prompt_fingerprint: Some(stable_provider_prompt_fingerprint(snapshot)),
            injection_fingerprint: None,
            request_fingerprint: Some(request_fingerprint),
            cache_policy_fingerprint: Some(stable_provider_cache_policy_fingerprint(snapshot)),
            cache_policy_diagnostic: provider_cache_policy_diagnostic(provider_family, snapshot),
            converter_version: 1,
        },
    )
}

/// Records one provider lowering repair into the segmented session runtime audit log.
/// 将单个 Provider 转换修复记录到分段会话运行时审计日志。
pub(crate) fn record_webui_provider_lowering_repair(
    session_directory: &Path,
    session_id: SessionId,
    event: vulcan_context::RuntimeProviderLoweringRepairEvent,
) -> Result<(), String> {
    vulcan_workspace_store::workspace_store::record_session_runtime_audit_event(
        session_directory,
        session_id,
        vulcan_context::RuntimeLogEvent::ProviderLoweringRepair(event),
        &current_unix_millis_text(),
    )
    .map(|_| ())
}

/// Records one context-injection merge event into the segmented session runtime audit log.
/// 将单个上下文注入合并事件记录到分段会话运行时审计日志。
pub(crate) fn record_webui_runtime_injection_merged(
    session_directory: &Path,
    session_id: SessionId,
    event: vulcan_context::RuntimeInjectionMergedEvent,
    timestamp: &str,
) -> Result<(), String> {
    vulcan_workspace_store::workspace_store::record_session_runtime_injection_merged(
        session_directory,
        session_id,
        event,
        timestamp,
    )
    .map(|_| ())
}

/// Records one provider response checkpoint into the segmented session runtime audit log.
/// 将单个 Provider 响应检查点记录到分段会话运行时审计日志。
pub(crate) fn record_webui_provider_response_checkpoint(
    session_directory: &Path,
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
            provider_family: preparation
                .model_blueprint
                .protocol_profile_key()
                .to_string(),
            finish_reason: Some(finish_reason.to_string()),
            native_state,
            usage: report.last_successful_provider_usage.clone(),
        },
    );
    vulcan_workspace_store::workspace_store::record_session_runtime_audit_event(
        session_directory,
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
///
/// # Parameters
/// 参数
/// `provider_family` is the exact manifest-resolved protocol profile identifier.
/// `provider_family` 是清单解析得到的精确协议配置档标识。
/// `snapshot` is the provider-neutral request inspected for cacheable boundaries.
/// `snapshot` 是用于检查可缓存边界的 Provider 中立请求。
///
/// # Returns
/// 返回值
/// A stable diagnostic only when the selected protocol cannot encode cache hints.
/// 仅当所选协议无法编码缓存提示时返回稳定诊断。
pub(crate) fn provider_cache_policy_diagnostic(
    provider_family: &str,
    snapshot: &ProviderRequestSnapshot,
) -> Option<String> {
    let has_cacheable_boundary = !snapshot.system_prompt.trim().is_empty()
        || !snapshot.prompt.trim().is_empty()
        || !snapshot.tool_schemas.is_empty()
        || !snapshot.history.is_empty();
    if has_cacheable_boundary
        && provider_family != vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID
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
        .protocol_profile_key()
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

#[cfg(test)]
mod tests {
    //! Provider checkpoint audit-event contract tests.
    //! Provider 检查点审计事件契约测试。

    use super::*;

    /// Keeps the prepared API family distinct from the provider identifier in persisted audit input.
    /// 在持久化审计输入中保持执行准备 API 协议族与 Provider 标识互相独立。
    #[test]
    fn provider_request_checkpoint_preserves_explicit_api_family() {
        // snapshot models a routed provider whose identity intentionally differs from its transport family.
        // snapshot 模拟 Provider 身份与其传输协议族刻意不同的路由场景。
        let snapshot = ProviderRequestSnapshot {
            provider_id: String::from("openrouter"),
            model_id: String::from("google/gemini-test"),
            system_prompt: String::from("system"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system"),
            ),
            prompt: String::from("prompt"),
            prompt_in_history: false,
            history: Vec::new(),
            temperature: None,
            max_turns: 2,
            tool_schemas: Vec::new(),
            additional_params: None,
            request_headers: None,
            continuation: vulcan_core::types::ProviderContinuation::default(),
        };
        assert_eq!(
            provider_cache_policy_diagnostic(
                vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID,
                &snapshot,
            ),
            None,
            "cache support must follow the protocol profile instead of the provider id",
        );
        assert_eq!(
            provider_cache_policy_diagnostic(
                vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,
                &snapshot,
            ),
            Some(String::from("cache_hints_unsupported")),
            "an OpenAI protocol must not inherit Anthropic cache semantics from a provider name",
        );
        // event is the exact runtime audit value handed to segmented-log persistence.
        // event 是交给分段日志持久化的精确运行时审计值。
        let event = provider_request_checkpoint_event(
            SessionId::nil(),
            "openai-chat-compatible",
            &snapshot,
        );

        assert!(matches!(
            event,
            vulcan_context::RuntimeLogEvent::ProviderRequestCheckpoint(event)
                if event.provider_family == "openai-chat-compatible"
                    && event.provider_id == "openrouter"
        ));
    }
}
