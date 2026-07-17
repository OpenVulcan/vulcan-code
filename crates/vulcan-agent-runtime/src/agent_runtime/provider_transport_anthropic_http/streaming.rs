use super::*;
use std::collections::HashMap;

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
    // pendingToolUses preserves exact parallel host tool blocks by Anthropic content index.
    // pendingToolUses 按 Anthropic 内容索引保留精确的并行宿主工具块。
    let mut pending_tool_uses = HashMap::<usize, PendingAnthropicToolUse>::new();
    // activeToolRequestStreams correlates visible request cards by exact content index.
    // activeToolRequestStreams 按精确内容索引关联可见请求卡片。
    let mut active_tool_request_streams = HashMap::<usize, String>::new();
    // pendingServerToolUses isolates server-owned argument streams from host tool streams.
    // pendingServerToolUses 将服务端自有参数流与宿主工具流隔离。
    let mut pending_server_tool_uses = HashMap::<usize, PendingAnthropicServerToolUse>::new();
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
            AnthropicStreamingEvent::ContentBlockStart {
                index,
                content_block,
            } => match content_block {
                AnthropicContentBlock::ToolUse { id, name, input } => {
                    // stableId preserves one correlation through streaming, finalization, and execution.
                    // stableId 在流式生成、最终化与执行期间保持同一关联。
                    let stable_id = if id.trim().is_empty() {
                        format!("call_{}", Uuid::new_v4())
                    } else {
                        id
                    };
                    // initialArguments preserves an explicit non-empty start payload.
                    // initialArguments 保留显式非空启动载荷。
                    let initial_arguments = if input.is_null() || input == json!({}) {
                        String::new()
                    } else {
                        serde_json::to_string(&input).unwrap_or_else(|_| input.to_string())
                    };
                    let argument_assembler = ToolCallDraftAssembler::new(initial_arguments.clone());
                    pending_tool_uses.insert(
                        index,
                        PendingAnthropicToolUse {
                            id: stable_id.clone(),
                            name: name.clone(),
                            input_json: argument_assembler.clone(),
                        },
                    );
                    if let Some(tool_name) =
                        resolve_exact_streamed_anthropic_tool_name(&name, provider_tool_definitions)
                    {
                        forward_anthropic_tool_request_fragment(
                            live_sender,
                            ToolRequestFragment::Detected {
                                request_stream_id: stable_id.clone(),
                                tool_call_id: stable_id.clone(),
                                tool_name,
                                initial_arguments,
                                argument_assembler: Some(argument_assembler),
                            },
                        );
                        active_tool_request_streams.insert(index, stable_id);
                    }
                }
                AnthropicContentBlock::ServerToolUse { id, name, input } => {
                    pending_server_tool_uses.insert(
                        index,
                        PendingAnthropicServerToolUse {
                            id,
                            name,
                            input_json: if input.is_null() || input == json!({}) {
                                String::new()
                            } else {
                                serde_json::to_string(&input).unwrap_or_else(|_| input.to_string())
                            },
                        },
                    );
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
            AnthropicStreamingEvent::ContentBlockDelta { index, delta } => match delta {
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
                    if let Some(server_tool_use) = pending_server_tool_uses.get_mut(&index) {
                        server_tool_use.input_json.push_str(&partial_json);
                    } else if let Some(tool_use) = pending_tool_uses.get_mut(&index) {
                        tool_use.input_json.append(&partial_json);
                    }
                    if let Some(request_stream_id) = active_tool_request_streams.get(&index) {
                        forward_anthropic_tool_request_fragment(
                            live_sender,
                            ToolRequestFragment::ArgumentsDelta {
                                request_stream_id: request_stream_id.clone(),
                                delta: partial_json,
                            },
                        );
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
            AnthropicStreamingEvent::ContentBlockStop { index } => {
                if let Some(text) = pending_text.take()
                    && !text.is_empty()
                {
                    finalized_provider_content_blocks.push(AnthropicContentBlock::Text { text });
                }
                if let Some(tool_use) =
                    take_anthropic_tool_use_at_content_block_stop(&mut pending_tool_uses, index)
                {
                    let content_block = anthropic_content_block_from_pending_tool_use(&tool_use);
                    let finalized =
                        finalize_pending_anthropic_tool_use(tool_use, provider_tool_definitions);
                    finish_anthropic_tool_request_stream(
                        live_sender,
                        &mut active_tool_request_streams,
                        index,
                        finalized.as_ref(),
                    );
                    finalized_provider_content_blocks.push(content_block?);
                    finalized_tool_calls.push(finalized?);
                }
                if let Some(server_tool_use) = pending_server_tool_uses.remove(&index) {
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
            AnthropicStreamingEvent::MessageDelta { _usage } => {
                usage_snapshot = merge_anthropic_partial_usage(usage_snapshot.take(), &_usage);
                // stopReason describes the message result; contentBlockStop remains the only in-stream tool-input completion boundary.
                // stopReason 描述整条消息的结果；contentBlockStop 仍是流内工具输入完成的唯一边界。
            }
            AnthropicStreamingEvent::MessageStop
            | AnthropicStreamingEvent::Ping
            | AnthropicStreamingEvent::Unknown => {}
        }
    }

    let mut tool_indices = pending_tool_uses.keys().copied().collect::<Vec<_>>();
    tool_indices.sort_unstable();
    for index in tool_indices {
        let Some(tool_use) = pending_tool_uses.remove(&index) else {
            continue;
        };
        let content_block = anthropic_content_block_from_pending_tool_use(&tool_use);
        let finalized = finalize_pending_anthropic_tool_use(tool_use, provider_tool_definitions);
        finish_anthropic_tool_request_stream(
            live_sender,
            &mut active_tool_request_streams,
            index,
            finalized.as_ref(),
        );
        finalized_provider_content_blocks.push(content_block?);
        finalized_tool_calls.push(finalized?);
    }
    let mut server_indices = pending_server_tool_uses.keys().copied().collect::<Vec<_>>();
    server_indices.sort_unstable();
    for index in server_indices {
        let Some(server_tool_use) = pending_server_tool_uses.remove(&index) else {
            continue;
        };
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

/// Takes one host tool only when the stopping content index owns that exact tool block.
/// 仅当结束内容索引准确归属该宿主工具块时取出该工具。
///
/// # Parameters
/// 参数
/// `pending_tool_uses` stores open host-tool blocks by Anthropic content index.
/// `pending_tool_uses` 按 Anthropic 内容索引保存尚未结束的宿主工具块。
/// `content_index` is the exact index carried by `content_block_stop`.
/// `content_index` 是 `content_block_stop` 携带的准确索引。
///
/// # Returns
/// 返回值
/// The matching pending tool block, or `None` when the stop belongs to thinking, text, or another block.
/// 返回匹配的待定工具块；当结束事件属于思考、文本或其他内容块时返回 `None`。
fn take_anthropic_tool_use_at_content_block_stop(
    pending_tool_uses: &mut HashMap<usize, PendingAnthropicToolUse>,
    content_index: usize,
) -> Option<PendingAnthropicToolUse> {
    pending_tool_uses.remove(&content_index)
}

/// Forwards one normalized Anthropic tool-request fragment to the runtime stream coordinator.
/// 将一条规范化 Anthropic 工具请求碎片转发给运行时流协调器。
///
/// # Parameters
/// 参数
/// `live_sender` is the host-owned live delta channel for this physical request.
/// `live_sender` 是当前物理请求的宿主自有实时增量通道。
/// `fragment` contains exact correlation and raw arguments only inside the runtime process.
/// `fragment` 仅在运行时进程内部包含精确关联与原始参数。
fn forward_anthropic_tool_request_fragment(
    live_sender: &ProviderLiveDeltaSender,
    fragment: ToolRequestFragment,
) {
    let _ = live_sender.send(Ok(ProviderStreamDelta::ToolRequestFragment(fragment)));
}

/// Finishes one exact Anthropic request stream according to canonical finalization outcome.
/// 根据规范最终化结果结束一条精确 Anthropic 请求流。
///
/// # Parameters
/// 参数
/// `live_sender` is the host-owned live delta channel.
/// `live_sender` 是宿主自有实时增量通道。
/// `active_requests` stores request-stream identifiers by exact content index.
/// `active_requests` 按精确内容索引保存请求流标识。
/// `content_index` is the provider block boundary being closed.
/// `content_index` 是正在闭合的 Provider 内容块边界。
/// `finalized` contains the canonical call or the exact parsing failure.
/// `finalized` 包含规范调用或准确的解析失败。
fn finish_anthropic_tool_request_stream(
    live_sender: &ProviderLiveDeltaSender,
    active_requests: &mut HashMap<usize, String>,
    content_index: usize,
    finalized: Result<&CoreConversationToolCall, &VulcanCoreError>,
) {
    let Some(request_stream_id) = active_requests.remove(&content_index) else {
        return;
    };
    // fragment makes success and failure terminal states explicit to the visible request card.
    // fragment 向可见请求卡明确区分成功与失败终态。
    let fragment = match finalized {
        Ok(tool_call) => {
            // canonicalToolCallId promotes a temporary correlation only after complete parsing succeeded.
            // canonicalToolCallId 仅在完整解析成功后提升临时关联。
            let canonical_tool_call_id = tool_call
                .tool_call_id
                .clone()
                .unwrap_or_else(|| tool_call.id.clone());
            ToolRequestFragment::Completed {
                request_stream_id,
                canonical_tool_call_id: Some(canonical_tool_call_id),
                final_arguments: None,
            }
        }
        Err(_) => ToolRequestFragment::Aborted {
            request_stream_id,
            reason: String::from("Anthropic tool input finalization failed"),
        },
    };
    forward_anthropic_tool_request_fragment(live_sender, fragment);
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

#[cfg(test)]
mod tests {
    //! Anthropic host-tool request correlation tests.
    //! Anthropic 宿主工具请求关联测试。

    use super::*;

    /// Verifies interleaved Anthropic content indices close the exact request identities in arrival order.
    /// 验证交错的 Anthropic 内容索引会按到达顺序闭合精确请求身份。
    #[test]
    fn parallel_content_indices_complete_exact_tool_requests() {
        // live channel records two detected requests followed by reverse-order block completion.
        // live 通道记录两个已识别请求以及逆序到达的内容块完成事件。
        let (live_sender, mut live_receiver) = tokio::sync::mpsc::unbounded_channel();
        let mut active = HashMap::from([
            (3_usize, String::from("anthropic-call-3")),
            (7_usize, String::from("anthropic-call-7")),
        ]);
        forward_anthropic_tool_request_fragment(
            &live_sender,
            ToolRequestFragment::Detected {
                request_stream_id: String::from("anthropic-call-3"),
                tool_call_id: String::from("anthropic-call-3"),
                tool_name: String::from("submit_plan"),
                initial_arguments: String::new(),
                argument_assembler: None,
            },
        );
        forward_anthropic_tool_request_fragment(
            &live_sender,
            ToolRequestFragment::Detected {
                request_stream_id: String::from("anthropic-call-7"),
                tool_call_id: String::from("anthropic-call-7"),
                tool_name: String::from("submit_plan"),
                initial_arguments: String::new(),
                argument_assembler: None,
            },
        );
        let call_three = CoreConversationToolCall {
            id: String::from("anthropic-call-3"),
            tool_call_id: Some(String::from("anthropic-call-3")),
            tool_name: String::from("submit_plan"),
            arguments: json!({ "name": "three" }),
        };
        let call_seven = CoreConversationToolCall {
            id: String::from("anthropic-call-7"),
            tool_call_id: Some(String::from("anthropic-call-7")),
            tool_name: String::from("submit_plan"),
            arguments: json!({ "name": "seven" }),
        };
        finish_anthropic_tool_request_stream(&live_sender, &mut active, 7, Ok(&call_seven));
        finish_anthropic_tool_request_stream(&live_sender, &mut active, 3, Ok(&call_three));

        let fragments = std::iter::from_fn(|| live_receiver.try_recv().ok())
            .map(|delta| delta.expect("provider fragment should be successful"))
            .collect::<Vec<_>>();
        assert_eq!(fragments.len(), 4);
        assert!(matches!(
            &fragments[2],
            ProviderStreamDelta::ToolRequestFragment(ToolRequestFragment::Completed {
                request_stream_id,
                canonical_tool_call_id: Some(canonical_tool_call_id),
                ..
            }) if request_stream_id == "anthropic-call-7"
                && canonical_tool_call_id == "anthropic-call-7"
        ));
        assert!(matches!(
            &fragments[3],
            ProviderStreamDelta::ToolRequestFragment(ToolRequestFragment::Completed {
                request_stream_id,
                canonical_tool_call_id: Some(canonical_tool_call_id),
                ..
            }) if request_stream_id == "anthropic-call-3"
                && canonical_tool_call_id == "anthropic-call-3"
        ));
        assert!(active.is_empty());
    }

    /// Verifies an invalid final argument stream aborts its visible request instead of reporting completion.
    /// 验证无效的最终参数流会中止可见请求，而不是错误地报告完成。
    #[test]
    fn failed_tool_input_finalization_aborts_request_stream() {
        // liveChannel captures the exact terminal fragment exposed to the runtime coordinator.
        // liveChannel 捕获暴露给运行时协调器的准确终态碎片。
        let (live_sender, mut live_receiver) = tokio::sync::mpsc::unbounded_channel();
        // active contains the one visible request awaiting finalization.
        // active 包含一条等待最终化的可见请求。
        let mut active = HashMap::from([(4_usize, String::from("anthropic-call-4"))]);
        // error represents the provider-side incomplete JSON failure without embedding arguments.
        // error 表示 Provider 侧不完整 JSON 失败，且不包含参数内容。
        let error = VulcanCoreError::ProviderFailure {
            provider_id: String::from("anthropic"),
            message: String::from("incomplete tool input"),
        };

        finish_anthropic_tool_request_stream(&live_sender, &mut active, 4, Err(&error));

        // fragment is the terminal request-stream event emitted after failure.
        // fragment 是失败后发出的请求流终态事件。
        let fragment = live_receiver
            .try_recv()
            .expect("aborted fragment should exist")
            .expect("provider fragment should be successful");
        assert!(matches!(
            fragment,
            ProviderStreamDelta::ToolRequestFragment(ToolRequestFragment::Aborted {
                request_stream_id,
                reason,
            }) if request_stream_id == "anthropic-call-4"
                && reason == "Anthropic tool input finalization failed"
        ));
        assert!(active.is_empty());
    }

    /// Verifies a thinking block stop cannot consume the following indexed tool-input block.
    /// 验证思考内容块结束事件不能消耗后续带独立索引的工具输入块。
    #[test]
    fn thinking_stop_preserves_following_tool_input_block() {
        // pending contains the tool block that follows thinking at the next content index.
        // pending 包含位于思考块下一内容索引的工具块。
        let mut pending = HashMap::from([(
            1_usize,
            PendingAnthropicToolUse {
                id: String::from("anthropic-call-1"),
                name: String::from("submit_plan"),
                input_json: ToolCallDraftAssembler::new(String::from(
                    r#"{"content":"完整工具参数"}"#,
                )),
            },
        )]);

        // thinkingStop uses the preceding thinking content index.
        // thinkingStop 使用前一个思考内容索引。
        let thinking_stop = take_anthropic_tool_use_at_content_block_stop(&mut pending, 0);
        assert!(thinking_stop.is_none());
        assert!(pending.contains_key(&1));

        // toolStop uses the exact tool content index and may now finalize the complete JSON.
        // toolStop 使用准确的工具内容索引，此时才能最终化完整 JSON。
        let tool_stop = take_anthropic_tool_use_at_content_block_stop(&mut pending, 1)
            .expect("tool block should close only at its own content index");
        let parsed = parse_pending_anthropic_tool_use_input_json(&tool_stop)
            .expect("complete tool input should parse");

        assert_eq!(parsed, json!({ "content": "完整工具参数" }));
        assert!(pending.is_empty());
    }
}
