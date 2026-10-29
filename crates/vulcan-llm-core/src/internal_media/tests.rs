//! Private-boundary and target-profile isolation regressions.
//! 私有边界与目标 Profile 隔离回归。

use super::*;
use crate::{
    AnthropicReasoningMode, encode_internal_responses_request, project_responses_for_provider,
};
use serde_json::json;
use vulcan_ai_contract::{
    AiExecutionRequest, AiMessage, AiOperation, ContentItem, MessageRole, PromptCachePolicy,
};

/// Builds a complete typed request carrying one tiny video; returns no network or filesystem resources.
/// 构建携带一个微小视频的完整强类型请求，不返回网络或文件系统资源。
fn request() -> AiExecutionRequest {
    AiExecutionRequest {
        operation: AiOperation::ConversationRespond,
        model_id: String::from("fixture-model"),
        cache_scope_id: None,
        continuation: None,
        messages: vec![AiMessage {
            role: MessageRole::User,
            content: vec![ContentItem::Resource {
                resource: ResourceReference {
                    id: String::from("video-1"),
                    kind: ResourceKind::Video,
                    media_type: Some(String::from("video/mp4")),
                    source_kind: ResourceSourceKind::InlineData,
                    file_name: Some(String::from("video.mp4")),
                    image_detail: None,
                    resource_path: None,
                    local_path: None,
                    url: Some(String::from("data:video/mp4;base64,aGVsbG8=")),
                },
            }],
            provider_context_items: Vec::new(),
        }],
        tools: Vec::new(),
        native_tools: Vec::new(),
        tool_choice: Default::default(),
        parallel_tool_calls: Default::default(),
        structured_output: Default::default(),
        stop_sequences: Vec::new(),
        prompt_cache: PromptCachePolicy::ProviderDefault,
        reasoning_effort: None,
        reasoning_output: Default::default(),
        model_parameters: Default::default(),
        resources: Vec::new(),
        output_token_limit: Some(512),
    }
}

/// Builds the exact converter context for `upstream_protocol`; returns native reasoning semantics.
/// 为 `upstream_protocol` 构建精确转换器上下文，返回原生思考语义。
fn context(upstream_protocol: UpstreamProtocol) -> ProviderRouteContext {
    ProviderRouteContext {
        upstream_protocol,
        anthropic_reasoning_mode: AnthropicReasoningMode::NativeBudget,
        reasoning_replay_contract: None,
    }
}

/// Verifies all standard public converter entries reject private resource envelopes.
/// 验证全部标准公共转换入口拒绝私有资源封装。
#[test]
fn public_converters_do_not_accept_internal_media_envelopes() {
    // Only the typed constructor may create an internal owner; this value simulates untrusted JSON.
    // 只有强类型构造器能创建内部所有者；此值模拟不可信 JSON。
    let mut internal =
        encode_internal_responses_request(&request()).expect("typed internal request");
    for protocol in [
        UpstreamProtocol::OpenAiResponses,
        UpstreamProtocol::OpenAiChatCompletions,
        UpstreamProtocol::AnthropicMessages,
        UpstreamProtocol::GeminiGenerateContent,
        UpstreamProtocol::GeminiInteractions,
        UpstreamProtocol::GoogleCodeAssist,
    ] {
        assert!(
            project_responses_for_provider(internal.body_mut().clone(), &context(protocol))
                .is_err(),
            "{protocol:?}"
        );
    }
}

/// Verifies a profile cannot claim the media mapping of another member of its protocol family.
/// 验证 Profile 不能声称拥有同协议族另一成员的媒体映射。
#[test]
fn exact_profile_and_input_position_are_required() {
    for (profile, protocol) in [
        (
            OPENAI_CHAT_PROFILE_ID,
            UpstreamProtocol::OpenAiChatCompletions,
        ),
        (
            ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
            UpstreamProtocol::AnthropicMessages,
        ),
        (
            GEMINI_GENERATE_CONTENT_PROFILE_ID,
            UpstreamProtocol::GeminiGenerateContent,
        ),
    ] {
        // The authorized user position supports the tiny video on these exact profiles.
        // 这些精确 Profile 的授权用户位置支持此微小视频。
        let internal =
            encode_internal_responses_request(&request()).expect("typed internal request");
        assert!(
            project_internal_request_for_provider(internal, &context(protocol), profile).is_ok()
        );
        // Replaying a private envelope from an assistant output cannot grant it input authority.
        // 从助手输出重放私有封装，不能授予它输入权限。
        let mut wrong_role =
            encode_internal_responses_request(&request()).expect("typed internal request");
        wrong_role.body_mut()["input"][0]["role"] = json!("assistant");
        assert!(
            project_internal_request_for_provider(wrong_role, &context(protocol), profile).is_err()
        );
    }
    assert!(
        project_internal_request_for_provider(
            encode_internal_responses_request(&request()).expect("typed request"),
            &context(UpstreamProtocol::AnthropicMessages),
            OPENAI_CHAT_PROFILE_ID
        )
        .is_err()
    );
    assert!(
        project_internal_request_for_provider(
            encode_internal_responses_request(&request()).expect("typed request"),
            &context(UpstreamProtocol::AnthropicMessages),
            ANTHROPIC_MESSAGES_PROFILE_ID
        )
        .is_err()
    );
}

