use super::*;

/// Executes a full host-owned multi-turn loop and persists assistant, tool, and continuation state.
/// 执行一条完整的宿主自有多轮循环，并持久化助手、工具与 continuation 状态。
#[test]
fn run_user_turn_executes_multi_turn_loop_and_refreshes_continuation() {
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        let adapter = MockProviderAdapter::new(vec![
            ProviderTurnResponse {
                assistant_text: String::new(),
                assistant_reasoning: String::from("先读取文件，再整理结论。"),
                provider_message_id: Some(String::from("resp-1")),
                provider_message_state: None,
                tool_calls: vec![ConversationToolCall {
                    id: String::from("tool-item-1"),
                    tool_call_id: Some(String::from("call-1")),
                    tool_name: String::from("read_file"),
                    arguments: json!({"path":"src/main.rs"}),
                    argument_parse_error: None,
                }],
                continuation: Some(ProviderContinuation {
                    provider_id: Some(String::from("mock-openai")),
                    model_id: Some(String::from("mock-model")),
                    last_provider_message_id: Some(String::from("resp-1")),
                    updated_at: Some(String::from("1000")),
                    ..ProviderContinuation::default()
                }),
                usage: None,
            },
            ProviderTurnResponse {
                assistant_text: String::from("文件检查完成，已经拿到关键上下文。"),
                assistant_reasoning: String::new(),
                provider_message_id: Some(String::from("resp-2")),
                provider_message_state: None,
                tool_calls: Vec::new(),
                continuation: Some(ProviderContinuation {
                    provider_id: Some(String::from("mock-openai")),
                    model_id: Some(String::from("mock-model")),
                    last_provider_message_id: Some(String::from("resp-2")),
                    updated_at: Some(String::from("2000")),
                    ..ProviderContinuation::default()
                }),
                usage: None,
            },
        ]);
        let executor = MockToolExecutor::new().with_outcome(
            "read_file",
            "已读取 src/main.rs 的摘要内容。",
            Some(json!({"kind":"file_read","payload":{"path":"src/main.rs"}})),
            false,
        );
        let mut controller =
            VulcanCoreController::new(build_test_config(4), ConversationSnapshot::default()).expect("valid core conversation snapshot");

        let result = controller
            .run_user_turn(&adapter, &executor, &AllowAllToolPolicy, "请检查主入口文件")
            .await
            .expect("run user turn");

        assert_eq!(result.provider_turns_executed, 2);
        assert_eq!(result.tool_calls_processed, 1);
        assert_eq!(controller.phase(), ControllerPhase::Completed);
        assert_eq!(controller.snapshot().messages.len(), 4);
        assert_eq!(
            controller
                .snapshot()
                .continuation
                .last_provider_message_id
                .as_deref(),
            Some("resp-2")
        );
        assert_eq!(
            controller
                .snapshot()
                .continuation
                .last_local_message_id
                .as_deref(),
            result.final_assistant_message_id.as_deref()
        );
        assert_eq!(executor.executed_requests().len(), 1);

        let built_requests = adapter.built_requests();
        assert_eq!(built_requests.len(), 2);
        assert_eq!(built_requests[0].history.len(), 1);
        assert_eq!(built_requests[1].history.len(), 3);
        assert_eq!(
            built_requests[1]
                .continuation
                .last_provider_message_id
                .as_deref(),
            Some("resp-1")
        );
        assert_eq!(
            built_requests[1]
                .continuation
                .last_local_message_id
                .as_deref(),
            Some(controller.snapshot().messages[1].id.as_str())
        );

        assert!(matches!(
            &controller.snapshot().messages[1].payload,
            ConversationPayload::Assistant { tool_calls, .. } if tool_calls.len() == 1
        ));
        assert!(matches!(
            &controller.snapshot().messages[2].payload,
            ConversationPayload::ToolResult { text, .. } if text == "已读取 src/main.rs 的摘要内容。"
        ));
        assert!(controller.snapshot().records.iter().any(|record| {
            record.title == "调用 read_file 工具"
                && record
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("kind"))
                    .and_then(Value::as_str)
                    == Some("file_read")
        }));
        assert!(controller.events().iter().any(|event| {
            matches!(event.kind, CoreEventKind::Final)
                && event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("provider_turns_executed"))
                    .and_then(Value::as_u64)
                    == Some(2)
        }));
    });
}

