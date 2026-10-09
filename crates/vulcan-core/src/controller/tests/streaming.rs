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

/// Verifies an executor failure becomes a complete error result and never aborts the Provider loop.
/// 验证执行器失败会成为完整错误结果，且绝不会中断 Provider 循环。
#[test]
fn run_user_turn_streaming_retains_executor_failure_as_tool_result() {
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        let adapter = MockStreamingProviderAdapter::new(vec![
            (
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
                            argument_parse_error: None,
                            raw_arguments: Default::default(),
                        },
                        ConversationToolCall {
                            id: String::from("tool-item-2"),
                            tool_call_id: Some(String::from("call-2")),
                            tool_name: String::from("second_tool"),
                            arguments: json!({"step": 2}),
                            argument_parse_error: None,
                            raw_arguments: Default::default(),
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
                        ..ProviderUsageSnapshot::default()
                    }),
                },
            ),
            (
                Vec::new(),
                ProviderTurnResponse {
                    assistant_text: String::from("已收到工具失败，停止自动重试。"),
                    assistant_reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-tools-2")),
                    provider_message_state: None,
                    tool_calls: Vec::new(),
                    continuation: None,
                    usage: None,
                },
            ),
        ]);
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

        assert!(result.is_ok());
        assert_eq!(checkpoints.len(), 3);
        assert_eq!(
            checkpoints[0].1,
            Some(ProviderUsageSnapshot {
                input_tokens: Some(10),
                uncached_input_tokens: Some(10),
                output_tokens: Some(2),
                reasoning_tokens: Some(1),
                total_tokens: Some(13),
                cached_input_tokens: None,
                cache_creation_input_tokens: None,
                ..ProviderUsageSnapshot::default()
            })
        );
        assert!(
            controller
                .snapshot()
                .messages
                .iter()
                .any(|message| matches!(
                    &message.payload,
                    ConversationPayload::ToolResult { text, tool_call_id, is_error: false }
                        if text == "第一个工具已完成。"
                            && tool_call_id.as_deref() == Some("call-1")
                ))
        );
        assert!(
            controller
                .snapshot()
                .messages
                .iter()
                .any(|message| matches!(
                    &message.payload,
                    ConversationPayload::ToolResult { tool_call_id, text, is_error: true }
                        if tool_call_id.as_deref() == Some("call-2")
                            && text.contains("No automatic Provider retry")
                ))
        );
        let failed_record = controller
            .snapshot()
            .records
            .iter()
            .rev()
            .find(|record| {
                record
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("tool_call_id").and_then(Value::as_str))
                    == Some("call-2")
            })
            .and_then(|record| record.metadata.as_ref())
            .expect("failed tool record should retain host diagnostics");
        assert_eq!(failed_record["kind"], "tool_execution_error");
        assert_eq!(failed_record["retryable"], false);
        assert_eq!(failed_record["possible_external_effects"], true);
    });
}

/// Verifies a durable batch-preparation failure rejects every call without aborting the Provider loop.
/// 验证可靠批次准备失败会拒绝每个调用，而不会中断 Provider 循环。
#[test]
fn run_user_turn_streaming_retains_batch_preparation_failure_as_tool_results() {
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        let adapter = MockStreamingProviderAdapter::new(vec![
            (
                Vec::new(),
                ProviderTurnResponse {
                    assistant_text: String::new(),
                    assistant_reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-prepare-1")),
                    provider_message_state: None,
                    tool_calls: vec![ConversationToolCall {
                        id: String::from("tool-item-prepare"),
                        tool_call_id: Some(String::from("call-prepare")),
                        tool_name: String::from("prepared_tool"),
                        arguments: json!({"step": 1}),
                        argument_parse_error: None,
                        raw_arguments: Default::default(),
                    }],
                    continuation: None,
                    usage: None,
                },
            ),
            (
                Vec::new(),
                ProviderTurnResponse {
                    assistant_text: String::from("已收到准备失败。"),
                    assistant_reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-prepare-2")),
                    provider_message_state: None,
                    tool_calls: Vec::new(),
                    continuation: None,
                    usage: None,
                },
            ),
        ]);
        let executor = MockToolExecutor::new().with_preparation_failure("模拟恢复边界持久化失败");
        let mut controller =
            VulcanCoreController::new(build_test_config(4), ConversationSnapshot::default())
                .expect("valid core conversation snapshot");

        let result = controller
            .run_user_turn_streaming(&adapter, &executor, &AllowAllToolPolicy, "请调用工具")
            .await;

        assert!(result.is_ok());
        assert!(executor.executed_requests().is_empty());
        let failed_record = controller
            .snapshot()
            .records
            .iter()
            .rev()
            .find_map(|record| record.metadata.as_ref())
            .expect("batch preparation failure should retain host diagnostics");
        assert_eq!(failed_record["kind"], "tool_batch_preparation_error");
        assert_eq!(failed_record["executed"], false);
        assert_eq!(failed_record["possible_external_effects"], false);
        assert_eq!(failed_record["retryable"], false);
    });
}

/// Emits one stable checkpoint after every result in a successful tool batch has been appended.
/// 在成功工具批次的全部结果均已追加后，仅发出一个稳定检查点。
#[test]
fn run_user_turn_streaming_checkpoints_complete_tool_batch_once() {
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        let adapter = MockStreamingProviderAdapter::new(vec![
            (
                Vec::new(),
                ProviderTurnResponse {
                    assistant_text: String::new(),
                    assistant_reasoning: String::new(),
                    provider_message_id: Some(String::from("response-tools")),
                    provider_message_state: None,
                    tool_calls: vec![
                        ConversationToolCall {
                            id: String::from("item-1"),
                            tool_call_id: Some(String::from("call-1")),
                            tool_name: String::from("first_tool"),
                            arguments: json!({}),
                            argument_parse_error: None,
                            raw_arguments: Default::default(),
                        },
                        ConversationToolCall {
                            id: String::from("item-2"),
                            tool_call_id: Some(String::from("call-2")),
                            tool_name: String::from("second_tool"),
                            arguments: json!({}),
                            argument_parse_error: None,
                            raw_arguments: Default::default(),
                        },
                    ],
                    continuation: None,
                    usage: None,
                },
            ),
            (
                Vec::new(),
                ProviderTurnResponse {
                    assistant_text: String::from("done"),
                    assistant_reasoning: String::new(),
                    provider_message_id: Some(String::from("response-final")),
                    provider_message_state: None,
                    tool_calls: Vec::new(),
                    continuation: None,
                    usage: None,
                },
            ),
        ]);
        let executor = MockToolExecutor::new()
            .with_outcome("first_tool", "first", None, false)
            .with_outcome("second_tool", "second", None, false);
        let mut controller =
            VulcanCoreController::new(build_test_config(4), ConversationSnapshot::default())
                .expect("valid core conversation snapshot");
        let mut checkpoints = Vec::<ConversationSnapshot>::new();

        controller
            .run_user_turn_streaming_with_checkpoints(
                &adapter,
                &executor,
                &AllowAllToolPolicy,
                "run both tools",
                |snapshot, _| checkpoints.push(snapshot.clone()),
            )
            .await
            .expect("successful tool batch should continue");

        assert_eq!(checkpoints.len(), 3);
        let stable_tool_checkpoint = &checkpoints[1];
        let tool_result_count = stable_tool_checkpoint
            .messages
            .iter()
            .filter(|message| matches!(message.payload, ConversationPayload::ToolResult { .. }))
            .count();
        assert_eq!(tool_result_count, 2);
    });
}