/// Verifies forged private envelope fields and cross-protocol cursors fail before producing a wire request.
/// 验证伪造私有封装字段及跨协议游标在产生线路请求之前失败。
#[test]
fn internal_envelopes_and_continuation_are_exact() {
    // An extra key must not be accepted as a future provider request control.
    // 额外字段不能被当成未来供应商请求控制项接受。
    let mut malformed = encode_internal_responses_request(&request()).expect("typed request");
    malformed.body_mut()["input"][0]["content"][0]["purpose"] = json!("independent_extension");
    assert!(
        project_internal_request_for_provider(
            malformed,
            &context(UpstreamProtocol::OpenAiChatCompletions),
            OPENAI_CHAT_PROFILE_ID
        )
        .is_err()
    );
    // The internal JSON contains only indices; payload and path metadata remain in the typed owner.
    // 内部 JSON 仅包含索引；载荷与路径元数据保留在强类型所有者中。
    let mut indexed = encode_internal_responses_request(&request()).expect("typed request");
    assert_eq!(
        indexed.body_mut()["input"][0]["content"][0],
        json!({"type":"vulcan_resource","resource_index":0})
    );
    assert!(!indexed.body_mut().to_string().contains("aGVsbG8="));
    indexed.body_mut()["input"][0]["content"][0]["resource_index"] = json!(999);
    assert!(
        project_internal_request_for_provider(
            indexed,
            &context(UpstreamProtocol::OpenAiChatCompletions),
            OPENAI_CHAT_PROFILE_ID
        )
        .is_err()
    );
    // A cursor implies omitted historical messages and cannot merely be stripped for another protocol.
    // 游标意味着历史消息已被省略，不能仅删除它后发往其他协议。
    let mut cursor = encode_internal_responses_request(&request()).expect("typed request");
    cursor.body_mut()["previous_response_id"] = json!("prior-codex-response");
    assert!(
        project_internal_request_for_provider(
            cursor,
            &context(UpstreamProtocol::GeminiGenerateContent),
            GEMINI_GENERATE_CONTENT_PROFILE_ID
        )
        .is_err()
    );
}

/// Verifies the final guard rejects private media but does not inspect unrelated tool argument data.
/// 验证最终护栏拒绝私有媒体，但不检查无关工具参数数据。
#[test]
fn final_media_guard_checks_only_real_content_positions() {
    assert!(
        crate::validate_target_media_projection(OPENAI_CHAT_PROFILE_ID, &json!({"messages":{}}))
            .is_err()
    );
    assert!(
        crate::validate_target_media_projection(
            OPENAI_RESPONSES_PROFILE_ID,
            &json!({"input":"plain text"})
        )
        .is_ok()
    );
    assert!(crate::validate_target_media_projection(OPENAI_RESPONSES_PROFILE_ID, &json!({"input":[{"type":"function_call_output","call_id":"call-1","output":[{"type":"vulcan_resource","resource_index":0}]}]})).is_err());
    // Content positions are converter-owned, so a private media tag there must never escape.
    // 内容位置归转换器持有，其中的私有媒体标签绝不能泄漏。
    let private =
        json!({"input":[{"role":"user","content":[{"type":"vulcan_resource","resource":{}}]}]});
    assert!(
        crate::validate_target_media_projection(OPENAI_RESPONSES_PROFILE_ID, &private).is_err()
    );
    assert!(
        crate::validate_target_media_projection(
            OPENAI_RESPONSES_PROFILE_ID,
            &json!({"input":[{"role":"user","content":[{"type":"input_video"}]}]})
        )
        .is_err()
    );
    assert!(
        crate::validate_target_media_projection(
            ANTHROPIC_MESSAGES_PROFILE_ID,
            &json!({"messages":[{"role":"user","content":[{"type":"video"}]}]})
        )
        .is_err()
    );
    // The same literal inside a tool argument is ordinary user data, not an internal media part.
    // 工具参数中的相同字面值是普通用户数据，不是内部媒体项。
    let tool_data = json!({"input":[{"type":"function_call","call_id":"call-1","name":"inspect","arguments":"{\"type\":\"vulcan_resource\"}"}]});
    assert!(
        crate::validate_target_media_projection(OPENAI_RESPONSES_PROFILE_ID, &tool_data).is_ok()
    );
}