/// Continues sampling when the provider explicitly marks a tool-free assistant message as non-terminal.
/// 当供应商明确将无工具调用的助理消息标记为非终态时，继续执行下一次采样。
#[test]
fn run_user_turn_respects_explicit_end_turn_false_without_tools() {
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        let adapter = MockProviderAdapter::new(vec![
            ProviderTurnResponse {
                assistant_text: String::from("Still working."),
                assistant_reasoning: String::new(),
                provider_message_id: Some(String::from("resp-progress")),
                provider_message_state: Some(json!({
                    "vulcan_ai": {
                        "turn_boundary": {
                            "end_turn": false
                        }
                    }
                })),
                tool_calls: Vec::new(),
                continuation: None,
                usage: None,
            },
            ProviderTurnResponse {
                assistant_text: String::from("Finished."),
                assistant_reasoning: String::new(),
                provider_message_id: Some(String::from("resp-final")),
                provider_message_state: Some(json!({
                    "vulcan_ai": {
                        "turn_boundary": {
                            "end_turn": true
                        }
                    }
                })),
                tool_calls: Vec::new(),
                continuation: None,
                usage: None,
            },
        ]);
        let executor = MockToolExecutor::new();
        let mut controller =
            VulcanCoreController::new(build_test_config(3), ConversationSnapshot::default())
                .expect("valid core conversation snapshot");

        let result = controller
            .run_user_turn(
                &adapter,
                &executor,
                &AllowAllToolPolicy,
                "Continue until the provider ends the turn.",
            )
            .await
            .expect("run user turn");

        assert_eq!(result.provider_turns_executed, 2);
        assert_eq!(result.tool_calls_processed, 0);
        assert_eq!(controller.phase(), ControllerPhase::Completed);
        assert_eq!(adapter.built_requests().len(), 2);
        assert!(matches!(
            controller.snapshot().messages.last().map(|message| &message.payload),
            Some(ConversationPayload::Assistant { text, .. }) if text == "Finished."
        ));
    });
}

/// Clears a stale provider continuation when the latest response does not expose a reusable cursor.
/// 当最新响应未提供可复用游标时，清除陈旧的 Provider continuation。
#[test]
fn refresh_continuation_clears_stale_cursor_when_response_has_none() {
    let mut controller = VulcanCoreController::new(
        build_test_config(2),
        ConversationSnapshot {
            continuation: ProviderContinuation {
                provider_id: Some(String::from("mock-openai")),
                model_id: Some(String::from("mock-model")),
                last_provider_message_id: Some(String::from("resp-stale")),
                last_local_message_id: Some(String::from("assistant-stale")),
                updated_at: Some(String::from("1000")),
                ..ProviderContinuation::default()
            },
            ..ConversationSnapshot::default()
        },
    )
    .expect("valid core conversation snapshot");
    controller.refresh_continuation(
        &ProviderTurnResponse {
            assistant_text: String::from("最新响应未返回 continuation 游标。"),
            assistant_reasoning: String::new(),
            provider_message_id: None,
            provider_message_state: None,
            tool_calls: Vec::new(),
            continuation: None,
            usage: None,
        },
        "assistant-new",
    );

    let continuation = &controller.snapshot().continuation;
    assert!(continuation.provider_id.is_none());
    assert!(continuation.model_id.is_none());
    assert!(continuation.last_provider_message_id.is_none());
    assert!(continuation.last_local_message_id.is_none());
    assert!(continuation.updated_at.is_none());
}

