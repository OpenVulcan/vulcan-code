use super::{
    build_google_gemini_request_payload, core_message_to_gemini_content,
    provider_tool_definition_to_gemini_function_declaration,
};
use crate::agent_runtime::provider_protocol_gemini::{
    GeminiContent, GeminiFunctionCall, GeminiPart, build_gemini_provider_message_state,
    build_scoped_gemini_provider_message_state, parse_gemini_provider_message_state,
};
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};
use vulcan_core::types::{
    ConversationAttachment, ConversationAttachmentKind,
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationToolCall as CoreConversationToolCall,
    ProviderContinuation as CoreProviderContinuation,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
};

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
    let file_path = std::env::temp_dir().join(format!("vulcan-gemini-{unique_id}.{extension}"));
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

/// Verifies one stored Gemini provider-message-state round-trips the native content payload without losing thought signatures.
/// 验证一份已存储 Gemini provider_message_state 可以在不丢失 thought signature 的前提下往返恢复原生内容载荷。
#[test]
fn gemini_provider_message_state_round_trips_native_content() {
    let content = GeminiContent::model(vec![GeminiPart::reasoning(
        "reasoning text",
        Some(String::from("sig-1")),
    )]);
    let state = build_scoped_gemini_provider_message_state("google", "gemini-test", &content);

    let restored = parse_gemini_provider_message_state(&state, "google", "gemini-test")
        .expect("gemini provider state should parse");

    assert_eq!(restored, content);
}

/// Verifies Gemini can still read legacy unscoped provider-message-state payloads produced before native state envelopes existed.
/// 验证 Gemini 仍可读取原生状态信封出现之前生成的旧版未带作用域 provider_message_state 载荷。
#[test]
fn gemini_provider_message_state_reads_legacy_unscoped_payload() {
    let content = GeminiContent::model(vec![GeminiPart::reasoning(
        "legacy reasoning",
        Some(String::from("legacy-sig")),
    )]);
    let state = build_gemini_provider_message_state(&content);

    let restored = parse_gemini_provider_message_state(&state, "google", "gemini-test")
        .expect("legacy gemini provider state should parse");

    assert_eq!(restored, content);
}

/// Verifies Gemini rejects scoped provider-message-state payloads when the provider or model no longer matches the current request.
/// 验证当 provider 或 model 与当前请求不匹配时，Gemini 会拒绝带作用域的 provider_message_state 载荷。
#[test]
fn gemini_provider_message_state_ignores_scoped_payload_after_provider_switch() {
    let content = GeminiContent::model(vec![GeminiPart::reasoning(
        "foreign reasoning",
        Some(String::from("foreign-sig")),
    )]);
    let state = build_scoped_gemini_provider_message_state("google", "gemini-old", &content);

    assert!(
        parse_gemini_provider_message_state(&state, "google", "gemini-new").is_none(),
        "model-scoped gemini provider state must not replay after a model switch"
    );
    assert!(
        parse_gemini_provider_message_state(&state, "minimax-cn", "gemini-old").is_none(),
        "provider-scoped gemini provider state must not replay after a provider switch"
    );
}

/// Verifies Gemini request building falls back to canonical assistant text when a stored native state envelope belongs to another provider/model.
/// 验证当存储的原生状态信封属于其他 provider/model 时，Gemini 请求构建会回退到规范助手文本。
#[test]
fn gemini_request_payload_ignores_scoped_state_after_provider_switch() {
    let assistant_state = build_scoped_gemini_provider_message_state(
        "google",
        "gemini-old",
        &GeminiContent::model(vec![GeminiPart::reasoning(
            "stale native reasoning",
            Some(String::from("stale-sig")),
        )]),
    );
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("google"),
            model_id: String::from("gemini-new"),
            system_prompt: String::from("You are helpful."),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("You are helpful."),
            ),
            prompt: String::from("继续"),
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
                text: String::from("canonical assistant text"),
                reasoning: String::from("local reasoning should not become a native part"),
                provider_message_id: None,
                provider_message_state: Some(assistant_state),
                tool_calls: Vec::new(),
                usage: None,
            },
        }],
        current_prompt: CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("继续"),
                attachments: Vec::new(),
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_google_gemini_request_payload(request, &[])
        .expect("gemini request payload should build");
    let contents = payload["contents"]
        .as_array()
        .expect("contents should be one array");

    assert_eq!(
        contents[0]["parts"][0]["text"],
        json!("canonical assistant text")
    );
    assert_eq!(contents[0]["parts"][0].get("thoughtSignature"), None);
    assert_ne!(
        contents[0]["parts"][0]["text"],
        json!("stale native reasoning"),
        "foreign scoped native state must not override canonical assistant fields"
    );
}

