use super::*;

#[test]
fn finalize_pending_anthropic_tool_use_synthesizes_local_identifier() {
    let tool_call = finalize_pending_anthropic_tool_use(
        PendingAnthropicToolUse {
            id: String::new(),
            name: String::from("read_file"),
            input_json: String::from("{\"path\":\"src/main.rs\"}"),
        },
        &[],
    )
    .expect("tool call should finalize");

    assert_eq!(tool_call.id, "read_file:{\"path\":\"src/main.rs\"}");
    assert_eq!(tool_call.tool_call_id, None);
    assert_eq!(tool_call.tool_name, "read_file");
    assert_eq!(tool_call.arguments, json!({ "path": "src/main.rs" }));
}

/// Verifies Anthropic request replay still sanitizes one historical invalid host tool identifier while the current provider tool definition already carries the unified safe public name.
/// 验证当当前 Provider 工具定义已经使用统一安全公开名时，Anthropic 请求回放仍会清洗历史遗留的非法宿主工具标识。
#[test]
fn request_payload_sanitizes_invalid_anthropic_tool_names() {
    let host_tool_name = "functions.shell_command";
    let provider_tool_name = build_anthropic_provider_tool_name(host_tool_name);
    assert_eq!(provider_tool_name, "functions_shell_command");
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
                text: String::from("先执行命令。"),
                reasoning: String::new(),
                provider_message_id: Some(String::from("resp-1")),
                provider_message_state: None,
                tool_calls: vec![CoreConversationToolCall {
                    id: String::from("tool-1"),
                    tool_call_id: Some(String::from("call-1")),
                    tool_name: String::from(host_tool_name),
                    arguments: json!({ "command": "pwd" }),
                }],
                usage: None,
            },
        }],
        current_prompt: CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_result_id: String::from("result-1"),
                tool_call_id: Some(String::from("call-1")),
                text: String::from(r"D:\openvulcan\vulcan-code"),
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_anthropic_request_payload(
        "claude-sonnet-4-6",
        request,
        &[ProviderToolDefinition {
            name: provider_tool_name.clone(),
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
    assert_eq!(messages[0]["content"][1]["name"], provider_tool_name);
    assert_eq!(payload["tools"][0]["name"], provider_tool_name);
}

/// Verifies Anthropic request payloads preserve provider-native server tools while appending host function tools.
/// 验证 Anthropic 请求载荷会保留 Provider 原生服务端工具，同时追加宿主函数工具。
#[test]
fn request_payload_preserves_native_web_search_and_appends_function_tools() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("anthropic"),
            model_id: String::from("claude-sonnet-4-6"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt").clone(),
            ),
            prompt: String::from("search and read"),
            history: Vec::new(),
            temperature: Some(0.2),
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: Some(json!({
                "tools": [{
                    "type": "web_search_20250305",
                    "name": "web_search"
                }]
            })),
            request_headers: None,
            continuation: CoreProviderContinuation::default(),
        },
        current_history: Vec::new(),
        current_prompt: CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("search and read"),
                attachments: Vec::new(),
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

    let tools = payload["tools"]
        .as_array()
        .expect("tools should be an array");
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0]["type"], "web_search_20250305");
    assert_eq!(tools[0]["name"], "web_search");
    assert_eq!(tools[1]["name"], "read_file");
    assert_eq!(tools[1]["description"], "Read one file");
    assert_eq!(payload["tool_choice"]["type"], "auto");
}

#[test]
fn finalize_pending_anthropic_tool_use_restores_host_tool_name() {
    let host_tool_name = "functions.shell_command";
    let tool_call = finalize_pending_anthropic_tool_use(
        PendingAnthropicToolUse {
            id: String::from("call-1"),
            name: build_anthropic_provider_tool_name(host_tool_name),
            input_json: String::from("{\"command\":\"pwd\"}"),
        },
        &[ProviderToolDefinition {
            name: String::from(host_tool_name),
            description: String::from("Run one shell command"),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" }
                }
            }),
        }],
    )
    .expect("tool call should finalize");

    assert_eq!(tool_call.tool_name, host_tool_name);
    assert_eq!(tool_call.tool_call_id.as_deref(), Some("call-1"));
    assert_eq!(tool_call.arguments, json!({ "command": "pwd" }));
}
