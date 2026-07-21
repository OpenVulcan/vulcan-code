//! Shared streaming adapter support helpers reused by provider-specific transport adapters.
//! 供各 Provider 专用传输适配器复用的共享流式适配器支持辅助模块。

use crate::agent_runtime::provider_protocol_bridge::{
    inject_previous_response_id_into_additional_params, scope_continuation_to_active_provider,
};
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_openai_responses_http::FORCE_FULL_OPENAI_RESPONSES_REPLAY_PARAM;
use crate::agent_runtime::provider_transport_turn_request::{
    OpenAiCompatibleCoreTurnRequest, build_host_controlled_turn_request,
};
use crate::provider_config::ModelInterleavedCapability;
use reqwest::header::HeaderMap;
use std::fmt::Display;
use vulcan_core::types::CoreError as VulcanCoreError;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationMessage as CoreConversationMessage,
    ConversationSnapshot as CoreConversationSnapshot,
    ProviderContinuation as CoreProviderContinuation,
};

/// Formats one provider label prefix so mixed English/Chinese runtime messages keep natural spacing automatically.
/// 格式化单个 Provider 标签前缀，使中英混排运行时文案能够自动保持自然空格。
fn format_provider_label_prefix(provider_label: &str) -> String {
    let needs_space = provider_label
        .chars()
        .last()
        .map(|character| character.is_ascii_alphanumeric())
        .unwrap_or(false);
    if needs_space {
        format!("{provider_label} ")
    } else {
        provider_label.to_string()
    }
}

/// Builds the standard invalid-state error returned when one streaming-only adapter is asked to run the non-streaming execution path.
/// 当仅支持流式的适配器被要求执行非流式路径时，构建其返回的标准无效状态错误。
pub(crate) fn unsupported_streaming_execute<T>(provider_label: &str) -> Result<T, VulcanCoreError> {
    let provider_label_prefix = format_provider_label_prefix(provider_label);
    Err(VulcanCoreError::InvalidState {
        message: format!(
            "{provider_label_prefix}流式适配器不支持非流式 execute_request 调用，请改用 execute_request_streaming。"
        ),
    })
}

/// Maps one low-level provider client build result into the normalized `vulcan-core` provider failure shape.
/// 将单个低层 Provider 客户端构建结果映射成规范化的 `vulcan-core` Provider 失败形态。
pub(crate) fn map_provider_client_build_result<T, E>(
    provider_id: &str,
    provider_label: &str,
    result: Result<T, E>,
) -> Result<T, VulcanCoreError>
where
    E: Display,
{
    let provider_label_prefix = format_provider_label_prefix(provider_label);
    result.map_err(|error| VulcanCoreError::ProviderFailure {
        provider_id: provider_id.to_string(),
        message: format!("创建 {provider_label_prefix}客户端失败：{error}"),
    })
}

/// Builds one standard host-controlled single-turn request from the canonical controller snapshot and forwards the structured snapshot immediately.
/// 根据规范控制器快照构建一份标准宿主控制单回合请求，并立即转发结构化快照。
pub(crate) fn build_standard_streaming_request(
    provider_id: &str,
    model_id: &str,
    request_label: &str,
    snapshot: &CoreConversationSnapshot,
    config: &CoreControllerConfig,
    provider_tool_definitions: &[ProviderToolDefinition],
    request_headers: &HeaderMap,
    live_sender: &ProviderLiveDeltaSender,
    interleaved_reasoning: Option<ModelInterleavedCapability>,
) -> Result<OpenAiCompatibleCoreTurnRequest, VulcanCoreError> {
    build_host_controlled_turn_request(
        provider_id,
        model_id,
        request_label,
        snapshot,
        config,
        provider_tool_definitions,
        request_headers,
        live_sender,
        config.additional_params.clone(),
        interleaved_reasoning,
    )
}

