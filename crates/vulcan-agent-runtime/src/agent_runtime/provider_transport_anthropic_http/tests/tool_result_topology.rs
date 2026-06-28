use super::*;

#[test]
fn request_payload_coalesces_consecutive_tool_results_into_one_user_message() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("anthropic"),
            model_id: String::from("claude-sonnet-4-6"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt").clone(),
            ),
            prompt: String::from("tool result body 4"),
            history: Vec::new(),
            temperature: Some(0.2),
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: Some(json!({})),
            request_headers: None,
            continuation: CoreProviderContinuation::default(),
        },
        current_history: vec![
            CoreConversationMessage {
                id: String::from("assistant-1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::from("并行等待四个子代理完成。"),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-1")),
                    provider_message_state: None,
                    tool_calls: vec![
                        CoreConversationToolCall {
                            id: String::from("local-call-1"),
                            tool_call_id: Some(String::from("call-1")),
                            tool_name: String::from("spawn_agent"),
                            arguments: json!({ "task": "delegate-1" }),
                        },
                        CoreConversationToolCall {
                            id: String::from("local-call-2"),
                            tool_call_id: Some(String::from("call-2")),
                            tool_name: String::from("spawn_agent"),
                            arguments: json!({ "task": "delegate-2" }),
                        },
                        CoreConversationToolCall {
                            id: String::from("local-call-3"),
                            tool_call_id: Some(String::from("call-3")),
                            tool_name: String::from("spawn_agent"),
                            arguments: json!({ "task": "delegate-3" }),
                        },
                        CoreConversationToolCall {
                            id: String::from("local-call-4"),
                            tool_call_id: Some(String::from("call-4")),
                            tool_name: String::from("spawn_agent"),
                            arguments: json!({ "task": "delegate-4" }),
                        },
                    ],
                    usage: None,
                },
            },
            CoreConversationMessage {
                id: String::from("tool-result-1"),
                payload: CoreConversationPayload::ToolResult {
                    tool_result_id: String::from("result-1"),
                    tool_call_id: Some(String::from("call-1")),
                    text: String::from("tool result body 1"),
                },
            },
            CoreConversationMessage {
                id: String::from("tool-result-2"),
                payload: CoreConversationPayload::ToolResult {
                    tool_result_id: String::from("result-2"),
                    tool_call_id: Some(String::from("call-2")),
                    text: String::from("tool result body 2"),
                },
            },
            CoreConversationMessage {
                id: String::from("tool-result-3"),
                payload: CoreConversationPayload::ToolResult {
                    tool_result_id: String::from("result-3"),
                    tool_call_id: Some(String::from("call-3")),
                    text: String::from("tool result body 3"),
                },
            },
        ],
        current_prompt: CoreConversationMessage {
            id: String::from("tool-result-4"),
            payload: CoreConversationPayload::ToolResult {
                tool_result_id: String::from("result-4"),
                tool_call_id: Some(String::from("call-4")),
                text: String::from("tool result body 4"),
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_anthropic_request_payload(
        "claude-sonnet-4-6",
        request,
        &[ProviderToolDefinition {
            name: String::from("spawn_agent"),
            description: String::from("Spawn one delegate agent"),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task": { "type": "string" }
                },
                "required": ["task"]
            }),
        }],
    )
    .expect("payload should build");

    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["role"], "assistant");
    assert_eq!(messages[1]["role"], "user");
    let tool_results = messages[1]["content"]
        .as_array()
        .expect("tool result content should be an array");
    assert_eq!(tool_results.len(), 4);
    assert_eq!(tool_results[0]["type"], "tool_result");
    assert_eq!(tool_results[0]["tool_use_id"], "call-1");
    assert_eq!(tool_results[1]["tool_use_id"], "call-2");
    assert_eq!(tool_results[2]["tool_use_id"], "call-3");
    assert_eq!(tool_results[3]["tool_use_id"], "call-4");
}

/// Verifies empty user content is not treated as a tool-result-only Anthropic message.
/// 验证空 user 内容不会被误判为仅包含工具结果的 Anthropic 消息。
#[test]
fn anthropic_user_tool_result_blocks_ignores_empty_user_content() {
    let message = json!({
        "role": "user",
        "content": []
    });

    assert!(anthropic_user_tool_result_blocks(&message).is_none());
}

