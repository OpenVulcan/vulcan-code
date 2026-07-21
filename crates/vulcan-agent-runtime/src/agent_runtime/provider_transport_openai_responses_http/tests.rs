use super::{
    append_openai_responses_tool_request_delta, build_openai_responses_request_payload,
    build_openai_responses_turn_response, complete_openai_responses_tool_request_from_done_event,
    complete_openai_responses_tool_request_from_output_item,
    start_openai_responses_tool_request_from_output_item,
    synthesize_openai_responses_fallback_response,
};
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use crate::agent_runtime::tool_request_stream::ToolRequestFragment;
use serde_json::json;
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};
use vulcan_core::types::{
    ConversationAttachment, ConversationAttachmentKind,
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationToolCall as CoreConversationToolCall,
    ProviderContinuation as CoreProviderContinuation,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
};
use vulcan_tools_manager::{VulcanToolSchemaProjectionProfile, project_tool_parameters_schema};

/// Builds one temporary attachment used by OpenAI Responses multimodal tests.
/// 构建一条供 OpenAI Responses 多模态测试使用的临时附件。
fn build_test_attachment(
    extension: &str,
    kind: ConversationAttachmentKind,
    mime_type: &str,
    file_name: &str,
) -> ConversationAttachment {
    let unique_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let file_path =
        std::env::temp_dir().join(format!("vulcan-openai-responses-{unique_id}.{extension}"));
    std::fs::write(&file_path, [0_u8, 1, 2, 3]).expect("temporary attachment should write");

    ConversationAttachment {
        kind,
        file_name: String::from(file_name),
        local_path: file_path.to_string_lossy().into_owned(),
        source_path: None,
        asset_id: None,
        byte_len: Some(4),
        preview_local_path: None,
        mime_type: String::from(mime_type),
    }
}

/// Verifies a chained OpenAI Responses request sends only the new tool-result tail already missing from the provider cursor.
/// 验证串联的 OpenAI Responses 请求只发送 Provider 游标尚未覆盖的新工具结果尾部。
#[test]
fn chained_request_payload_sends_only_new_tool_result_tail() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-5"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt").clone(),
            ),
            prompt: String::from("tool result body"),
            history: Vec::new(),
            temperature: Some(0.2),
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: Some(json!({
                "reasoning": { "effort": "high" },
                "previous_response_id": "resp_prev"
            })),
            request_headers: None,
            continuation: CoreProviderContinuation {
                last_local_message_id: Some(String::from("assistant-1")),
                ..CoreProviderContinuation::default()
            },
        },
        current_history: vec![CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::from("我先读取文件。"),
                reasoning: String::from("这段思考不应回放。"),
                provider_message_id: Some(String::from("resp-1")),
                provider_message_state: None,
                tool_calls: vec![CoreConversationToolCall {
                    id: String::from("fc_1"),
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

    let payload = build_openai_responses_request_payload(
        "gpt-5",
        request,
        &[ProviderToolDefinition {
            name: String::from("read_file"),
            description: String::from("Read one file"),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                }
            }),
        }],
    )
    .expect("payload should build");

    let input = payload["input"]
        .as_array()
        .expect("input should be an array");
    assert_eq!(payload["instructions"], "system prompt");
    assert_eq!(payload["stream"], true);
    assert_eq!(payload["previous_response_id"], "resp_prev");
    assert_eq!(payload["include"][0], "reasoning.encrypted_content");
    assert_eq!(input.len(), 1);
    assert_eq!(input[0]["type"], "function_call_output");
    assert_eq!(input[0]["call_id"], "call-1");
    assert_eq!(input[0]["output"], "tool result body");
    assert_eq!(payload["tools"][0]["type"], "function");
    assert_eq!(payload["tools"][0]["strict"], true);
    assert_eq!(
        payload["tools"][0]["parameters"]["additionalProperties"],
        false
    );
    assert_eq!(payload["tools"][0]["parameters"]["required"][0], "path");
}

