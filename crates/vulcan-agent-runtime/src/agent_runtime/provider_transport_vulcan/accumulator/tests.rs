//! Regression tests for the accumulator boundary.
//! accumulator边界的回归测试。

use super::*;

/// RecordingProviderDeltaSink retains canonical Core deltas for adapter projection tests.
/// RecordingProviderDeltaSink 为适配器投影测试保留规范 Core 增量。
#[derive(Default)]
struct RecordingProviderDeltaSink {
    /// deltas contains every canonical delta received from the public adapter.
    /// deltas 包含公共适配器发出的每一项规范增量。
    deltas: Vec<ProviderTurnDelta>,
}

impl VulcanProviderDeltaSink for RecordingProviderDeltaSink {
    /// Stores one canonical provider delta.
    /// 保存一项规范 Provider 增量。
    ///
    /// # Parameters
    /// 参数
    /// `delta` is the exact decoded delta produced by the public protocol.
    /// `delta` 是公共协议产出的精确解码增量。
    fn push_delta(&mut self, delta: ProviderTurnDelta) {
        self.deltas.push(delta);
    }
}

/// Builds one accumulator and applies the supplied normalized events in order.
/// 构建一个累积器，并按顺序应用给定的规范化事件。
fn accumulated_public_turn(events: Vec<AiStreamEvent>) -> PublicTurnAccumulator {
    let (live_sender, _live_receiver) = tokio::sync::mpsc::unbounded_channel();
    let mut sink = RecordingProviderDeltaSink::default();
    let mut accumulator = PublicTurnAccumulator::new(
        "provider",
        "connection",
        "https://provider.example/responses",
        "openai-responses.v1",
        "model",
        None,
    );
    for event in events {
        accumulator
            .apply_event("provider", &live_sender, &mut sink, event)
            .expect("normalized test event should accumulate");
    }
    accumulator
}

/// Successful transport completion without normalized durable semantics is rejected.
/// 没有规范化可靠语义的传输成功终态会被拒绝。
#[test]
fn successful_empty_or_transient_only_response_is_rejected() {
    let empty = accumulated_public_turn(vec![AiStreamEvent::ResponseCompleted]);
    assert!(
        empty
            .finish()
            .expect_err("empty successful response must fail")
            .to_string()
            .contains(
                "Provider returned a successful terminal response without durable semantic output."
            )
    );

    let transient = accumulated_public_turn(vec![
        AiStreamEvent::ReasoningSummaryUpdated {
            item_id: String::from("summary-1"),
            text: String::from("temporary"),
        },
        AiStreamEvent::ResponseCompleted,
    ]);
    assert!(transient.finish().is_err());

    let mut context_only = accumulated_public_turn(vec![AiStreamEvent::ResponseCompleted]);
    context_only.provider_message_id = Some(String::from("provider-message-only"));
    context_only.provider_context_items.push(json!({
        "type": "message",
        "id": "context-only",
        "status": "completed",
        "role": "assistant",
        "content": [],
    }));
    assert!(context_only.finish().is_err());
}