/// Builds a typed request whose only content is one uploaded image referenced by provider file id.
/// 构建仅包含一张以供应商文件 ID 引用的已上传图片的强类型请求。
fn uploaded_image_request() -> AiExecutionRequest {
    let mut request = request();
    request.messages[0].content = vec![ContentItem::Resource {
        resource: ResourceReference {
            id: String::from("file-fixture-1"),
            kind: ResourceKind::Image,
            media_type: Some(String::from("image/png")),
            source_kind: ResourceSourceKind::ProviderFileId,
            file_name: Some(String::from("image.png")),
            image_detail: None,
            resource_path: None,
            local_path: None,
            url: None,
        },
    }];
    request
}

/// Verifies an uploaded image id uses each protocol's standard file-reference block, and that a
/// protocol without a registered image file-reference shape fails before dispatch.
/// 验证已上传图片 ID 使用各协议标准文件引用块，且未注册图片文件引用形态的协议在分派前失败。
#[test]
fn uploaded_image_ids_use_standard_protocol_file_blocks() {
    for (protocol, profile, pointer, expected) in [
        (
            UpstreamProtocol::OpenAiResponses,
            OPENAI_RESPONSES_PROFILE_ID,
            "/input/0/content/0",
            json!({"type":"input_image","file_id":"file-fixture-1"}),
        ),
        (
            UpstreamProtocol::AnthropicMessages,
            ANTHROPIC_MESSAGES_PROFILE_ID,
            "/messages/0/content/0",
            json!({"type":"image","source":{"type":"file","file_id":"file-fixture-1"}}),
        ),
    ] {
        let internal = encode_internal_responses_request(&uploaded_image_request())
            .expect("typed internal request");
        let encoded = project_internal_request_for_provider(internal, &context(protocol), profile)
            .expect("uploaded image projection");
        assert_eq!(encoded.body.pointer(pointer), Some(&expected), "{profile}");
        assert!(crate::protocol_profile_supports_media_source(
            profile,
            ResourceKind::Image,
            "image/png",
            ResourceSourceKind::ProviderFileId
        ));
    }
    // OpenAI Chat image parts only carry URLs, so an id must never be smuggled into one.
    // OpenAI Chat 图片部件只携带 URL，因此 ID 绝不能被塞入其中。
    let internal =
        encode_internal_responses_request(&uploaded_image_request()).expect("typed request");
    assert!(
        project_internal_request_for_provider(
            internal,
            &context(UpstreamProtocol::OpenAiChatCompletions),
            OPENAI_CHAT_PROFILE_ID
        )
        .is_err()
    );
    assert!(!crate::protocol_profile_supports_media_source(
        OPENAI_CHAT_PROFILE_ID,
        ResourceKind::Image,
        "image/png",
        ResourceSourceKind::ProviderFileId
    ));
    // Anthropic file sources stay image-only until another kind has a registered shape.
    // Anthropic 文件来源在其他类型注册结构之前仅限图片。
    assert!(!crate::protocol_profile_supports_media_source(
        ANTHROPIC_MESSAGES_PROFILE_ID,
        ResourceKind::File,
        "application/pdf",
        ResourceSourceKind::ProviderFileId
    ));
}

/// Explicit caching reaches the Anthropic projection and is rejected by targets without a projector.
/// 显式缓存到达 Anthropic 投影，没有投影器的目标则拒绝它。
#[test]
fn internal_envelopes_carry_explicit_cache_only_to_anthropic() {
    let text = |role, text: &str| AiMessage {
        role,
        content: vec![ContentItem::Text {
            text: text.to_string(),
        }],
        provider_context_items: Vec::new(),
    };
    let cached = AiExecutionRequest {
        messages: vec![
            text(MessageRole::System, "stable rules"),
            text(MessageRole::User, "hello"),
        ],
        prompt_cache: crate::anthropic_default_prompt_cache_policy(),
        ..request()
    };
    let projected = project_internal_request_for_provider(
        encode_internal_responses_request(&cached).unwrap(),
        &context(UpstreamProtocol::AnthropicMessages),
        ANTHROPIC_MESSAGES_PROFILE_ID,
    )
    .unwrap();
    assert_eq!(
        projected.body["system"][0]["cache_control"],
        json!({ "type": "ephemeral" })
    );
    let messages = projected.body["messages"].as_array().unwrap();
    let last_block = messages.last().unwrap()["content"]
        .as_array()
        .unwrap()
        .last()
        .unwrap();
    assert_eq!(last_block["cache_control"], json!({ "type": "ephemeral" }));

    let rejected = project_internal_request_for_provider(
        encode_internal_responses_request(&cached).unwrap(),
        &context(UpstreamProtocol::OpenAiChatCompletions),
        OPENAI_CHAT_PROFILE_ID,
    );
    assert!(rejected.is_err());
}