/// Verifies a chained request preserves every local message appended after the exact cursor boundary.
/// 验证串联请求会保留精确游标边界之后追加的每一条本地消息。
#[test]
fn chained_request_payload_sends_complete_uncovered_suffix() {
    // user_message builds deterministic text-only inputs for the covered and uncovered regions.
    // user_message 为已覆盖区与未覆盖区构建确定性的纯文本输入。
    let user_message = |id: &str, text: &str| CoreConversationMessage {
        id: id.to_string(),
        payload: CoreConversationPayload::UserText {
            text: text.to_string(),
            attachments: Vec::new(),
        },
    };
    // boundary_message is the exact local assistant response represented by resp_prev.
    // boundary_message 是 resp_prev 所表示的精确本地助手响应。
    let boundary_message = CoreConversationMessage {
        id: String::from("assistant-boundary"),
        payload: CoreConversationPayload::Assistant {
            text: String::from("covered response"),
            reasoning: String::new(),
            provider_message_id: Some(String::from("resp_prev")),
            provider_message_state: None,
            tool_calls: Vec::new(),
            usage: None,
        },
    };
    // request carries one covered message pair, one intermediate unsent message, and the current prompt.
    // request 携带一组已覆盖消息、一条中间未发送消息与当前提示消息。
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-5"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt"),
            ),
            prompt: String::from("current tail"),
            history: Vec::new(),
            temperature: None,
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: Some(json!({"previous_response_id": "resp_prev"})),
            request_headers: None,
            continuation: CoreProviderContinuation {
                last_local_message_id: Some(String::from("assistant-boundary")),
                ..CoreProviderContinuation::default()
            },
        },
        current_history: vec![
            user_message("user-covered", "covered question"),
            boundary_message,
            user_message("user-intermediate", "intermediate tail"),
        ],
        current_prompt: user_message("user-current", "current tail"),
        interleaved_reasoning: None,
    };

    let payload = build_openai_responses_request_payload("gpt-5", request, &[])
        .expect("chained suffix payload should build");
    let input = payload["input"]
        .as_array()
        .expect("input should be an array");

    assert_eq!(input.len(), 2);
    assert_eq!(input[0]["content"][0]["text"], "intermediate tail");
    assert_eq!(input[1]["content"][0]["text"], "current tail");
}

/// Verifies a cursor-free OpenAI Responses request replays canonical history before the new tail.
/// 验证不带游标的 OpenAI Responses 请求会在新增尾部之前回放规范历史。
#[test]
fn cursor_free_request_payload_replays_history_and_new_tail() {
    // request models the same function-call exchange without provider continuation coverage.
    // request 模拟同一段函数调用交互，但不存在 Provider continuation 覆盖。
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-5"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt"),
            ),
            prompt: String::from("tool result body"),
            history: Vec::new(),
            temperature: None,
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: None,
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
                    id: String::from("fc_1"),
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

    // payload must carry each historical function item once before its output.
    // payload 必须在函数输出之前各携带一次历史函数项。
    let payload = build_openai_responses_request_payload("gpt-5", request, &[])
        .expect("cursor-free payload should build");
    let input = payload["input"]
        .as_array()
        .expect("input should be an array");

    assert_eq!(input.len(), 3);
    assert_eq!(input[0]["role"], "assistant");
    assert_eq!(input[0]["content"][0]["type"], "output_text");
    assert!(input[0].get("reasoning").is_none());
    assert_eq!(input[1]["type"], "function_call");
    assert_eq!(input[1]["call_id"], "call-1");
    assert_eq!(input[2]["type"], "function_call_output");
    assert_eq!(input[2]["call_id"], "call-1");
}

/// Verifies OpenAI Responses preserves provider-native tools while appending host function tools.
/// 验证 OpenAI Responses 会保留 Provider 原生工具，并继续追加宿主函数工具。
#[test]
fn request_payload_preserves_native_tools_and_appends_function_tools() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-5"),
            system_prompt: String::new(),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::new().clone(),
            ),
            prompt: String::from("搜索最新信息"),
            history: Vec::new(),
            temperature: None,
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: Some(json!({
                "tools": [{ "type": "web_search_preview" }]
            })),
            request_headers: None,
            continuation: CoreProviderContinuation::default(),
        },
        current_history: Vec::new(),
        current_prompt: CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("搜索最新信息"),
                attachments: Vec::new(),
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_openai_responses_request_payload(
        "gpt-5",
        request,
        &[ProviderToolDefinition {
            name: String::from("read_file"),
            description: String::from("Read one file"),
            parameters: json!({ "type": "object", "properties": {} }),
        }],
    )
    .expect("payload should build");

    assert_eq!(payload["tools"][0]["type"], "web_search_preview");
    assert_eq!(payload["tools"][1]["type"], "function");
    assert_eq!(payload["tools"][1]["name"], "read_file");
}