/// Reasoning, assistant text, and complete tool calls each count as durable semantics.
/// 思考、assistant 正文与完整工具调用均分别算作可靠语义。
#[test]
fn successful_response_requires_one_normalized_durable_semantic_item() {
    let reasoning = accumulated_public_turn(vec![
        AiStreamEvent::ReasoningStarted {
            item_id: String::from("reasoning-1"),
        },
        AiStreamEvent::ReasoningDelta {
            item_id: String::from("reasoning-1"),
            delta: String::from("durable reasoning"),
        },
        AiStreamEvent::ReasoningCompleted {
            item_id: String::from("reasoning-1"),
            signature: None,
        },
        AiStreamEvent::ResponseCompleted,
    ]);
    assert!(reasoning.finish().is_ok());

    let text = accumulated_public_turn(vec![
        AiStreamEvent::ResponseItemStarted {
            item_id: String::from("text-1"),
            item: ContentItem::Text {
                text: String::new(),
            },
        },
        AiStreamEvent::ResponseTextDelta {
            item_id: String::from("text-1"),
            delta: String::from("assistant text"),
        },
        AiStreamEvent::ResponseItemCompleted {
            item_id: String::from("text-1"),
        },
        AiStreamEvent::ResponseCompleted,
    ]);
    assert!(text.finish().is_ok());

    let tool = accumulated_public_turn(vec![
        AiStreamEvent::ToolCallStarted {
            item_id: String::from("tool-item-1"),
            call_id: String::from("provider-tool-1"),
            tool_name: String::from("shell_command"),
            index: 0,
        },
        AiStreamEvent::ToolCallCompleted {
            item_id: String::from("tool-item-1"),
            call: PublicToolCall {
                id: String::from("provider-tool-1"),
                name: String::from("shell_command"),
                arguments: json!({"command": "pwd"}),
                signature: None,
            },
        },
        AiStreamEvent::ResponseCompleted,
    ]);
    assert!(tool.finish().is_ok());
}

/// Verifies split Anthropic usage events retain start-event cache fields and terminal output fields.
/// 验证拆分的 Anthropic 用量事件会同时保留开始事件的缓存字段与终态输出字段。
#[test]
fn partial_usage_updates_merge_without_erasing_known_fields() {
    let reported = |value| UsageValue::Known {
        value,
        source: vulcan_ai_contract::UsageValueSource::Reported,
    };
    let mut usage = RequestUsage {
        input_tokens: reported(1_000),
        cache_read_tokens: reported(800),
        cache_write_tokens: reported(120),
        ..RequestUsage::default()
    };

    usage.merge_update(RequestUsage {
        output_tokens: reported(75),
        total_tokens: reported(1_075),
        ..RequestUsage::default()
    });

    assert_eq!(usage.input_tokens, reported(1_000));
    assert_eq!(usage.cache_read_tokens, reported(800));
    assert_eq!(usage.cache_write_tokens, reported(120));
    assert_eq!(usage.output_tokens, reported(75));
    assert_eq!(usage.total_tokens, reported(1_075));
}

/// Verifies a frozen response-profile chain selects exactly one transient status mode.
/// 验证冻结响应 Profile 链只允许选择一种瞬态状态模式。
#[test]
fn transient_reasoning_mode_fails_closed_on_multiple_profile_claims() {
    // profile builds one minimal response Profile with an explicit transient-mode claim.
    // profile 构建一个带显式瞬态模式声明的最小响应 Profile。
    let profile = |profile_id: &str, mode| ResponseProjectionProfile {
        profile_id: profile_id.to_string(),
        revision: 1,
        steps: Vec::new(),
        transient_reasoning_status: Some(mode),
    };
    assert_eq!(
        transient_reasoning_status_kind(
            "provider",
            &[profile("summary", TransientReasoningStatusMode::Summary,)],
        )
        .expect("one transient profile should resolve"),
        Some(TransientReasoningStatusKind::Summary)
    );
    assert!(
        transient_reasoning_status_kind(
            "provider",
            &[
                profile("summary", TransientReasoningStatusMode::Summary),
                profile("encrypted", TransientReasoningStatusMode::Encrypted),
            ],
        )
        .expect_err("multiple transient profiles must fail closed")
        .to_string()
        .contains("Multiple response profiles")
    );
}

