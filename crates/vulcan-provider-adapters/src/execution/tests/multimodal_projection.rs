//! Production encoder regressions for the complete internal media contract.
//! 完整内部媒体契约的生产编码器回归测试。

use super::super::ProviderExecutionClient;
use super::super::test_support::{minimal_request, pinned_openrouter_route};
use serde_json::{Value, json};
use std::sync::Arc;
use vulcan_ai_contract::{
    ContentItem, DispatchOutcome, ResourceKind, ResourceReference, ResourceSourceKind,
};
use vulcan_llm_core::{
    ANTHROPIC_MESSAGES_PROFILE_ID, ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
    CLAUDE_CODE_MESSAGES_PROFILE_ID, GEMINI_GENERATE_CONTENT_PROFILE_ID,
    GEMINI_INTERACTIONS_PROFILE_ID, GOOGLE_CODE_ASSIST_PROFILE_ID, OPENAI_CHAT_PROFILE_ID,
    OPENAI_CODEX_RESPONSES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID, ProtocolProfile,
    ProtocolRegistry,
};

/// Builds an offline client exercising the same encoder as HTTP and WebSocket dispatch.
/// 构建离线客户端，执行与 HTTP 和 WebSocket 分派相同的编码器。
fn client() -> ProviderExecutionClient {
    ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("registered profiles")),
    )
}

/// Builds typed inline `kind` with exact `mime`; returns a resource with deliberate private path metadata.
/// 根据精确 `mime` 构建强类型内联 `kind`，返回有意带私有路径元数据的资源。
fn inline_resource(kind: ResourceKind, mime: &str) -> ResourceReference {
    ResourceReference {
        id: String::from("media-1"),
        kind,
        media_type: Some(mime.to_string()),
        source_kind: ResourceSourceKind::InlineData,
        file_name: Some(String::from("document.pdf")),
        image_detail: None,
        resource_path: Some(String::from(".vulcan/attachments/media-1")),
        local_path: Some(String::from("D:/private-owner/media-1")),
        url: Some(format!("data:{mime};base64,aGVsbG8=")),
    }
}

/// Verifies preflight source capabilities agree with the actual production target encoder.
/// 验证预检来源能力与实际生产目标编码器一致。
#[test]
fn production_multimodal_projection_matches_source_capability_registry() {
    // Reuse immutable registry and route fixtures across the full cross-product.
    // 在完整交叉组合中复用不可变注册表与路由夹具。
    let client = client();
    let baseline_route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
    // This cross-product checks representability only, not live model/provider acceptance.
    // 此交叉组合仅检查表示能力，不代表真实模型或供应商验收。
    let profiles = [
        OPENAI_RESPONSES_PROFILE_ID,
        OPENAI_CODEX_RESPONSES_PROFILE_ID,
        OPENAI_CHAT_PROFILE_ID,
        ANTHROPIC_MESSAGES_PROFILE_ID,
        ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
        CLAUDE_CODE_MESSAGES_PROFILE_ID,
        GEMINI_GENERATE_CONTENT_PROFILE_ID,
        GEMINI_INTERACTIONS_PROFILE_ID,
        GOOGLE_CODE_ASSIST_PROFILE_ID,
    ];
    let media = [
        (ResourceKind::Image, "image/png"),
        (ResourceKind::Image, "image/svg+xml"),
        (ResourceKind::Audio, "audio/wav"),
        (ResourceKind::Audio, "audio/opus"),
        (ResourceKind::Video, "video/mp4"),
        (ResourceKind::Video, "video/webm"),
        (ResourceKind::File, "application/pdf"),
    ];
    for profile in profiles {
        for (kind, mime) in media {
            for source in [
                ResourceSourceKind::InlineData,
                ResourceSourceKind::RemoteUrl,
                ResourceSourceKind::ProviderFileId,
            ] {
                // Exact source fields deliberately exercise each target's different ID and URI semantics.
                // 精确来源字段有意覆盖各目标不同的 ID 与 URI 语义。
                let mut resource = inline_resource(kind, mime);
                resource.source_kind = source;
                resource.url = match source {
                    ResourceSourceKind::InlineData => resource.url,
                    ResourceSourceKind::RemoteUrl => Some(String::from(
                        "https://generativelanguage.googleapis.com/v1beta/files/source",
                    )),
                    ResourceSourceKind::ProviderFileId => None,
                    ResourceSourceKind::HostResolved => unreachable!("prepared source cases only"),
                };
                // The frozen route selects the real converter; no test-only codec performs the projection.
                // 冻结路由选择真实转换器，不使用测试专用编码器投影。
                let mut route = baseline_route.clone();
                route.protocol_route.protocol_profile_id = profile.to_string();
                let mut request = minimal_request();
                request.messages[0]
                    .content
                    .push(ContentItem::Resource { resource });
                let encoded = client.encode_request(&route, &request);
                assert_eq!(
                    encoded.is_ok(),
                    vulcan_llm_core::protocol_profile_supports_media_source(
                        profile, kind, mime, source
                    ),
                    "{profile} {kind:?} {mime} {source:?}: {encoded:?}"
                );
            }
        }
    }
}

