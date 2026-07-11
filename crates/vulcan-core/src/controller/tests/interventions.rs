use super::*;

/// Interjects host guidance after one streaming tool result before building the next provider request.
/// 在一次流式工具结果之后、构建下一次 Provider 请求之前插入宿主引导。
#[test]
fn run_user_turn_streaming_intervention_reaches_next_provider_request() {
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        let adapter = MockStreamingProviderAdapter::new(vec![
            (
                vec![ProviderTurnDelta::Thinking {
                    delta: String::from("先生成素材。"),
                }],
                ProviderTurnResponse {
                    assistant_text: String::new(),
                    assistant_reasoning: String::from("先生成素材。"),
                    provider_message_id: Some(String::from("resp-intervention-1")),
                    provider_message_state: None,
                    tool_calls: vec![ConversationToolCall {
                        id: String::from("tool-item-image"),
                        tool_call_id: Some(String::from("call-image")),
                        tool_name: String::from("image_generation"),
                        arguments: json!({"prompt": "wanderer cover"}),
                    }],
                    continuation: Some(ProviderContinuation {
                        provider_id: Some(String::from("mock-openai-streaming")),
                        model_id: Some(String::from("mock-model")),
                        last_provider_message_id: Some(String::from("resp-intervention-1")),
                        updated_at: Some(String::from("4001")),
                    }),
                    usage: None,
                },
            ),
            (
                vec![ProviderTurnDelta::Output {
                    delta: String::from("已遵循引导。"),
                }],
                ProviderTurnResponse {
                    assistant_text: String::from("已遵循引导。"),
                    assistant_reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-intervention-2")),
                    provider_message_state: None,
                    tool_calls: Vec::new(),
                    continuation: Some(ProviderContinuation {
                        provider_id: Some(String::from("mock-openai-streaming")),
                        model_id: Some(String::from("mock-model")),
                        last_provider_message_id: Some(String::from("resp-intervention-2")),
                        updated_at: Some(String::from("4002")),
                    }),
                    usage: None,
                },
            ),
        ]);
        let executor = MockToolExecutor::new().with_outcome(
            "image_generation",
            "图片已生成。",
            Some(json!({"file": "wanderer_cover.jpg"})),
            false,
        );
        let mut controller =
            VulcanCoreController::new(build_test_config(4), ConversationSnapshot::default()).expect("valid core conversation snapshot");
        let mut checkpoints = Vec::<ConversationSnapshot>::new();
        let mut injected = false;
        let guided_attachment = ConversationAttachment {
            kind: crate::types::ConversationAttachmentKind::Image,
            file_name: String::from("reference.png"),
            local_path: String::from("D:/tmp/reference.png"),
            source_path: None,
            asset_id: Some(String::from("asset-reference")),
            byte_len: Some(42),
            preview_local_path: None,
            mime_type: String::from("image/png"),
        };

        let result = controller
            .run_user_turn_streaming_with_checkpoints_and_interventions(
                &adapter,
                &executor,
                &AllowAllToolPolicy,
                "生成一张图",
                |snapshot, _| checkpoints.push(snapshot.clone()),
                |controller| {
                    if injected {
                        return Ok(false);
                    }
                    injected = true;
                    Ok(controller
                        .append_interjected_user_turn_with_attachments(
                            "不需要配图了",
                            vec![guided_attachment.clone()],
                        )
                        .is_some())
                },
            )
            .await
            .expect("run guided streaming user turn");

        assert_eq!(result.provider_turns_executed, 2);
        assert_eq!(result.tool_calls_processed, 1);
        assert_eq!(checkpoints.len(), 2);
        assert!(matches!(
            checkpoints[1].messages.last(),
            Some(ConversationMessage {
                payload: ConversationPayload::UserText { text, attachments },
                ..
            }) if text == "不需要配图了" && attachments == &vec![guided_attachment.clone()]
        ));
        assert!(matches!(
            checkpoints[1].records.last(),
            Some(ConversationRecord {
                title,
                metadata: Some(metadata),
                ..
            }) if title == "用户指令"
                && metadata.get("canonical_user_text").and_then(Value::as_str) == Some("不需要配图了")
                && metadata.get("source").and_then(Value::as_str) == Some("user_interrupt")
                && metadata.get("starts_user_turn").and_then(Value::as_bool) == Some(false)
                && metadata.get("turn_process").and_then(Value::as_bool) == Some(true)
                && metadata.get("collapsed_by_default").and_then(Value::as_bool) == Some(true)
                && metadata.get("editable").and_then(Value::as_bool) == Some(false)
                && metadata.get("transcript_attachments").is_some()
        ));

        let built_requests = adapter.built_requests();
        assert_eq!(built_requests.len(), 2);
        assert!(matches!(
            built_requests[1].history.last(),
            Some(ConversationMessage {
                payload: ConversationPayload::UserText { text, attachments },
                ..
            }) if text == "不需要配图了" && attachments == &vec![guided_attachment]
        ));
    });
}

