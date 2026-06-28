use super::*;

/// Verifies one provider tool-call batch executes concurrently while preserving ordered tool-result replay.
/// 验证单个 Provider 工具调用批次会并发执行，同时保留有序工具结果回放。
#[test]
fn run_user_turn_streaming_executes_tool_batch_concurrently() {
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        let adapter = MockStreamingProviderAdapter::new(vec![
            (
                Vec::new(),
                ProviderTurnResponse {
                    assistant_text: String::new(),
                    assistant_reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-batch-concurrent")),
                    provider_message_state: None,
                    tool_calls: vec![
                        ConversationToolCall {
                            id: String::from("tool-item-a"),
                            tool_call_id: Some(String::from("call-a")),
                            tool_name: String::from("slow_a"),
                            arguments: json!({"step": "a"}),
                        },
                        ConversationToolCall {
                            id: String::from("tool-item-b"),
                            tool_call_id: Some(String::from("call-b")),
                            tool_name: String::from("slow_b"),
                            arguments: json!({"step": "b"}),
                        },
                    ],
                    continuation: None,
                    usage: None,
                },
            ),
            (
                Vec::new(),
                ProviderTurnResponse {
                    assistant_text: String::from("两个工具已完成。"),
                    assistant_reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-final")),
                    provider_message_state: None,
                    tool_calls: Vec::new(),
                    continuation: None,
                    usage: None,
                },
            ),
        ]);
        let executor = ConcurrentToolExecutor::default();
        let mut controller =
            VulcanCoreController::new(build_test_config(4), ConversationSnapshot::default());

        let result = controller
            .run_user_turn_streaming(&adapter, &executor, &AllowAllToolPolicy, "并发调用两个工具")
            .await
            .expect("run concurrent tool batch");

        assert_eq!(result.tool_calls_processed, 2);
        assert_eq!(executor.max_running.load(Ordering::SeqCst), 2);
        let tool_result_ids = controller
            .snapshot()
            .messages
            .iter()
            .filter_map(|message| match &message.payload {
                ConversationPayload::ToolResult { tool_call_id, .. } => tool_call_id.as_deref(),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(tool_result_ids, vec!["call-a", "call-b"]);
    });
}

/// Generates a synthetic skipped tool result when the host policy refuses execution and continues the loop.
/// 当宿主策略拒绝执行时生成一条合成跳过工具结果，并继续推进后续循环。
#[test]
fn run_user_turn_synthesizes_skipped_tool_result_when_policy_blocks_tool() {
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        let adapter = MockProviderAdapter::new(vec![
            ProviderTurnResponse {
                assistant_text: String::new(),
                assistant_reasoning: String::new(),
                provider_message_id: Some(String::from("resp-skip-1")),
                provider_message_state: None,
                tool_calls: vec![ConversationToolCall {
                    id: String::from("tool-item-skip-1"),
                    tool_call_id: Some(String::from("call-skip-1")),
                    tool_name: String::from("blocked_tool"),
                    arguments: json!({"id":"42"}),
                }],
                continuation: None,
                usage: None,
            },
            ProviderTurnResponse {
                assistant_text: String::from("已收到宿主返回的跳过结果。"),
                assistant_reasoning: String::new(),
                provider_message_id: Some(String::from("resp-skip-2")),
                provider_message_state: None,
                tool_calls: Vec::new(),
                continuation: None,
                usage: None,
            },
        ]);
        let executor = MockToolExecutor::new();
        let policy = ExactToolSetPolicy::new(["allowed_tool"]);
        let mut controller =
            VulcanCoreController::new(build_test_config(3), ConversationSnapshot::default());

        let result = controller
            .run_user_turn(&adapter, &executor, &policy, "请调用被限制的工具")
            .await
            .expect("run blocked tool turn");

        assert_eq!(result.provider_turns_executed, 2);
        assert_eq!(result.tool_calls_processed, 1);
        assert!(executor.executed_requests().is_empty());
        assert!(matches!(
            &controller.snapshot().messages[2].payload,
            ConversationPayload::ToolResult { text, .. }
                if text == "Tool execution was skipped by the host policy."
        ));
        assert!(controller.snapshot().records.iter().any(|record| {
            record.title == "调用 blocked_tool 工具"
                && record.body == "Tool execution was skipped by the host policy."
        }));
    });
}

/// Verifies the controller falls back to the local tool-call identifier when the provider-visible id is empty.
/// 验证当 Provider 可见 id 为空时，控制器会回退到本地工具调用标识。
#[test]
fn resolved_tool_call_id_falls_back_when_provider_visible_id_is_empty() {
    let tool_call = crate::types::ConversationToolCall {
        id: String::from("local-call-1"),
        tool_call_id: Some(String::from("   ")),
        tool_name: String::from("read_file"),
        arguments: json!({ "path": "src/main.rs" }),
    };

    assert_eq!(resolved_tool_call_id(&tool_call), "local-call-1");
}
