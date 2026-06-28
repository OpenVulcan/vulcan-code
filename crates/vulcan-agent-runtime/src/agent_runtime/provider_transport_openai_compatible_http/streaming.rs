use super::*;

pub(crate) async fn execute_openai_compatible_http_streaming_turn<S>(
    provider_id: &str,
    provider_label: &str,
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    request_headers: HeaderMap,
    request: OpenAiCompatibleCoreTurnRequest,
    provider_tool_definitions: &[ProviderToolDefinition],
    live_sender: &ProviderLiveDeltaSender,
    sink: &mut S,
) -> Result<CoreProviderTurnResponse, VulcanCoreError>
where
    S: VulcanProviderDeltaSink,
{
    execute_openai_compatible_http_streaming_turn_with_options(
        provider_id,
        api_key,
        model_id,
        request_headers,
        request,
        provider_tool_definitions,
        live_sender,
        sink,
        OpenAiCompatibleHttpStreamingOptions {
            provider_label: provider_label.to_string(),
            endpoint: build_openai_compatible_chat_completions_endpoint(base_url),
            include_model_field: true,
            authorization_strategy: OpenAiCompatibleAuthorizationStrategy::Bearer,
        },
    )
    .await
}

/// Executes one direct OpenAI-compatible wire-protocol turn using caller-supplied transport options such as auth strategy and final endpoint.
/// 使用调用方提供的鉴权策略与最终端点等传输配置执行一次直连 OpenAI 兼容线协议回合。
pub(crate) async fn execute_openai_compatible_http_streaming_turn_with_options<S>(
    provider_id: &str,
    api_key: &str,
    model_id: &str,
    request_headers: HeaderMap,
    request: OpenAiCompatibleCoreTurnRequest,
    provider_tool_definitions: &[ProviderToolDefinition],
    live_sender: &ProviderLiveDeltaSender,
    sink: &mut S,
    options: OpenAiCompatibleHttpStreamingOptions,
) -> Result<CoreProviderTurnResponse, VulcanCoreError>
where
    S: VulcanProviderDeltaSink,
{
    let OpenAiCompatibleHttpStreamingOptions {
        provider_label,
        endpoint,
        include_model_field,
        authorization_strategy,
    } = options;
    let mut wire_headers = request_headers.clone();
    match authorization_strategy {
        OpenAiCompatibleAuthorizationStrategy::Bearer => ensure_bearer_authorization_header(
            provider_id,
            provider_label.as_str(),
            api_key,
            &mut wire_headers,
        )?,
        OpenAiCompatibleAuthorizationStrategy::StaticApiKeyHeader {
            header_name,
            header_label,
        } => ensure_string_header(
            provider_id,
            provider_label.as_str(),
            &mut wire_headers,
            header_name,
            api_key,
            header_label,
        )?,
    }
    ensure_json_content_type(&mut wire_headers);
    let client = build_openai_compatible_http_client(
        provider_id,
        provider_label.as_str(),
        api_key,
        request_headers,
        authorization_strategy,
    )?;
    let payload = build_openai_compatible_request_payload(
        model_id,
        request,
        provider_tool_definitions,
        include_model_field,
    )?;
    let dry_run = provider_request_dry_run_active(api_key);
    let wire_request = emit_provider_wire_request(
        live_sender,
        provider_id,
        model_id,
        "POST",
        endpoint.as_str(),
        &wire_headers,
        &payload,
        dry_run,
    );
    if let Some(response) = provider_request_dry_run_response(&wire_request, dry_run) {
        return Ok(response);
    }
    let mut response = client
        .post(endpoint)
        .json(&payload)
        .send()
        .await
        .map_err(|error| VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("{provider_label} 请求发送失败：{error}"),
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.text().await.unwrap_or_default();
        let message = augment_openai_compatible_error_with_tool_context(
            format_standard_error_response_with_headers(status, &headers, &body),
            &body,
            provider_tool_definitions,
        );
        return Err(VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("{provider_label} request failed: {message}"),
        });
    }

    let byte_stream = Box::pin(async_stream::stream! {
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => yield Ok::<Vec<u8>, std::io::Error>(chunk.to_vec()),
                Ok(None) => break,
                Err(error) => {
                    yield Err(std::io::Error::other(error.to_string()));
                    break;
                }
            }
        }
    });

    let events = iter_sse_messages(byte_stream);
    futures_util::pin_mut!(events);

    let mut response_id = None;
    let mut assistant_text = String::new();
    let mut assistant_reasoning = String::new();
    let mut pending_tool_calls: HashMap<usize, CompatibleAccumulatedToolCall> = HashMap::new();
    let mut finalized_tool_calls = Vec::new();
    let mut final_usage = None::<CoreProviderUsageSnapshot>;
    let mut provider_visible_assistant_fields = JsonMap::new();
    let mut saw_reasoning_field = false;
    let mut raw_reasoning_text = String::new();
    let usage_policy = resolve_openai_compatible_streaming_usage_policy(provider_id);

    while let Some(event_result) = events.next().await {
        let event = event_result.map_err(|error| VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("{provider_label} SSE 解码失败：{error}"),
        })?;

        if event.data.trim().is_empty() || event.data.trim() == "[DONE]" {
            continue;
        }

        let chunk = normalize_openai_compatible_chunk_with_policy(
            provider_id,
            provider_label.as_str(),
            &event.data,
            &usage_policy,
        )?;
        response_id = chunk.response_id.or(response_id);
        if let Some(usage) = chunk
            .usage
            .filter(CoreProviderUsageSnapshot::has_any_tokens)
        {
            final_usage = Some(usage);
        }
        let Some(choice) = chunk.choice else {
            continue;
        };

        for (field_name, field_value) in choice.provider_visible_assistant_fields {
            merge_openai_compatible_assistant_field(
                &mut provider_visible_assistant_fields,
                field_name,
                field_value,
            );
        }

        process_compatible_tool_call_chunks(
            &mut pending_tool_calls,
            &mut finalized_tool_calls,
            choice.tool_calls,
        );

        if let Some(reasoning) = choice.reasoning {
            saw_reasoning_field = true;
            raw_reasoning_text.push_str(&reasoning);
            if !reasoning.is_empty() {
                assistant_reasoning.push_str(&reasoning);
                forward_openai_compatible_delta(
                    live_sender,
                    sink,
                    CoreProviderTurnDelta::Thinking { delta: reasoning },
                );
            }
        }

        if let Some(text) = choice.text
            && !text.is_empty()
        {
            assistant_text.push_str(&text);
            forward_openai_compatible_delta(
                live_sender,
                sink,
                CoreProviderTurnDelta::Output { delta: text },
            );
        }

        if choice.finish_reason == CompatibleFinishReason::ToolCalls {
            flush_pending_compatible_tool_calls(&mut pending_tool_calls, &mut finalized_tool_calls);
        }
    }

    flush_pending_compatible_tool_calls(&mut pending_tool_calls, &mut finalized_tool_calls);

    let provider_message_state = build_openai_compatible_provider_message_state(
        &assistant_text,
        &finalized_tool_calls,
        saw_reasoning_field.then_some(raw_reasoning_text),
        provider_visible_assistant_fields,
        provider_id,
        model_id,
    );
    let mut assistant_content = Vec::new();
    if !assistant_text.is_empty() {
        assistant_content.push(ProviderAssistantContent::Text(assistant_text));
    }
    if !assistant_reasoning.is_empty() {
        assistant_content.push(ProviderAssistantContent::Reasoning(vec![
            ProviderReasoningPart::Text(assistant_reasoning),
        ]));
    }
    assistant_content.extend(
        finalized_tool_calls
            .into_iter()
            .map(ProviderAssistantContent::ToolCall),
    );

    let mut response = build_openai_compatible_turn_response(
        provider_id,
        model_id,
        response_id,
        &assistant_content,
        final_usage,
    );
    response.provider_message_state = provider_message_state;
    Ok(response)
}