/// Verifies OpenAI Responses user input items carry staged image attachments as `input_image` parts.
/// 验证 OpenAI Responses 用户输入项会把已暂存图片附件编码为 `input_image` Part。
#[test]
fn request_payload_embeds_user_image_attachments_as_input_image_parts() {
    let attachment = build_test_attachment(
        "png",
        ConversationAttachmentKind::Image,
        "image/png",
        "diagram.png",
    );
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-5"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt").clone(),
            ),
            prompt: String::from("分析图片"),
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
                text: String::from("分析图片"),
                attachments: vec![attachment.clone()],
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_openai_responses_request_payload("gpt-5", request, &[])
        .expect("payload should build");
    let input = payload["input"]
        .as_array()
        .expect("input should be an array");
    let content = input[0]["content"]
        .as_array()
        .expect("user content should be one part array");

    assert_eq!(input[0]["role"], "user");
    assert_eq!(content[0]["type"], "input_text");
    assert_eq!(content[1]["type"], "input_image");
    assert_eq!(
        content[1]["image_url"],
        json!("data:image/png;base64,AAECAw==")
    );

    let _ = std::fs::remove_file(&attachment.local_path);
}

/// Verifies OpenAI Responses user input items encode staged PDF and audio attachments as `input_file` parts.
/// 验证 OpenAI Responses 用户输入项会把已暂存 PDF 与音频附件编码为 `input_file` Part。
#[test]
fn request_payload_embeds_user_file_like_attachments_as_input_file_parts() {
    let pdf_attachment = build_test_attachment(
        "pdf",
        ConversationAttachmentKind::Pdf,
        "application/pdf",
        "guide.pdf",
    );
    let audio_attachment = build_test_attachment(
        "mp3",
        ConversationAttachmentKind::Audio,
        "audio/mpeg",
        "sample.mp3",
    );
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-5"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt").clone(),
            ),
            prompt: String::from("分析附件"),
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
                text: String::from("分析附件"),
                attachments: vec![pdf_attachment.clone(), audio_attachment.clone()],
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_openai_responses_request_payload("gpt-5", request, &[])
        .expect("payload should build");
    let input = payload["input"]
        .as_array()
        .expect("input should be an array");
    let content = input[0]["content"]
        .as_array()
        .expect("user content should be one part array");

    assert_eq!(content[1]["type"], "input_file");
    assert_eq!(content[1]["filename"], "guide.pdf");
    assert_eq!(
        content[1]["file_data"],
        json!("data:application/pdf;base64,AAECAw==")
    );
    assert_eq!(content[2]["type"], "input_file");
    assert_eq!(content[2]["filename"], "sample.mp3");
    assert_eq!(
        content[2]["file_data"],
        json!("data:audio/mpeg;base64,AAECAw==")
    );

    let _ = std::fs::remove_file(&pdf_attachment.local_path);
    let _ = std::fs::remove_file(&audio_attachment.local_path);
}

