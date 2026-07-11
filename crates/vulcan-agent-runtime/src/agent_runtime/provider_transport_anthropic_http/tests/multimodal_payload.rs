use super::*;

#[test]
fn golden_fixture_anthropic_image_understanding_payload_shape() {
    let attachment = build_test_attachment(
        "png",
        ConversationAttachmentKind::Image,
        "image/png",
        "scene.png",
    );
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: build_test_request_snapshot("请分析这张图片"),
        current_history: Vec::new(),
        current_prompt: CoreConversationMessage {
            id: String::from("user-image"),
            payload: CoreConversationPayload::UserText {
                text: String::from("请分析这张图片"),
                attachments: vec![attachment.clone()],
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_anthropic_request_payload("claude-sonnet-4-6", request, &[])
        .expect("anthropic golden payload should build");
    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    let content = messages[0]["content"]
        .as_array()
        .expect("content should be an array");

    assert_eq!(payload["model"], "claude-sonnet-4-6");
    assert_eq!(payload["system"][0]["type"], "text");
    assert_eq!(payload["system"][0]["cache_control"]["type"], "ephemeral");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[0]["text"], "请分析这张图片");
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["source"]["type"], "base64");
    assert_eq!(content[1]["source"]["media_type"], "image/png");
    assert_eq!(content[1]["source"]["data"], "AAECAw==");

    let _ = std::fs::remove_file(&attachment.local_path);
}

/// Verifies the direct Anthropic request payload replays legacy tool-turn reasoning blocks while preserving tool-use and tool-result message shapes.
/// 验证直连 Anthropic 请求载荷会为旧版工具回合回放思考块，同时保留 tool_use 与 tool_result 消息形状。
#[test]
fn request_payload_replays_legacy_reasoning_for_tool_turns_and_preserves_anthropic_tool_shapes() {
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
    .expect("payload should build");

    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    assert_eq!(payload["max_tokens"], 64000);
    assert_eq!(messages[0]["role"], "assistant");
    assert_eq!(messages[0]["content"][0]["type"], "thinking");
    assert_eq!(messages[0]["content"][0]["thinking"], "这段思考不应回放。");
    assert_eq!(messages[0]["content"][1]["type"], "text");
    assert_eq!(messages[0]["content"][2]["type"], "tool_use");
    assert_eq!(messages[0]["content"][2]["id"], "call-1");
    assert_eq!(messages[0]["content"][2]["name"], "read_file");
    assert!(messages[0].get("reasoning").is_none());
    assert_eq!(messages[1]["role"], "user");
    assert_eq!(messages[1]["content"][0]["type"], "tool_result");
    assert_eq!(messages[1]["content"][0]["tool_use_id"], "call-1");
    assert_eq!(
        messages[1]["content"][0]["content"][0]["text"],
        "tool result body"
    );
    assert_eq!(payload["tools"][0]["name"], "read_file");
    assert_eq!(payload["tool_choice"]["type"], "auto");
}

/// Verifies Anthropic user messages encode staged image attachments as base64 image blocks.
/// 验证 Anthropic 用户消息会把已暂存图片附件编码为 Base64 图片块。
#[test]
fn request_payload_embeds_user_image_attachments_as_image_blocks() {
    let attachment = build_test_attachment(
        "png",
        ConversationAttachmentKind::Image,
        "image/png",
        "diagram.png",
    );
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("anthropic"),
            model_id: String::from("claude-sonnet-4-6"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt").clone(),
            ),
            prompt: String::from("分析图片"),
            history: Vec::new(),
            temperature: None,
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: Some(json!({})),
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

    let payload = build_anthropic_request_payload("claude-sonnet-4-6", request, &[])
        .expect("payload should build");
    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    let content = messages[0]["content"]
        .as_array()
        .expect("user content should be one block array");

    assert_eq!(messages[0]["role"], "user");
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["source"]["type"], "base64");
    assert_eq!(content[1]["source"]["media_type"], "image/png");
    assert_eq!(content[1]["source"]["data"], "AAECAw==");

    let _ = std::fs::remove_file(&attachment.local_path);
}

/// Verifies Anthropic user messages encode staged PDF attachments as document blocks and degrade unsupported video attachments into explicit text errors.
/// 验证 Anthropic 用户消息会把已暂存 PDF 附件编码为文档块，并将不受支持的视频附件降级为显式文本错误。
#[test]
fn request_payload_embeds_pdf_and_degrades_video_attachments() {
    let pdf_attachment = build_test_attachment(
        "pdf",
        ConversationAttachmentKind::Pdf,
        "application/pdf",
        "guide.pdf",
    );
    let video_attachment = build_test_attachment(
        "mp4",
        ConversationAttachmentKind::Video,
        "video/mp4",
        "clip.mp4",
    );
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("anthropic"),
            model_id: String::from("claude-sonnet-4-6"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt").clone(),
            ),
            prompt: String::from("分析附件"),
            history: Vec::new(),
            temperature: None,
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: Some(json!({})),
            request_headers: None,
            continuation: CoreProviderContinuation::default(),
        },
        current_history: Vec::new(),
        current_prompt: CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("分析附件"),
                attachments: vec![pdf_attachment.clone(), video_attachment.clone()],
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
        .expect("user content should be one block array");

    assert_eq!(content[1]["type"], "document");
    assert_eq!(content[1]["source"]["media_type"], "application/pdf");
    assert_eq!(content[1]["source"]["data"], "AAECAw==");
    assert_eq!(
        content[0]["text"],
        json!(
            "分析附件\n\nERROR: Cannot read \"clip.mp4\" (this API path does not support video input). Inform the user."
        )
    );

    let _ = std::fs::remove_file(&pdf_attachment.local_path);
    let _ = std::fs::remove_file(&video_attachment.local_path);
}

/// Verifies a pure attachment-only Anthropic user message remains provider-visible when its managed replay file is gone.
/// 验证纯附件 Anthropic 用户消息在托管回放文件缺失时仍会以文本提示形式传给 Provider。
#[test]
fn request_payload_replaces_missing_attachment_only_message_with_text_notice() {
    let attachment = build_test_attachment(
        "png",
        ConversationAttachmentKind::Image,
        "image/png",
        "lost-diagram.png",
    );
    std::fs::remove_file(&attachment.local_path).expect("managed attachment should remove");
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("anthropic"),
            model_id: String::from("claude-sonnet-4-6"),
            system_prompt: String::new(),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::new().clone(),
            ),
            prompt: String::new(),
            history: Vec::new(),
            temperature: None,
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: Some(json!({})),
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

    let payload = build_anthropic_request_payload("claude-sonnet-4-6", request, &[])
        .expect("payload should build");
    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    let content = messages[0]["content"]
        .as_array()
        .expect("user content should be one block array");

    assert_eq!(messages[0]["role"], "user");
    assert_eq!(content[0]["type"], "text");
    assert_eq!(
        content[0]["text"],
        json!("ERROR: Attachment \"lost-diagram.png\" no longer exists. Please ignore it.")
    );
}
