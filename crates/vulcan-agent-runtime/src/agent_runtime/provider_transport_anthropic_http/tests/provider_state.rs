use super::*;

#[test]
fn request_payload_replays_stored_anthropic_provider_message_state() {
    let assistant_state = build_anthropic_provider_message_state(
        "anthropic",
        "claude-sonnet-4-6",
        &[
            AnthropicContentBlock::Thinking {
                thinking: String::from("先检查目录。"),
                signature: Some(String::from("sig-1")),
            },
            AnthropicContentBlock::Text {
                text: String::from("我先读取文件。"),
            },
            AnthropicContentBlock::ToolUse {
                id: String::from("call-1"),
                name: String::from("functions_shell_command"),
                input: json!({ "command": "Get-ChildItem" }),
            },
        ],
    );
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("anthropic"),
            model_id: String::from("claude-sonnet-4-6"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt").clone(),
            ),
            prompt: String::from("tool result body"),
            history: Vec::new(),
            temperature: Some(0.2),
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: Some(json!({})),
            request_headers: None,
            continuation: CoreProviderContinuation::default(),
        },
        current_history: vec![CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::from("我先读取文件。"),
                reasoning: String::from("先检查目录。"),
                provider_message_id: Some(String::from("resp-1")),
                provider_message_state: Some(assistant_state),
                tool_calls: vec![CoreConversationToolCall {
                    id: String::from("tool-1"),
                    tool_call_id: Some(String::from("call-1")),
                    tool_name: String::from("functions.shell_command"),
                    arguments: json!({ "command": "Get-ChildItem" }),
                }],
                usage: None,
            },
        }],
        current_prompt: CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("call-1")),
                text: String::from("tool result body"),
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_anthropic_request_payload(
        "claude-sonnet-4-6",
        request,
        &[ProviderToolDefinition {
            name: String::from("functions.shell_command"),
            description: String::from("Run one shell command"),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" }
                },
                "required": ["command"]
            }),
        }],
    )
    .expect("payload should build");

    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    assert_eq!(messages[0]["content"][0]["type"], "thinking");
    assert_eq!(messages[0]["content"][0]["thinking"], "先检查目录。");
    assert_eq!(messages[0]["content"][0]["signature"], "sig-1");
    assert_eq!(messages[0]["content"][2]["type"], "tool_use");
    assert_eq!(messages[0]["content"][2]["name"], "functions_shell_command");
}

/// Verifies Anthropic replay ignores foreign provider-message-state payloads and rebuilds the assistant tool turn from canonical text/tool-call fields when a session switches from another protocol family.
/// 验证当会话从其他协议家族切换过来时，Anthropic 回放会忽略外部 provider-message-state，并改为基于规范文本与工具调用字段重建助手工具回合。
#[test]
fn request_payload_ignores_foreign_provider_message_state_after_protocol_switch() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("anthropic"),
            model_id: String::from("claude-sonnet-4-6"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt").clone(),
            ),
            prompt: String::from("tool result body"),
            history: Vec::new(),
            temperature: Some(0.2),
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: Some(json!({})),
            request_headers: None,
            continuation: CoreProviderContinuation::default(),
        },
        current_history: vec![CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::from("我先继续调用工具。"),
                reasoning: String::new(),
                provider_message_id: Some(String::from("foreign-resp-1")),
                provider_message_state: Some(json!({
                    "gemini_native": {
                        "role": "model",
                        "parts": [{ "text": "这是一段外部协议状态" }]
                    }
                })),
                tool_calls: vec![CoreConversationToolCall {
                    id: String::from("local-call-1"),
                    tool_call_id: Some(String::from("call-1")),
                    tool_name: String::from("spawn_agent"),
                    arguments: json!({ "task": "delegate-1" }),
                }],
                usage: None,
            },
        }],
        current_prompt: CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("call-1")),
                text: String::from("tool result body"),
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
    assert_eq!(messages[0]["role"], "assistant");
    assert_eq!(messages[0]["content"][0]["type"], "text");
    assert_eq!(messages[0]["content"][0]["text"], "我先继续调用工具。");
    assert_eq!(messages[0]["content"][1]["type"], "tool_use");
    assert_eq!(messages[0]["content"][1]["id"], "call-1");
    assert_eq!(messages[1]["content"][0]["tool_use_id"], "call-1");
}

/// Verifies Anthropic replay falls back to the local tool-call identifier when the upstream provider omits a stable visible id.
/// 验证当上游 Provider 缺失稳定可见 id 时，Anthropic 回放会回退到本地工具调用标识。
#[test]
fn request_payload_falls_back_to_local_tool_id_when_provider_visible_id_is_missing() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("anthropic"),
            model_id: String::from("claude-sonnet-4-6"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt").clone(),
            ),
            prompt: String::from("tool result body"),
            history: Vec::new(),
            temperature: Some(0.2),
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: Some(json!({})),
            request_headers: None,
            continuation: CoreProviderContinuation::default(),
        },
        current_history: vec![CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::from("我先读取文件。"),
                reasoning: String::from("这段思考不应回放。"),
                provider_message_id: Some(String::from("resp-1")),
                provider_message_state: None,
                tool_calls: vec![CoreConversationToolCall {
                    id: String::from("local-call-1"),
                    tool_call_id: Some(String::from("")),
                    tool_name: String::from("read_file"),
                    arguments: json!({ "path": "src/main.rs" }),
                }],
                usage: None,
            },
        }],
        current_prompt: CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("local-call-1")),
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
    assert_eq!(messages[0]["content"][2]["id"], "local-call-1");
    assert_eq!(messages[1]["content"][0]["tool_use_id"], "local-call-1");
}