/// Verifies every registered production conversion family handles its exact declared media shapes.
/// 验证每个已注册生产转换协议族处理其精确声明媒体结构。
#[test]
fn production_multimodal_projection_covers_all_target_families() {
    // These profile cases test protocol mapping, not live provider/model acceptance.
    // 这些 Profile 用例测试协议映射，不代表真实供应商或模型验收。
    let cases = [
        (
            OPENAI_RESPONSES_PROFILE_ID,
            "/input/0/content/1",
            false,
            false,
        ),
        (
            OPENAI_CODEX_RESPONSES_PROFILE_ID,
            "/input/0/content/1",
            false,
            false,
        ),
        (OPENAI_CHAT_PROFILE_ID, "/messages/0/content/1", true, true),
        (
            ANTHROPIC_MESSAGES_PROFILE_ID,
            "/messages/0/content/1",
            false,
            false,
        ),
        (
            ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
            "/messages/0/content/1",
            false,
            true,
        ),
        (
            CLAUDE_CODE_MESSAGES_PROFILE_ID,
            "/messages/0/content/1",
            false,
            false,
        ),
        (
            GEMINI_GENERATE_CONTENT_PROFILE_ID,
            "/contents/0/parts/1",
            true,
            true,
        ),
        (
            GEMINI_INTERACTIONS_PROFILE_ID,
            "/input/0/content/1",
            true,
            true,
        ),
        (
            GOOGLE_CODE_ASSIST_PROFILE_ID,
            "/request/contents/0/parts/1",
            true,
            true,
        ),
    ];
    // Media fixtures deliberately keep the original kind separate from source encoding.
    // 媒体夹具有意将原始类型与来源编码分开。
    let media = [
        (ResourceKind::Image, "image/png"),
        (ResourceKind::Audio, "audio/wav"),
        (ResourceKind::Video, "video/mp4"),
        (ResourceKind::File, "application/pdf"),
    ];
    for (profile, pointer, audio_supported, video_supported) in cases {
        for (kind, mime) in media {
            // The synthetic frozen route selects an exact existing converter without editing manifests.
            // 合成冻结路由选择精确既有转换器，不修改清单。
            let mut route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
            route.protocol_route.protocol_profile_id = profile.to_string();
            // The same public request must remain unchanged after every projection.
            // 同一公共请求在每次投影后必须保持不变。
            let mut request = minimal_request();
            request.messages[0].content.push(ContentItem::Resource {
                resource: inline_resource(kind, mime),
            });
            // Serialized before-image proves internal source fields survive rather than being mutated.
            // 序列化前镜像证明内部来源字段被保留，而不是遭到修改。
            let before = serde_json::to_value(&request).expect("request fixture");
            // Result comes from the actual public execution encoder, not a standalone profile shortcut.
            // 结果来自实际公共执行编码器，而不是独立 Profile 捷径。
            let result = client().encode_request(&route, &request);
            if (kind == ResourceKind::Audio && !audio_supported)
                || (kind == ResourceKind::Video && !video_supported)
            {
                assert_eq!(
                    result
                        .expect_err("unsupported target must reject before dispatch")
                        .error
                        .dispatch_outcome,
                    DispatchOutcome::NotDispatched
                );
                continue;
            }
            // The target block must exist at the exact converter-owned input position.
            // 目标内容块必须存在于转换器持有的精确输入位置。
            let encoded = result.unwrap_or_else(|error| panic!("{profile} {kind:?}: {error:?}"));
            // Each media kind maps to one distinct native block rather than a text placeholder.
            // 每种媒体映射为一个不同的原生内容块，而非文本占位符。
            let part = encoded.body.pointer(pointer).expect("target media part");
            match profile {
                OPENAI_RESPONSES_PROFILE_ID | OPENAI_CODEX_RESPONSES_PROFILE_ID => {
                    assert_eq!(
                        part["type"],
                        if kind == ResourceKind::Image {
                            "input_image"
                        } else {
                            "input_file"
                        }
                    );
                }
                OPENAI_CHAT_PROFILE_ID => {
                    assert_eq!(
                        part["type"],
                        match kind {
                            ResourceKind::Image => "image_url",
                            ResourceKind::Audio => "input_audio",
                            ResourceKind::Video => "video_url",
                            ResourceKind::File => "file",
                        }
                    );
                    if kind == ResourceKind::Audio {
                        assert_eq!(
                            part["input_audio"],
                            json!({"data":"aGVsbG8=", "format":"wav"})
                        );
                    }
                }
                ANTHROPIC_MESSAGES_PROFILE_ID
                | ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
                | CLAUDE_CODE_MESSAGES_PROFILE_ID => {
                    assert_eq!(
                        part["type"],
                        match kind {
                            ResourceKind::Image => "image",
                            ResourceKind::File => "document",
                            ResourceKind::Video => "video",
                            ResourceKind::Audio => unreachable!("audio rejected above"),
                        }
                    );
                    assert_eq!(part["source"]["media_type"], mime);
                    assert_eq!(part["source"]["data"], "aGVsbG8=");
                }
                GEMINI_INTERACTIONS_PROFILE_ID => {
                    assert_eq!(
                        part["type"],
                        match kind {
                            ResourceKind::Image => "image",
                            ResourceKind::Audio => "audio",
                            ResourceKind::Video => "video",
                            ResourceKind::File => "document",
                        }
                    );
                    assert_eq!(part["mime_type"], mime);
                    assert_eq!(part["data"], "aGVsbG8=");
                }
                _ => assert_eq!(
                    part["inlineData"],
                    json!({"mimeType":mime, "data":"aGVsbG8="})
                ),
            }
            assert!(!encoded.body.to_string().contains("vulcan_resource"));
            assert!(!encoded.body.to_string().contains("private-owner"));
            assert!(!encoded.body.to_string().contains("resource_path"));
            assert_eq!(
                serde_json::to_value(&request).expect("request fixture"),
                before
            );
        }
    }
}