/// Reuses the host-preseeded latest user prompt during streaming execution so the canonical snapshot does not duplicate one turn boundary.
/// 在流式执行期间复用宿主预播种的最新用户提示词，从而避免规范快照重复记录同一轮次边界。
#[test]
fn run_user_turn_streaming_reuses_host_preseeded_user_prompt() {
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        let adapter = MockStreamingProviderAdapter::new(vec![(
            vec![ProviderTurnDelta::Output {
                delta: String::from("已完成分析。"),
            }],
            ProviderTurnResponse {
                assistant_text: String::from("已完成分析。"),
                assistant_reasoning: String::new(),
                provider_message_id: Some(String::from("resp-stream-2")),
                provider_message_state: None,
                tool_calls: Vec::new(),
                continuation: None,
                usage: None,
            },
        )]);
        let executor = MockToolExecutor::new();
        let mut controller = VulcanCoreController::new(
            build_test_config(2),
            ConversationSnapshot {
                messages: vec![ConversationMessage {
                    id: String::from("host-user-1"),
                    payload: ConversationPayload::UserText {
                        text: String::from("请先分析文件"),
                        attachments: Vec::new(),
                    },
                }],
                records: vec![ConversationRecord {
                    id: String::from("host-user-record-1"),
                    role: TranscriptRole::User,
                    record_kind: TranscriptRecordKind::Visible,
                    visible: true,
                    title: String::from("用户输入"),
                    body: String::from("请先分析文件"),
                    metadata: None,
                    created_at: String::from("1000"),
                }],
                continuation: ProviderContinuation::default(),
            },
        )
        .expect("valid core conversation snapshot");

        let result = controller
            .run_user_turn_streaming(&adapter, &executor, &AllowAllToolPolicy, "请先分析文件")
            .await
            .expect("run streaming user turn");

        assert_eq!(result.provider_turns_executed, 1);
        assert_eq!(controller.snapshot().messages.len(), 2);
        assert_eq!(
            controller
                .snapshot()
                .messages
                .iter()
                .filter(|message| matches!(message.payload, ConversationPayload::UserText { .. }))
                .count(),
            1
        );
    });
}

/// Starts a streaming provider turn from a preseeded agent message without adding a user prompt.
/// 从已预播种的智能体消息启动流式 Provider 回合，不追加用户提示词。
#[test]
fn run_seeded_turn_streaming_uses_preseeded_agent_message() {
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        let adapter = MockStreamingProviderAdapter::new(vec![(
            vec![ProviderTurnDelta::Output {
                delta: String::from("收到智能体结果。"),
            }],
            ProviderTurnResponse {
                assistant_text: String::from("收到智能体结果。"),
                assistant_reasoning: String::new(),
                provider_message_id: Some(String::from("resp-seeded-1")),
                provider_message_state: None,
                tool_calls: Vec::new(),
                continuation: None,
                usage: None,
            },
        )]);
        let executor = MockToolExecutor::new();
        let mut controller =
            VulcanCoreController::new(build_test_config(2), ConversationSnapshot::default())
                .expect("valid core conversation snapshot");
        controller.append_agent_message(AgentMessageAppendRequest {
            message_type: String::from("FINAL_ANSWER"),
            task_name: String::from("quick_agent"),
            sender: String::from("quick_agent"),
            recipient: String::from("parent"),
            payload: String::from("abc"),
            trigger_turn: true,
            metadata: None,
        });

        let result = controller
            .run_seeded_turn_streaming_with_checkpoints_and_interventions(
                &adapter,
                &executor,
                &AllowAllToolPolicy,
                |_, _| {},
                |_| Ok(false),
            )
            .await
            .expect("run seeded streaming turn");

        assert_eq!(result.provider_turns_executed, 1);
        assert_eq!(controller.snapshot().messages.len(), 2);
        assert_eq!(
            controller
                .snapshot()
                .messages
                .iter()
                .filter(|message| matches!(message.payload, ConversationPayload::UserText { .. }))
                .count(),
            0
        );
        let built_requests = adapter.built_requests();
        assert_eq!(built_requests.len(), 1);
        assert!(matches!(
            &built_requests[0].history[0].payload,
            ConversationPayload::AgentMessage { payload, .. } if payload == "abc"
        ));
    });
}