/// Verifies intervention errors discard callback mutations and fail the controller explicitly.
/// 验证干预错误会丢弃回调修改并显式使控制器失败。
#[test]
fn streaming_intervention_error_discards_callback_draft() {
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        // adapter yields one tool boundary so the failing intervention is invoked exactly once.
        // adapter 产生一个工具边界，使失败干预恰好调用一次。
        let adapter = MockStreamingProviderAdapter::new(vec![(
            Vec::new(),
            ProviderTurnResponse {
                assistant_text: String::new(),
                assistant_reasoning: String::new(),
                provider_message_id: Some(String::from("resp-error-1")),
                provider_message_state: None,
                tool_calls: vec![ConversationToolCall {
                    id: String::from("tool-error"),
                    tool_call_id: Some(String::from("call-error")),
                    tool_name: String::from("read_file"),
                    arguments: json!({"path": "README.md"}),
                }],
                continuation: None,
                usage: None,
            },
        )]);
        let executor = MockToolExecutor::new().with_outcome("read_file", "content", None, false);
        let mut controller =
            VulcanCoreController::new(build_test_config(2), ConversationSnapshot::default())
                .expect("valid core conversation snapshot");

        let error = controller
            .run_user_turn_streaming_with_checkpoints_and_interventions(
                &adapter,
                &executor,
                &AllowAllToolPolicy,
                "read the file",
                |_, _| {},
                |controller| {
                    controller.append_interjected_user_turn("must be rolled back");
                    Err(CoreError::InvalidState {
                        message: String::from("intervention failed"),
                    })
                },
            )
            .await
            .expect_err("intervention error must stop the controller");

        assert!(error.to_string().contains("intervention failed"));
        assert_eq!(controller.phase(), ControllerPhase::Failed);
        assert!(!controller.snapshot().messages.iter().any(|message| {
            matches!(
                &message.payload,
                ConversationPayload::UserText { text, .. } if text == "must be rolled back"
            )
        }));
        assert!(
            !controller
                .snapshot()
                .records
                .iter()
                .any(|record| record.body == "must be rolled back")
        );
    });
}

/// Verifies no-attachment interjected user turns still carry process metadata.
/// 验证无附件插队用户回合仍然携带过程元数据。
#[test]
fn interjected_user_turn_without_attachments_keeps_process_metadata() {
    let mut controller =
        VulcanCoreController::new(build_test_config(4), ConversationSnapshot::default())
            .expect("valid core conversation snapshot");

    let message_id = controller
        .append_interjected_user_turn_with_attachments("停止当前循环", Vec::new())
        .expect("interjected message id");

    assert!(message_id.starts_with("user-message-"));
    assert!(matches!(
        controller.snapshot().records.last(),
        Some(ConversationRecord {
            title,
            metadata: Some(metadata),
            ..
        }) if title == "用户指令"
            && metadata.get("canonical_user_text").and_then(Value::as_str) == Some("停止当前循环")
            && metadata.get("source").and_then(Value::as_str) == Some("user_interrupt")
            && metadata.get("starts_user_turn").and_then(Value::as_bool) == Some(false)
            && metadata.get("turn_process").and_then(Value::as_bool) == Some(true)
            && metadata.get("collapsed_by_default").and_then(Value::as_bool) == Some(true)
            && metadata.get("editable").and_then(Value::as_bool) == Some(false)
            && metadata.get("transcript_attachments").is_none()
    ));
}