/// Verifies one mixed user input retains ordered text, image, audio, video and PDF on a capable target.
/// 验证一个混合用户输入在支持目标上保留有序文本、图片、音频、视频与 PDF。
#[test]
fn production_multimodal_projection_keeps_mixed_input_order() {
    // Gemini provides native mappings for all four media kinds in one message.
    // Gemini 为一条消息中的四种媒体均提供原生映射。
    let mut route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
    route.protocol_route.protocol_profile_id = GEMINI_GENERATE_CONTENT_PROFILE_ID.to_string();
    // Ordered canonical content remains the source of truth.
    // 有序规范内容保持为唯一来源。
    let mut request = minimal_request();
    for (kind, mime) in [
        (ResourceKind::Image, "image/png"),
        (ResourceKind::Audio, "audio/wav"),
        (ResourceKind::Video, "video/mp4"),
        (ResourceKind::File, "application/pdf"),
    ] {
        request.messages[0].content.push(ContentItem::Resource {
            resource: inline_resource(kind, mime),
        });
    }
    // Encoding must not split or reorder the original user content.
    // 编码不能拆散或重排原始用户内容。
    let encoded = client()
        .encode_request(&route, &request)
        .expect("mixed input should encode");
    assert_eq!(
        encoded.body["contents"][0]["parts"],
        json!([
            {"text":"hello"},
            {"inlineData":{"mimeType":"image/png","data":"aGVsbG8="}},
            {"inlineData":{"mimeType":"audio/wav","data":"aGVsbG8="}},
            {"inlineData":{"mimeType":"video/mp4","data":"aGVsbG8="}},
            {"inlineData":{"mimeType":"application/pdf","data":"aGVsbG8="}},
        ])
    );
}