/// Verifies Gemini request building replays stored native assistant parts and maps tool results into native function responses with the matching function name.
/// 验证 Gemini 请求构建会回放已存储的原生助手 Part，并把工具结果映射成带匹配函数名称的原生 function response。
#[test]
fn gemini_request_payload_replays_native_parts_and_function_response() {
    let assistant_state = build_scoped_gemini_provider_message_state(
        "google",
        "gemini-2.5-flash",
        &GeminiContent::model(vec![
            GeminiPart::reasoning("thinking text", Some(String::from("sig-1"))),
            GeminiPart::function_call(GeminiFunctionCall {
                id: Some(String::from("provider-call-1")),
                name: String::from("read_file"),
                args: json!({"path":"src/main.rs"}),
            }),
        ]),
    );
    let assistant_message = CoreConversationMessage {
        id: String::from("assistant-1"),
        payload: CoreConversationPayload::Assistant {
            text: String::new(),
            reasoning: String::from("thinking text"),
            provider_message_id: None,
            provider_message_state: Some(assistant_state),
            tool_calls: vec![CoreConversationToolCall {
                id: String::from("tool-item-1"),
                tool_call_id: Some(String::from("provider-call-1")),
                tool_name: String::from("read_file"),
                arguments: json!({"path":"src/main.rs"}),
            }],
            usage: None,
        },
    };
    let tool_result_message = CoreConversationMessage {
        id: String::from("tool-result-1"),
        payload: CoreConversationPayload::ToolResult {
            tool_result_id: String::from("result-1"),
            tool_call_id: Some(String::from("provider-call-1")),
            text: String::from("{\"summary\":\"done\"}"),
        },
    };
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("google"),
            model_id: String::from("gemini-2.5-flash"),
            system_prompt: String::from("You are helpful."),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("You are helpful.").clone(),
            ),
            prompt: String::from("继续"),
            history: Vec::new(),
            temperature: Some(0.2),
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: Some(json!({
                "generationConfig": {
                    "thinkingConfig": {
                        "thinkingBudget": 2048
                    }
                }
            })),
            request_headers: None,
            continuation: CoreProviderContinuation::default(),
        },
        current_history: vec![assistant_message, tool_result_message],
        current_prompt: CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("继续"),
                attachments: Vec::new(),
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_google_gemini_request_payload(
        request,
        &[ProviderToolDefinition {
            name: String::from("read_file"),
            description: String::from("Read file"),
            parameters: json!({"type":"object"}),
        }],
    )
    .expect("gemini request payload should build");

    let contents = payload["contents"]
        .as_array()
        .expect("contents should be one array");
    assert_eq!(contents[0]["parts"][0]["thoughtSignature"], json!("sig-1"));
    assert_eq!(
        contents[1]["parts"][0]["functionResponse"]["name"],
        json!("read_file")
    );
    assert_eq!(
        contents[1]["parts"][0]["functionResponse"]["id"],
        json!("provider-call-1")
    );
    assert_eq!(
        payload["generationConfig"]["thinkingConfig"]["thinkingBudget"],
        json!(2048)
    );
}

