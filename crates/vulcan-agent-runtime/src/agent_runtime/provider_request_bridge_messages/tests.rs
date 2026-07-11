use super::{
    core_messages_to_provider_visible_history_for_model,
    core_messages_to_provider_visible_history_for_model_with_system_updates,
    validate_provider_visible_tool_topology,
};
use crate::agent_runtime::provider_system_update_lowering::ProviderSystemUpdateLoweringPolicy;
use crate::provider_capabilities::ResolvedModelCapabilities;
use crate::provider_config::ProviderApiFamily;
use vulcan_context::{
    RuntimeSystemUpdateEvent, RuntimeSystemUpdateLoweringHint, RuntimeSystemUpdateTrust,
};
use vulcan_core::types::{
    ConversationAttachment, ConversationAttachmentKind,
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationToolCall as CoreConversationToolCall,
};

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
        source_path: None,
        asset_id: Some(format!("asset-{file_name}")),
        byte_len: Some(4),
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
        Some(ProviderApiFamily::OpenAiChatCompatible),
        None,
        &updates,
        ProviderSystemUpdateLoweringPolicy::default(),
    )
    .expect("system update wrapper should project");

    assert_eq!(projected.len(), 2);
    assert_eq!(projected[1].id, "runtime-system-update:system-update-1");
    let CoreConversationPayload::UserText { text, attachments } = &projected[1].payload else {
        panic!("system update should become user-scoped wrapper text");
    };
    assert!(attachments.is_empty());
    assert!(text.contains("user scoped"));
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
                }],
            },
        },
        CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("provider-call-1")),
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
        Some(ProviderApiFamily::OpenAiChatCompatible),
        None,
        &updates,
        ProviderSystemUpdateLoweringPolicy::default(),
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
                    },
                    CoreConversationToolCall {
                        id: String::from("local-call-2"),
                        tool_call_id: Some(String::from("provider-call-2")),
                        tool_name: String::from("list_files"),
                        arguments: serde_json::json!({}),
                    },
                ],
            },
        },
        CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("provider-call-1")),
                text: String::from("file body"),
            },
        },
        CoreConversationMessage {
            id: String::from("tool-result-2"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("provider-call-2")),
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
        Some(ProviderApiFamily::OpenAiChatCompatible),
        None,
        &updates,
        ProviderSystemUpdateLoweringPolicy::default(),
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
    let capabilities = ResolvedModelCapabilities {
        input_modalities: vec![String::from("text")],
        ..ResolvedModelCapabilities::default()
    };
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
        Some(ProviderApiFamily::OpenAiChatCompatible),
        Some(&capabilities),
    );

    let CoreConversationPayload::UserText { text, attachments } = &projected[0].payload else {
        panic!("expected projected user text");
    };
    assert!(attachments.is_empty());
    assert!(text.contains("selected model"));
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
        Some(ProviderApiFamily::OpenAiChatCompatible),
        None,
    );

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
                }],
            },
        },
        CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("provider-call")),
                text: String::from("file body"),
            },
        },
    ];

    let projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(ProviderApiFamily::OpenAiChatCompatible),
        None,
    );

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
                    },
                    CoreConversationToolCall {
                        id: String::from("local-call-2"),
                        tool_call_id: Some(String::from("provider-call-2")),
                        tool_name: String::from("second_tool"),
                        arguments: serde_json::json!({}),
                    },
                ],
            },
        },
        CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("provider-call-1")),
                text: String::from("first done"),
            },
        },
        CoreConversationMessage {
            id: String::from("tool-result-2"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("local-call-2")),
                text: String::from("second done"),
            },
        },
    ];

    let projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(ProviderApiFamily::OpenAiChatCompatible),
        None,
    );

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
            text: String::from("orphan body"),
        },
    }];

    let projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(ProviderApiFamily::OpenAiChatCompatible),
        None,
    );

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
                    },
                    CoreConversationToolCall {
                        id: String::from("local-call-2"),
                        tool_call_id: Some(String::from("provider-call-2")),
                        tool_name: String::from("second_tool"),
                        arguments: serde_json::json!({"step": 2}),
                    },
                ],
            },
        },
        CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("provider-call-1")),
                text: String::from("first done"),
            },
        },
    ];

    let projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(ProviderApiFamily::OpenAiChatCompatible),
        None,
    );

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
            }],
        },
    }];

    let projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(ProviderApiFamily::OpenAiChatCompatible),
        None,
    );

    assert_eq!(projected.len(), 1);
    let CoreConversationPayload::UserText { text, attachments } = &projected[0].payload else {
        panic!("unanswered tool batch should downgrade to text");
    };
    assert!(attachments.is_empty());
    assert!(text.contains("interrupted before every requested tool returned"));
    assert!(text.contains("ask"));
    assert!(text.contains("provider-call-1"));
}

/// Verifies the plain-text canonical fixture keeps user and assistant turns in deterministic order.
/// 验证普通文本 canonical fixture 会以确定顺序保留用户与助手回合。
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
                reasoning: String::from("internal reasoning should be stripped"),
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
        Some(ProviderApiFamily::OpenAiChatCompatible),
        None,
    );

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
    assert!(reasoning.is_empty());
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
                }],
            },
        },
        CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("provider-call-1")),
                text: String::from("file body"),
            },
        },
    ];

    let projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(ProviderApiFamily::OpenAiChatCompatible),
        None,
    );

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
        Some(ProviderApiFamily::OpenAiChatCompatible),
        None,
    );

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
        Some(ProviderApiFamily::OpenAiChatCompatible),
        None,
        &updates,
        ProviderSystemUpdateLoweringPolicy::default(),
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
    assert!(text.contains("user scoped"));
}

/// Verifies multimodal canonical fixtures preserve image and video attachments only when provider and model support them.
/// 验证多模态 canonical fixture 只会在 provider 与模型均支持时保留图片和视频附件。
#[test]
fn canonical_fixture_image_and_video_understanding_respects_model_capabilities() {
    let capabilities = ResolvedModelCapabilities {
        input_modalities: vec![
            String::from("text"),
            String::from("image"),
            String::from("video"),
        ],
        ..ResolvedModelCapabilities::default()
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

    let gemini_projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(ProviderApiFamily::GoogleGeminiGenerateContent),
        Some(&capabilities),
    );
    let CoreConversationPayload::UserText {
        attachments: gemini_attachments,
        ..
    } = &gemini_projected[0].payload
    else {
        panic!("Gemini fixture should remain user text");
    };
    assert_eq!(gemini_attachments.len(), 2);

    let openai_projected = core_messages_to_provider_visible_history_for_model(
        &messages,
        Some(ProviderApiFamily::OpenAiChatCompatible),
        Some(&capabilities),
    );
    let CoreConversationPayload::UserText {
        text,
        attachments: openai_attachments,
    } = &openai_projected[0].payload
    else {
        panic!("OpenAI-compatible fixture should remain user text");
    };
    assert_eq!(openai_attachments.len(), 1);
    assert_eq!(
        openai_attachments[0].kind,
        ConversationAttachmentKind::Image
    );
    assert!(text.contains("this API path"));
}