/// Builds one direct OpenAI-compatible HTTP client with merged authorization and caller-supplied request headers.
/// 构建一个直连 OpenAI 兼容 HTTP 客户端，并合并鉴权头与调用方提供的请求头。
pub(super) fn build_openai_compatible_http_client(
    provider_id: &str,
    provider_label: &str,
    api_key: &str,
    mut request_headers: HeaderMap,
    authorization_strategy: OpenAiCompatibleAuthorizationStrategy,
) -> Result<reqwest::Client, VulcanCoreError> {
    match authorization_strategy {
        OpenAiCompatibleAuthorizationStrategy::Bearer => ensure_bearer_authorization_header(
            provider_id,
            provider_label,
            api_key,
            &mut request_headers,
        )?,
        OpenAiCompatibleAuthorizationStrategy::StaticApiKeyHeader {
            header_name,
            header_label,
        } => ensure_string_header(
            provider_id,
            provider_label,
            &mut request_headers,
            header_name,
            api_key,
            header_label,
        )?,
    }
    ensure_json_content_type(&mut request_headers);
    build_json_http_client(provider_id, provider_label, request_headers)
}

/// Builds the final OpenAI-compatible `/chat/completions` endpoint from one optional base URL override.
/// 根据可选基础地址覆盖值构建最终 OpenAI 兼容 `/chat/completions` 端点。
pub(super) fn build_openai_compatible_chat_completions_endpoint(base_url: Option<&str>) -> String {
    if let Some(base_url) = base_url
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().trim_end_matches('/'))
    {
        if base_url.ends_with("/chat/completions") {
            return base_url.to_string();
        }
    }

    build_rest_endpoint(base_url, "https://api.openai.com/v1", "chat/completions")
}

/// Forwards one normalized provider delta into both the controller sink and the host live sender.
/// 将一条规范 Provider 增量同时转发给控制器汇聚器与宿主实时发送器。
pub(super) fn forward_openai_compatible_delta<S>(
    live_sender: &ProviderLiveDeltaSender,
    sink: &mut S,
    delta: CoreProviderTurnDelta,
) where
    S: VulcanProviderDeltaSink,
{
    sink.push_delta(delta.clone());
    let _ = live_sender.send(Ok(core_provider_turn_delta_to_runtime_delta(delta)));
}
