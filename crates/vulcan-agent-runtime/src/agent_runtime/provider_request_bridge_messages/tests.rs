use super::{
    core_message_to_session_message, core_messages_to_provider_visible_history_for_model,
    core_messages_to_provider_visible_history_for_model_with_system_updates,
    session_message_to_core_message, validate_provider_visible_tool_topology,
};
use crate::agent_runtime::provider_system_update_lowering::ProviderSystemUpdateLoweringPolicy;
use crate::conversation_protocol::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationToolCall,
};
use vulcan_ai_contract::{ModelCapabilities, ResourceKind};
use vulcan_context::{
    RuntimeSystemUpdateEvent, RuntimeSystemUpdateLoweringHint, RuntimeSystemUpdateTrust,
};
use vulcan_core::types::{
    ConversationAttachment, ConversationAttachmentKind,
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationToolCall as CoreConversationToolCall,
};

/// Verifies session/Core conversion retains the durable rejection fact for malformed tool arguments.
/// 验证会话/Core 转换会保留异常工具参数的可靠拒绝事实。
#[test]
fn session_core_conversion_preserves_tool_argument_parse_error() {
    // decoderMessage is intentionally stored apart from the valid JSON placeholder.
    // decoderMessage 被刻意独立于有效 JSON 占位值保存。
    let decoder_message = "OpenAI Responses tool input ended before valid JSON was complete";
    let session_message = SessionConversationMessage {
        id: String::from("assistant-invalid-tool"),
        payload: SessionConversationPayload::Assistant {
            text: String::new(),
            reasoning: String::new(),
            provider_message_id: Some(String::from("response-invalid-tool")),
            provider_message_state: None,
            usage: None,
            tool_calls: vec![SessionConversationToolCall {
                id: String::from("call-invalid"),
                tool_call_id: Some(String::from("call-invalid")),
                tool_name: String::from("file_edit"),
                arguments: serde_json::Value::Null,
                argument_parse_error: Some(decoder_message.to_string()),
            }],
        },
    };

    let core_message = session_message_to_core_message(&session_message);
    let restored_message = core_message_to_session_message(&core_message);
    let SessionConversationPayload::Assistant { tool_calls, .. } = restored_message.payload else {
        panic!("round trip should retain the assistant tool-call payload");
    };
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].arguments, serde_json::Value::Null);
    assert_eq!(
        tool_calls[0].argument_parse_error.as_deref(),
        Some(decoder_message)
    );
}

/// Builds one replayable attachment payload used by provider-history projection tests.
/// 构建供 Provider 历史投影测试使用的可回放附件载荷。
fn replayable_attachment(
    kind: ConversationAttachmentKind,
    file_name: &str,
    mime_type: &str,
) -> ConversationAttachment {
    ConversationAttachment {
        kind,
        file_name: file_name.to_string(),
        local_path: std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join("Cargo.toml")
            .to_string_lossy()
            .to_string(),
        resource_path: Some(format!(".vulcan/code/attachments/{file_name}")),
        source_path: None,
        asset_id: Some(format!("asset-{file_name}")),
        byte_len: Some(4),
        source_sha256: None,
        delivery_mode: None,
        preview_local_path: None,
        mime_type: mime_type.to_string(),
    }
}

/// Builds one runtime system update used by provider-visible projection tests.
/// 构建一条供 Provider 可见投影测试使用的 runtime system update。
fn runtime_system_update(
    update_id: &str,
    effective_after_event_id: Option<&str>,
    hint: RuntimeSystemUpdateLoweringHint,
) -> RuntimeSystemUpdateEvent {
    RuntimeSystemUpdateEvent {
        turn_id: Some(String::from("turn-1")),
        update_id: update_id.to_string(),
        effective_after_event_id: effective_after_event_id.map(str::to_string),
        text: String::from("Use the compacted project rules from now on."),
        trust: RuntimeSystemUpdateTrust::TrustedHost,
        lowering_hint: hint,
        fingerprint: Some(format!("fp-{update_id}")),
        source: Some(String::from("context_compaction")),
    }
}

/// Verifies chronological system updates lower to user-scoped wrapper messages when the provider bridge has no native system-message slot.
/// 验证当 Provider 桥接层没有原生 system 消息槽位时，时间顺序 system update 会转换为用户作用域包装消息。
#[test]
fn provider_visible_history_applies_system_update_as_user_wrapper() {
    let messages = vec![CoreConversationMessage {
        id: String::from("user-1"),
        payload: CoreConversationPayload::UserText {
            text: String::from("继续"),
            attachments: Vec::new(),
        },
    }];
    let updates = vec![runtime_system_update(
        "system-update-1",
        Some("user-1"),
        RuntimeSystemUpdateLoweringHint::Auto,
    )];

    let projected = core_messages_to_provider_visible_history_for_model_with_system_updates(
        &messages,
        Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        None,
        &updates,
        ProviderSystemUpdateLoweringPolicy,
    )
    .expect("system update wrapper should project");

    assert_eq!(projected.len(), 2);
    assert_eq!(projected[1].id, "runtime-system-update:system-update-1");
    let CoreConversationPayload::UserText { text, attachments } = &projected[1].payload else {
        panic!("system update should become user-scoped wrapper text");
    };
    assert!(attachments.is_empty());
    assert!(text.contains("vulcan_context_update"));
    assert!(text.contains("system-update-1"));
    assert!(text.contains("compacted project rules"));
}