/// Verifies a pure attachment-only OpenAI Responses user item remains provider-visible when its managed replay file is gone.
/// 验证纯附件 OpenAI Responses 用户项在托管回放文件缺失时仍会以文本提示形式传给 Provider。
#[test]
fn request_payload_replaces_missing_attachment_only_input_with_text_notice() {
    let attachment = build_test_attachment(
        "png",
        ConversationAttachmentKind::Image,
        "image/png",
        "lost-diagram.png",
    );
    std::fs::remove_file(&attachment.local_path).expect("managed attachment should remove");
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-5"),
            system_prompt: String::new(),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::new().clone(),
            ),
            prompt: String::new(),
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
                text: String::new(),
                attachments: vec![attachment],
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_openai_responses_request_payload("gpt-5", request, &[])
        .expect("payload should build");
    let input = payload["input"]
        .as_array()
        .expect("input should be an array");
    let content = input[0]["content"]
        .as_array()
        .expect("user content should be one part array");

    assert_eq!(input[0]["role"], "user");
    assert_eq!(content[0]["type"], "input_text");
    assert_eq!(
        content[0]["text"],
        json!("ERROR: Attachment \"lost-diagram.png\" no longer exists. Please ignore it.")
    );
}

/// Verifies completed OpenAI Responses outputs become the canonical assistant text, reasoning, tool calls, and continuation response identifier.
/// 验证已完成的 OpenAI Responses 输出会转成规范助手文本、思考、工具调用以及 continuation 响应标识。
#[test]
fn completed_response_builds_canonical_turn_response() {
    let response = serde_json::from_value(json!({
        "id": "resp_done",
        "output": [
            {
                "type": "message",
                "content": [
                    { "type": "output_text", "text": "已读取文件。" }
                ]
            },
            {
                "type": "reasoning",
                "summary": [
                    { "type": "summary_text", "text": "接下来需要修改函数。" }
                ],
                "encrypted_content": "opaque"
            },
            {
                "type": "function_call",
                "id": "fc_1",
                "call_id": "call_1",
                "name": "read_file",
                "arguments": "{\"path\":\"src/main.rs\"}"
            }
        ]
    }))
    .expect("response should deserialize");

    let turn_response = build_openai_responses_turn_response("openai", "gpt-5", response)
        .expect("turn response should build");

    assert_eq!(
        turn_response.provider_message_id.as_deref(),
        Some("resp_done")
    );
    assert_eq!(turn_response.assistant_text, "已读取文件。");
    assert_eq!(turn_response.assistant_reasoning, "接下来需要修改函数。");
    assert_eq!(turn_response.tool_calls.len(), 1);
    assert_eq!(turn_response.tool_calls[0].id, "fc_1");
    assert_eq!(
        turn_response.tool_calls[0].tool_call_id.as_deref(),
        Some("call_1")
    );
    assert_eq!(turn_response.tool_calls[0].tool_name, "read_file");
    assert_eq!(turn_response.tool_calls[0].arguments["path"], "src/main.rs");
    assert_eq!(
        turn_response
            .continuation
            .as_ref()
            .and_then(|continuation| continuation.last_provider_message_id.as_deref()),
        Some("resp_done")
    );
}

/// Verifies fallback stream synthesis preserves structured output items when the provider omits `response.completed`.
/// 验证 Provider 缺失 `response.completed` 时，流式兜底合成会保留结构化输出项。
#[test]
fn fallback_response_preserves_structured_output_items() {
    let output = vec![
        serde_json::from_value(json!({
            "type": "message",
            "content": [
                { "type": "output_text", "text": "已拿到搜索结果。" }
            ]
        }))
        .expect("message item should deserialize"),
        serde_json::from_value(json!({
            "type": "function_call",
            "id": "fc_search",
            "call_id": "call_search",
            "name": "web_lookup",
            "arguments": "{\"q\":\"AMD AI MAX 395\"}"
        }))
        .expect("function call item should deserialize"),
    ];
    let response = synthesize_openai_responses_fallback_response(
        Some(String::from("resp_stream")),
        output,
        String::from("会被结构化消息覆盖"),
        String::new(),
    )
    .expect("fallback response should synthesize");

    let turn_response = build_openai_responses_turn_response("openai", "gpt-5", response)
        .expect("turn response should build");

    assert_eq!(
        turn_response.provider_message_id.as_deref(),
        Some("resp_stream")
    );
    assert_eq!(turn_response.assistant_text, "已拿到搜索结果。");
    assert_eq!(turn_response.tool_calls.len(), 1);
    assert_eq!(turn_response.tool_calls[0].tool_name, "web_lookup");
}