/// Verifies captured Responses output items survive stream accumulation and durable replay.
/// 验证捕获的 Responses 输出项可完整经过流累积与持久化重放。
#[test]
fn public_turn_persists_exact_responses_provider_context() {
    // liveChannel and sink satisfy the normal accumulator projection interfaces.
    // liveChannel 与 sink 满足累积器的常规投影接口。
    let (live_sender, _live_receiver) = tokio::sync::mpsc::unbounded_channel();
    let mut sink = RecordingProviderDeltaSink::default();
    let mut accumulator = PublicTurnAccumulator::new(
        "deepseek",
        "connection-deepseek",
        "https://api.deepseek.com/responses",
        "openai-responses.v1",
        "deepseek-v4-flash",
        None,
    );
    // providerItem mirrors a completed assistant message emitted by Responses SSE.
    // providerItem 镜像 Responses SSE 发出的一条已完成助手消息。
    let provider_item = json!({
        "type": "message",
        "id": "message-item",
        "status": "completed",
        "role": "assistant",
        "content": [{"type": "output_text", "text": "done"}],
    });

    for event in [
        AiStreamEvent::ResponseItemStarted {
            item_id: String::from("message-item"),
            item: ContentItem::Text {
                text: String::new(),
            },
        },
        AiStreamEvent::ResponseTextDelta {
            item_id: String::from("message-item"),
            delta: String::from("done"),
        },
        AiStreamEvent::ResponseItemCompleted {
            item_id: String::from("message-item"),
        },
        AiStreamEvent::ProviderContextItemCaptured {
            item: provider_item.clone(),
        },
        AiStreamEvent::ResponseCompleted,
    ] {
        accumulator
            .apply_event("deepseek", &live_sender, &mut sink, event)
            .expect("completed Responses event should accumulate");
    }

    // responseState is the exact extension persisted into the assistant conversation node.
    // responseState 是持久化到助手会话节点中的精确扩展状态。
    // response is the durable controller value after all transient state was cleared.
    // response 是全部瞬态状态清除后的持久化控制器值。
    let response = accumulator
        .finish()
        .expect("completed Responses turn should finish");
    // replay_value contains only the canonical durable replay envelope.
    // replay_value 只包含规范持久化重放信封。
    let replay_value = response
        .provider_message_state
        .as_ref()
        .and_then(|state| state.pointer("/vulcan_ai/replay"))
        .expect("replay envelope should be persisted");
    // replay proves no summary text or status metadata crossed the persistence boundary.
    // replay 证明没有摘要文本或状态元数据越过持久化边界。
    let replay: ProviderReplayEnvelope =
        serde_json::from_value(replay_value.clone()).expect("replay envelope should deserialize");
    assert_eq!(
        replay.content,
        vec![ContentItem::Text {
            text: String::from("done"),
        }]
    );
    assert_eq!(replay.provider_context_items, vec![provider_item]);
}

/// Verifies malformed streamed tool arguments become one non-executable tool call instead of a retryable Provider failure.
/// 验证格式错误的流式工具参数会成为一条不可执行工具调用，而不是可重试 Provider 失败。
#[test]
fn public_tool_abort_becomes_non_executable_tool_call() {
    // liveChannel and sink capture both the visible tool draft and its abort boundary.
    // liveChannel 与 sink 同时捕获可见工具草稿及其中止边界。
    let (live_sender, mut live_receiver) = tokio::sync::mpsc::unbounded_channel();
    let mut sink = RecordingProviderDeltaSink::default();
    // accumulator represents one physical Provider request after the latest controller checkpoint.
    // accumulator 表示最近控制器检查点之后的一次真实 Provider 请求。
    let mut accumulator = PublicTurnAccumulator::new(
        "opencode-go",
        "connection-opencode-go",
        "https://opencode.example/v1/responses",
        "openai-responses.v1",
        "deepseek-v4-flash",
        None,
    );

    accumulator
        .apply_event(
            "opencode-go",
            &live_sender,
            &mut sink,
            AiStreamEvent::ToolCallStarted {
                item_id: String::from("tool-item"),
                call_id: String::from("call-1"),
                tool_name: String::from("vulcan_file_edit"),
                index: 0,
            },
        )
        .expect("tool draft should start");
    // decoderMessage mirrors the controlled error emitted by OpenAI Responses argument parsing.
    // decoderMessage 镜像 OpenAI Responses 参数解析发出的受控错误。
    let decoder_message = "OpenAI Responses tool input ended before valid JSON was complete";
    accumulator
        .apply_event(
            "opencode-go",
            &live_sender,
            &mut sink,
            AiStreamEvent::ToolCallAborted {
                item_id: String::from("tool-item"),
                message: decoder_message.to_string(),
            },
        )
        .expect("identified malformed tool call should remain inside the successful turn");
    accumulator
        .apply_event(
            "opencode-go",
            &live_sender,
            &mut sink,
            AiStreamEvent::ResponseCompleted,
        )
        .expect("response completion should remain valid after the controlled rejection");

    let response = accumulator
        .finish()
        .expect("controlled malformed tool call should finish the Provider turn");
    assert_eq!(response.tool_calls.len(), 1);
    assert_eq!(response.tool_calls[0].tool_name, "vulcan_file_edit");
    assert_eq!(response.tool_calls[0].arguments, Value::Null);
    assert_eq!(
        response.tool_calls[0].argument_parse_error.as_deref(),
        Some(decoder_message)
    );

    assert!(matches!(
        live_receiver.try_recv(),
        Ok(Ok(ProviderStreamDelta::ToolRequestFragment(
            ToolRequestFragment::Detected { .. }
        )))
    ));
    assert!(matches!(
        live_receiver.try_recv(),
        Ok(Ok(ProviderStreamDelta::ToolRequestFragment(
            ToolRequestFragment::Invalid { .. }
        )))
    ));
    assert!(live_receiver.try_recv().is_err());
}

