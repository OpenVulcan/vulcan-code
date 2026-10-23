//! Offline tests for actual media reading, tool exposure and protocol-native input.
//! 实际媒体读取、工具暴露及协议原生输入的离线测试。

use super::*;
use crate::agent_runtime::events::ProviderRequestMediaContext;
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc};
use vulcan_ai_contract::{AiExecutionRequest, AiMessage, ContentItem, MessageRole, ToolResult};
use vulcan_provider_adapters::{
    CredentialConnection, CredentialMode, CredentialSecretEnvelope, CredentialSecretResolver,
    CredentialSecretStore, PublicAiRuntime,
};

/// Isolated credential owner; no user secrets or remote endpoints are used.
/// 隔离凭据所有者；不使用用户秘密或远程端点。
struct OfflineSecrets;
#[async_trait::async_trait]
impl CredentialSecretResolver for OfflineSecrets {
    /// Returns an isolated secret for `reference`; unknown references fail explicitly.
    /// 为`reference`返回隔离秘密；未知引用明确失败。
    async fn read_optional_secret(
        &self,
        reference: &str,
    ) -> Result<Option<CredentialSecretEnvelope>, String> {
        assert_eq!(reference, "native-media-test-secret");
        Ok(Some(CredentialSecretEnvelope::new_api_key(
            "offline-only".into(),
        )?))
    }
}
#[async_trait::async_trait]
impl CredentialSecretStore for OfflineSecrets {
    /// Rejects creation because the fixture is immutable; arguments never reach a real store.
    /// 夹具不可变，因此拒绝创建；参数绝不进入真实存储。
    async fn create_secret(&self, _: &str, _: &CredentialSecretEnvelope) -> Result<(), String> {
        Err("read-only fixture".into())
    }
    /// Rejects replacement and ignores the supplied revision; returns an explicit fixture error.
    /// 拒绝替换并忽略提供的修订；返回明确夹具错误。
    async fn replace_secret(
        &self,
        _: &str,
        _: u64,
        _: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err("read-only fixture".into())
    }
    /// Rejects deletion of the supplied reference; returns an explicit fixture error.
    /// 拒绝删除提供的引用；返回明确夹具错误。
    async fn delete_secret(&self, _: &str) -> Result<(), String> {
        Err("read-only fixture".into())
    }
    /// Rejects restoration of supplied bytes; returns an explicit fixture error.
    /// 拒绝恢复提供的字节；返回明确夹具错误。
    async fn restore_secret(&self, _: &str, _: &CredentialSecretEnvelope) -> Result<(), String> {
        Err("read-only fixture".into())
    }
}