/// Verifies fallback stream synthesis can rebuild text and reasoning from deltas when no structured output item arrives.
/// 验证在没有结构化输出项到达时，流式兜底合成仍可依据增量重建文本与思考。
#[test]
fn fallback_response_rebuilds_delta_only_output() {
    let response = synthesize_openai_responses_fallback_response(
        Some(String::from("resp_delta_only")),
        Vec::new(),
        String::from("已完成联网搜索。"),
        String::from("先检索，再归纳。"),
    )
    .expect("fallback response should synthesize");

    let turn_response = build_openai_responses_turn_response("openai", "gpt-5", response)
        .expect("turn response should build");

    assert_eq!(
        turn_response.provider_message_id.as_deref(),
        Some("resp_delta_only")
    );
    assert_eq!(turn_response.assistant_text, "已完成联网搜索。");
    assert_eq!(turn_response.assistant_reasoning, "先检索，再归纳。");
    assert_eq!(
        turn_response
            .continuation
            .as_ref()
            .and_then(|continuation| continuation.last_provider_message_id.as_deref()),
        Some("resp_delta_only")
    );
}

/// Verifies Responses added, argument delta, and done events emit one request lifecycle without duplicate completion.
/// 验证 Responses 的 added、参数增量与 done 事件只发出一次请求生命周期且不会重复完成。
#[test]
fn responses_function_argument_events_emit_one_correlated_request_lifecycle() {
    // live channel captures exact provider-neutral request fragments in send order.
    // live 通道按发送顺序捕获精确的 Provider 中立请求碎片。
    let (live_sender, mut live_receiver) = tokio::sync::mpsc::unbounded_channel();
    let provider_tools = vec![ProviderToolDefinition {
        name: String::from("submit_plan"),
        description: String::from("Submit a plan"),
        parameters: json!({ "type": "object" }),
    }];
    let mut pending = HashMap::new();
    start_openai_responses_tool_request_from_output_item(
        &live_sender,
        &provider_tools,
        &json!({
            "output_index": 2,
            "item": {
                "type": "function_call",
                "id": "fc_2",
                "call_id": "call_2",
                "name": "submit_plan",
                "arguments": ""
            }
        }),
        &mut pending,
    );
    append_openai_responses_tool_request_delta(
        &live_sender,
        &json!({
            "output_index": 2,
            "item_id": "fc_2",
            "delta": "{\"name\":\"demo\"}"
        }),
        &mut pending,
    );
    complete_openai_responses_tool_request_from_done_event(
        &live_sender,
        &provider_tools,
        &json!({
            "output_index": 2,
            "item_id": "fc_2",
            "name": "submit_plan",
            "arguments": "{\"name\":\"demo\"}"
        }),
        &mut pending,
    );
    // A later output-item completion must be a no-op after the dedicated done event closed the request.
    // 专用 done 事件闭合请求后，稍后的 output-item 完成必须保持无操作。
    complete_openai_responses_tool_request_from_output_item(
        &live_sender,
        &json!({
            "output_index": 2,
            "item": {
                "type": "function_call",
                "id": "fc_2",
                "call_id": "call_2",
                "name": "submit_plan",
                "arguments": "{\"name\":\"demo\"}"
            }
        }),
        &mut pending,
    );

    let fragments = std::iter::from_fn(|| live_receiver.try_recv().ok())
        .map(|delta| delta.expect("provider fragment should be successful"))
        .collect::<Vec<_>>();
    assert_eq!(fragments.len(), 3);
    assert!(matches!(
        &fragments[0],
        ProviderStreamDelta::ToolRequestFragment(ToolRequestFragment::Detected {
            request_stream_id,
            tool_call_id,
            tool_name,
            initial_arguments,
            argument_assembler: Some(argument_assembler),
        }) if request_stream_id == "fc_2"
            && tool_call_id == "fc_2"
            && tool_name == "submit_plan"
            && initial_arguments.is_empty()
            && argument_assembler.read(|arguments| arguments == "{\"name\":\"demo\"}")
    ));
    assert!(matches!(
        &fragments[1],
        ProviderStreamDelta::ToolRequestFragment(ToolRequestFragment::ArgumentsDelta {
            request_stream_id,
            delta,
        }) if request_stream_id == "fc_2" && delta == "{\"name\":\"demo\"}"
    ));
    assert!(matches!(
        &fragments[2],
        ProviderStreamDelta::ToolRequestFragment(ToolRequestFragment::Completed {
            request_stream_id,
            canonical_tool_call_id: Some(canonical_tool_call_id),
            final_arguments: None,
        }) if request_stream_id == "fc_2"
            && canonical_tool_call_id == "call_2"
    ));
    assert!(pending.is_empty());
}

