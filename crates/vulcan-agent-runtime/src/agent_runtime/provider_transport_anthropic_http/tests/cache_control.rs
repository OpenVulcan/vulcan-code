use super::*;

#[test]
fn request_payload_marks_system_block_with_ephemeral_cache_control() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: build_test_request_snapshot("hello"),
        current_history: Vec::new(),
        current_prompt: CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("hello"),
                attachments: Vec::new(),
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_anthropic_request_payload("claude-sonnet-4-6", request, &[])
        .expect("anthropic payload should build");

    assert_eq!(
        payload["system"][0]["cache_control"]["type"],
        json!("ephemeral")
    );
}

/// Verifies Anthropic request payloads attach an ephemeral cache-control hint to the latest user-authored block.
/// 验证 Anthropic 请求载荷会把 ephemeral 缓存提示附加到最新用户创作内容块。
#[test]
fn request_payload_marks_latest_user_block_with_ephemeral_cache_control() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: build_test_request_snapshot("new prompt"),
        current_history: vec![CoreConversationMessage {
            id: String::from("user-0"),
            payload: CoreConversationPayload::UserText {
                text: String::from("old prompt"),
                attachments: Vec::new(),
            },
        }],
        current_prompt: CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("new prompt"),
                attachments: Vec::new(),
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_anthropic_request_payload("claude-sonnet-4-6", request, &[])
        .expect("anthropic payload should build");
    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    let latest_content = messages[1]["content"]
        .as_array()
        .expect("content should be an array")
        .last()
        .expect("latest user block should exist");

    assert_eq!(latest_content["text"], "new prompt");
    assert_eq!(latest_content["cache_control"]["type"], "ephemeral");
}

/// Verifies Anthropic request payloads attach an ephemeral cache-control hint to the latest tool result block.
/// 验证 Anthropic 请求载荷会把 ephemeral 缓存提示附加到最新工具结果内容块。
#[test]
fn request_payload_marks_latest_tool_result_block_with_ephemeral_cache_control() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: build_test_request_snapshot("tool result body"),
        current_history: vec![CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::from("我会读取文件。"),
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
    .expect("anthropic payload should build");
    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    let tool_result = &messages[1]["content"][0];

    assert_eq!(tool_result["type"], "tool_result");
    assert_eq!(tool_result["tool_use_id"], "call-1");
    assert_eq!(tool_result["cache_control"]["type"], "ephemeral");
}

/// Verifies Anthropic request payloads attach an ephemeral cache-control hint to the latest tool definition.
/// 验证 Anthropic 请求载荷会把 ephemeral 缓存提示附加到最新工具定义。
#[test]
fn request_payload_marks_latest_tool_definition_with_ephemeral_cache_control() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: build_test_request_snapshot("hello"),
        current_history: Vec::new(),
        current_prompt: CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("hello"),
                attachments: Vec::new(),
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_anthropic_request_payload(
        "claude-sonnet-4-6",
        request,
        &[
            ProviderToolDefinition {
                name: String::from("read_file"),
                description: String::from("Read one file"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
            },
            ProviderToolDefinition {
                name: String::from("write_file"),
                description: String::from("Write one file"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["path", "content"]
                }),
            },
        ],
    )
    .expect("anthropic payload should build");

    assert!(payload["tools"][0].get("cache_control").is_none());
    assert_eq!(payload["tools"][1]["name"], "write_file");
    assert_eq!(payload["tools"][1]["cache_control"]["type"], "ephemeral");
}