/// Resumes a seeded streaming turn from a committed tool result without appending the original user prompt again.
/// 从已提交工具结果恢复播种流式回合，且不再次追加原始用户提示词。
#[test]
fn run_seeded_turn_streaming_resumes_from_tool_result() {
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        // firstAdapter creates one committed assistant tool call before the configured turn limit stops the original controller.
        // firstAdapter 在配置的回合限制终止原控制器前创建一条已提交的助手工具调用。
        let first_adapter = MockProviderAdapter::new(vec![ProviderTurnResponse {
            assistant_text: String::new(),
            assistant_reasoning: String::new(),
            provider_message_id: Some(String::from("resp-tool-before-retry")),
            provider_message_state: None,
            tool_calls: vec![ConversationToolCall {
                id: String::from("tool-item-before-retry"),
                tool_call_id: Some(String::from("call-before-retry")),
                tool_name: String::from("read_file"),
                arguments: json!({"path":"src/lib.rs"}),
                argument_parse_error: None,
            }],
            continuation: None,
            usage: None,
        }]);
        let executor =
            MockToolExecutor::new().with_outcome("read_file", "已读取 src/lib.rs。", None, false);
        let mut first_controller =
            VulcanCoreController::new(build_test_config(1), ConversationSnapshot::default())
                .expect("valid initial core conversation snapshot");
        first_controller
            .run_user_turn(
                &first_adapter,
                &executor,
                &AllowAllToolPolicy,
                "请读取文件后继续",
            )
            .await
            .expect_err("turn limit should preserve the committed tool boundary");
        let retry_snapshot = first_controller.snapshot().clone();
        assert!(matches!(
            retry_snapshot.messages.last().map(|message| &message.payload),
            Some(ConversationPayload::ToolResult { tool_call_id, .. })
                if tool_call_id.as_deref() == Some("call-before-retry")
        ));

        // retryAdapter returns the final response from the exact committed tool boundary.
        // retryAdapter 从精确的已提交工具边界返回最终响应。
        let retry_adapter = MockStreamingProviderAdapter::new(vec![(
            vec![ProviderTurnDelta::Output {
                delta: String::from("已继续处理。"),
            }],
            ProviderTurnResponse {
                assistant_text: String::from("已继续处理。"),
                assistant_reasoning: String::new(),
                provider_message_id: Some(String::from("resp-after-retry")),
                provider_message_state: None,
                tool_calls: Vec::new(),
                continuation: None,
                usage: None,
            },
        )]);
        let original_message_count = retry_snapshot.messages.len();
        let mut retry_controller = VulcanCoreController::new(build_test_config(2), retry_snapshot)
            .expect("valid retry core conversation snapshot");
        retry_controller
            .run_seeded_turn_streaming_with_checkpoints_and_interventions(
                &retry_adapter,
                &executor,
                &AllowAllToolPolicy,
                |_, _| {},
                |_| Ok(false),
            )
            .await
            .expect("resume seeded streaming turn from tool result");

        assert_eq!(
            retry_controller.snapshot().messages.len(),
            original_message_count + 1
        );
        let built_requests = retry_adapter.built_requests();
        assert_eq!(built_requests.len(), 1);
        assert!(matches!(
            built_requests[0].history.last().map(|message| &message.payload),
            Some(ConversationPayload::ToolResult { tool_call_id, .. })
                if tool_call_id.as_deref() == Some("call-before-retry")
        ));
    });
}

/// Build a retained user/assistant boundary; text and reasoning identify the interrupted output.
/// 构建保留的用户／助手边界；文本和推理参数指定中断输出。
/// Returns an independent canonical snapshot without adding another user message.
/// 返回独立规范快照，不增加另一条用户消息。
fn retained_assistant_snapshot(text: &str, reasoning: &str) -> ConversationSnapshot {
    ConversationSnapshot {
        messages: vec![
            ConversationMessage {
                id: "continue-user".into(),
                payload: ConversationPayload::UserText {
                    text: "Inspect this source".into(),
                    attachments: Vec::new(),
                },
            },
            ConversationMessage {
                id: "continue-partial-assistant".into(),
                payload: ConversationPayload::Assistant {
                    text: text.into(),
                    reasoning: reasoning.into(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            },
        ],
        ..Default::default()
    }
}

/// A host-admitted retained loop may preserve partial output, while ordinary seeded input stays strict.
/// 宿主已准入的保留循环可以保留部分输出，普通播种输入仍保持严格校验。
/// No inputs; verifies text-only and reasoning-only tails and the exact next request history.
/// 无输入；验证纯文本、纯推理尾部及下一请求的精确历史。
#[test]
fn run_restored_provider_loop_preserves_partial_assistant_without_new_input() {
    // Use a single runtime worker; the test requires no shared service or network.
    // 使用单个运行时线程；测试不需要共享服务或网络。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        for (text, reasoning) in [("Partial answer", ""), ("", "Partial reasoning")] {
            // Both entrypoints receive the same immutable original history.
            // 两个入口接收相同的不可变原始历史。
            let snapshot = retained_assistant_snapshot(text, reasoning);
            let adapter = MockStreamingProviderAdapter::new(vec![(
                Vec::new(),
                ProviderTurnResponse {
                    assistant_text: "Continued answer".into(),
                    assistant_reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    tool_calls: Vec::new(),
                    continuation: None,
                    usage: None,
                },
            )]);
            let executor = MockToolExecutor::new();
            let mut seeded =
                VulcanCoreController::new(build_test_config(2), snapshot.clone()).unwrap();
            assert!(
                seeded
                    .run_seeded_turn_streaming_with_checkpoints_and_interventions(
                        &adapter,
                        &executor,
                        &AllowAllToolPolicy,
                        |_, _| {},
                        |_| Ok(false),
                    )
                    .await
                    .is_err()
            );
            assert!(adapter.built_requests().is_empty());
            let mut restored =
                VulcanCoreController::new(build_test_config(2), snapshot.clone()).unwrap();
            let result = restored
                .run_restored_provider_loop_streaming_with_checkpoints_and_interventions(
                    &adapter,
                    &executor,
                    &AllowAllToolPolicy,
                    None,
                    |_, _| {},
                    |_| Ok(false),
                )
                .await
                .expect("continue from retained assistant output");
            assert_eq!(result.provider_turns_executed, 1);
            assert_eq!(adapter.built_requests()[0].history, snapshot.messages);
            assert_eq!(
                restored.snapshot().messages.len(),
                snapshot.messages.len() + 1
            );
        }
    });
}