/// Verifies system updates anchored after an assistant tool call are moved after the matching tool result.
/// 验证锚定在助手工具调用之后的 system update 会移动到匹配工具结果之后。
#[test]
fn provider_visible_history_moves_system_update_after_tool_result_gap() {
    let messages = vec![
        CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
                tool_calls: vec![CoreConversationToolCall {
                    id: String::from("local-call-1"),
                    tool_call_id: Some(String::from("provider-call-1")),
                    tool_name: String::from("read_file"),
                    arguments: serde_json::json!({ "path": "Cargo.toml" }),
                    argument_parse_error: None,
                }],
            },
        },
        CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("provider-call-1")),
                is_error: false,
                text: String::from("file body"),
            },
        },
    ];
    let updates = vec![runtime_system_update(
        "system-update-gap",
        Some("assistant-1"),
        RuntimeSystemUpdateLoweringHint::Auto,
    )];

    let projected = core_messages_to_provider_visible_history_for_model_with_system_updates(
        &messages,
        Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        None,
        &updates,
        ProviderSystemUpdateLoweringPolicy,
    )
    .expect("system update in a tool-result gap should be moved");

    assert_eq!(projected.len(), 3);
    assert_eq!(projected[0].id, "assistant-1");
    assert_eq!(projected[1].id, "tool-result-1");
    assert_eq!(projected[2].id, "runtime-system-update:system-update-gap");
    validate_provider_visible_tool_topology(&projected).expect("valid provider topology");
}

/// Verifies system updates anchored on one result inside a parallel tool batch move after every contiguous result.
/// 验证锚定在并行工具批次某个结果上的 system update 会移动到所有连续结果之后。
#[test]
fn provider_visible_history_moves_system_update_after_parallel_tool_results() {
    let messages = vec![
        CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
                tool_calls: vec![
                    CoreConversationToolCall {
                        id: String::from("local-call-1"),
                        tool_call_id: Some(String::from("provider-call-1")),
                        tool_name: String::from("read_file"),
                        arguments: serde_json::json!({ "path": "Cargo.toml" }),
                        argument_parse_error: None,
                    },
                    CoreConversationToolCall {
                        id: String::from("local-call-2"),
                        tool_call_id: Some(String::from("provider-call-2")),
                        tool_name: String::from("list_files"),
                        arguments: serde_json::json!({}),
                        argument_parse_error: None,
                    },
                ],
            },
        },
        CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("provider-call-1")),
                is_error: false,
                text: String::from("file body"),
            },
        },
        CoreConversationMessage {
            id: String::from("tool-result-2"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("provider-call-2")),
                is_error: false,
                text: String::from("file list"),
            },
        },
    ];
    let updates = vec![runtime_system_update(
        "system-update-parallel-gap",
        Some("tool-result-1"),
        RuntimeSystemUpdateLoweringHint::Auto,
    )];

    let projected = core_messages_to_provider_visible_history_for_model_with_system_updates(
        &messages,
        Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        None,
        &updates,
        ProviderSystemUpdateLoweringPolicy,
    )
    .expect("system update in a parallel tool-result gap should be moved");

    assert_eq!(projected.len(), 4);
    assert_eq!(projected[0].id, "assistant-1");
    assert_eq!(projected[1].id, "tool-result-1");
    assert_eq!(projected[2].id, "tool-result-2");
    assert_eq!(
        projected[3].id,
        "runtime-system-update:system-update-parallel-gap"
    );
    validate_provider_visible_tool_topology(&projected).expect("valid provider topology");
}

/// Verifies historical attachments are downgraded before transport encoding when the selected model is text-only.
/// 验证历史附件会在传输编码前根据纯文本模型能力降级。
#[test]
fn provider_visible_history_downgrades_model_unsupported_attachments() {
    let capabilities = ModelCapabilities::default();
    let messages = vec![CoreConversationMessage {
        id: String::from("user-1"),
        payload: CoreConversationPayload::UserText {
            text: String::from("继续分析"),
            attachments: vec![replayable_attachment(
                ConversationAttachmentKind::Image,
                "demo.png",
                "image/png",
            )],
        },
    }];

    let projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        Some(&capabilities),
    )
    .expect("valid provider history projection");

    let CoreConversationPayload::UserText { text, attachments } = &projected[0].payload else {
        panic!("expected projected user text");
    };
    assert!(attachments.is_empty());
    assert!(text.contains("selected model"));
    assert!(text.contains(".vulcan/code/attachments/demo.png"));
    let CoreConversationPayload::UserText {
        attachments: canonical_attachments,
        ..
    } = &messages[0].payload
    else {
        panic!("expected canonical user text");
    };
    assert_eq!(canonical_attachments.len(), 1);
}

/// Verifies legacy project-scope Explore records are removed from provider-visible history.
/// 验证旧版项目范围 Explore 记录会从 Provider 可见历史中移除。
#[test]
fn provider_visible_history_filters_legacy_explore_assistant_projection() {
    let messages = vec![
        CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("介绍下你自己"),
                attachments: Vec::new(),
            },
        },
        CoreConversationMessage {
            id: String::from("explore-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::from(
                    "This session is bound to project `test-project` with root directory `D:\\openvulcan\\test-project`. Future explore and triage steps should prioritize that scope.",
                ),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
                tool_calls: Vec::new(),
            },
        },
        CoreConversationMessage {
            id: String::from("user-2"),
            payload: CoreConversationPayload::UserText {
                text: String::from("继续"),
                attachments: Vec::new(),
            },
        },
    ];

    let projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        None,
    )
    .expect("valid provider history projection");

    assert_eq!(projected.len(), 2);
    assert!(projected.iter().all(|message| message.id != "explore-1"));
    assert!(matches!(
        &projected[0].payload,
        CoreConversationPayload::UserText { text, .. } if text == "介绍下你自己"
    ));
    assert!(matches!(
        &projected[1].payload,
        CoreConversationPayload::UserText { text, .. } if text == "继续"
    ));
}

