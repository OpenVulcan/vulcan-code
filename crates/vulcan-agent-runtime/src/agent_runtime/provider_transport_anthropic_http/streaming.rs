use super::*;

/// Executes one direct Anthropic streaming turn without going through any third-party completion-model abstraction.
/// 在不经过任何第三方 completion model 抽象的前提下执行一次直连 Anthropic 流式回合。
pub(crate) async fn execute_anthropic_http_streaming_turn<S>(
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
    let mut wire_headers = request_headers.clone();
    ensure_string_header(
        provider_id,
        provider_label,
        &mut wire_headers,
        "x-api-key",
        api_key,
        "API Key ",
    )?;
    ensure_static_header(
        &mut wire_headers,
        "anthropic-version",
        ANTHROPIC_API_VERSION_LATEST,
    );
    ensure_json_content_type(&mut wire_headers);
    let client =
        build_anthropic_http_client(provider_id, provider_label, api_key, request_headers)?;
    let endpoint = build_anthropic_messages_endpoint(base_url);
    let payload = build_anthropic_request_payload(model_id, request, provider_tool_definitions)?;
    emit_anthropic_lowering_repair_if_needed(live_sender, provider_id, model_id, &payload);
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
        let message = format_standard_error_response_with_headers(status, &headers, &body);
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
    let mut response_model = None;
    let mut usage_snapshot = None::<CoreProviderUsageSnapshot>;
    let mut assistant_text = String::new();
    let mut assistant_reasoning_blocks = Vec::new();
    let mut finalized_provider_content_blocks = Vec::new();
    let mut pending_tool_use = None::<PendingAnthropicToolUse>;
    let mut pending_server_tool_use = None::<PendingAnthropicServerToolUse>;
    let mut pending_thinking = None::<PendingAnthropicThinking>;
    let mut pending_text = None::<String>;
    let mut finalized_tool_calls = Vec::new();

    while let Some(event_result) = events.next().await {
        let event = event_result.map_err(|error| VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("{provider_label} SSE 解码失败：{error}"),
        })?;

        if event.data.trim().is_empty() {
            continue;
        }

        let event =
            serde_json::from_str::<AnthropicStreamingEvent>(&event.data).map_err(|error| {
                VulcanCoreError::ProviderFailure {
                    provider_id: provider_id.to_string(),
                    message: format!("{provider_label} 流式事件解析失败：{error}"),
                }
            })?;

        match event {
            AnthropicStreamingEvent::MessageStart { message } => {
                usage_snapshot = anthropic_usage_to_snapshot(&message._usage);
                response_id = Some(message.id);
                response_model = Some(message.model);
            }
            AnthropicStreamingEvent::ContentBlockStart { content_block, .. } => match content_block
            {
                AnthropicContentBlock::ToolUse { id, name, input } => {
                    pending_tool_use = Some(PendingAnthropicToolUse {
                        id,
                        name,
                        input_json: if input.is_null() || input == json!({}) {
                            String::new()
                        } else {
                            serde_json::to_string(&input).unwrap_or_else(|_| input.to_string())
                        },
                    });
                }
                AnthropicContentBlock::ServerToolUse { id, name, input } => {
                    pending_server_tool_use = Some(PendingAnthropicServerToolUse {
                        id,
                        name,
                        input_json: if input.is_null() || input == json!({}) {
                            String::new()
                        } else {
                            serde_json::to_string(&input).unwrap_or_else(|_| input.to_string())
                        },
                    });
                }
                AnthropicContentBlock::WebSearchToolResult {
                    tool_use_id,
                    content,
                } => {
                    finalized_provider_content_blocks.push(
                        AnthropicContentBlock::WebSearchToolResult {
                            tool_use_id,
                            content,
                        },
                    );
                }
                AnthropicContentBlock::Thinking {
                    thinking,
                    signature,
                } => {
                    pending_thinking = Some(PendingAnthropicThinking {
                        thinking,
                        signature: signature.unwrap_or_default(),
                    });
                }
                AnthropicContentBlock::RedactedThinking { _data } => {
                    assistant_reasoning_blocks.push(ProviderReasoningPart::Opaque);
                    finalized_provider_content_blocks
                        .push(AnthropicContentBlock::RedactedThinking { _data });
                }
                AnthropicContentBlock::Text { text } => {
                    pending_text = Some(text.clone());
                    if !text.is_empty() {
                        assistant_text.push_str(&text);
                        forward_anthropic_delta(
                            live_sender,
                            sink,
                            CoreProviderTurnDelta::Output { delta: text },
                        );
                    }
                }
            },
            AnthropicStreamingEvent::ContentBlockDelta { delta, .. } => match delta {
                AnthropicContentDelta::TextDelta { text } => {
                    pending_text.get_or_insert_with(String::new).push_str(&text);
                    assistant_text.push_str(&text);
                    forward_anthropic_delta(
                        live_sender,
                        sink,
                        CoreProviderTurnDelta::Output { delta: text },
                    );
                }
                AnthropicContentDelta::InputJsonDelta { partial_json } => {
                    if let Some(server_tool_use) = pending_server_tool_use.as_mut() {
                        server_tool_use.input_json.push_str(&partial_json);
                    } else {
                        pending_tool_use
                            .get_or_insert_with(PendingAnthropicToolUse::default)
                            .input_json
                            .push_str(&partial_json);
                    }
                }
                AnthropicContentDelta::ThinkingDelta { thinking } => {
                    pending_thinking
                        .get_or_insert_with(PendingAnthropicThinking::default)
                        .thinking
                        .push_str(&thinking);
                    forward_anthropic_delta(
                        live_sender,
                        sink,
                        CoreProviderTurnDelta::Thinking { delta: thinking },
                    );
                }
                AnthropicContentDelta::SignatureDelta { signature } => {
                    pending_thinking
                        .get_or_insert_with(PendingAnthropicThinking::default)
                        .signature
                        .push_str(&signature);
                }
            },
            AnthropicStreamingEvent::ContentBlockStop { .. } => {
                if let Some(text) = pending_text.take()
                    && !text.is_empty()
                {
                    finalized_provider_content_blocks.push(AnthropicContentBlock::Text { text });
                }
                if let Some(tool_use) = pending_tool_use.take() {
                    finalized_provider_content_blocks
                        .push(anthropic_content_block_from_pending_tool_use(&tool_use)?);
                    finalized_tool_calls.push(finalize_pending_anthropic_tool_use(
                        tool_use,
                        provider_tool_definitions,
                    )?);
                }
                if let Some(server_tool_use) = pending_server_tool_use.take() {
                    finalized_provider_content_blocks.push(
                        anthropic_content_block_from_pending_server_tool_use(&server_tool_use),
                    );
                }
                if let Some(thinking) = pending_thinking.take()
                    && !thinking.thinking.is_empty()
                {
                    finalized_provider_content_blocks.push(AnthropicContentBlock::Thinking {
                        thinking: thinking.thinking.clone(),
                        signature: (!thinking.signature.is_empty()).then_some(thinking.signature),
                    });
                    assistant_reasoning_blocks.push(ProviderReasoningPart::Text(thinking.thinking));
                }
            }
            AnthropicStreamingEvent::MessageDelta { delta, _usage } => {
                usage_snapshot = merge_anthropic_partial_usage(usage_snapshot.take(), &_usage);
                if delta.stop_reason.as_deref() == Some("tool_use") {
                    if let Some(tool_use) = pending_tool_use.take() {
                        finalized_provider_content_blocks
                            .push(anthropic_content_block_from_pending_tool_use(&tool_use)?);
                        finalized_tool_calls.push(finalize_pending_anthropic_tool_use(
                            tool_use,
                            provider_tool_definitions,
                        )?);
                    }
                    if let Some(server_tool_use) = pending_server_tool_use.take() {
                        finalized_provider_content_blocks.push(
                            anthropic_content_block_from_pending_server_tool_use(&server_tool_use),
                        );
                    }
                }
            }
            AnthropicStreamingEvent::MessageStop
            | AnthropicStreamingEvent::Ping
            | AnthropicStreamingEvent::Unknown => {}
        }
    }

    if let Some(tool_use) = pending_tool_use.take() {
        finalized_provider_content_blocks
            .push(anthropic_content_block_from_pending_tool_use(&tool_use)?);
        finalized_tool_calls.push(finalize_pending_anthropic_tool_use(
            tool_use,
            provider_tool_definitions,
        )?);
    }
    if let Some(server_tool_use) = pending_server_tool_use.take() {
        finalized_provider_content_blocks.push(
            anthropic_content_block_from_pending_server_tool_use(&server_tool_use),
        );
    }
    if let Some(text) = pending_text.take()
        && !text.is_empty()
    {
        finalized_provider_content_blocks.push(AnthropicContentBlock::Text { text });
    }
    if let Some(thinking) = pending_thinking.take()
        && !thinking.thinking.is_empty()
    {
        finalized_provider_content_blocks.push(AnthropicContentBlock::Thinking {
            thinking: thinking.thinking.clone(),
            signature: (!thinking.signature.is_empty()).then_some(thinking.signature),
        });
        assistant_reasoning_blocks.push(ProviderReasoningPart::Text(thinking.thinking));
    }

    let mut assistant_content = Vec::new();
    if !assistant_text.is_empty() {
        assistant_content.push(ProviderAssistantContent::Text(assistant_text));
    }
    if !assistant_reasoning_blocks.is_empty() {
        assistant_content.push(ProviderAssistantContent::Reasoning(
            assistant_reasoning_blocks,
        ));
    }
    assistant_content.extend(
        finalized_tool_calls
            .into_iter()
            .map(ProviderAssistantContent::ToolCall),
    );

    let mut response = build_openai_compatible_turn_response(
        provider_id,
        response_model.as_deref().unwrap_or(model_id),
        response_id,
        &assistant_content,
        usage_snapshot,
    );
    response.provider_message_state = (!finalized_provider_content_blocks.is_empty()).then(|| {
        build_anthropic_provider_message_state(
            provider_id,
            model_id,
            &finalized_provider_content_blocks,
        )
    });
    Ok(response)
}