/// Verifies Gemini removes provider-native search tools before appending function declarations.
/// 验证 Gemini 会在追加函数声明前移除 Provider 原生搜索工具。
#[test]
fn gemini_request_payload_removes_google_search_before_appending_function_tools() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("google"),
            model_id: String::from("gemini-2.5-flash"),
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
                "tools": [{ "google_search": {} }]
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

    let payload = build_google_gemini_request_payload(
        request,
        &[ProviderToolDefinition {
            name: String::from("read_file"),
            description: String::from("Read file"),
            parameters: json!({"type":"object"}),
        }],
    )
    .expect("gemini request payload should build");

    assert!(payload["tools"][0].get("google_search").is_none());
    assert_eq!(
        payload["tools"][0]["functionDeclarations"][0]["name"],
        json!("read_file")
    );
    assert_eq!(
        payload["toolConfig"]["functionCallingConfig"]["mode"],
        json!("AUTO")
    );
}

/// Verifies Gemini drops stale provider-native search tools even when no function tools are available.
/// 验证即使没有函数工具，Gemini 也会移除陈旧的 Provider 原生搜索工具。
#[test]
fn gemini_request_payload_removes_google_search_without_function_tools() {
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("google"),
            model_id: String::from("gemini-2.5-flash"),
            system_prompt: String::new(),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::new(),
            ),
            prompt: String::from("写代码"),
            history: Vec::new(),
            temperature: None,
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: Some(json!({
                "tools": [{ "google_search": {} }]
            })),
            request_headers: None,
            continuation: CoreProviderContinuation::default(),
        },
        current_history: Vec::new(),
        current_prompt: CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("写代码"),
                attachments: Vec::new(),
            },
        },
        interleaved_reasoning: None,
    };

    let payload =
        build_google_gemini_request_payload(request, &[]).expect("gemini request payload");

    assert!(payload.get("tools").is_none());
}

/// Verifies Gemini tool-result conversion omits the native response `id` when the originating function call only had a local controller identifier.
/// 验证当原始函数调用只有本地控制器标识时，Gemini 工具结果转换不会附带原生响应 `id`。
#[test]
fn gemini_tool_result_omits_provider_visible_id_when_unavailable() {
    let assistant_message = CoreConversationMessage {
        id: String::from("assistant-1"),
        payload: CoreConversationPayload::Assistant {
            text: String::new(),
            reasoning: String::new(),
            provider_message_id: None,
            provider_message_state: None,
            tool_calls: vec![CoreConversationToolCall {
                id: String::from("local-tool-id"),
                tool_call_id: None,
                tool_name: String::from("read_file"),
                arguments: json!({"path":"src/main.rs"}),
            }],
            usage: None,
        },
    };
    let tool_result_message = CoreConversationMessage {
        id: String::from("tool-result-1"),
        payload: CoreConversationPayload::ToolResult {
            tool_result_id: String::from("result-1"),
            tool_call_id: Some(String::from("local-tool-id")),
            text: String::from("done"),
        },
    };

    let content = core_message_to_gemini_content(
        &tool_result_message,
        &[assistant_message],
        "google",
        "gemini-test",
    )
    .expect("tool result should convert into gemini content");

    assert!(content.parts[0].function_response.is_some());
    assert_eq!(
        content.parts[0]
            .function_response
            .as_ref()
            .and_then(|response| response.id.clone()),
        None
    );
}

/// Verifies Gemini user content includes inline image data parts when the prompt carries staged image attachments.
/// 验证当提示词携带已暂存图片附件时，Gemini 用户内容会包含内联图片数据 Part。
#[test]
fn gemini_request_payload_embeds_user_image_attachments_as_inline_data() {
    let attachment = build_test_attachment(
        "png",
        ConversationAttachmentKind::Image,
        "image/png",
        "diagram.png",
    );
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("google"),
            model_id: String::from("gemini-2.5-flash"),
            system_prompt: String::from("You are helpful."),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("You are helpful.").clone(),
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

    let payload = build_google_gemini_request_payload(request, &[])
        .expect("gemini request payload should build");
    let contents = payload["contents"]
        .as_array()
        .expect("contents should be one array");
    let parts = contents[0]["parts"]
        .as_array()
        .expect("user parts should be one array");

    assert_eq!(contents[0]["role"], "user");
    assert_eq!(parts[0]["text"], "分析图片");
    assert_eq!(parts[1]["inlineData"]["mimeType"], "image/png");
    assert_eq!(parts[1]["inlineData"]["data"], "AAECAw==");

    let _ = std::fs::remove_file(&attachment.local_path);
}