/// Verifies partial provider captures cannot replace complete normalized assistant history.
/// 验证不完整的供应商捕获项无法替代完整的规范助手历史。
#[test]
fn partial_provider_context_falls_back_to_normalized_replay() {
    // content requires both reasoning and visible assistant text.
    // content 同时要求推理与可见助手文本。
    let content = vec![
        ContentItem::Reasoning {
            text: String::from("inspect"),
            signature: None,
        },
        ContentItem::Text {
            text: String::from("done"),
        },
    ];
    // partialItems intentionally omit the provider message item that owns visible text.
    // partialItems 刻意缺少承载可见文本的供应商消息项。
    let partial_items = vec![json!({
        "type": "reasoning",
        "id": "reasoning-item",
        "status": "completed",
        "summary": [{"type": "summary_text", "text": "inspect"}],
    })];

    assert!(!provider_context_items_cover_replay_content(
        content.as_slice(),
        partial_items.as_slice(),
    ));
}

/// Verifies public text and reasoning deltas reach both Core and the live workbench stream.
/// 验证公共文本与推理增量同时到达 Core 和实时工作台流。
#[test]
fn public_turn_deltas_are_forwarded_to_live_workbench_stream() {
    let (live_sender, mut live_receiver) = tokio::sync::mpsc::unbounded_channel();
    let mut sink = RecordingProviderDeltaSink::default();
    let mut accumulator = PublicTurnAccumulator::new(
        "minimax",
        "connection-minimax",
        "https://api.minimax.io/v1/messages",
        "anthropic-messages",
        "MiniMax-M3",
        None,
    );

    accumulator
        .apply_event(
            "minimax",
            &live_sender,
            &mut sink,
            AiStreamEvent::ResponseItemStarted {
                item_id: String::from("text-item"),
                item: ContentItem::Text {
                    text: String::new(),
                },
            },
        )
        .expect("text item should start");
    accumulator
        .apply_event(
            "minimax",
            &live_sender,
            &mut sink,
            AiStreamEvent::ResponseTextDelta {
                item_id: String::from("text-item"),
                delta: String::from("最终答案"),
            },
        )
        .expect("text delta should be accepted");
    accumulator
        .apply_event(
            "minimax",
            &live_sender,
            &mut sink,
            AiStreamEvent::ReasoningStarted {
                item_id: String::from("reasoning-item"),
            },
        )
        .expect("reasoning item should start");
    accumulator
        .apply_event(
            "minimax",
            &live_sender,
            &mut sink,
            AiStreamEvent::ReasoningDelta {
                item_id: String::from("reasoning-item"),
                delta: String::from("先分析"),
            },
        )
        .expect("reasoning delta should be accepted");

    assert!(matches!(
        sink.deltas.as_slice(),
        [
            ProviderTurnDelta::Output { delta: output },
            ProviderTurnDelta::Thinking {
                delta: reasoning
            },
        ] if output == "最终答案" && reasoning == "先分析"
    ));
    assert!(matches!(
        live_receiver.try_recv(),
        Ok(Ok(ProviderStreamDelta::Output(output))) if output == "最终答案"
    ));
    assert!(matches!(
        live_receiver.try_recv(),
        Ok(Ok(ProviderStreamDelta::Thinking(reasoning))) if reasoning == "先分析"
    ));
}

