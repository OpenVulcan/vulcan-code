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
                }],
                continuation: Some(ProviderContinuation {
                    provider_id: Some(String::from("mock-openai")),
                    model_id: Some(String::from("mock-model")),
                    last_provider_message_id: Some(String::from("resp-1")),
                    updated_at: Some(String::from("1000")),
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