/// Verifies the Gemini golden payload keeps system instruction and native inline image/video parts.
/// 验证 Gemini golden 载荷会保留 system instruction 与原生内联图片/视频 Part。
#[test]
fn golden_fixture_gemini_multimodal_payload_shape() {
    let image_attachment = build_test_attachment(
        "png",
        ConversationAttachmentKind::Image,
        "image/png",
        "scene.png",
    );
    let video_attachment = build_test_attachment(
        "mp4",
        ConversationAttachmentKind::Video,
        "video/mp4",
        "clip.mp4",
    );
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("google"),
            model_id: String::from("gemini-2.5-flash"),
            system_prompt: String::from("You are helpful."),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("You are helpful."),
            ),
            prompt: String::from("分析图片和视频"),
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
            id: String::from("user-multimodal"),
            payload: CoreConversationPayload::UserText {
                text: String::from("分析图片和视频"),
                attachments: vec![image_attachment.clone(), video_attachment.clone()],
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_google_gemini_request_payload(request, &[])
        .expect("Gemini golden payload should build");
    let contents = payload["contents"]
        .as_array()
        .expect("contents should be one array");
    let parts = contents[0]["parts"]
        .as_array()
        .expect("user parts should be one array");

    assert_eq!(
        payload["systemInstruction"]["parts"][0]["text"],
        "You are helpful."
    );
    assert_eq!(contents[0]["role"], "user");
    assert_eq!(parts[0]["text"], "分析图片和视频");
    assert_eq!(parts[1]["inlineData"]["mimeType"], "image/png");
    assert_eq!(parts[1]["inlineData"]["data"], "AAECAw==");
    assert_eq!(parts[2]["inlineData"]["mimeType"], "video/mp4");
    assert_eq!(parts[2]["inlineData"]["data"], "AAECAw==");

    let _ = std::fs::remove_file(&image_attachment.local_path);
    let _ = std::fs::remove_file(&video_attachment.local_path);
}

/// Verifies Gemini function declarations normalize nested container schemas before they are sent to the upstream GenerateContent API.
/// 验证 Gemini 函数声明会在发送到上游 GenerateContent API 之前规范化嵌套容器 schema。
#[test]
fn gemini_function_declaration_normalizes_nested_container_types() {
    let declaration =
        provider_tool_definition_to_gemini_function_declaration(&ProviderToolDefinition {
            name: String::from("apply_patch_set"),
            description: String::from("Apply one patch batch"),
            parameters: json!({
                "type": "object",
                "properties": {
                    "patches": {
                        "items": {
                            "properties": {
                                "file": { "type": "string" },
                                "replacement": { "type": "string" }
                            }
                        }
                    }
                }
            }),
        });

    let parameters = declaration.parameters.expect("parameters should exist");
    assert_eq!(parameters["properties"]["patches"]["type"], json!("array"));
    assert_eq!(
        parameters["properties"]["patches"]["items"]["type"],
        json!("object")
    );
}

/// Verifies Gemini function declarations hydrate `anyOf` required branches with their minimal property schemas.
/// 验证 Gemini 函数声明会为 `anyOf` 必填分支补充最小属性 Schema。
#[test]
fn gemini_function_declaration_hydrates_required_anyof_branch_properties() {
    let declaration =
        provider_tool_definition_to_gemini_function_declaration(&ProviderToolDefinition {
            name: String::from("vulcan_curl_get"),
            description: String::from("Execute one simple GET request"),
            parameters: json!({
                "type": "object",
                "properties": {
                    "basic": {
                        "type": "object",
                        "properties": {
                            "username": { "type": "string" },
                            "password": { "type": "string" },
                            "user": { "type": "string" },
                            "pass": { "type": "string" }
                        },
                        "anyOf": [
                            { "required": ["username", "password"] },
                            { "required": ["user", "pass"] }
                        ]
                    }
                }
            }),
        });

    let parameters = declaration.parameters.expect("parameters should exist");
    let basic = &parameters["properties"]["basic"];
    assert_eq!(basic["anyOf"][0]["type"], json!("object"));
    assert_eq!(
        basic["anyOf"][0]["properties"]["username"]["type"],
        json!("string")
    );
    assert_eq!(
        basic["anyOf"][0]["properties"]["password"]["type"],
        json!("string")
    );
    assert!(basic["anyOf"][0]["properties"]["user"].is_null());
    assert_eq!(basic["anyOf"][1]["type"], json!("object"));
    assert_eq!(
        basic["anyOf"][1]["properties"]["user"]["type"],
        json!("string")
    );
    assert_eq!(
        basic["anyOf"][1]["properties"]["pass"]["type"],
        json!("string")
    );
    assert!(basic["anyOf"][1]["properties"]["username"].is_null());
}