/// Reject empty, assistant-only and unresolved-tool recovery before the model is invoked.
/// 在调用模型前拒绝空历史、仅助手历史和未解决工具的恢复。
/// No inputs; verifies invalid retained boundaries cannot bypass recovery validation.
/// 无输入；验证非法保留边界不能绕过恢复校验。
#[test]
fn run_restored_provider_loop_rejects_invalid_retained_boundaries() {
    // Each case owns its history so failures cannot mutate another case.
    // 每个用例独立持有历史，失败不能修改其他用例。
    let mut assistant_only = retained_assistant_snapshot("Partial answer", "");
    assistant_only.messages.remove(0);
    let mut unresolved = retained_assistant_snapshot("Partial answer", "");
    if let ConversationPayload::Assistant { tool_calls, .. } = &mut unresolved.messages[1].payload {
        tool_calls.push(ConversationToolCall {
            id: "pending-item".into(),
            tool_call_id: Some("pending-call".into()),
            tool_name: "read_file".into(),
            arguments: json!({"path":"src/lib.rs"}),
            argument_parse_error: None,
        });
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        for snapshot in [
            ConversationSnapshot::default(),
            assistant_only,
            unresolved,
            retained_assistant_snapshot("", ""),
        ] {
            let adapter = MockStreamingProviderAdapter::new(Vec::new());
            let executor = MockToolExecutor::new();
            let mut controller = VulcanCoreController::new(build_test_config(2), snapshot).unwrap();
            assert!(
                controller
                    .run_restored_provider_loop_streaming_with_checkpoints_and_interventions(
                        &adapter,
                        &executor,
                        &AllowAllToolPolicy,
                        None,
                        |_, _| {},
                        |_| Ok(false),
                    )
                    .await
                    .is_err()
            );
            assert!(adapter.built_requests().is_empty());
        }
    });
}

/// Stops the host-owned loop with a controller error once the configured provider-turn budget has been exhausted.
/// 一旦达到已配置的 Provider 回合预算上限，就以控制器错误终止宿主自有循环。
#[test]
fn run_user_turn_fails_when_provider_turn_budget_is_exhausted() {
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        let adapter = MockProviderAdapter::new(vec![ProviderTurnResponse {
            assistant_text: String::new(),
            assistant_reasoning: String::new(),
            provider_message_id: Some(String::from("resp-limit-1")),
            provider_message_state: None,
            tool_calls: vec![ConversationToolCall {
                id: String::from("tool-item-limit-1"),
                tool_call_id: Some(String::from("call-limit-1")),
                tool_name: String::from("read_file"),
                arguments: json!({"path":"src/lib.rs"}),
                argument_parse_error: None,
            }],
            continuation: None,
            usage: None,
        }]);
        let executor =
            MockToolExecutor::new().with_outcome("read_file", "已读取 src/lib.rs。", None, false);
        let mut controller =
            VulcanCoreController::new(build_test_config(1), ConversationSnapshot::default())
                .expect("valid core conversation snapshot");

        let error = controller
            .run_user_turn(&adapter, &executor, &AllowAllToolPolicy, "请继续检查")
            .await
            .expect_err("expect provider turn budget failure");

        assert!(matches!(error, CoreError::InvalidState { .. }));
        assert_eq!(controller.phase(), ControllerPhase::Failed);
        assert!(controller.events().iter().any(|event| {
            matches!(event.kind, CoreEventKind::Error)
                && event.body.contains("最大 Provider 回合限制")
        }));
        assert_eq!(controller.snapshot().messages.len(), 3);
    });
}