/// Verifies mixed Anthropic user content is split so a valid tool result remains adjacent to its assistant tool-use while ordinary text stays replayable.
/// 验证混合 Anthropic user 内容会被拆分，使合法工具结果仍紧邻其 assistant 工具调用，同时普通文本仍可回放。
#[test]
fn repair_anthropic_tool_result_topology_splits_mixed_valid_tool_result_content() {
    let repaired = repair_anthropic_tool_result_topology(vec![
        json!({
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "id": "call-1",
                "name": "image_generation",
                "input": { "prompt": "mountain" }
            }]
        }),
        json!({
            "role": "user",
            "content": [
                {
                    "type": "text",
                    "text": "historical note"
                },
                {
                    "type": "tool_result",
                    "tool_use_id": "call-1",
                    "content": [{
                        "type": "text",
                        "text": "tool result body"
                    }]
                }
            ]
        }),
    ]);

    assert_eq!(repaired.len(), 3);
    assert_eq!(repaired[1]["role"], "user");
    assert_eq!(repaired[1]["content"][0]["type"], "tool_result");
    assert_eq!(repaired[1]["content"][0]["tool_use_id"], "call-1");
    assert_eq!(repaired[2]["role"], "user");
    assert_eq!(repaired[2]["content"][0]["type"], "text");
    assert_eq!(repaired[2]["content"][0]["text"], "historical note");
}

/// Verifies mixed Anthropic user content downgrades a tool result after ordinary text has broken the required assistant adjacency.
/// 验证普通文本已打断必需的 assistant 相邻关系后，混合 Anthropic user 内容中的工具结果会被降级。
#[test]
fn repair_anthropic_tool_result_topology_downgrades_mixed_orphan_after_user_text() {
    let repaired = repair_anthropic_tool_result_topology(vec![
        json!({
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "id": "call-1",
                "name": "image_generation",
                "input": { "prompt": "mountain" }
            }]
        }),
        json!({
            "role": "user",
            "content": [{
                "type": "text",
                "text": "historical interruption"
            }]
        }),
        json!({
            "role": "user",
            "content": [
                {
                    "type": "text",
                    "text": "preserved note"
                },
                {
                    "type": "tool_result",
                    "tool_use_id": "call-1",
                    "content": [{
                        "type": "text",
                        "text": "tool result body"
                    }]
                }
            ]
        }),
    ]);

    assert_eq!(repaired.len(), 3);
    let content = repaired[2]["content"]
        .as_array()
        .expect("mixed downgraded content should be an array");
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[0]["text"], "preserved note");
    assert_eq!(content[1]["type"], "text");
    assert!(
        content[1]["text"]
            .as_str()
            .expect("downgraded content should be text")
            .contains("no immediately preceding matching tool_use")
    );
}

/// Verifies Anthropic request payloads downgrade an orphan tool result into ordinary user text instead of sending an illegal leading tool_result block.
/// 验证 Anthropic 请求载荷会把孤儿工具结果降级为普通用户文本，而不是发送非法的前置 tool_result 内容块。
#[test]
fn request_payload_downgrades_leading_orphan_tool_result() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: build_test_request_snapshot("tool result body"),
        current_history: Vec::new(),
        current_prompt: CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_result_id: String::from("result-1"),
                tool_call_id: Some(String::from("call-1")),
                text: String::from("tool result body"),
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_anthropic_request_payload("claude-sonnet-4-6", request, &[])
        .expect("payload should build");
    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    let content = messages[0]["content"]
        .as_array()
        .expect("downgraded content should be an array");

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(content[0]["type"], "text");
    assert_eq!(anthropic_downgraded_tool_result_count(&payload), 1);
    assert!(
        content[0]["text"]
            .as_str()
            .expect("downgraded content should be text")
            .contains("no immediately preceding matching tool_use")
    );
}

/// Verifies Anthropic request payloads downgrade tool results whose ids do not match the immediately preceding assistant tool-use batch.
/// 验证 Anthropic 请求载荷会降级与紧邻上一条 assistant 工具调用批次不匹配的工具结果。
#[test]
fn request_payload_downgrades_mismatched_tool_result_id() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: build_test_request_snapshot("tool result body"),
        current_history: vec![CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::from("我先调用工具。"),
                reasoning: String::new(),
                provider_message_id: Some(String::from("resp-1")),
                provider_message_state: None,
                tool_calls: vec![CoreConversationToolCall {
                    id: String::from("tool-1"),
                    tool_call_id: Some(String::from("call-1")),
                    tool_name: String::from("read_file"),
                    arguments: json!({ "path": "src/main.rs" }),
                }],
                usage: None,
            },
        }],
        current_prompt: CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_result_id: String::from("result-1"),
                tool_call_id: Some(String::from("call-2")),
                text: String::from("tool result body"),
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_anthropic_request_payload(
        "claude-sonnet-4-6",
        request,
        &[ProviderToolDefinition {
            name: String::from("read_file"),
            description: String::from("Read one file"),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        }],
    )
    .expect("payload should build");
    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    let content = messages[1]["content"]
        .as_array()
        .expect("downgraded content should be an array");

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["content"][1]["id"], "call-1");
    assert_eq!(messages[1]["role"], "user");
    assert_eq!(content[0]["type"], "text");
    assert_eq!(anthropic_downgraded_tool_result_count(&payload), 1);
    assert!(
        content[0]["text"]
            .as_str()
            .expect("downgraded content should be text")
            .contains("tool_use_id=\"call-2\"")
    );
}