/// Builds one OpenAI Responses host-controlled single-turn request while injecting `previous_response_id` when the current continuation snapshot already carries one.
/// 构建一份 OpenAI Responses 宿主控制单回合请求，并在当前 continuation 快照已经携带上游响应标识时注入 `previous_response_id`。
pub(crate) fn build_openai_responses_streaming_request(
    provider_id: &str,
    model_id: &str,
    snapshot: &CoreConversationSnapshot,
    config: &CoreControllerConfig,
    provider_tool_definitions: &[ProviderToolDefinition],
    request_headers: &HeaderMap,
    live_sender: &ProviderLiveDeltaSender,
) -> Result<OpenAiCompatibleCoreTurnRequest, VulcanCoreError> {
    let scoped_continuation =
        scope_continuation_to_active_provider(&snapshot.continuation, provider_id, model_id);
    // append_vmm_continuation_disabled rolls back only append-only VMM cursor reuse.
    // append_vmm_continuation_disabled 仅回滚追加式 VMM 游标复用。
    let append_vmm_continuation_disabled = append_vmm_continuation_is_disabled(
        config,
        vulcan_vmm::VmmRuntimeFeaturePolicy::from_env().openai_append_continuation_enabled,
    );
    // uncovered_message_start is the first exact local message not represented by the provider cursor.
    // uncovered_message_start 是 Provider 游标尚未表示的第一条精确本地消息位置。
    let uncovered_message_start =
        continuation_uncovered_message_start(&snapshot.messages, &scoped_continuation);
    // continuation_boundary_is_proven rejects legacy or divergent cursors without an exact local boundary.
    // continuation_boundary_is_proven 拒绝没有精确本地边界的旧版或分歧游标。
    let continuation_boundary_is_proven =
        scoped_continuation.last_provider_message_id.is_none() || uncovered_message_start.is_some();
    // uncovered_message_ids are the complete append-only local suffix, including agent messages and batched tool results.
    // uncovered_message_ids 是完整的追加式本地后缀，包括代理消息与批量工具结果。
    let uncovered_message_ids = uncovered_message_start
        .map(|start| {
            snapshot.messages[start..]
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let force_full_replay = append_vmm_continuation_disabled
        || !continuation_boundary_is_proven
        || !config.continuation_reuse_is_proven(&scoped_continuation, &uncovered_message_ids);
    let effective_additional_params = if force_full_replay {
        // stripped_params removes any unproven caller-supplied cursor before forcing a full replay.
        // stripped_params 在强制完整回放前移除任何未经证明的调用方游标。
        let mut params = inject_previous_response_id_into_additional_params(
            config.additional_params.clone(),
            &CoreProviderContinuation::default(),
        )
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
        params.insert(
            String::from(FORCE_FULL_OPENAI_RESPONSES_REPLAY_PARAM),
            serde_json::Value::Bool(true),
        );
        Some(serde_json::Value::Object(params))
    } else {
        inject_previous_response_id_into_additional_params(
            config.additional_params.clone(),
            &scoped_continuation,
        )
    };
    build_host_controlled_turn_request(
        provider_id,
        model_id,
        "OpenAI Responses",
        snapshot,
        config,
        provider_tool_definitions,
        request_headers,
        live_sender,
        effective_additional_params,
        None,
    )
}

/// Resolves the first local message not covered by one provider continuation cursor.
/// 解析一条 Provider continuation 游标尚未覆盖的第一条本地消息。
///
/// # Parameters
/// 参数
/// `messages` is the exact canonical provider-visible window before overlay rendering.
/// `messages` 是覆盖层渲染前的精确规范 Provider 可见消息窗口。
/// `continuation` is the provider/model-scoped cursor carrying its local boundary id.
/// `continuation` 是携带本地边界标识且已限定 Provider/模型作用域的游标。
///
/// # Returns
/// 返回值
/// Zero without a cursor, the first uncovered index for one unique non-tail boundary, or `None` when proof fails.
/// 无游标时返回零；唯一且非尾部边界时返回首个未覆盖索引；无法证明时返回 `None`。
fn continuation_uncovered_message_start(
    messages: &[CoreConversationMessage],
    continuation: &CoreProviderContinuation,
) -> Option<usize> {
    if continuation.last_provider_message_id.is_none() {
        return Some(0);
    }
    // boundary_id must be present because provider response ids alone cannot locate the local append suffix.
    // boundary_id 必须存在，因为仅凭 Provider 响应标识无法定位本地追加后缀。
    let boundary_id = continuation
        .last_local_message_id
        .as_deref()
        .filter(|message_id| !message_id.trim().is_empty())?;
    // matching_indices enforces one exact local boundary and rejects missing or duplicated identities.
    // matching_indices 强制唯一精确本地边界，并拒绝缺失或重复身份。
    let matching_indices = messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| (message.id == boundary_id).then_some(index))
        .collect::<Vec<_>>();
    let [boundary_index] = matching_indices.as_slice() else {
        return None;
    };
    let uncovered_start = boundary_index.saturating_add(1);
    (uncovered_start < messages.len()).then_some(uncovered_start)
}

/// Returns whether the rollout policy disables only append-only VMM Responses continuation.
/// 返回灰度策略是否仅禁用追加式 VMM Responses continuation。
///
/// # Parameters
/// 参数
/// `config` is the exact controller configuration for the provider request.
/// `config` 是 Provider 请求使用的精确控制器配置。
/// `feature_enabled` is the resolved internal append-continuation rollout decision.
/// `feature_enabled` 是解析后的内部追加续传灰度决策。
///
/// # Returns
/// 返回值
/// `true` only for disabled rollout plus an append-only overlay containing VMM anchors.
/// 仅在灰度关闭且覆盖层为包含 VMM 锚点的追加式变更时返回 `true`。
fn append_vmm_continuation_is_disabled(
    config: &CoreControllerConfig,
    feature_enabled: bool,
) -> bool {
    !feature_enabled
        && config.context_injection.as_ref().is_some_and(|injection| {
            injection.mutation_kind == vulcan_core::types::ProviderContextMutationKind::AppendOnly
                && !injection.anchored_prefixes.is_empty()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the rollout switch does not suppress unrelated continuation shapes.
    /// 验证灰度开关不会抑制无关的 continuation 形态。
    #[test]
    fn append_continuation_rollout_targets_only_append_only_vmm_anchors() {
        let mut config = CoreControllerConfig::default();
        assert!(!append_vmm_continuation_is_disabled(&config, false));
        config.context_injection = Some(vulcan_core::types::ProviderContextInjection {
            mutation_kind: vulcan_core::types::ProviderContextMutationKind::AppendOnly,
            anchored_prefixes: vec![vulcan_core::types::ProviderContextAnchor {
                anchor_id: String::from("vmm-memory-1"),
                target_message_id: String::from("user-1"),
                prefix: String::from("memory"),
            }],
            ..vulcan_core::types::ProviderContextInjection::default()
        });

        assert!(append_vmm_continuation_is_disabled(&config, false));
        assert!(!append_vmm_continuation_is_disabled(&config, true));
        config
            .context_injection
            .as_mut()
            .expect("context injection should exist")
            .mutation_kind = vulcan_core::types::ProviderContextMutationKind::HistoricalRewrite;
        assert!(!append_vmm_continuation_is_disabled(&config, false));
    }
}