/// Verifies summary status replacement and token updates never enter durable reasoning state.
/// 验证摘要状态替换与 Token 更新绝不进入持久化思考状态。
#[test]
fn transient_reasoning_summary_is_live_only_and_clears_at_terminal() {
    // live_sender and live_receiver observe the request-local status stream.
    // live_sender 与 live_receiver 观察请求局部状态流。
    let (live_sender, mut live_receiver) = tokio::sync::mpsc::unbounded_channel();
    // sink proves no transient content enters canonical Core deltas.
    // sink 证明没有瞬态内容进入规范 Core 增量。
    let mut sink = RecordingProviderDeltaSink::default();
    // accumulator owns a profile-configured summary status for one physical request.
    // accumulator 为单次物理请求持有 Profile 配置的摘要状态。
    let mut accumulator = PublicTurnAccumulator::new(
        "commandcode",
        "connection-commandcode",
        "https://api.commandcode.ai/provider/v1/chat/completions",
        "openai-chat-completions.v1",
        "gpt-5.6-luna",
        Some(TransientReasoningStatusKind::Summary),
    );

    for event in [
        AiStreamEvent::ResponseStarted {
            execution_id: String::from("response-1"),
        },
        AiStreamEvent::ReasoningSummaryUpdated {
            item_id: String::from("summary-0"),
            text: String::from("正在检查协议"),
        },
        AiStreamEvent::ReasoningSummaryUpdated {
            item_id: String::from("summary-0"),
            text: String::from("正在检查协议响应"),
        },
        AiStreamEvent::ReasoningSummaryUpdated {
            item_id: String::from("summary-1"),
            text: String::from("正在核对清单"),
        },
        AiStreamEvent::UsageUpdated {
            usage: Box::new(RequestUsage {
                reasoning_tokens: UsageValue::Known {
                    value: 321,
                    source: vulcan_ai_contract::UsageValueSource::Reported,
                },
                ..RequestUsage::default()
            }),
        },
        AiStreamEvent::ResponseCompleted,
    ] {
        accumulator
            .apply_event("commandcode", &live_sender, &mut sink, event)
            .expect("transient summary lifecycle should accumulate");
    }

    assert!(sink.deltas.is_empty());
    assert!(matches!(
        live_receiver.try_recv(),
        Ok(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(Some(status))))
            if status.kind == TransientReasoningStatusKind::Summary
                && status.summaries.is_empty()
                && status.reasoning_tokens.is_none()
    ));
    assert!(matches!(
        live_receiver.try_recv(),
        Ok(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(Some(status))))
            if status.kind == TransientReasoningStatusKind::Summary
                && matches!(status.summaries.as_slice(), [summary]
                    if summary.item_id == "summary-0" && summary.text == "正在检查协议")
                && status.reasoning_tokens.is_none()
    ));
    assert!(matches!(
        live_receiver.try_recv(),
        Ok(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(Some(status))))
            if status.kind == TransientReasoningStatusKind::Summary
                && matches!(status.summaries.as_slice(), [summary]
                    if summary.item_id == "summary-0" && summary.text == "正在检查协议响应")
                && status.reasoning_tokens.is_none()
    ));
    assert!(matches!(
        live_receiver.try_recv(),
        Ok(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(Some(status))))
            if status.kind == TransientReasoningStatusKind::Summary
                && matches!(status.summaries.as_slice(), [first, second]
                    if first.item_id == "summary-0"
                        && first.text == "正在检查协议响应"
                        && second.item_id == "summary-1"
                        && second.text == "正在核对清单")
                && status.reasoning_tokens.is_none()
    ));
    assert!(matches!(
        live_receiver.try_recv(),
        Ok(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(Some(status))))
            if status.kind == TransientReasoningStatusKind::Summary
                && status.summaries.len() == 2
                && status.reasoning_tokens == Some(321)
    ));
    assert!(matches!(
        live_receiver.try_recv(),
        Ok(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(
            None
        )))
    ));

    let error = accumulator
        .finish()
        .expect_err("transient-only completion must not create durable Provider semantics");
    assert!(
        error
            .to_string()
            .contains("without durable semantic output")
    );
}