/// Verifies newly stored Anthropic provider-message-state tool-use blocks already carry the replayable local fallback identifier when the upstream provider omitted its visible id.
/// 验证当上游 Provider 缺失可见 id 时，新写入的 Anthropic provider-message-state 工具块会直接携带可回放的本地回退标识。
#[test]
fn anthropic_provider_state_stores_replayable_local_tool_id_when_upstream_id_is_blank() {
    let block = anthropic_content_block_from_pending_tool_use(&PendingAnthropicToolUse {
        id: String::new(),
        name: String::from("spawn_agent"),
        input_json: String::from("{\"task\":\"delegate-1\"}").into(),
    })
    .expect("tool-use block should build");

    match block {
        AnthropicContentBlock::ToolUse { id, name, input } => {
            assert_eq!(id, "spawn_agent:{\"task\":\"delegate-1\"}");
            assert_eq!(name, "spawn_agent");
            assert_eq!(input, json!({ "task": "delegate-1" }));
        }
        other => panic!("unexpected content block: {other:?}"),
    }
}

/// Verifies Anthropic request replay heals older stored provider-message-state blocks whose tool-use ids were blank, so legacy broken sessions can still resend parallel tool results.
/// 验证 Anthropic 请求回放会修复历史上已存入空白 tool-use id 的 provider-message-state，从而让旧的损坏会话仍能重放并行工具结果。
#[test]
fn request_payload_repairs_stored_blank_tool_use_ids_for_parallel_tool_results() {
    let assistant_state = build_anthropic_provider_message_state(
        "anthropic",
        "claude-sonnet-4-6",
        &[
            AnthropicContentBlock::ToolUse {
                id: String::new(),
                name: String::from("spawn_agent"),
                input: json!({ "task": "delegate-1" }),
            },
            AnthropicContentBlock::ToolUse {
                id: String::new(),
                name: String::from("spawn_agent"),
                input: json!({ "task": "delegate-2" }),
            },
            AnthropicContentBlock::ToolUse {
                id: String::new(),
                name: String::from("spawn_agent"),
                input: json!({ "task": "delegate-3" }),
            },
            AnthropicContentBlock::ToolUse {
                id: String::new(),
                name: String::from("spawn_agent"),
                input: json!({ "task": "delegate-4" }),
            },
        ],
    );
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
                    text: String::new(),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-1")),
                    provider_message_state: Some(assistant_state),
                    tool_calls: vec![
                        CoreConversationToolCall {
                            id: String::from("local-call-1"),
                            tool_call_id: None,
                            tool_name: String::from("spawn_agent"),
                            arguments: json!({ "task": "delegate-1" }),
                        },
                        CoreConversationToolCall {
                            id: String::from("local-call-2"),
                            tool_call_id: None,
                            tool_name: String::from("spawn_agent"),
                            arguments: json!({ "task": "delegate-2" }),
                        },
                        CoreConversationToolCall {
                            id: String::from("local-call-3"),
                            tool_call_id: None,
                            tool_name: String::from("spawn_agent"),
                            arguments: json!({ "task": "delegate-3" }),
                        },
                        CoreConversationToolCall {
                            id: String::from("local-call-4"),
                            tool_call_id: None,
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
                    tool_call_id: Some(String::from("local-call-1")),
                    text: String::from("tool result body 1"),
                },
            },
            CoreConversationMessage {
                id: String::from("tool-result-2"),
                payload: CoreConversationPayload::ToolResult {
                    tool_call_id: Some(String::from("local-call-2")),
                    text: String::from("tool result body 2"),
                },
            },
            CoreConversationMessage {
                id: String::from("tool-result-3"),
                payload: CoreConversationPayload::ToolResult {
                    tool_call_id: Some(String::from("local-call-3")),
                    text: String::from("tool result body 3"),
                },
            },
        ],
        current_prompt: CoreConversationMessage {
            id: String::from("tool-result-4"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("local-call-4")),
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
    assert_eq!(messages[0]["content"][0]["id"], "local-call-1");
    assert_eq!(messages[0]["content"][1]["id"], "local-call-2");
    assert_eq!(messages[0]["content"][2]["id"], "local-call-3");
    assert_eq!(messages[0]["content"][3]["id"], "local-call-4");
    let tool_results = messages[1]["content"]
        .as_array()
        .expect("tool result content should be an array");
    assert_eq!(tool_results[0]["tool_use_id"], "local-call-1");
    assert_eq!(tool_results[1]["tool_use_id"], "local-call-2");
    assert_eq!(tool_results[2]["tool_use_id"], "local-call-3");
    assert_eq!(tool_results[3]["tool_use_id"], "local-call-4");
}

#[test]
fn anthropic_provider_state_stores_replayable_server_tool_use() {
    let block =
        anthropic_content_block_from_pending_server_tool_use(&PendingAnthropicServerToolUse {
            id: String::new(),
            name: String::from("web_search"),
            input_json: String::from("{\"query\":\"vulcan\"}"),
        });

    match block {
        AnthropicContentBlock::ServerToolUse { id, name, input } => {
            assert_eq!(id, "server:web_search:{\"query\":\"vulcan\"}");
            assert_eq!(name, "web_search");
            assert_eq!(input, json!({ "query": "vulcan" }));
        }
        other => panic!("unexpected content block: {other:?}"),
    }
}