/// Verifies Gemini user content includes inline data parts for audio, video, and PDF attachments in addition to images.
/// 验证 Gemini 用户内容除了图片外，也会为音频、视频与 PDF 附件生成内联数据 Part。
#[test]
fn gemini_request_payload_embeds_audio_video_and_pdf_attachments_as_inline_data() {
    let audio_attachment = build_test_attachment(
        "mp3",
        ConversationAttachmentKind::Audio,
        "audio/mpeg",
        "sample.mp3",
    );
    let video_attachment = build_test_attachment(
        "mp4",
        ConversationAttachmentKind::Video,
        "video/mp4",
        "clip.mp4",
    );
    let pdf_attachment = build_test_attachment(
        "pdf",
        ConversationAttachmentKind::Pdf,
        "application/pdf",
        "guide.pdf",
    );
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("google"),
            model_id: String::from("gemini-2.5-flash"),
            system_prompt: String::from("You are helpful."),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("You are helpful.").clone(),
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
                attachments: vec![
                    audio_attachment.clone(),
                    video_attachment.clone(),
                    pdf_attachment.clone(),
                ],
            },
        },
        interleaved_reasoning: None,
    };

    let payload = build_google_gemini_request_payload(request, &[])
        .expect("gemini request payload should build");
    let contents = payload["contents"]
        .as_array()
        .expect("contents should be one array");
    let parts = contents[0]["parts"]
        .as_array()
        .expect("user parts should be one array");

    assert_eq!(parts[1]["inlineData"]["mimeType"], "audio/mpeg");
    assert_eq!(parts[2]["inlineData"]["mimeType"], "video/mp4");
    assert_eq!(parts[3]["inlineData"]["mimeType"], "application/pdf");

    let _ = std::fs::remove_file(&audio_attachment.local_path);
    let _ = std::fs::remove_file(&video_attachment.local_path);
    let _ = std::fs::remove_file(&pdf_attachment.local_path);
}

/// Verifies a pure attachment-only Gemini user content remains provider-visible when its managed replay file is gone.
/// 验证纯附件 Gemini 用户内容在托管回放文件缺失时仍会以文本提示形式传给 Provider。
#[test]
fn gemini_request_payload_replaces_missing_attachment_only_content_with_text_notice() {
    let attachment = build_test_attachment(
        "png",
        ConversationAttachmentKind::Image,
        "image/png",
        "lost-diagram.png",
    );
    std::fs::remove_file(&attachment.local_path).expect("managed attachment should remove");
    let request = OpenAiCompatibleCoreTurnRequest {
        snapshot: CoreProviderRequestSnapshot {
            provider_id: String::from("google"),
            model_id: String::from("gemini-2.5-flash"),
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

    let payload = build_google_gemini_request_payload(request, &[])
        .expect("gemini request payload should build");
    let contents = payload["contents"]
        .as_array()
        .expect("contents should be one array");
    let parts = contents[0]["parts"]
        .as_array()
        .expect("user parts should be one array");

    assert_eq!(contents[0]["role"], "user");
    assert_eq!(
        parts[0]["text"],
        json!("ERROR: Attachment \"lost-diagram.png\" no longer exists. Please ignore it.")
    );
}