/// Verifies matched tool results remain protocol tool responses in provider-visible history.
/// 验证已匹配的工具结果在 Provider 可见历史中仍保持协议工具响应形态。
#[test]
fn provider_visible_history_preserves_matched_tool_result() {
    let messages = vec![
        CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::from("local reasoning"),
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
                tool_calls: vec![CoreConversationToolCall {
                    id: String::from("local-call"),
                    tool_call_id: Some(String::from("provider-call")),
                    tool_name: String::from("read_file"),
                    arguments: serde_json::json!({"path": "Cargo.toml"}),
                    argument_parse_error: None,
                }],
            },
        },
        CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("provider-call")),
                is_error: false,
                text: String::from("file body"),
            },
        },
    ];

    let projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        None,
    )
    .expect("valid provider history projection");

    assert!(matches!(
        &projected[1].payload,
        CoreConversationPayload::ToolResult { text, .. } if text == "file body"
    ));
}

/// Verifies projected provider history satisfies the canonical tool topology validator.
/// 验证投影后的 Provider 历史满足规范工具拓扑校验器。
#[test]
fn provider_visible_history_passes_canonical_tool_topology_validator() {
    let messages = vec![
        CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
                tool_calls: vec![
                    CoreConversationToolCall {
                        id: String::from("local-call-1"),
                        tool_call_id: Some(String::from("provider-call-1")),
                        tool_name: String::from("first_tool"),
                        arguments: serde_json::json!({}),
                        argument_parse_error: None,
                    },
                    CoreConversationToolCall {
                        id: String::from("local-call-2"),
                        tool_call_id: Some(String::from("provider-call-2")),
                        tool_name: String::from("second_tool"),
                        arguments: serde_json::json!({}),
                        argument_parse_error: None,
                    },
                ],
            },
        },
        CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("provider-call-1")),
                is_error: false,
                text: String::from("first done"),
            },
        },
        CoreConversationMessage {
            id: String::from("tool-result-2"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("local-call-2")),
                is_error: false,
                text: String::from("second done"),
            },
        },
    ];

    let projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        None,
    )
    .expect("valid provider history projection");

    let CoreConversationPayload::Assistant { tool_calls, .. } = &projected[0].payload else {
        panic!("first projected message should retain the assistant tool batch");
    };
    assert_eq!(
        tool_calls[1].tool_call_id.as_deref(),
        Some("provider-call-2")
    );
    let CoreConversationPayload::ToolResult { tool_call_id, .. } = &projected[2].payload else {
        panic!("third projected message should retain the second tool result");
    };
    assert_eq!(tool_call_id.as_deref(), Some("provider-call-2"));
    validate_provider_visible_tool_topology(&projected).expect("valid provider topology");
}

/// Verifies the canonical tool topology validator rejects interrupted live tool batches.
/// 验证规范工具拓扑校验器会拒绝被打断的实时工具批次。
#[test]
fn provider_visible_tool_topology_validator_rejects_interrupted_batch() {
    let messages = vec![
        CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
                tool_calls: vec![CoreConversationToolCall {
                    id: String::from("local-call-1"),
                    tool_call_id: Some(String::from("provider-call-1")),
                    tool_name: String::from("ask"),
                    arguments: serde_json::json!({}),
                    argument_parse_error: None,
                }],
            },
        },
        CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("new prompt"),
                attachments: Vec::new(),
            },
        },
    ];

    let error = validate_provider_visible_tool_topology(&messages)
        .expect_err("interrupted batch should be rejected");
    assert!(error.contains("interrupted assistant tool batch"));
}

/// Verifies the canonical tool topology validator rejects orphan tool results.
/// 验证规范工具拓扑校验器会拒绝孤立工具结果。
#[test]
fn provider_visible_tool_topology_validator_rejects_orphan_result() {
    let messages = vec![CoreConversationMessage {
        id: String::from("tool-result-1"),
        payload: CoreConversationPayload::ToolResult {
            tool_call_id: Some(String::from("missing-call")),
            is_error: false,
            text: String::from("orphan body"),
        },
    }];

    let error = validate_provider_visible_tool_topology(&messages)
        .expect_err("orphan result should be rejected");
    assert!(error.contains("unmatched tool call"));
}

/// Verifies orphaned tool results are downgraded to plain user-visible text before provider transport.
/// 验证孤立工具结果会在 Provider 传输前降级为普通用户可见文本。
#[test]
fn provider_visible_history_downgrades_orphan_tool_result() {
    let messages = vec![CoreConversationMessage {
        id: String::from("tool-result-1"),
        payload: CoreConversationPayload::ToolResult {
            tool_call_id: Some(String::from("missing-call")),
            is_error: false,
            text: String::from("orphan body"),
        },
    }];

    let projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        None,
    )
    .expect("valid provider history projection");

    let CoreConversationPayload::UserText { text, attachments } = &projected[0].payload else {
        panic!("expected orphan tool result to become plain user text");
    };
    assert!(attachments.is_empty());
    assert!(text.contains("cannot be replayed"));
    assert!(text.contains("orphan body"));
}

/// Verifies partially completed parallel tool batches are downgraded before provider replay.
/// 验证部分完成的并行工具批次会在 Provider 回放前降级。
#[test]
fn provider_visible_history_downgrades_partial_tool_batch() {
    let messages = vec![
        CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::from("call two tools"),
                provider_message_id: Some(String::from("resp-1")),
                provider_message_state: None,
                usage: None,
                tool_calls: vec![
                    CoreConversationToolCall {
                        id: String::from("local-call-1"),
                        tool_call_id: Some(String::from("provider-call-1")),
                        tool_name: String::from("first_tool"),
                        arguments: serde_json::json!({"step": 1}),
                        argument_parse_error: None,
                    },
                    CoreConversationToolCall {
                        id: String::from("local-call-2"),
                        tool_call_id: Some(String::from("provider-call-2")),
                        tool_name: String::from("second_tool"),
                        arguments: serde_json::json!({"step": 2}),
                        argument_parse_error: None,
                    },
                ],
            },
        },
        CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("provider-call-1")),
                is_error: false,
                text: String::from("first done"),
            },
        },
    ];

    let projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        None,
    )
    .expect("valid provider history projection");

    assert_eq!(projected.len(), 1);
    let CoreConversationPayload::UserText { text, attachments } = &projected[0].payload else {
        panic!("partial tool batch should downgrade to text");
    };
    assert!(attachments.is_empty());
    assert!(text.contains("interrupted before every requested tool returned"));
    assert!(text.contains("first_tool"));
    assert!(text.contains("second_tool"));
    assert!(text.contains("first done"));
}

