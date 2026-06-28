use super::*;

/// Verifies OpenAI-compatible request payloads append host function tools after provider-native tool patches instead of overwriting them.
/// 验证 OpenAI 兼容请求载荷会在 Provider 原生工具补丁之后追加宿主函数工具，而不是将其覆盖。
#[test]
fn request_payload_appends_function_tools_after_existing_native_tools() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("alibaba-coding-plan-cn"),
            model_id: String::from("qwen3.6-plus"),
            system_prompt: String::new(),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::new().clone(),
            ),
            prompt: String::from("请搜索最新资料"),
            history: Vec::new(),
            temperature: None,
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: Some(json!({
                "enable_search": true,
                "tools": [
                    { "type": "web_search" }
                ]
            })),
            request_headers: None,
            continuation: CoreProviderContinuation::default(),
        },
        current_history: Vec::new(),
        current_prompt: CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("请搜索最新资料"),
                attachments: Vec::new(),
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_openai_compatible_request_payload(
        "qwen3.6-plus",
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
        true,
    )
    .expect("payload should build");

    let tools = payload["tools"]
        .as_array()
        .expect("tools should be an array");
    assert_eq!(tools[0]["type"], "web_search");
    assert_eq!(tools[1]["function"]["name"], "read_file");
    assert_eq!(payload["enable_search"], true);
    assert!(payload.get("search_options").is_none());
    assert_eq!(payload["stream_options"]["include_usage"], true);
}

/// Verifies OpenAI-compatible function-tool conversion fills in explicit array and nested object types so Gemini-backed proxy routes can validate host schemas.
/// 验证 OpenAI 兼容函数工具转换会补齐显式数组与嵌套对象类型，以便 Gemini 背后的代理路由能够校验宿主 schema。
#[test]
fn openai_compatible_tool_conversion_normalizes_nested_container_types() {
    let tool = provider_tool_definition_to_openai_compatible_tool(
        &ProviderToolDefinition {
            name: String::from("read_nodes"),
            description: String::from("Read structured nodes"),
            parameters: json!({
                "type": "object",
                "properties": {
                    "nodes": {
                        "items": {
                            "properties": {
                                "file": { "type": "string" },
                                "selector": { "type": "string" }
                            },
                            "required": ["file", "selector"]
                        }
                    }
                }
            }),
        },
        VulcanToolSchemaProjectionProfile::StrictContainer,
    );

    assert_eq!(
        tool["function"]["parameters"]["properties"]["nodes"]["type"],
        json!("array")
    );
    assert_eq!(
        tool["function"]["parameters"]["properties"]["nodes"]["items"]["type"],
        json!("object")
    );
}

/// Verifies OpenRouter Gemini-family models use the compact Gemini schema projection in the final wire payload.
/// 验证 OpenRouter 的 Gemini 模型会在最终线协议载荷中使用紧凑 Gemini Schema 投影。
#[test]
fn openrouter_gemini_payload_uses_compact_schema_projection() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("openrouter"),
            model_id: String::from("google/gemini-2.5-pro"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt"),
            ),
            prompt: String::from("test request"),
            history: Vec::new(),
            temperature: None,
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: None,
            request_headers: None,
            continuation: CoreProviderContinuation::default(),
        },
        current_history: Vec::new(),
        current_prompt: CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("test request"),
                attachments: Vec::new(),
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_openai_compatible_request_payload(
        "google/gemini-2.5-pro",
        request,
        &[ProviderToolDefinition {
            name: String::from("vulcan_codekit_patch"),
            description: String::from("Patch function nodes"),
            parameters: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "file": { "type": "string" },
                    "replacement": { "type": "string" },
                    "patches": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "file": { "type": "string" },
                                "replacement": { "type": "string" }
                            },
                            "required": ["file", "replacement"]
                        }
                    }
                },
                "oneOf": [
                    { "required": ["file", "replacement"] },
                    { "required": ["patches"] }
                ]
            }),
        }],
        true,
    )
    .expect("payload should build");

    let parameters = &payload["tools"][0]["function"]["parameters"];
    assert!(parameters["additionalProperties"].is_null());
    assert_eq!(
        parameters["oneOf"],
        json!([
            {
                "type": "object",
                "required": ["file", "replacement"],
                "properties": {
                    "file": { "type": "string" },
                    "replacement": { "type": "string" }
                }
            },
            {
                "type": "object",
                "required": ["patches"],
                "properties": {
                    "patches": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "file": { "type": "string" },
                                "replacement": { "type": "string" }
                            },
                            "required": ["file", "replacement"]
                        }
                    }
                }
            }
        ])
    );
    assert!(parameters["oneOf"][0]["properties"]["patches"].is_null());
}

/// Verifies OpenAI-compatible error augmentation maps Gemini-style function declaration indices back to the original host tool names.
/// 验证 OpenAI 兼容错误增强逻辑会把 Gemini 风格的函数声明索引映射回原始宿主工具名称。
#[test]
fn openai_compatible_error_augmentation_reports_tool_names_for_function_indices() {
    let tool_definitions = vec![
        ProviderToolDefinition {
            name: String::from("ask"),
            description: String::from("Ask the user"),
            parameters: json!({"type":"object"}),
        },
        ProviderToolDefinition {
            name: String::from("vulcan_codekit_node_source"),
            description: String::from("Read node source"),
            parameters: json!({"type":"object"}),
        },
        ProviderToolDefinition {
            name: String::from("vulcan_codekit_patch"),
            description: String::from("Patch one node"),
            parameters: json!({"type":"object"}),
        },
    ];
    let body = "* GenerateContentRequest.tools[0].function_declarations[1].parameters.properties[nodes].items: missing field.\n\
* GenerateContentRequest.tools[0].function_declarations[2].parameters.properties[patches].items: missing field.";

    let message = augment_openai_compatible_error_with_tool_context(
        String::from("HTTP 400 Bad Request | Provider returned error"),
        body,
        &tool_definitions,
    );

    assert_eq!(
        collect_openai_compatible_function_declaration_indices(body),
        vec![1, 2]
    );
    assert!(message.contains("tool[1]=vulcan_codekit_node_source"));
    assert!(message.contains("tool[2]=vulcan_codekit_patch"));
}