/// Builds one direct Anthropic HTTP client with merged API key, version, and caller-supplied request headers.
/// 构建一个直连 Anthropic HTTP 客户端，并合并 API Key、版本头与调用方提供的请求头。
pub(super) fn build_anthropic_http_client(
    provider_id: &str,
    provider_label: &str,
    api_key: &str,
    mut request_headers: HeaderMap,
) -> Result<reqwest::Client, VulcanCoreError> {
    ensure_string_header(
        provider_id,
        provider_label,
        &mut request_headers,
        "x-api-key",
        api_key,
        "API Key ",
    )?;
    ensure_static_header(
        &mut request_headers,
        "anthropic-version",
        ANTHROPIC_API_VERSION_LATEST,
    );
    ensure_json_content_type(&mut request_headers);
    build_json_http_client(provider_id, provider_label, request_headers)
}

/// Builds the final Anthropic `/v1/messages` endpoint from one optional base URL override.
/// 根据可选基础地址覆盖值构建最终 Anthropic `/v1/messages` 端点。
pub(super) fn build_anthropic_messages_endpoint(base_url: Option<&str>) -> String {
    let normalized_base = base_url
        .filter(|value| !value.trim().is_empty())
        .map(normalize_anthropic_base_url)
        .unwrap_or_else(|| String::from("https://api.anthropic.com"));
    format!("{}/v1/messages", normalized_base.trim_end_matches('/'))
}

/// Forwards one normalized provider delta into both the controller sink and the host live sender.
/// 将一条规范 Provider 增量同时转发给控制器汇聚器与宿主实时发送器。
pub(super) fn forward_anthropic_delta<S>(
    live_sender: &ProviderLiveDeltaSender,
    sink: &mut S,
    delta: CoreProviderTurnDelta,
) where
    S: VulcanProviderDeltaSink,
{
    sink.push_delta(delta.clone());
    let _ = live_sender.send(Ok(core_provider_turn_delta_to_runtime_delta(delta)));
}