/// Verifies provider file IDs, uploaded URLs and inline data remain distinct through production encoding.
/// 验证供应商文件 ID、上传 URL 与内联数据经过生产编码后仍保持区别。
#[test]
fn production_multimodal_projection_preserves_uploaded_source_identity() {
    for (profile, kind, mime, source, pointer, expected) in [
        (
            OPENAI_RESPONSES_PROFILE_ID,
            ResourceKind::File,
            "application/pdf",
            ResourceSourceKind::ProviderFileId,
            "/input/0/content/1/file_id",
            "file-uploaded",
        ),
        (
            OPENAI_RESPONSES_PROFILE_ID,
            ResourceKind::Image,
            "image/png",
            ResourceSourceKind::ProviderFileId,
            "/input/0/content/1/file_id",
            "file-uploaded",
        ),
        (
            OPENAI_CHAT_PROFILE_ID,
            ResourceKind::File,
            "application/pdf",
            ResourceSourceKind::ProviderFileId,
            "/messages/0/content/1/file/file_id",
            "file-uploaded",
        ),
        (
            OPENAI_CHAT_PROFILE_ID,
            ResourceKind::Video,
            "video/mp4",
            ResourceSourceKind::RemoteUrl,
            "/messages/0/content/1/video_url/url",
            "ms://uploaded-video",
        ),
        (
            ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
            ResourceKind::Video,
            "video/mp4",
            ResourceSourceKind::RemoteUrl,
            "/messages/0/content/1/source/url",
            "ms://uploaded-video",
        ),
        (
            GEMINI_GENERATE_CONTENT_PROFILE_ID,
            ResourceKind::File,
            "application/pdf",
            ResourceSourceKind::RemoteUrl,
            "/contents/0/parts/1/fileData/fileUri",
            "https://generativelanguage.googleapis.com/v1beta/files/uploaded-document",
        ),
    ] {
        // Only the exact registered profile is changed in this isolated encoding route.
        // 此隔离编码路由只修改精确已注册 Profile。
        let mut route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
        route.protocol_route.protocol_profile_id = profile.to_string();
        // This reference simulates already-completed provider preparation, without uploading.
        // 此引用模拟已完成的供应商准备，不执行上传。
        let mut resource = inline_resource(kind, mime);
        resource.id = String::from("file-uploaded");
        resource.source_kind = source;
        resource.url = if source == ResourceSourceKind::ProviderFileId {
            None
        } else {
            Some(expected.to_string())
        };
        // The request still owns the full source metadata while the wire sees only a valid reference.
        // 请求仍持有完整来源元数据，而线路仅看到有效引用。
        let mut request = minimal_request();
        request.messages[0]
            .content
            .push(ContentItem::Resource { resource });
        // Exact pointer assertions prevent a file ID from being silently turned into a URL.
        // 精确路径断言防止文件 ID 被静默转为 URL。
        let encoded = client()
            .encode_request(&route, &request)
            .expect("prepared resource should encode");
        assert_eq!(
            encoded.body.pointer(pointer),
            Some(&Value::String(expected.to_string()))
        );
        assert!(!encoded.body.to_string().contains("base64"));
    }
}