/// Verifies interrupted tool batches with no results are never replayed as live provider tool calls.
/// 验证没有任何结果的中断工具批次绝不会作为实时 Provider 工具调用回放。
#[test]
fn provider_visible_history_downgrades_unanswered_tool_batch() {
    let messages = vec![CoreConversationMessage {
        id: String::from("assistant-1"),
        payload: CoreConversationPayload::Assistant {
            text: String::from("I need to ask the user."),
            reasoning: String::from("call ask"),
            provider_message_id: Some(String::from("resp-1")),
            provider_message_state: None,
            usage: None,
            tool_calls: vec![CoreConversationToolCall {
                id: String::from("local-call-1"),
                tool_call_id: Some(String::from("provider-call-1")),
                tool_name: String::from("ask"),
                arguments: serde_json::json!({
                    "questions": [{
                        "header": "Choice",
                        "question": "Pick one?",
                        "options": []
                    }]
                }),
                argument_parse_error: None,
            }],
        },
    }];

    let projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        None,
    )
    .expect("valid provider history projection");

    assert_eq!(projected.len(), 1);
    let CoreConversationPayload::UserText { text, attachments } = &projected[0].payload else {
        panic!("unanswered tool batch should downgrade to text");
    };
    assert!(attachments.is_empty());
    assert!(text.contains("interrupted before every requested tool returned"));
    assert!(text.contains("ask"));
    assert!(text.contains("provider-call-1"));
}

/// Verifies the plain-text canonical fixture keeps ordered turns and assistant reasoning.
/// 验证普通文本 canonical fixture 会保留有序回合与助手推理。
#[test]
fn canonical_fixture_plain_text_history_projects_stably() {
    let messages = vec![
        CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("你好"),
                attachments: Vec::new(),
            },
        },
        CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::from("你好，我在。"),
                reasoning: String::from("preserved reasoning"),
                provider_message_id: Some(String::from("provider-1")),
                provider_message_state: None,
                usage: None,
                tool_calls: Vec::new(),
            },
        },
        CoreConversationMessage {
            id: String::from("user-2"),
            payload: CoreConversationPayload::UserText {
                text: String::from("继续"),
                attachments: Vec::new(),
            },
        },
    ];

    let projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        None,
    )
    .expect("valid provider history projection");

    assert_eq!(projected.len(), 3);
    assert_eq!(projected[0].id, "user-1");
    assert_eq!(projected[1].id, "assistant-1");
    assert_eq!(projected[2].id, "user-2");
    let CoreConversationPayload::Assistant {
        text,
        reasoning,
        tool_calls,
        ..
    } = &projected[1].payload
    else {
        panic!("second projected message should be assistant text");
    };
    assert_eq!(text, "你好，我在。");
    assert_eq!(reasoning, "preserved reasoning");
    assert!(tool_calls.is_empty());
}

/// Verifies the tool-call canonical fixture keeps matched tool calls and results as live protocol state.
/// 验证工具调用 canonical fixture 会把已匹配的工具调用与结果保留为实时协议状态。
#[test]
fn canonical_fixture_tool_call_and_result_projects_as_protocol_pair() {
    let messages = vec![
        CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::from("读取文件。"),
                reasoning: String::from("read first"),
                provider_message_id: Some(String::from("provider-1")),
                provider_message_state: None,
                usage: None,
                tool_calls: vec![CoreConversationToolCall {
                    id: String::from("local-call-1"),
                    tool_call_id: Some(String::from("provider-call-1")),
                    tool_name: String::from("read_file"),
                    arguments: serde_json::json!({ "path": "Cargo.toml" }),
                    argument_parse_error: None,
                }],
            },
        },
        CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("provider-call-1")),
                is_error: false,
                text: String::from("file body"),
            },
        },
    ];

    let projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        None,
    )
    .expect("valid provider history projection");

    assert_eq!(projected.len(), 2);
    let CoreConversationPayload::Assistant { tool_calls, .. } = &projected[0].payload else {
        panic!("first projected message should keep assistant tool calls");
    };
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(
        tool_calls[0].tool_call_id.as_deref(),
        Some("provider-call-1")
    );
    let CoreConversationPayload::ToolResult {
        tool_call_id, text, ..
    } = &projected[1].payload
    else {
        panic!("second projected message should keep the matching tool result");
    };
    assert_eq!(tool_call_id.as_deref(), Some("provider-call-1"));
    assert_eq!(text, "file body");
    validate_provider_visible_tool_topology(&projected).expect("tool topology should be valid");
}

/// Verifies a provider-switch fixture degrades stale live tool state before the next user turn is projected.
/// 验证 provider 切换 fixture 会在下一条用户回合投影前降级过期实时工具状态。
#[test]
fn canonical_fixture_provider_switch_downgrades_stale_tool_state() {
    let messages = vec![
        CoreConversationMessage {
            id: String::from("assistant-old-provider"),
            payload: CoreConversationPayload::Assistant {
                text: String::from("旧 provider 准备调用工具。"),
                reasoning: String::new(),
                provider_message_id: Some(String::from("old-provider-message")),
                provider_message_state: Some(serde_json::json!({
                    "protocol": "anthropic",
                    "payload": { "id": "old" }
                })),
                usage: None,
                tool_calls: vec![CoreConversationToolCall {
                    id: String::from("local-old-call"),
                    tool_call_id: Some(String::from("old-provider-call")),
                    tool_name: String::from("image_generation"),
                    arguments: serde_json::json!({ "prompt": "demo" }),
                    argument_parse_error: None,
                }],
            },
        },
        CoreConversationMessage {
            id: String::from("user-after-switch"),
            payload: CoreConversationPayload::UserText {
                text: String::from("切到 MiniMax 后继续"),
                attachments: Vec::new(),
            },
        },
    ];

    let projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        None,
    )
    .expect("valid provider history projection");

    assert_eq!(projected.len(), 2);
    let CoreConversationPayload::UserText { text, attachments } = &projected[0].payload else {
        panic!("stale tool state should become historical user text");
    };
    assert!(attachments.is_empty());
    assert!(text.contains("historical text only"));
    assert!(text.contains("old-provider-call"));
    let CoreConversationPayload::UserText { text, .. } = &projected[1].payload else {
        panic!("latest user prompt should remain user text");
    };
    assert_eq!(text, "切到 MiniMax 后继续");
    validate_provider_visible_tool_topology(&projected)
        .expect("downgraded switch history should be topology-safe");
}

