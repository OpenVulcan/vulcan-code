use super::*;

/// Verifies OpenAI-compatible transports keep parallel tool results as separate `tool` messages instead of borrowing Anthropic-only coalescing behavior.
/// 验证 OpenAI 兼容传输会把并行工具结果保持为独立的 `tool` 消息，而不会借用 Anthropic 专用的合并行为。
#[test]
fn request_payload_keeps_parallel_tool_results_split_for_openai_compatible() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-test"),
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
                    tool_call_id: Some(String::from("call-1")),
                    text: String::from("tool result body 1"),
                },
            },
            CoreConversationMessage {
                id: String::from("tool-result-2"),
                payload: CoreConversationPayload::ToolResult {
                    tool_call_id: Some(String::from("call-2")),
                    text: String::from("tool result body 2"),
                },
            },
            CoreConversationMessage {
                id: String::from("tool-result-3"),
                payload: CoreConversationPayload::ToolResult {
                    tool_call_id: Some(String::from("call-3")),
                    text: String::from("tool result body 3"),
                },
            },
        ],
        current_prompt: CoreConversationMessage {
            id: String::from("tool-result-4"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("call-4")),
                text: String::from("tool result body 4"),
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_openai_compatible_request_payload(
        "gpt-test",
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
        true,
    )
    .expect("payload should build");

    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    assert_eq!(messages.len(), 6);
    assert_eq!(messages[0]["role"], "system");
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[1]["tool_calls"][0]["id"], "call-1");
    assert_eq!(messages[1]["tool_calls"][1]["id"], "call-2");
    assert_eq!(messages[1]["tool_calls"][2]["id"], "call-3");
    assert_eq!(messages[1]["tool_calls"][3]["id"], "call-4");
    assert_eq!(messages[2]["role"], "tool");
    assert_eq!(messages[3]["role"], "tool");
    assert_eq!(messages[4]["role"], "tool");
    assert_eq!(messages[5]["role"], "tool");
    assert_eq!(messages[2]["tool_call_id"], "call-1");
    assert_eq!(messages[3]["tool_call_id"], "call-2");
    assert_eq!(messages[4]["tool_call_id"], "call-3");
    assert_eq!(messages[5]["tool_call_id"], "call-4");
}