/// Resolves a real isolated catalog then freezes its test `profile`; returns runtime, configuration and selection.
/// 解析真实隔离目录，再冻结测试`profile`；返回运行时、配置及选择。
fn fixture(
    profile: &str,
) -> (
    Arc<PublicAiRuntime>,
    PersistedAiConfiguration,
    ResolvedPublicAiSelection,
) {
    // Runtime owns only this fixture's secret resolver.
    // 运行时仅持有本夹具秘密解析器。
    let runtime = Arc::new(PublicAiRuntime::first_batch(Arc::new(OfflineSecrets)).unwrap());
    // Configuration is memory-only and never saved to the user workspace.
    // 配置仅存在于内存，绝不保存到用户工作区。
    let mut config = PersistedAiConfiguration::default();
    config
        .connection_graph
        .add_connection(
            &runtime.providers().unwrap(),
            CredentialConnection {
                id: "native-media-test".into(),
                provider_id: "openrouter".into(),
                service_entry_id: "public-api".into(),
                credential_profile_id: "api-key".into(),
                credential_mode: CredentialMode::Token,
                credential_id: "native-media-credential".into(),
                display_name: "offline".into(),
                secret_reference: "native-media-test-secret".into(),
                management_secret_reference: None,
                service_origin_override: None,
                prompt_cache_by_protocol: BTreeMap::new(),
            },
        )
        .unwrap();
    config.set_model_enabled("openrouter", "openai/gpt-5.6-terra", true);
    // The catalog explicitly declares all modalities; each protocol still must prove encoder support.
    // 目录明确声明全部模态；每种协议仍须证明编码器支持。
    let mut catalog = vulcan_provider_adapters::parse_openrouter_model_catalog("native-media-test", 1,
        r#"{"data":[{"id":"openai/gpt-5.6-terra","name":"Native","context_length":8192,"supported_parameters":["tools"],"architecture":{"input_modalities":["text","image","audio","video","file"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}}]}"#).unwrap();
    for model in &mut catalog.models {
        model.protocol_variants.insert(
            vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID.into(),
            vulcan_provider_adapters::ProtocolModelVariant {
                capabilities: model.capabilities.clone(),
                capability_evidence: Default::default(),
                discovery_signature: "offline-native-media".into(),
                catalog_generation: catalog.generation,
            },
        );
    }
    runtime.stage_source_catalog_for_testing(catalog).unwrap();
    runtime.replace_configuration(config.clone()).unwrap();
    runtime.reapply_model_catalog_overlays(&config).unwrap();
    // Inject a frozen protocol fixture, without changing any shared provider manifest.
    // 注入冻结协议夹具，不改变任何共享供应商清单。
    let mut selection = runtime
        .resolve_connection_model_selection(
            &config,
            "native-media-test",
            "openrouter",
            "openai/gpt-5.6-terra",
        )
        .unwrap();
    selection.route.protocol_route.protocol_profile_id = profile.into();
    selection
        .route
        .protocol_route
        .input_resource_constraints
        .clear();
    selection.model.capabilities.input_resources = vec![
        ResourceKind::Image,
        ResourceKind::Audio,
        ResourceKind::Video,
        ResourceKind::File,
    ];
    (runtime, config, selection)
}

/// Builds a canonical result message for exact `id` and `output`, preserving the original tool topology.
/// 为精确`id`及`output`构建规范结果消息，保留原工具拓扑。
fn result_message(id: &str, output: Value) -> AiMessage {
    AiMessage {
        role: MessageRole::Tool,
        content: vec![ContentItem::ToolResult {
            result: ToolResult {
                call_id: id.into(),
                is_error: false,
                content: output,
            },
        }],
        provider_context_items: Vec::new(),
    }
}

/// A PNG-only native model receives converted TIFF bytes, with an explicit receipt and unchanged original file.
/// 仅支持PNG的原生模型收到转换后的TIFF字节，同时具有明确回执且原文件不变。
#[tokio::test]
async fn native_media_image_conversion_delivers_png_and_reports_decode_errors() {
    // Freeze PNG-only admission through the same route contract used by real tool execution.
    // 通过真实工具执行使用的同一路由契约冻结仅PNG准入。
    let (runtime, _, mut selection) = fixture(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID);
    selection.route.protocol_route.input_resource_constraints =
        vec![vulcan_provider_adapters::InputResourceConstraint {
            resource_kind: ResourceKind::Image,
            accepted_mime_types: vec!["image/png".into()],
            max_bytes: None,
            max_count: None,
            reference_policy: Default::default(),
            upload_channel_refs: vec![],
        }];
    // Use a genuine TIFF; renamed arbitrary bytes must never produce a conversion success.
    // 使用真实TIFF；仅改名的任意字节绝不能转换成功。
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("picture.tiff");
    image::RgbImage::from_pixel(2, 1, image::Rgb([1, 2, 3]))
        .save(&path)
        .unwrap();
    let original = std::fs::read(&path).unwrap();
    let result = tool::read_media(
        runtime.clone(),
        selection.clone(),
        None,
        None,
        "s",
        json!({"path":path}),
    )
    .await
    .unwrap();
    let envelope = parse_envelope(result.model_output.as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(envelope.resource.media_type.as_deref(), Some("image/png"));
    assert_eq!(envelope.resource.file_name.as_deref(), Some("picture.png"));
    assert!(
        envelope
            .text
            .contains("Automatically converted from image/tiff to image/png")
    );
    assert!(
        envelope
            .resource
            .url
            .unwrap()
            .starts_with("data:image/png;base64,iVBOR")
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
    std::fs::write(&path, b"corrupt TIFF").unwrap();
    let error = tool::read_media(runtime, selection, None, None, "s", json!({"path":path}))
        .await
        .unwrap_err();
    assert!(error.contains("Image conversion from image/tiff to image/png failed:"));
    assert!(error.contains("No content was attached"));
}

/// Confirms native modalities hide both kinds of configured external backend before validating missing routes.
/// 确认原生模态在校验缺失路线前隐藏两种已配置外部后端。
#[test]
fn native_media_exposure_replaces_only_supported_understanding() {
    // Responses supports images and PDF, even when the model also declares audio/video.
    // 即使模型同时声明音频及视频，Responses仍仅支持图片及PDF。
    let (runtime, mut config, selection) = fixture(vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID);
    config
        .provider_tool_enabled_capabilities
        .insert(ProviderToolCapabilityKind::ImageUnderstand);
    // Deliberately omit the external route: hidden redundant backends must not block the turn.
    // 刻意省略外部路线：隐藏的重复后端不能阻塞回合。
    let before = serde_json::to_value(&config).unwrap();
    let candidates = crate::provider_tool_bridge::build_provider_tool_candidates(
        runtime.clone(),
        config.clone(),
        None,
        &None,
        &None,
        &None,
        &None,
        &None,
        false,
        Some(&selection),
        None,
    )
    .unwrap();
    assert!(
        candidates
            .iter()
            .any(|tool| tool.descriptor.id == "read_media")
    );
    assert!(
        !candidates
            .iter()
            .any(|tool| tool.descriptor.id == "image_understand")
    );
    assert_eq!(before, serde_json::to_value(&config).unwrap());
    assert!(supported_mimes(&selection).contains(&"application/pdf"));
    assert!(
        !supported_mimes(&selection)
            .iter()
            .any(|mime| mime.starts_with("audio/"))
    );
    for backend in [
        vulcan_provider_adapters::MediaUnderstandingBackendSelection::ProviderTool {
            selection: vulcan_provider_adapters::ProviderToolTargetSelection {
                connection_id: "missing-external-account".into(),
                target_id: "missing-target".into(),
            },
        },
        vulcan_provider_adapters::MediaUnderstandingBackendSelection::ModelReplacement {
            selection: vulcan_provider_adapters::ActiveAiModelSelection {
                provider_id: "missing-provider".into(),
                connection_id: "missing-external-account".into(),
                model_id: "missing-model".into(),
            },
            reasoning_effort: None,
        },
    ] {
        // Both obsolete external routes must be ignored when the exact current model accepts images.
        // 精确当前模型接受图片时，两个过期外部路线都必须被忽略。
        let mut configured = config.clone();
        configured
            .media_understanding_selections
            .insert(ProviderToolCapabilityKind::ImageUnderstand, backend);
        let tools = crate::provider_tool_bridge::build_provider_tool_candidates(
            runtime.clone(),
            configured,
            None,
            &None,
            &None,
            &None,
            &None,
            &None,
            false,
            Some(&selection),
            None,
        )
        .unwrap();
        assert!(
            !tools
                .iter()
                .any(|tool| tool.descriptor.id == "image_understand")
        );
    }
    // Unsupported audio keeps its existing enabled gate and therefore requires its real backend.
    // 不支持的音频保留既有启用闸门，因此要求真实后端。
    config
        .provider_tool_enabled_capabilities
        .insert(ProviderToolCapabilityKind::AudioUnderstand);
    assert!(
        crate::provider_tool_bridge::build_provider_tool_candidates(
            runtime,
            config,
            None,
            &None,
            &None,
            &None,
            &None,
            &None,
            false,
            Some(&selection),
            None
        )
        .is_err()
    );
    // A protocol with all three native modalities suppresses every external understanding backend together.
    // 同时具备三种原生模态的协议会一并隐藏全部外部理解后端。
    let (runtime, mut all_config, all_selection) = fixture(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID);
    all_config.provider_tool_enabled_capabilities.extend([
        ProviderToolCapabilityKind::ImageUnderstand,
        ProviderToolCapabilityKind::AudioUnderstand,
        ProviderToolCapabilityKind::VideoUnderstand,
    ]);
    // Missing external routes cannot obstruct the current model's own native inputs.
    // 缺失外部路线不能阻碍当前模型自己的原生输入。
    let all_tools = crate::provider_tool_bridge::build_provider_tool_candidates(
        runtime,
        all_config,
        None,
        &None,
        &None,
        &None,
        &None,
        &None,
        false,
        Some(&all_selection),
        None,
    )
    .unwrap();
    assert!(
        all_tools
            .iter()
            .any(|tool| tool.descriptor.id == "read_media")
    );
    assert!(!all_tools.iter().any(|tool| matches!(
        tool.descriptor.id.as_str(),
        "image_understand" | "audio_understand" | "video_understand"
    )));
}

/// An image-only model sees explicit type boundaries before calling the tool and receives the same routing on misuse.
/// 仅支持图片的模型在调用前看到明确类型边界，误调用时获得相同工具选择指引。
#[tokio::test]
async fn native_media_instructions_distinguish_image_support_from_video_understanding() {
    // Freeze image-only support even though the protocol itself can encode more modalities.
    // 即使协议本身能编码更多模态，也冻结为仅支持图片。
    let (runtime, _, mut selection) = fixture(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID);
    selection.model.capabilities.input_resources = vec![ResourceKind::Image];
    // Without an independent MIME restriction, the reader advertises the existing default image formats.
    // 没有独立MIME限制时，读取器声明既有默认图片格式。
    let default_candidate =
        tool::build_read_media_candidate(runtime.clone(), selection.clone(), None, None).unwrap();
    assert!(
        default_candidate
            .descriptor
            .description
            .contains("image/png")
    );
    assert!(
        default_candidate
            .descriptor
            .description
            .contains("image/jpeg")
    );
    selection.route.protocol_route.input_resource_constraints =
        vec![vulcan_provider_adapters::InputResourceConstraint {
            resource_kind: ResourceKind::Image,
            accepted_mime_types: vec!["image/png".into()],
            max_bytes: None,
            max_count: None,
            reference_policy: vulcan_provider_adapters::ResourceReferencePolicy::Any,
            upload_channel_refs: vec![],
        }];
    // Inspect the real candidate description and schema that enter tool registration.
    // 检查进入工具注册的真实候选说明及Schema。
    let candidate =
        tool::build_read_media_candidate(runtime.clone(), selection.clone(), None, None)
            .expect("image-only model must receive read_media");
    // A single admitted format must not advertise the broader host image registry.
    // 仅准入一种格式时，不能声明更广的宿主图片注册表。
    let description = &candidate.descriptor.description;
    assert!(description.contains("Images: SUPPORTED only for image/png."));
    assert!(description.contains("Additional images automatically converted to PNG: image/jpeg"));
    assert!(description.contains("animated GIF/WebP and multipage TIFF fail"));
    assert!(description.contains("Audio: NOT SUPPORTED by read_media."));
    assert!(description.contains("Video: NOT SUPPORTED by read_media."));
    assert!(description.contains("PDF: NOT SUPPORTED by read_media."));
    assert!(description.contains("call video_understand if it is in your available tools"));
    assert!(description.contains("call audio_understand if it is in your available tools"));
    assert!(
        description.contains("otherwise report that no suitable understanding tool is available")
    );
    assert!(description.contains("Image support does not imply video support."));
    // The path argument advertises the identical MIME boundary instead of a generic media promise.
    // 路径参数声明相同MIME边界，不泛化承诺媒体支持。
    let path_description = candidate.parameters_schema["properties"]["path"]["description"]
        .as_str()
        .unwrap();
    assert!(path_description.contains("Native MIME types ONLY: image/png."));
    assert!(!path_description.contains("video/mp4"));
    // A real video path must fail before any native content or external inference is produced.
    // 真实视频路径必须在产出原生内容或外部推理之前失败。
    let directory = tempfile::tempdir().unwrap();
    // Keep the registered video extension so execution resolves its actual category.
    // 保留已注册视频扩展名，使执行解析其实际类别。
    let path = directory.path().join("unsupported.mp4");
    std::fs::write(&path, b"video-fixture").unwrap();
    // Runtime errors repeat the same positive alternative instead of inviting repeated read attempts.
    // 运行时错误重复相同可选工具指引，不引导反复尝试读取。
    let error = tool::read_media(
        runtime.clone(),
        selection.clone(),
        None,
        None,
        "s",
        json!({"path":path}),
    )
    .await
    .unwrap_err();
    assert!(error.contains("cannot read MIME type `video/mp4`"));
    assert!(error.contains("Video: NOT SUPPORTED by read_media."));
    assert!(error.contains("call video_understand if it is in your available tools"));
    // A new frozen model that supports video must advertise native video and stop recommending its external backend.
    // 新冻结模型支持视频后必须声明原生视频，并停止推荐对应外部后端。
    selection
        .model
        .capabilities
        .input_resources
        .push(ResourceKind::Video);
    // Rebuild the exposed candidate from the updated exact capability snapshot.
    // 根据更新后的精确能力快照重新构建可见候选。
    let video_candidate = tool::build_read_media_candidate(runtime, selection, None, None).unwrap();
    assert!(
        video_candidate
            .descriptor
            .description
            .contains("Video: SUPPORTED only for")
    );
    assert!(
        !video_candidate
            .descriptor
            .description
            .contains("video_understand")
    );
}

/// Reads all four actual local formats and proves the final encoder receives media after the complete tool batch.
/// 读取四种实际本地格式，证明最终编码器在完整工具批次之后接收媒体。
#[tokio::test]
async fn native_media_reads_reach_protocol_input_without_external_understanding() {
    for (profile, extension, mime, native_field) in [
        (
            vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
            "png",
            "image/png",
            "image_url",
        ),
        (
            vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
            "wav",
            "audio/wav",
            "input_audio",
        ),
        (
            vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
            "mp4",
            "video/mp4",
            "video_url",
        ),
        (
            vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
            "pdf",
            "application/pdf",
            "input_file",
        ),
        (
            vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID,
            "png",
            "image/png",
            "base64",
        ),
        (
            vulcan_llm_core::GEMINI_GENERATE_CONTENT_PROFILE_ID,
            "mp4",
            "video/mp4",
            "inlineData",
        ),
    ] {
        // Exact frozen selection supplies both the reader and final projector.
        // 精确冻结选择同时提供给读取器及最终投影器。
        let (runtime, _, selection) = fixture(profile);
        // Small original bytes exercise transport integrity without decoding or transforming the media.
        // 小型原字节验证传输完整性，不解码或转换媒体。
        let directory = tempfile::tempdir().unwrap();
        // Keep the source extension explicit for the host format registry.
        // 为宿主格式注册表保留明确来源扩展名。
        let path = directory.path().join(format!("source.{extension}"));
        std::fs::write(&path, b"original-media-bytes").unwrap();
        // Execute only the native reader; this fixture has no reachable inference endpoint.
        // 仅执行原生读取器；本夹具没有可达推理端点。
        let output = tool::read_media(
            runtime.clone(),
            selection.clone(),
            None,
            None,
            "native-session",
            json!({"path":path}),
        )
        .await
        .unwrap();
        // Decode the durable marker to verify exact original data, then simulate V1 persistence.
        // 解码持久标记验证精确原数据，再模拟V1持久化。
        let envelope = parse_envelope(output.model_output.as_str().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(envelope.resource.media_type.as_deref(), Some(mime));
        assert_eq!(
            envelope.resource.url.as_deref(),
            Some(format!("data:{mime};base64,b3JpZ2luYWwtbWVkaWEtYnl0ZXM=").as_str())
        );
        // Removing the original source proves request replay uses the immutable tool result.
        // 移除原来源证明请求重放使用不可变工具结果。
        std::fs::remove_file(path).unwrap();
        // The real V1 wrapper owns model_output; arbitrary outer JSON is never passed as media bytes.
        // 真实V1包装器持有model_output；任意外层JSON绝不作为媒体字节传递。
        let durable =
            json!({"model_output":output.model_output,"host_result":null,"tool_name":"read_media"});
        // Preserve two adjacent tool results before appending one synthetic input message.
        // 在追加一条合成输入消息前保留两个相邻工具结果。
        let mut messages = vec![
            AiMessage {
                role: MessageRole::Assistant,
                content: ["media-call", "sibling-call"]
                    .into_iter()
                    .map(|id| ContentItem::ToolCall {
                        call: vulcan_ai_contract::ToolCall {
                            id: id.into(),
                            name: "read_media".into(),
                            arguments: json!({"path":"source"}),
                            signature: None,
                        },
                    })
                    .collect(),
                provider_context_items: vec![],
            },
            result_message(
                "media-call",
                v1_native_media_content(&durable).unwrap().unwrap(),
            ),
            result_message("sibling-call", json!("done")),
        ];
        // The current execution is resolved from the same frozen selection, never global settings.
        // 当前执行从相同冻结选择解析，绝不使用全局设置。
        let context = ProviderRequestMediaContext {
            execution: Arc::new(
                runtime
                    .resolve_execution_from_frozen_selection(selection)
                    .await
                    .unwrap(),
            ),
            session_scope_id: "native-session".into(),
        };
        // Capture and compaction consume controller text containing the full V1 completion wrapper.
        // 捕获及压缩消费包含完整V1完成包装器的控制器正文。
        let mut wrapped_messages = vec![result_message(
            "media-call",
            Value::String(durable.to_string()),
        )];
        project_native_media_messages(&mut wrapped_messages, Some(&context)).unwrap();
        assert!(
            wrapped_messages
                .last()
                .unwrap()
                .content
                .iter()
                .any(|item| matches!(item, ContentItem::Resource { .. }))
        );
        assert!(
            !serde_json::to_string(&wrapped_messages)
                .unwrap()
                .contains(MARKER)
        );
        project_native_media_messages(&mut messages, Some(&context)).unwrap();
        assert_eq!(
            messages
                .iter()
                .map(|message| message.role)
                .collect::<Vec<_>>(),
            vec![
                MessageRole::Assistant,
                MessageRole::Tool,
                MessageRole::Tool,
                MessageRole::User
            ]
        );
        assert!(
            messages
                .last()
                .unwrap()
                .content
                .iter()
                .any(|item| matches!(item, ContentItem::Resource { .. }))
        );
        // The actual protocol encoder must emit a native media field, not the host JSON marker.
        // 实际协议编码器必须输出原生媒体字段，而非宿主JSON标记。
        let request: AiExecutionRequest = serde_json::from_value(json!({"operation":"conversation_respond","model_id":"native-test-model","messages":messages,"output_token_limit":1024})).unwrap();
        // Resolve the registered encoder by exact profile without any network call.
        // 按精确Profile解析已注册编码器，不执行任何网络调用。
        let wire = vulcan_llm_core::ProtocolRegistry::first_batch()
            .unwrap()
            .require(profile)
            .unwrap()
            .encode(&request)
            .unwrap();
        let encoded = serde_json::to_string(&wire.body).unwrap();
        assert!(
            encoded.contains(native_field),
            "{profile} did not encode {native_field}: {encoded}"
        );
        assert!(!encoded.contains(MARKER));
    }
}

/// Checks stale model support and a shared request budget produce explicit unsent text, never leaked Base64.
/// 检查过期模型支持与共享请求预算产生明确未发送文字，绝不泄露Base64。
#[tokio::test]
async fn native_media_replay_respects_current_model_and_user_budget() {
    // One actual read supplies an immutable inline result for both replay cases.
    // 一次实际读取为两种重放情况提供不可变内联结果。
    let (runtime, _, selection) = fixture(vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sample.png");
    std::fs::write(&path, b"abc").unwrap();
    let output = tool::read_media(
        runtime.clone(),
        selection.clone(),
        None,
        None,
        "s",
        json!({"path":path}),
    )
    .await
    .unwrap()
    .model_output;
    let envelope = parse_envelope(output.as_str().unwrap()).unwrap().unwrap();
    // Existing user media reserves the only four encoded bytes before tool output admission.
    // 在准入工具输出前，既有用户媒体预留唯一的四个编码字节。
    let mut budget_selection = selection.clone();
    budget_selection
        .media_delivery_policy
        .conversation
        .max_total_encoded_bytes = 4;
    let budget_context = ProviderRequestMediaContext {
        execution: Arc::new(
            runtime
                .resolve_execution_from_frozen_selection(budget_selection)
                .await
                .unwrap(),
        ),
        session_scope_id: "s".into(),
    };
    let mut messages = vec![
        AiMessage {
            role: MessageRole::User,
            content: vec![ContentItem::Resource {
                resource: envelope.resource,
            }],
            provider_context_items: vec![],
        },
        result_message("read", output.clone()),
    ];
    project_native_media_messages(&mut messages, Some(&budget_context)).unwrap();
    assert_eq!(messages.len(), 2);
    assert!(
        serde_json::to_string(&messages.last())
            .unwrap()
            .contains("combined encoded files")
    );
    // A subsequent text-only model must not receive historical image bytes.
    // 后续纯文本模型不能接收历史图片字节。
    let mut text_selection = selection;
    text_selection.model.capabilities.input_resources.clear();
    let context = ProviderRequestMediaContext {
        execution: Arc::new(
            runtime
                .resolve_execution_from_frozen_selection(text_selection)
                .await
                .unwrap(),
        ),
        session_scope_id: "s".into(),
    };
    let mut messages = vec![result_message("read", output)];
    project_native_media_messages(&mut messages, Some(&context)).unwrap();
    assert!(!serde_json::to_string(&messages).unwrap().contains("YWJj"));
    assert!(
        serde_json::to_string(&messages)
            .unwrap()
            .contains("not attached")
    );
}

/// Preserves original upload ownership and TTL; replay never uploads again when either becomes invalid.
/// 保留原上传归属及TTL；任何一项无效时，重放绝不再次上传。
#[tokio::test]
async fn native_media_uploaded_history_is_bound_to_original_route_and_lifetime() {
    // The fixture declares a real channel whose endpoint must never be contacted during replay.
    // 夹具声明真实通道，但重放期间绝不能访问其端点。
    let (runtime, _, mut selection) = fixture(vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID);
    selection.route.upload_channels = vec![serde_json::from_value(json!({
        "upload_channel_id":"native-test-files", "primitive":"provider_file_create", "endpoint_host":"unreachable.invalid", "endpoint_path":"/files", "method":"POST", "file_field":"file", "accepted_mime_types":["image/png"], "max_bytes":1048576, "max_count":1, "timeout_ms":1000, "response_id_pointer":"/id", "reference_encoding":"provider_file_id", "reference_scope":"session", "cache_duration_ms":60000
    })).unwrap()];
    selection.route.protocol_route.input_resource_constraints =
        vec![vulcan_provider_adapters::InputResourceConstraint {
            resource_kind: ResourceKind::Image,
            accepted_mime_types: vec!["image/png".into()],
            max_bytes: Some(1048576),
            max_count: Some(1),
            reference_policy: vulcan_provider_adapters::ResourceReferencePolicy::Any,
            upload_channel_refs: vec!["native-test-files".into()],
        }];
    // Capture original account and route before execution consumes the selection.
    // 执行消费选择之前捕获原账号及路线。
    let identity = crate::agent_runtime::provider::provider_route_lease_identity(&selection);
    let execution = Arc::new(
        runtime
            .resolve_execution_from_frozen_selection(selection)
            .await
            .unwrap(),
    );
    let contract = execution
        .client
        .conversation_media_upload_contract(&execution.route, ResourceKind::Image, "image/png")
        .unwrap()
        .unwrap();
    let context = ProviderRequestMediaContext {
        execution,
        session_scope_id: "original-session".into(),
    };
    // Seed a previously completed receipt rather than performing an actual upload.
    // 写入一份此前已完成回执，不执行实际上传。
    let envelope = NativeMediaEnvelope {
        version: ENVELOPE_VERSION,
        text: "Read image".into(),
        byte_len: 3,
        resource: vulcan_ai_contract::ResourceReference {
            id: "file-native-original".into(),
            kind: ResourceKind::Image,
            media_type: Some("image/png".into()),
            source_kind: vulcan_ai_contract::ResourceSourceKind::ProviderFileId,
            file_name: Some("image.png".into()),
            image_detail: None,
            resource_path: Some("image.png".into()),
            local_path: None,
            url: None,
        },
        upload: Some(NativeUploadOrigin {
            route: identity,
            session_id: "original-session".into(),
            contract_fingerprint: contract.contract_fingerprint,
            expires_at_unix_millis: u64::MAX,
        }),
    };
    for case in [
        "valid",
        "expired",
        "other-model",
        "other-session",
        "other-contract",
    ] {
        // Each case changes only one original replay fact.
        // 每个用例仅改变一项原重放事实。
        let mut result = envelope.clone();
        let origin = result.upload.as_mut().unwrap();
        match case {
            "expired" => origin.expires_at_unix_millis = 0,
            "other-model" => origin.route.model_id = "another-model".into(),
            "other-session" => origin.session_id = "another-session".into(),
            "other-contract" => origin.contract_fingerprint = "another-contract".into(),
            "valid" => {}
            _ => unreachable!(),
        }
        let mut messages = vec![result_message(
            "uploaded",
            json!(serde_json::to_string(&result).unwrap()),
        )];
        project_native_media_messages(&mut messages, Some(&context)).unwrap();
        assert_eq!(
            messages
                .iter()
                .flat_map(|message| &message.content)
                .any(|item| matches!(item, ContentItem::Resource { .. })),
            case == "valid",
            "{case}"
        );
        if case != "valid" {
            assert!(
                !serde_json::to_string(&messages)
                    .unwrap()
                    .contains("file-native-original")
            );
        }
    }
}

/// Rejects wrong MIME/count declarations and inline limits before returning model-visible binary data.
/// 在返回模型可见二进制数据前拒绝错误MIME、数量声明及内联限制。
#[tokio::test]
async fn native_media_reader_enforces_frozen_constraints_and_limits() {
    // The same actual file is admitted or rejected solely by the frozen target contract.
    // 相同实际文件的准入或拒绝仅由冻结目标契约决定。
    let (runtime, _, mut selection) = fixture(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("image.png");
    std::fs::write(&path, b"abc").unwrap();
    selection
        .media_delivery_policy
        .conversation
        .image
        .without_upload_max_bytes = 2;
    assert!(
        tool::read_media(
            runtime,
            selection.clone(),
            None,
            None,
            "s",
            json!({"path":path})
        )
        .await
        .unwrap_err()
        .contains("inline size limit")
    );
    selection.route.protocol_route.input_resource_constraints =
        vec![vulcan_provider_adapters::InputResourceConstraint {
            resource_kind: ResourceKind::Image,
            accepted_mime_types: vec!["image/jpeg".into()],
            max_bytes: None,
            max_count: None,
            reference_policy: vulcan_provider_adapters::ResourceReferencePolicy::Any,
            upload_channel_refs: vec![],
        }];
    assert!(!supports_mime(&selection, ResourceKind::Image, "image/png"));
    assert!(supports_mime(&selection, ResourceKind::Image, "image/jpeg"));
    selection.route.protocol_route.input_resource_constraints[0].max_count = Some(0);
    assert!(!supports_mime(
        &selection,
        ResourceKind::Image,
        "image/jpeg"
    ));
}