/// Verifies the injection fixture projects dynamic runtime overlays after the stable user baseline.
/// 验证注入 fixture 会在稳定用户基线之后投影动态 runtime 覆盖。
#[test]
fn canonical_fixture_dynamic_injection_projects_as_system_update_wrapper() {
    let messages = vec![CoreConversationMessage {
        id: String::from("user-1"),
        payload: CoreConversationPayload::UserText {
            text: String::from("继续任务"),
            attachments: Vec::new(),
        },
    }];
    let updates = vec![runtime_system_update(
        "dynamic-injection-1",
        Some("user-1"),
        RuntimeSystemUpdateLoweringHint::Auto,
    )];

    let projected = core_messages_to_provider_visible_history_for_model_with_system_updates(
        &messages,
        Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        None,
        &updates,
        ProviderSystemUpdateLoweringPolicy,
    )
    .expect("dynamic injection wrapper should project");

    assert_eq!(projected.len(), 2);
    assert_eq!(projected[0].id, "user-1");
    assert_eq!(projected[1].id, "runtime-system-update:dynamic-injection-1");
    let CoreConversationPayload::UserText { text, attachments } = &projected[1].payload else {
        panic!("dynamic injection should become user-scoped wrapper text");
    };
    assert!(attachments.is_empty());
    assert!(text.contains("dynamic-injection-1"));
    assert!(text.contains("vulcan_context_update"));
}

/// Verifies multimodal canonical fixtures preserve image and video attachments only when provider and model support them.
/// 验证多模态 canonical fixture 只会在 provider 与模型均支持时保留图片和视频附件。
#[test]
fn canonical_fixture_image_and_video_understanding_respects_model_capabilities() {
    let capabilities = ModelCapabilities {
        input_resources: vec![ResourceKind::Image, ResourceKind::Video],
        ..ModelCapabilities::default()
    };
    let messages = vec![CoreConversationMessage {
        id: String::from("user-multimodal"),
        payload: CoreConversationPayload::UserText {
            text: String::from("理解图片和视频"),
            attachments: vec![
                replayable_attachment(ConversationAttachmentKind::Image, "scene.png", "image/png"),
                replayable_attachment(ConversationAttachmentKind::Video, "clip.mp4", "video/mp4"),
            ],
        },
    }];

    let responses_projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID),
        Some(&capabilities),
    )
    .expect("valid Responses provider history projection");
    let CoreConversationPayload::UserText {
        text: responses_text,
        attachments: responses_attachments,
        ..
    } = &responses_projected[0].payload
    else {
        panic!("Responses fixture should remain user text");
    };
    assert_eq!(responses_attachments.len(), 1);
    assert_eq!(
        responses_attachments[0].kind,
        ConversationAttachmentKind::Image
    );
    assert!(responses_text.contains("selected protocol profile"));

    let openai_projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        Some(&capabilities),
    )
    .expect("valid OpenAI provider history projection");
    let CoreConversationPayload::UserText {
        text,
        attachments: openai_attachments,
    } = &openai_projected[0].payload
    else {
        panic!("OpenAI-compatible fixture should remain user text");
    };
    assert_eq!(openai_attachments.len(), 2);
    assert_eq!(text, "理解图片和视频");
}

// S3.3 tool pairing, parallel results, media interleaving, and insertion-boundary coverage.
// S3.3 工具配对、并行结果、媒体交错与插入边界覆盖。

/// Builds one canonical tool call whose provider-visible wire id may differ from its local runtime id.
/// 构建一条 Provider 可见线协议标识可能不同于本地运行时标识的规范工具调用。
fn canonical_tool_call(
    local_id: &str,
    provider_visible_id: Option<&str>,
    tool_name: &str,
) -> CoreConversationToolCall {
    CoreConversationToolCall {
        id: local_id.to_string(),
        tool_call_id: provider_visible_id.map(str::to_string),
        tool_name: tool_name.to_string(),
        arguments: serde_json::json!({ "path": "Cargo.toml" }),
        argument_parse_error: None,
    }
}

/// Builds one canonical assistant message that owns an ordered tool-call batch.
/// 构建一条持有有序工具调用批次的规范助手消息。
fn assistant_tool_batch(
    message_id: &str,
    tool_calls: Vec<CoreConversationToolCall>,
) -> CoreConversationMessage {
    CoreConversationMessage {
        id: message_id.to_string(),
        payload: CoreConversationPayload::Assistant {
            text: String::new(),
            reasoning: String::new(),
            provider_message_id: Some(String::from("provider-response-1")),
            provider_message_state: None,
            usage: None,
            tool_calls,
        },
    }
}

/// Builds one canonical tool-result message bound to an exact call identifier.
/// 构建一条绑定到精确调用标识的规范工具结果消息。
fn tool_result_message(
    message_id: &str,
    tool_call_id: Option<&str>,
    text: &str,
) -> CoreConversationMessage {
    CoreConversationMessage {
        id: message_id.to_string(),
        payload: CoreConversationPayload::ToolResult {
            tool_call_id: tool_call_id.map(str::to_string),
            is_error: false,
            text: text.to_string(),
        },
    }
}