/// Verifies malformed media cannot exploit MIME relabelling or source-field ambiguity.
/// 验证格式错误的媒体不能利用 MIME 重标记或来源字段歧义。
#[test]
fn production_multimodal_projection_rejects_contradictory_media() {
    // Chat has enough media support to expose validation failures rather than broad modality rejection.
    // Chat 具备足够媒体支持，使测试暴露校验失败而不是宽泛模态拒绝。
    let route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
    for mut resource in [
        inline_resource(ResourceKind::Video, "image/png"),
        inline_resource(ResourceKind::File, "video/mp4"),
        inline_resource(ResourceKind::Audio, "audio/wav"),
    ] {
        if resource.kind == ResourceKind::Audio {
            resource.url = Some(String::from("data:audio/mp3;base64,aGVsbG8="));
        }
        // Contradictory canonical data must fail before transport, not be repaired by guessing.
        // 矛盾规范数据必须在传输前失败，不能通过猜测修复。
        let mut request = minimal_request();
        request.messages[0]
            .content
            .push(ContentItem::Resource { resource });
        assert_eq!(
            client()
                .encode_request(&route, &request)
                .expect_err("contradictory MIME must reject")
                .error
                .dispatch_outcome,
            DispatchOutcome::NotDispatched
        );
    }
}

/// Verifies standard Responses remains strict even though the internal constructor accepts video.
/// 验证即使内部构造器接受视频，标准 Responses 仍保持严格。
#[test]
fn production_multimodal_projection_keeps_standard_responses_contract() {
    // The typed request is valid internally and invalid only for an unsupported wire target.
    // 强类型请求在内部有效，仅对不支持的线路目标无效。
    let mut request = minimal_request();
    request.messages[0].content.push(ContentItem::Resource {
        resource: inline_resource(ResourceKind::Video, "video/mp4"),
    });
    assert!(vulcan_llm_core::encode_internal_responses_request(&request).is_ok());
    assert!(
        vulcan_llm_core::OpenAiResponsesProfile
            .encode(&request)
            .is_err()
    );
    assert!(
        vulcan_llm_core::OpenAiCodexResponsesProfile
            .encode(&request)
            .is_err()
    );
    request.messages[0].content.pop();
    request.cache_scope_id = Some(String::from("private-session-identity"));
    // The standard target receives its output limit without Codex-only client metadata.
    // 标准目标接收自身输出限制，不接收 Codex 专用客户端元数据。
    let mut route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
    route.protocol_route.protocol_profile_id = OPENAI_RESPONSES_PROFILE_ID.to_string();
    // Compare standard and Codex outputs from the same canonical request.
    // 比较同一规范请求的标准与 Codex 输出。
    let standard = client()
        .encode_request(&route, &request)
        .expect("standard Responses");
    assert!(standard.body.get("client_metadata").is_none());
    assert!(standard.body.get("prompt_cache_key").is_none());
    assert_eq!(standard.body["max_output_tokens"], 512);
    route.protocol_route.protocol_profile_id = OPENAI_CODEX_RESPONSES_PROFILE_ID.to_string();
    // Codex retains its exact cache identity and omits the unsupported output-limit field.
    // Codex 保留精确缓存身份，并省略不支持的输出限制字段。
    let codex = client()
        .encode_request(&route, &request)
        .expect("Codex Responses");
    assert_eq!(
        codex.body["client_metadata"]["session_id"],
        "private-session-identity"
    );
    assert!(codex.body.get("max_output_tokens").is_none());
}