/// Verifies done-only Responses events wait for a real call id before canonical request completion.
/// 验证仅有 done 的 Responses 事件会等待真实调用 ID，再完成规范请求关联。
#[test]
fn responses_done_without_added_waits_for_real_call_id() {
    // doneEvent omits call_id by protocol and therefore may start visibility but cannot promote identity.
    // doneEvent 按协议不含 call_id，因此可以启动可见性但不能提升身份。
    let (live_sender, mut live_receiver) = tokio::sync::mpsc::unbounded_channel();
    let provider_tools = vec![ProviderToolDefinition {
        name: String::from("submit_plan"),
        description: String::from("Submit a plan"),
        parameters: json!({ "type": "object" }),
    }];
    let done_event = json!({
        "output_index": 1,
        "item_id": "fc_done_only",
        "name": "submit_plan",
        "arguments": "{\"name\":\"demo\"}"
    });
    let mut pending = HashMap::new();
    complete_openai_responses_tool_request_from_done_event(
        &live_sender,
        &provider_tools,
        &done_event,
        &mut pending,
    );
    complete_openai_responses_tool_request_from_done_event(
        &live_sender,
        &provider_tools,
        &done_event,
        &mut pending,
    );
    let first = live_receiver
        .try_recv()
        .expect("done-only event should create one detected fragment")
        .expect("detected fragment should be successful");
    assert!(matches!(
        first,
        ProviderStreamDelta::ToolRequestFragment(ToolRequestFragment::Detected { .. })
    ));
    assert!(live_receiver.try_recv().is_err());
    assert!(
        pending
            .get(&1)
            .is_some_and(|request| request.call_id.is_none())
    );

    complete_openai_responses_tool_request_from_output_item(
        &live_sender,
        &json!({
            "output_index": 1,
            "item": {
                "type": "function_call",
                "id": "fc_done_only",
                "call_id": "call_done_only",
                "name": "submit_plan",
                "arguments": "{\"name\":\"demo\"}"
            }
        }),
        &mut pending,
    );
    let completed = live_receiver
        .try_recv()
        .expect("real call id should complete the request")
        .expect("completed fragment should be successful");
    assert!(matches!(
        completed,
        ProviderStreamDelta::ToolRequestFragment(ToolRequestFragment::Completed {
            canonical_tool_call_id: Some(canonical_tool_call_id),
            ..
        }) if canonical_tool_call_id == "call_done_only"
    ));
    assert!(pending.is_empty());
}

/// Verifies schema sanitization adds OpenAI strict-tool requirements recursively while preserving `$ref` exclusivity.
/// 验证 schema 规范化会递归补齐 OpenAI 严格工具要求，并保留 `$ref` 的独占性。
#[test]
fn sanitize_schema_adds_strict_openai_constraints() {
    let schema = json!({
        "type": "object",
        "properties": {
            "location": {
                "$ref": "#/$defs/Location",
                "description": "The user's location"
            }
        },
        "$defs": {
            "Location": {
                "type": "object",
                "properties": {
                    "city": { "type": "string" }
                }
            }
        }
    });

    let schema = project_tool_parameters_schema(
        &schema,
        VulcanToolSchemaProjectionProfile::OpenAiResponsesStrict,
    );

    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["required"][0], "location");
    assert_eq!(
        schema["properties"]["location"],
        json!({ "$ref": "#/$defs/Location" })
    );
    assert_eq!(schema["$defs"]["Location"]["additionalProperties"], false);
    assert_eq!(schema["$defs"]["Location"]["required"][0], "city");
}