/// Builds one canonical user message carrying ordered native attachments.
/// 构建一条携带有序原生附件的规范用户消息。
fn user_message_with_attachments(
    message_id: &str,
    text: &str,
    attachments: Vec<ConversationAttachment>,
) -> CoreConversationMessage {
    CoreConversationMessage {
        id: message_id.to_string(),
        payload: CoreConversationPayload::UserText {
            text: text.to_string(),
            attachments,
        },
    }
}

/// Builds one host-managed MCP media result through the production envelope producer.
/// 通过生产 envelope 生产者构建一份宿主管理的 MCP 媒体结果。
fn matched_media_tool_result_text() -> String {
    let canonical = crate::agent_runtime::tool_result_media::canonicalize_tool_model_output(
        serde_json::json!({
            "content": [
                { "type": "text", "text": "screenshot captured" },
                { "type": "image", "data": "aGVsbG8=", "mimeType": "image/png" }
            ]
        }),
    )
    .expect("valid MCP media result envelope");
    canonical
        .as_str()
        .expect("a validated media result is persisted as a JSON string")
        .to_string()
}

/// Returns the exact position of one projected message located by identity rather than by ordinal.
/// 按身份而不是序号返回一条投影消息的精确位置。
fn projected_position(projected: &[CoreConversationMessage], message_id: &str) -> usize {
    projected
        .iter()
        .position(|message| message.id == message_id)
        .unwrap_or_else(|| panic!("projected history must still contain `{message_id}`"))
}

/// Verifies a final assistant tool batch whose remaining results never arrived is rejected instead of replayed.
/// 验证末尾助手工具批次缺失剩余结果时会被拒绝，而不是被回放。
#[test]
fn provider_visible_tool_topology_validator_rejects_unfinished_final_tool_batch() {
    let messages = vec![
        assistant_tool_batch(
            "assistant-1",
            vec![
                canonical_tool_call("local-1", Some("provider-1"), "first_tool"),
                canonical_tool_call("local-2", Some("provider-2"), "second_tool"),
            ],
        ),
        tool_result_message("tool-result-1", Some("provider-1"), "first done"),
    ];

    let error = validate_provider_visible_tool_topology(&messages)
        .expect_err("a batch missing one result must fail closed");
    assert!(
        error.contains("ended without all tool results"),
        "unexpected validator error: {error}"
    );
}

/// Verifies a second assistant tool batch cannot start before the previous batch completed.
/// 验证前一批次完成之前不得开始第二个助手工具批次。
#[test]
fn provider_visible_tool_topology_validator_rejects_second_tool_batch_before_completion() {
    let messages = vec![
        assistant_tool_batch(
            "assistant-1",
            vec![canonical_tool_call(
                "local-1",
                Some("provider-1"),
                "first_tool",
            )],
        ),
        assistant_tool_batch(
            "assistant-2",
            vec![canonical_tool_call(
                "local-2",
                Some("provider-2"),
                "second_tool",
            )],
        ),
    ];

    let error = validate_provider_visible_tool_topology(&messages)
        .expect_err("a nested tool batch must fail closed");
    assert!(
        error.contains("started a new tool batch before index"),
        "unexpected validator error: {error}"
    );
}

/// Verifies an assistant tool call without a provider-visible identifier is rejected as unreplayable.
/// 验证缺少 Provider 可见标识的助手工具调用会因不可回放而被拒绝。
#[test]
fn provider_visible_tool_topology_validator_rejects_tool_call_without_provider_visible_identifier()
{
    let messages = vec![
        assistant_tool_batch(
            "assistant-1",
            vec![canonical_tool_call("local-only", None, "ask")],
        ),
        tool_result_message("tool-result-1", Some("local-only"), "answer"),
    ];

    let error = validate_provider_visible_tool_topology(&messages)
        .expect_err("a call without a provider-visible id must fail closed");
    assert!(
        error.contains("without unique provider-visible identifiers"),
        "unexpected validator error: {error}"
    );
}

/// Verifies two calls in one batch cannot share one provider-visible identifier.
/// 验证同一批次内的两个调用不能共享同一个 Provider 可见标识。
#[test]
fn provider_visible_tool_topology_validator_rejects_duplicate_provider_visible_identifiers() {
    let messages = vec![
        assistant_tool_batch(
            "assistant-1",
            vec![
                canonical_tool_call("local-1", Some("provider-duplicate"), "first_tool"),
                canonical_tool_call("local-2", Some("provider-duplicate"), "second_tool"),
            ],
        ),
        tool_result_message("tool-result-1", Some("provider-duplicate"), "first done"),
        tool_result_message("tool-result-2", Some("provider-duplicate"), "second done"),
    ];

    let error = validate_provider_visible_tool_topology(&messages)
        .expect_err("duplicate provider-visible ids must fail closed");
    assert!(
        error.contains("without unique provider-visible identifiers"),
        "unexpected validator error: {error}"
    );
}

/// Verifies tool results without an exact replayable call identifier are rejected in both degraded forms.
/// 验证两种缺失形态的工具结果都会因缺少精确可回放调用标识而被拒绝。
#[test]
fn provider_visible_tool_topology_validator_rejects_tool_result_without_replayable_identifier() {
    for missing_identifier in [None, Some("   ")] {
        let messages = vec![
            assistant_tool_batch(
                "assistant-1",
                vec![canonical_tool_call(
                    "local-1",
                    Some("provider-1"),
                    "read_file",
                )],
            ),
            tool_result_message("tool-result-1", missing_identifier, "file body"),
        ];

        let error = validate_provider_visible_tool_topology(&messages)
            .expect_err("a result without a replayable id must fail closed");
        assert!(
            error.contains("does not carry a replayable tool_call_id"),
            "unexpected validator error for {missing_identifier:?}: {error}"
        );
    }
}

