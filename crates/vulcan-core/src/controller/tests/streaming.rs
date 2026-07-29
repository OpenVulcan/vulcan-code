use super::*;

/// Executes a full host-owned streaming loop and preserves provider deltas in the controller event trace before finalizing the assistant turn.
/// 执行一条完整的宿主自有流式循环，并在完成最终助手回合前把 Provider 增量保留到控制器事件轨迹中。
#[test]
fn run_user_turn_streaming_records_provider_deltas_before_finalizing() {
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        let adapter = MockStreamingProviderAdapter::new(vec![(
            vec![
                ProviderTurnDelta::Thinking {
                    delta: String::from("先读取文件。"),
                },
                ProviderTurnDelta::Output {
                    delta: String::from("已完成分析。"),
                },
            ],
            ProviderTurnResponse {
                assistant_text: String::from("已完成分析。"),
                assistant_reasoning: String::from("先读取文件。"),
                provider_message_id: Some(String::from("resp-stream-1")),
                provider_message_state: None,
                tool_calls: Vec::new(),
                continuation: Some(ProviderContinuation {
                    provider_id: Some(String::from("mock-openai-streaming")),
                    model_id: Some(String::from("mock-model")),
                    last_provider_message_id: Some(String::from("resp-stream-1")),
                    updated_at: Some(String::from("2001")),
                    ..ProviderContinuation::default()
                }),
                usage: None,
            },
        )]);
        let executor = MockToolExecutor::new();
        let mut controller =
            VulcanCoreController::new(build_test_config(2), ConversationSnapshot::default())
                .expect("valid core conversation snapshot");

        let result = controller
            .run_user_turn_streaming(&adapter, &executor, &AllowAllToolPolicy, "请先分析文件")
            .await
            .expect("run streaming user turn");

        assert_eq!(result.provider_turns_executed, 1);
        assert_eq!(result.tool_calls_processed, 0);
        assert_eq!(controller.phase(), ControllerPhase::Completed);

        let thinking_events = controller
            .events()
            .iter()
            .filter(|event| matches!(event.kind, CoreEventKind::Thinking))
            .collect::<Vec<_>>();
        let output_events = controller
            .events()
            .iter()
            .filter(|event| matches!(event.kind, CoreEventKind::Output))
            .collect::<Vec<_>>();

        assert_eq!(thinking_events.len(), 1);
        assert_eq!(thinking_events[0].body, "先读取文件。");
        assert_eq!(output_events.len(), 1);
        assert_eq!(output_events[0].body, "已完成分析。");
        assert!(controller.events().iter().any(|event| {
            matches!(event.kind, CoreEventKind::Final) && event.body.contains("Provider 回合数为 1")
        }));
        assert!(matches!(
            &controller.snapshot().messages[1].payload,
            ConversationPayload::Assistant {
                text,
                reasoning,
                provider_message_id,
                provider_message_state: _,
                tool_calls,
                usage: _,
            } if text == "已完成分析。"
                && reasoning == "先读取文件。"
                && provider_message_id.as_deref() == Some("resp-stream-1")
                && tool_calls.is_empty()
        ));
    });
}

/// Avoids checkpointing a partial streaming tool batch when a later tool in that batch fails.
/// 当同一批次中的后续工具失败时，避免为部分完成的流式工具批次建立检查点。
#[test]
fn run_user_turn_streaming_skips_partial_checkpoint_when_batch_tool_fails() {
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        let adapter = MockStreamingProviderAdapter::new(vec![(
            vec![ProviderTurnDelta::Thinking {
                delta: String::from("先调用两个工具。"),
            }],
            ProviderTurnResponse {
                assistant_text: String::new(),
                assistant_reasoning: String::from("先调用两个工具。"),
                provider_message_id: Some(String::from("resp-tools-1")),
                provider_message_state: None,
                tool_calls: vec![
                    ConversationToolCall {
                        id: String::from("tool-item-1"),
                        tool_call_id: Some(String::from("call-1")),
                        tool_name: String::from("first_tool"),
                        arguments: json!({"step": 1}),
                    },
                    ConversationToolCall {
                        id: String::from("tool-item-2"),
                        tool_call_id: Some(String::from("call-2")),
                        tool_name: String::from("second_tool"),
                        arguments: json!({"step": 2}),
                    },
                ],
                continuation: Some(ProviderContinuation {
                    provider_id: Some(String::from("mock-openai-streaming")),
                    model_id: Some(String::from("mock-model")),
                    last_provider_message_id: Some(String::from("resp-tools-1")),
                    updated_at: Some(String::from("3001")),
                    ..ProviderContinuation::default()
                }),
                usage: Some(ProviderUsageSnapshot {
                    input_tokens: Some(10),
                    uncached_input_tokens: Some(10),
                    output_tokens: Some(2),
                    reasoning_tokens: Some(1),
                    total_tokens: Some(13),
                    cached_input_tokens: None,
                    cache_creation_input_tokens: None,
                }),
            },
        )]);
        let executor = MockToolExecutor::new()
            .with_outcome(
                "first_tool",
                "第一个工具已完成。",
                Some(json!({"tool": "first"})),
                false,
            )
            .with_failure("second_tool", "第二个工具模拟失败");
        let mut controller =
            VulcanCoreController::new(build_test_config(4), ConversationSnapshot::default())
                .expect("valid core conversation snapshot");
        let mut checkpoints = Vec::<(ConversationSnapshot, Option<ProviderUsageSnapshot>)>::new();

        let result = controller
            .run_user_turn_streaming_with_checkpoints(
                &adapter,
                &executor,
                &AllowAllToolPolicy,
                "请先调用两个工具",
                |snapshot, usage| checkpoints.push((snapshot.clone(), usage.cloned())),
            )
            .await;

        assert!(result.is_err());
        assert!(checkpoints.is_empty());
        assert!(
            !controller
                .snapshot()
                .messages
                .iter()
                .any(|message| matches!(
                    &message.payload,
                    ConversationPayload::ToolResult { text, tool_call_id, .. }
                        if text == "第一个工具已完成。"
                            && tool_call_id.as_deref() == Some("call-1")
                ))
        );
        assert!(
            !controller
                .snapshot()
                .messages
                .iter()
                .any(|message| matches!(
                    &message.payload,
                    ConversationPayload::ToolResult { tool_call_id, .. }
                        if tool_call_id.as_deref() == Some("call-2")
                ))
        );
    });
}
