use super::*;

/// Verifies the MiniMax OpenAI-compatible golden payload uses chat content parts with inline image data URLs.
/// 验证 MiniMax OpenAI 兼容 golden 载荷会使用 Chat content parts 并以内联图片 Data URL 表达。
#[test]
fn golden_fixture_minimax_image_understanding_payload_shape() {
    let attachment = build_test_attachment(
        "jpg",
        ConversationAttachmentKind::Image,
        "image/jpeg",
        "minimax-image.jpg",
    );
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("minimax-cn-coding-plan"),
            model_id: String::from("MiniMax-M3"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt"),
            ),
            prompt: String::from("理解这个图片"),
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
            id: String::from("user-image"),
            payload: CoreConversationPayload::UserText {
                text: String::from("理解这个图片"),
                attachments: vec![attachment.clone()],
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_openai_compatible_request_payload("MiniMax-M3", request, &[], true)
        .expect("MiniMax golden payload should build");
    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    let content = messages[1]["content"]
        .as_array()
        .expect("user content should be an array");

    assert_eq!(payload["model"], "MiniMax-M3");
    assert_eq!(messages[0]["role"], "system");
    assert_eq!(messages[1]["role"], "user");
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[0]["text"], "理解这个图片");
    assert_eq!(content[1]["type"], "image_url");
    assert_eq!(
        content[1]["image_url"]["url"],
        json!("data:image/jpeg;base64,AAECAw==")
    );

    let _ = std::fs::remove_file(&attachment.local_path);
}

/// Verifies OpenAI-compatible user messages switch to content-part arrays and carry staged image attachments as data URLs.
/// 验证 OpenAI 兼容用户消息会切换到内容 Part 数组，并以 Data URL 携带已暂存图片附件。
#[test]
fn request_payload_embeds_user_image_attachments_as_data_urls() {
    let attachment = build_test_attachment(
        "png",
        ConversationAttachmentKind::Image,
        "image/png",
        "diagram.png",
    );
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-4.1"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt").clone(),
            ),
            prompt: String::from("请分析这张图"),
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
                text: String::from("请分析这张图"),
                attachments: vec![attachment.clone()],
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_openai_compatible_request_payload("gpt-4.1", request, &[], true)
        .expect("payload should build");
    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    let content = messages[1]["content"]
        .as_array()
        .expect("user content should be one part array");

    assert_eq!(messages[1]["role"], "user");
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[1]["type"], "image_url");
    assert_eq!(
        content[1]["image_url"]["url"],
        json!("data:image/png;base64,AAECAw==")
    );

    let _ = std::fs::remove_file(&attachment.local_path);
}

/// Verifies strict OpenAI-compatible Chat payloads downgrade staged MP3 attachments to explicit text notices.
/// 验证严格 OpenAI 兼容 Chat 载荷会把已暂存 MP3 附件降级为显式文本提示。
#[test]
fn request_payload_downgrades_user_audio_attachments_to_text_notice() {
    let attachment = build_test_attachment(
        "mp3",
        ConversationAttachmentKind::Audio,
        "audio/mpeg",
        "sample.mp3",
    );
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-4.1"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt").clone(),
            ),
            prompt: String::from("请转写这段音频"),
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
                text: String::from("请转写这段音频"),
                attachments: vec![attachment.clone()],
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_openai_compatible_request_payload("gpt-4.1", request, &[], true)
        .expect("payload should build");
    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    let content = messages[1]["content"]
        .as_str()
        .expect("user content should downgrade to text");

    assert!(content.contains("请转写这段音频"));
    assert!(content.contains("Cannot read \"sample.mp3\""));
    assert!(content.contains("this API path"));

    let _ = std::fs::remove_file(&attachment.local_path);
}

/// Verifies strict OpenAI-compatible Chat payloads downgrade staged PDF and video attachments to explicit text notices.
/// 验证严格 OpenAI 兼容 Chat 载荷会把已暂存 PDF 与视频附件降级为显式文本提示。
#[test]
fn request_payload_downgrades_user_file_like_attachments_to_text_notice() {
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
            provider_id: String::from("openai"),
            model_id: String::from("gpt-4.1"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt").clone(),
            ),
            prompt: String::from("请分析这些附件"),
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
                text: String::from("请分析这些附件"),
                attachments: vec![pdf_attachment.clone(), video_attachment.clone()],
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_openai_compatible_request_payload("gpt-4.1", request, &[], true)
        .expect("payload should build");
    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    let content = messages[1]["content"]
        .as_str()
        .expect("user content should downgrade to text");

    assert!(content.contains("请分析这些附件"));
    assert!(content.contains("Cannot read \"guide.pdf\""));
    assert!(content.contains("Cannot read \"clip.mp4\""));
    assert!(content.contains("this API path"));

    let _ = std::fs::remove_file(&pdf_attachment.local_path);
    let _ = std::fs::remove_file(&video_attachment.local_path);
}

/// Verifies a pure attachment-only OpenAI-compatible user message remains provider-visible when its managed replay file is gone.
/// 验证纯附件 OpenAI 兼容用户消息在托管回放文件缺失时仍会以文本提示形式传给 Provider。
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
            provider_id: String::from("openai"),
            model_id: String::from("gpt-4.1"),
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

    let payload = build_openai_compatible_request_payload("gpt-4.1", request, &[], true)
        .expect("payload should build");
    let messages = payload["messages"]
        .as_array()
        .expect("messages should be an array");
    let content_text = messages[0]["content"]
        .as_array()
        .and_then(|content| content.first())
        .and_then(|part| part.get("text"))
        .and_then(|value| value.as_str())
        .or_else(|| messages[0]["content"].as_str())
        .expect("user content should expose one text notice");

    assert_eq!(messages[0]["role"], "user");
    assert_eq!(
        content_text,
        "ERROR: Attachment \"lost-diagram.png\" no longer exists. Please ignore it."
    );
}