/// Verifies parallel tool results pair by call identity, not by arrival position or alias form.
/// 验证并行工具结果按调用身份配对，而不是按到达位置或别名形态配对。
#[test]
fn provider_visible_history_pairs_parallel_results_out_of_order_by_identity() {
    let messages = vec![
        assistant_tool_batch(
            "assistant-1",
            vec![
                canonical_tool_call("local-1", Some("provider-1"), "first_tool"),
                canonical_tool_call("local-2", Some("provider-2"), "second_tool"),
            ],
        ),
        tool_result_message("tool-result-2", Some("provider-2"), "second done"),
        tool_result_message("tool-result-1", Some("local-1"), "first done"),
    ];

    let projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        None,
    )
    .expect("out-of-order parallel results should project");

    assert_eq!(projected.len(), 3);
    let CoreConversationPayload::Assistant { tool_calls, .. } = &projected[0].payload else {
        panic!("a fully completed parallel batch must stay a native assistant batch");
    };
    assert_eq!(
        tool_calls[0].tool_call_id.as_deref(),
        Some("provider-1"),
        "provider-visible ids must follow the canonical call order"
    );
    assert_eq!(tool_calls[1].tool_call_id.as_deref(), Some("provider-2"));

    let CoreConversationPayload::ToolResult {
        tool_call_id, text, ..
    } = &projected[1].payload
    else {
        panic!("the second arriving result must stay a native tool response");
    };
    assert_eq!(tool_call_id.as_deref(), Some("provider-2"));
    assert_eq!(text, "second done");

    let CoreConversationPayload::ToolResult {
        tool_call_id, text, ..
    } = &projected[2].payload
    else {
        panic!("a local-id result must be rewritten to its provider-visible response");
    };
    assert_eq!(tool_call_id.as_deref(), Some("provider-1"));
    assert_eq!(text, "first done");
    validate_provider_visible_tool_topology(&projected).expect("valid provider topology");
}

/// Verifies one tool call identity completes exactly once, so a duplicate result degrades to labelled text.
/// 验证一个工具调用身份只被完成一次，因此重复结果会降级为带标签文本。
#[test]
fn provider_visible_history_completes_each_tool_call_identity_exactly_once() {
    let messages = vec![
        assistant_tool_batch(
            "assistant-1",
            vec![canonical_tool_call(
                "local-1",
                Some("provider-1"),
                "read_file",
            )],
        ),
        tool_result_message("tool-result-1", Some("provider-1"), "first body"),
        tool_result_message("tool-result-2", Some("provider-1"), "duplicate body"),
    ];

    let projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        None,
    )
    .expect("a duplicated result must not abort projection");

    assert_eq!(projected.len(), 3);
    let CoreConversationPayload::ToolResult {
        tool_call_id, text, ..
    } = &projected[1].payload
    else {
        panic!("the first result must remain the single native tool response");
    };
    assert_eq!(tool_call_id.as_deref(), Some("provider-1"));
    assert_eq!(text, "first body");
    let CoreConversationPayload::UserText { text, attachments } = &projected[2].payload else {
        panic!("a duplicate result must become plain labelled history text");
    };
    assert!(attachments.is_empty());
    assert!(text.contains("cannot be replayed"), "{text}");
    assert!(text.contains("duplicate body"), "{text}");
    validate_provider_visible_tool_topology(&projected).expect("valid provider topology");
}

/// Verifies a media-bearing user message between a tool call and its result never splits a native pair.
/// 验证工具调用与结果之间的媒体用户消息绝不会拆开原生配对。
#[test]
fn provider_visible_history_degrades_tool_pair_split_by_media_and_keeps_media_native() {
    let capabilities = ModelCapabilities {
        input_resources: vec![ResourceKind::Image],
        ..ModelCapabilities::default()
    };
    let messages = vec![
        assistant_tool_batch(
            "assistant-1",
            vec![canonical_tool_call(
                "local-1",
                Some("provider-1"),
                "read_file",
            )],
        ),
        user_message_with_attachments(
            "user-media",
            "看这张图",
            vec![replayable_attachment(
                ConversationAttachmentKind::Image,
                "scene.png",
                "image/png",
            )],
        ),
        tool_result_message("tool-result-1", Some("provider-1"), "file body"),
    ];

    let projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        Some(&capabilities),
    )
    .expect("a media-interleaved tool pair must degrade instead of failing");

    assert_eq!(projected.len(), 3);
    let CoreConversationPayload::UserText { text, attachments } = &projected[0].payload else {
        panic!("an interrupted tool batch must never be replayed as live tool protocol state");
    };
    assert!(attachments.is_empty());
    assert!(
        text.contains("interrupted before every requested tool returned"),
        "{text}"
    );

    let CoreConversationPayload::UserText { text, attachments } = &projected[1].payload else {
        panic!("the interleaved media message must stay native user content");
    };
    assert_eq!(text, "看这张图");
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].kind, ConversationAttachmentKind::Image);

    let CoreConversationPayload::UserText { text, attachments } = &projected[2].payload else {
        panic!("the split tool result must become labelled historical text");
    };
    assert!(attachments.is_empty());
    assert!(text.contains("cannot be replayed"), "{text}");
    assert!(text.contains("file body"), "{text}");
    validate_provider_visible_tool_topology(&projected).expect("valid provider topology");
}

