use super::*;

#[cfg(test)]
pub(super) fn normalize_openai_compatible_chunk(
    provider_id: &str,
    provider_label: &str,
    data: &str,
) -> Result<
    crate::agent_runtime::provider_protocol_openai_compatible::CompatibleChunk,
    VulcanCoreError,
> {
    let usage_policy = resolve_openai_compatible_streaming_usage_policy(provider_id);
    normalize_openai_compatible_chunk_with_policy(provider_id, provider_label, data, &usage_policy)
}

/// Normalizes one OpenAI-compatible SSE `data:` payload into the shared compatibility chunk shape using one resolved provider usage policy.
/// 使用一份已解析的供应商 usage 策略，将单条 OpenAI 兼容 SSE `data:` 载荷规范化为共享兼容块形状。
pub(super) fn normalize_openai_compatible_chunk_with_policy(
    provider_id: &str,
    provider_label: &str,
    data: &str,
    usage_policy: &OpenAiCompatibleStreamingUsageConfig,
) -> Result<
    crate::agent_runtime::provider_protocol_openai_compatible::CompatibleChunk,
    VulcanCoreError,
> {
    let chunk = serde_json::from_str::<OpenAiCompatibleStreamingChunk>(data).map_err(|error| {
        VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("{provider_label} 流式数据解析失败：{error}"),
        }
    })?;

    Ok(normalize_first_choice_chunk(
        chunk.id.clone(),
        extract_openai_compatible_terminal_usage(&chunk, usage_policy),
        &chunk.choices,
        |choice| {
            let mut provider_visible_assistant_fields = choice.delta.additional_fields.clone();
            merge_openai_compatible_reasoning_replay_fields(
                &mut provider_visible_assistant_fields,
                &choice.delta,
            );
            CompatibleChoiceData {
                finish_reason: if choice.finish_reason
                    == Some(OpenAiCompatibleFinishReason::ToolCalls)
                {
                    CompatibleFinishReason::ToolCalls
                } else {
                    CompatibleFinishReason::Other
                },
                text: choice.delta.content.clone(),
                reasoning: extract_openai_compatible_reasoning_delta(&choice.delta),
                provider_visible_assistant_fields,
                tool_calls: tool_call_chunks(&choice.delta.tool_calls),
            }
        },
    ))
}

/// Resolves one provider-scoped OpenAI-compatible streaming usage policy from the provider catalog and falls back to conservative terminal-only defaults.
/// 从供应商目录解析单个 Provider 作用域的 OpenAI 兼容流式 usage 策略，并在缺失时回退到保守的终态默认值。
pub(super) fn resolve_openai_compatible_streaming_usage_policy(
    provider_id: &str,
) -> OpenAiCompatibleStreamingUsageConfig {
    supported_provider_by_id(provider_id)
        .and_then(|definition| definition.usage_capture.openai_compatible_streaming.clone())
        .unwrap_or_default()
}

/// Returns the effective ordered terminal usage sources for one OpenAI-compatible provider policy.
/// 返回单个 OpenAI 兼容 Provider 策略生效中的终态 usage 来源顺序。
pub(super) fn effective_openai_compatible_streaming_usage_sources(
    usage_policy: &OpenAiCompatibleStreamingUsageConfig,
) -> &[OpenAiCompatibleStreamingUsageSource] {
    if usage_policy.response_sources.is_empty() {
        &DEFAULT_OPENAI_COMPATIBLE_STREAMING_USAGE_SOURCES
    } else {
        usage_policy.response_sources.as_slice()
    }
}

/// Extracts one terminal OpenAI-compatible usage snapshot from the current streamed chunk according to the resolved provider policy.
/// 按照已解析的供应商策略，从当前流式块中提取一份终态 OpenAI 兼容用量快照。
pub(super) fn extract_openai_compatible_terminal_usage(
    chunk: &OpenAiCompatibleStreamingChunk,
    usage_policy: &OpenAiCompatibleStreamingUsageConfig,
) -> Option<CoreProviderUsageSnapshot> {
    for source in effective_openai_compatible_streaming_usage_sources(usage_policy) {
        let usage = match source {
            OpenAiCompatibleStreamingUsageSource::TopLevelEmptyChoicesChunk => {
                if chunk.choices.is_empty() {
                    chunk
                        .usage
                        .clone()
                        .and_then(openai_compatible_usage_to_snapshot)
                } else {
                    None
                }
            }
            OpenAiCompatibleStreamingUsageSource::TopLevelTerminalChunk => {
                if chunk
                    .choices
                    .iter()
                    .any(|choice| choice.finish_reason.is_some())
                {
                    chunk
                        .usage
                        .clone()
                        .and_then(openai_compatible_usage_to_snapshot)
                } else {
                    None
                }
            }
            OpenAiCompatibleStreamingUsageSource::TerminalChoice => chunk
                .choices
                .iter()
                .find(|choice| choice.finish_reason.is_some())
                .and_then(|choice| choice.usage.clone())
                .and_then(openai_compatible_usage_to_snapshot),
        };

        if usage
            .as_ref()
            .is_some_and(CoreProviderUsageSnapshot::has_any_tokens)
        {
            return usage;
        }
    }

    None
}

pub(super) fn openai_compatible_usage_to_snapshot(
    usage: OpenAiCompatibleUsage,
) -> Option<CoreProviderUsageSnapshot> {
    let snapshot = CoreProviderUsageSnapshot {
        input_tokens: usage.prompt_tokens,
        output_tokens: usage.completion_tokens,
        reasoning_tokens: usage
            .completion_tokens_details
            .and_then(|details| details.reasoning_tokens),
        total_tokens: usage.total_tokens,
        cached_input_tokens: usage
            .prompt_tokens_details
            .and_then(|details| details.cached_tokens),
        cache_creation_input_tokens: None,
    };
    snapshot.has_any_tokens().then_some(snapshot)
}