/// Delays host guidance until every tool result in the current provider batch has been appended.
/// 延迟宿主引导，直到当前 Provider 批次中的所有工具结果都已追加。
#[test]
fn run_user_turn_streaming_intervention_waits_for_full_tool_batch() {
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
                    provider_message_id: Some(String::from("resp-batch-1")),
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
                        last_provider_message_id: Some(String::from("resp-batch-1")),
                        updated_at: Some(String::from("4101")),
                    }),
                    usage: None,
                },
            ),
            (
                vec![ProviderTurnDelta::Output {
                    delta: String::from("已处理引导。"),
                }],
                ProviderTurnResponse {
                    assistant_text: String::from("已处理引导。"),
                    assistant_reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-batch-2")),
                    provider_message_state: None,
                    tool_calls: Vec::new(),
                    continuation: Some(ProviderContinuation {
                        provider_id: Some(String::from("mock-openai-streaming")),
                        model_id: Some(String::from("mock-model")),
                        last_provider_message_id: Some(String::from("resp-batch-2")),
                        updated_at: Some(String::from("4102")),
                    }),
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
            .with_outcome(
                "second_tool",
                "第二个工具已完成。",
                Some(json!({"tool": "second"})),
                false,
            );
        let mut controller =
            VulcanCoreController::new(build_test_config(4), ConversationSnapshot::default())
                .expect("valid core conversation snapshot");
        let mut checkpoints = Vec::<ConversationSnapshot>::new();
        let mut injected = false;

        let result = controller
            .run_user_turn_streaming_with_checkpoints_and_interventions(
                &adapter,
                &executor,
                &AllowAllToolPolicy,
                "请先调用两个工具",
                |snapshot, _| checkpoints.push(snapshot.clone()),
                |controller| {
                    if injected {
                        return Ok(false);
                    }
                    injected = true;
                    Ok(controller
                        .append_interjected_user_turn("批次完成后再引导")
                        .is_some())
                },
            )
            .await
            .expect("run guided batch streaming user turn");

        assert_eq!(result.provider_turns_executed, 2);
        assert_eq!(result.tool_calls_processed, 2);
        assert_eq!(checkpoints.len(), 3);
        assert!(matches!(
            checkpoints[0].messages.last(),
            Some(ConversationMessage {
                payload: ConversationPayload::ToolResult { tool_call_id, .. },
                ..
            }) if tool_call_id.as_deref() == Some("call-1")
        ));
        assert!(matches!(
            checkpoints[1].messages.last(),
            Some(ConversationMessage {
                payload: ConversationPayload::ToolResult { tool_call_id, .. },
                ..
            }) if tool_call_id.as_deref() == Some("call-2")
        ));
        assert!(matches!(
            checkpoints[2].messages.last(),
            Some(ConversationMessage {
                payload: ConversationPayload::UserText { text, .. },
                ..
            }) if text == "批次完成后再引导"
        ));

        let built_requests = adapter.built_requests();
        assert_eq!(built_requests.len(), 2);
        assert!(matches!(
            built_requests[1].history.last(),
            Some(ConversationMessage {
                payload: ConversationPayload::UserText { text, .. },
                ..
            }) if text == "批次完成后再引导"
        ));
        assert!(built_requests[1].history.iter().any(|message| matches!(
            &message.payload,
            ConversationPayload::ToolResult { tool_call_id, .. }
                if tool_call_id.as_deref() == Some("call-1")
        )));
        assert!(built_requests[1].history.iter().any(|message| matches!(
            &message.payload,
            ConversationPayload::ToolResult { tool_call_id, .. }
                if tool_call_id.as_deref() == Some("call-2")
        )));
    });
}