/// Verifies a matched tool-result media envelope survives projection byte-for-byte for native transport projection.
/// 验证已匹配的工具结果媒体 envelope 逐字节保留到原生传输投影。
#[test]
fn provider_visible_history_preserves_matched_tool_result_media_envelope() {
    let media_envelope = matched_media_tool_result_text();
    let messages = vec![
        assistant_tool_batch(
            "assistant-1",
            vec![canonical_tool_call(
                "local-1",
                Some("provider-1"),
                "screenshot",
            )],
        ),
        tool_result_message("tool-result-1", Some("provider-1"), &media_envelope),
    ];

    let projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(vulcan_llm_core::OPENAI_CODEX_RESPONSES_PROFILE_ID),
        None,
    )
    .expect("a matched media tool result should project");

    let CoreConversationPayload::ToolResult {
        tool_call_id, text, ..
    } = &projected[1].payload
    else {
        panic!("a matched media result must remain a native tool response");
    };
    assert_eq!(tool_call_id.as_deref(), Some("provider-1"));
    assert_eq!(
        text, &media_envelope,
        "transport media projection requires the exact persisted envelope bytes"
    );
    validate_provider_visible_tool_topology(&projected).expect("valid provider topology");
}

/// Verifies an orphaned media tool result becomes explicit non-replayable history text without native structure.
/// 验证孤立媒体工具结果会变成明确不可回放的历史文本，且不保留原生结构。
#[test]
fn provider_visible_history_orphan_media_result_becomes_labelled_plain_text() {
    let media_envelope = matched_media_tool_result_text();
    let messages = vec![tool_result_message(
        "tool-result-orphan",
        Some("compacted-away-call"),
        &media_envelope,
    )];

    let projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(vulcan_llm_core::OPENAI_CODEX_RESPONSES_PROFILE_ID),
        None,
    )
    .expect("an orphan tool result must degrade instead of aborting");

    assert_eq!(projected.len(), 1);
    let CoreConversationPayload::UserText { text, attachments } = &projected[0].payload else {
        panic!("an orphan tool result cannot stay a native tool response");
    };
    assert!(attachments.is_empty());
    assert!(text.contains("cannot be replayed"), "{text}");
    assert!(text.contains("compacted-away-call"), "{text}");
    assert!(
        text.contains("Treat the following as historical text only"),
        "{text}"
    );
    assert!(
        !projected
            .iter()
            .any(|message| matches!(message.payload, CoreConversationPayload::ToolResult { .. })),
        "no unmatched native tool response may survive projection"
    );
    validate_provider_visible_tool_topology(&projected).expect("valid provider topology");
}

/// Verifies a system update anchored on a media message is inserted outside the following tool pair.
/// 验证锚定在媒体消息上的 system update 会插入到后续工具配对之外。
#[test]
fn provider_visible_history_inserts_system_update_after_media_message_outside_tool_pair() {
    let capabilities = ModelCapabilities {
        input_resources: vec![ResourceKind::Image],
        ..ModelCapabilities::default()
    };
    let messages = vec![
        user_message_with_attachments(
            "user-media",
            "先看这张图",
            vec![replayable_attachment(
                ConversationAttachmentKind::Image,
                "scene.png",
                "image/png",
            )],
        ),
        assistant_tool_batch(
            "assistant-1",
            vec![canonical_tool_call(
                "local-1",
                Some("provider-1"),
                "read_file",
            )],
        ),
        tool_result_message("tool-result-1", Some("provider-1"), "file body"),
    ];
    let updates = vec![runtime_system_update(
        "system-update-after-media",
        Some("user-media"),
        RuntimeSystemUpdateLoweringHint::Auto,
    )];

    let projected = core_messages_to_provider_visible_history_for_model_with_system_updates(
        &messages,
        Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        Some(&capabilities),
        &updates,
        ProviderSystemUpdateLoweringPolicy,
    )
    .expect("a media-anchored system update should project");

    assert_eq!(projected.len(), 4);
    let media_position = projected_position(&projected, "user-media");
    let update_position = projected_position(
        &projected,
        "runtime-system-update:system-update-after-media",
    );
    let assistant_position = projected_position(&projected, "assistant-1");
    let result_position = projected_position(&projected, "tool-result-1");
    assert!(
        media_position < update_position && update_position < assistant_position,
        "the lowered wrapper must land between the media message and the tool pair"
    );
    assert!(
        assistant_position < result_position,
        "the tool pair itself must stay contiguous and ordered"
    );
    let CoreConversationPayload::UserText { attachments, .. } = &projected[media_position].payload
    else {
        panic!("the media message must stay native user content");
    };
    assert_eq!(attachments.len(), 1);
    validate_provider_visible_tool_topology(&projected).expect("valid provider topology");
}

/// Verifies system updates sharing one effective anchor keep their chronological input order.
/// 验证共享同一生效锚点的 system update 会保持时间顺序的输入顺序。
#[test]
fn provider_visible_history_keeps_same_anchor_system_updates_in_input_order() {
    let messages = vec![
        assistant_tool_batch(
            "assistant-1",
            vec![canonical_tool_call(
                "local-1",
                Some("provider-1"),
                "read_file",
            )],
        ),
        tool_result_message("tool-result-1", Some("provider-1"), "file body"),
    ];
    let updates = vec![
        runtime_system_update(
            "system-update-first",
            Some("assistant-1"),
            RuntimeSystemUpdateLoweringHint::Auto,
        ),
        runtime_system_update(
            "system-update-second",
            Some("assistant-1"),
            RuntimeSystemUpdateLoweringHint::Auto,
        ),
    ];

    let projected = core_messages_to_provider_visible_history_for_model_with_system_updates(
        &messages,
        Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        None,
        &updates,
        ProviderSystemUpdateLoweringPolicy,
    )
    .expect("two updates sharing one anchor should project");

    let lowered_ids = projected
        .iter()
        .filter(|message| message.id.starts_with("runtime-system-update:"))
        .map(|message| message.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        lowered_ids,
        vec![
            "runtime-system-update:system-update-first",
            "runtime-system-update:system-update-second",
        ],
        "same-anchor updates must not be reordered by insertion"
    );
    assert!(
        projected_position(&projected, "tool-result-1")
            < projected_position(&projected, "runtime-system-update:system-update-first"),
        "every lowered wrapper must stay after the completed tool batch"
    );
    validate_provider_visible_tool_topology(&projected).expect("valid provider topology");
}