/// Verifies non-empty full reasoning permanently supersedes summary and encrypted fallbacks.
/// 验证非空完整思考会永久取代摘要与加密回退状态。
#[test]
fn durable_reasoning_supersedes_transient_fallbacks() {
    // liveSender and liveReceiver observe the exact fallback-clear ordering.
    // liveSender 与 liveReceiver 观察精确的回退清除顺序。
    let (live_sender, mut live_receiver) = tokio::sync::mpsc::unbounded_channel();
    // sink records only durable reasoning after arbitration.
    // sink 仅记录仲裁后的持久化思考。
    let mut sink = RecordingProviderDeltaSink::default();
    // accumulator starts in encrypted fallback mode before readable summary arrives.
    // accumulator 在可读摘要到达前以加密回退模式启动。
    let mut accumulator = PublicTurnAccumulator::new(
        "commandcode",
        "connection-commandcode",
        "https://api.commandcode.ai/provider/v1/chat/completions",
        "openai-chat-completions.v1",
        "gpt-5.6-luna",
        Some(TransientReasoningStatusKind::Encrypted),
    );
    for event in [
        AiStreamEvent::ResponseStarted {
            execution_id: String::from("response-priority"),
        },
        AiStreamEvent::ReasoningSummaryUpdated {
            item_id: String::from("summary-0"),
            text: String::from("正在分析"),
        },
        AiStreamEvent::ReasoningStarted {
            item_id: String::from("reasoning-0"),
        },
        AiStreamEvent::ReasoningDelta {
            item_id: String::from("reasoning-0"),
            delta: String::from("完整思考"),
        },
        AiStreamEvent::ReasoningSummaryUpdated {
            item_id: String::from("summary-1"),
            text: String::from("不得重新出现"),
        },
        AiStreamEvent::ReasoningCompleted {
            item_id: String::from("reasoning-0"),
            signature: None,
        },
        AiStreamEvent::ResponseCompleted,
    ] {
        accumulator
            .apply_event("commandcode", &live_sender, &mut sink, event)
            .expect("priority arbitration should remain valid");
    }

    assert!(matches!(
        live_receiver.try_recv(),
        Ok(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(Some(status))))
            if status.kind == TransientReasoningStatusKind::Encrypted
    ));
    assert!(matches!(
        live_receiver.try_recv(),
        Ok(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(Some(status))))
            if status.kind == TransientReasoningStatusKind::Summary
                && status.summaries.len() == 1
    ));
    assert!(matches!(
        live_receiver.try_recv(),
        Ok(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(
            None
        )))
    ));
    assert!(matches!(
        live_receiver.try_recv(),
        Ok(Ok(ProviderStreamDelta::Thinking(reasoning))) if reasoning == "完整思考"
    ));
    assert!(live_receiver.try_recv().is_err());
    assert!(matches!(
        sink.deltas.as_slice(),
        [ProviderTurnDelta::Thinking { delta }] if delta == "完整思考"
    ));
    assert_eq!(
        accumulator
            .finish()
            .expect("completed priority turn should finish")
            .assistant_reasoning,
        "完整思考"
    );
}
