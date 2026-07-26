//! Explicit AI-tool host-binding candidate registration helpers.
//! 显式 AI 工具宿主绑定候选注册辅助逻辑。

use super::*;

/// Appends host-owned backing candidates for configured API-native compatibility tools.
/// 为已配置的 API 原生兼容工具追加宿主侧承载候选项。
pub(super) fn append_configured_ai_tool_backing_candidates(
    candidates: &mut Vec<VulcanToolCandidate>,
    configuration: PersistedAiToolConfiguration,
    workspace_root: Option<PathBuf>,
    conversation_attachments: Vec<ConversationAttachment>,
) {
    if let Some(candidate) = configured_web_search_candidate(configuration.clone()) {
        candidates.push(candidate);
    }
    if let Some(candidate) = configured_understand_image_candidate(
        configuration.clone(),
        conversation_attachments.clone(),
    ) {
        candidates.push(candidate);
    }
    if let Some(candidate) = configured_image_generation_candidate(
        configuration.clone(),
        workspace_root.clone(),
        conversation_attachments.clone(),
    ) {
        candidates.push(candidate);
    }
    if let Some(candidate) = configured_video_generation_candidate(
        configuration.clone(),
        workspace_root.clone(),
        conversation_attachments.clone(),
    ) {
        candidates.push(candidate);
    }
    if let Some(candidate) =
        configured_speech_synthesis_candidate(configuration.clone(), workspace_root.clone())
    {
        candidates.push(candidate);
    }
    if let Some(candidate) =
        configured_music_generation_candidate(configuration.clone(), workspace_root.clone())
    {
        candidates.push(candidate);
    }
    if let Some(candidate) =
        configured_music_cover_candidate(configuration, workspace_root, conversation_attachments)
    {
        candidates.push(candidate);
    }
}

/// Builds one AI-facing description for a generated-media host tool without exposing its frozen provider target.
/// 构建一条不暴露已冻结供应商目标的生成媒体宿主工具 AI 描述。
fn generated_media_tool_description(base_description: &str) -> String {
    format!("{base_description}{GENERATED_MEDIA_OUTPUT_DESCRIPTION_SUFFIX}")
}

/// Returns whether one capability has exactly one host-bound target and is safe to register.
/// 返回某项能力是否恰好绑定一个宿主目标并可安全注册。
fn has_exact_bound_target(
    configuration: &PersistedAiToolConfiguration,
    capability_kind: AiToolKind,
) -> bool {
    configuration.usable_targets_for_kind(capability_kind).len() == 1
}

/// Builds the hidden backing tool used by the semantic `web_search` fallback alias.
/// 构建语义 `web_search` 回退别名使用的隐藏承载工具。
fn configured_web_search_candidate(
    configuration: PersistedAiToolConfiguration,
) -> Option<VulcanToolCandidate> {
    if !has_exact_bound_target(&configuration, AiToolKind::WebSearch) {
        return None;
    }

    Some(VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: WEB_SEARCH_HOST_BINDING_TOOL_ID.to_string(),
            name: WEB_SEARCH_HOST_BINDING_TOOL_ID.to_string(),
            description: String::from(
                "Execute web search through the one external search provider selected in the host configuration.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: configured_web_search_parameters_schema(&configuration),
        ai_visible: false,
        plan_policy: VulcanPlanToolPolicy::HeuristicText(String::from(
            "web search browse internet search news docs",
        )),
        request_streaming: None,
        invoke: std::sync::Arc::new(move |args, _session_id| {
            let configuration = configuration.clone();
            Box::pin(async move {
                execute_configured_web_search(args, configuration)
                    .await
                    .map(VulcanToolInvocationResult::from_model_output)
            })
        }),
    })
}

/// Builds the hidden backing tool used by the semantic `understand_image` fallback alias.
/// 构建语义 `understand_image` 回退别名使用的隐藏承载工具。
pub(super) fn configured_understand_image_candidate(
    configuration: PersistedAiToolConfiguration,
    conversation_attachments: Vec<ConversationAttachment>,
) -> Option<VulcanToolCandidate> {
    if !has_exact_bound_target(&configuration, AiToolKind::UnderstandImage) {
        return None;
    }

    let image_attachments = conversation_attachments
        .into_iter()
        .filter(|attachment| attachment.kind == ConversationAttachmentKind::Image)
        .collect::<Vec<_>>();

    Some(VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: UNDERSTAND_IMAGE_HOST_BINDING_TOOL_ID.to_string(),
            name: UNDERSTAND_IMAGE_HOST_BINDING_TOOL_ID.to_string(),
            description: String::from(
                "Read image attachments from the current session through the one image-understanding provider selected in the host configuration. Use a resource id for pasted or uploaded attachments, or a full local path for path-based images.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: configured_understand_image_parameters_schema(
            &configuration,
            &image_attachments,
        ),
        ai_visible: false,
        plan_policy: VulcanPlanToolPolicy::HeuristicText(String::from(
            "understand image inspect screenshot analyze visual content OCR",
        )),
        request_streaming: None,
        invoke: std::sync::Arc::new(move |args, _session_id| {
            let configuration = configuration.clone();
            let conversation_attachments = image_attachments.clone();
            Box::pin(async move {
                execute_configured_understand_image(args, configuration, conversation_attachments)
                    .await
                    .map(VulcanToolInvocationResult::from_model_output)
            })
        }),
    })
}

/// Builds the hidden backing tool used by the semantic `image_generation` fallback alias.
/// 构建语义 `image_generation` 回退别名使用的隐藏承载工具。
fn configured_image_generation_candidate(
    configuration: PersistedAiToolConfiguration,
    workspace_root: Option<PathBuf>,
    conversation_attachments: Vec<ConversationAttachment>,
) -> Option<VulcanToolCandidate> {
    if !has_exact_bound_target(&configuration, AiToolKind::ImageGeneration) {
        return None;
    }
    let workspace_path = vulcan_runtime_assets::native_host_path_text_from_path(&workspace_root?);

    Some(VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: IMAGE_GENERATION_HOST_BINDING_TOOL_ID.to_string(),
            name: IMAGE_GENERATION_HOST_BINDING_TOOL_ID.to_string(),
            description: generated_media_tool_description(
                "Generate images through the one image-generation provider selected in the host configuration.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: configured_image_generation_parameters_schema(
            &configuration,
            &conversation_attachments,
        ),
        ai_visible: false,
        plan_policy: VulcanPlanToolPolicy::HeuristicText(String::from(
            "generate image create illustration render picture",
        )),
        request_streaming: None,
        invoke: std::sync::Arc::new(move |args, _session_id| {
            let configuration = configuration.clone();
            let workspace_path = workspace_path.clone();
            let conversation_attachments = conversation_attachments.clone();
            Box::pin(async move {
                execute_configured_image_generation(
                    args,
                    configuration,
                    workspace_path,
                    conversation_attachments,
                )
                .await
            })
        }),
    })
}

/// Builds the hidden backing tool used by the semantic `video_generation` fallback alias.
/// 构建语义 `video_generation` 回退别名使用的隐藏承载工具。
fn configured_video_generation_candidate(
    configuration: PersistedAiToolConfiguration,
    workspace_root: Option<PathBuf>,
    conversation_attachments: Vec<ConversationAttachment>,
) -> Option<VulcanToolCandidate> {
    if !has_exact_bound_target(&configuration, AiToolKind::VideoGeneration) {
        return None;
    }
    let workspace_path = vulcan_runtime_assets::native_host_path_text_from_path(&workspace_root?);

    Some(VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: VIDEO_GENERATION_HOST_BINDING_TOOL_ID.to_string(),
            name: VIDEO_GENERATION_HOST_BINDING_TOOL_ID.to_string(),
            description: generated_media_tool_description(
                "Generate videos through the one video-generation provider selected in the host configuration.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: configured_video_generation_parameters_schema(
            &configuration,
            &conversation_attachments,
        ),
        ai_visible: false,
        plan_policy: VulcanPlanToolPolicy::HeuristicText(String::from(
            "generate video animate clip movie image to video",
        )),
        request_streaming: None,
        invoke: std::sync::Arc::new(move |args, _session_id| {
            let configuration = configuration.clone();
            let workspace_path = workspace_path.clone();
            let conversation_attachments = conversation_attachments.clone();
            Box::pin(async move {
                execute_configured_video_generation(
                    args,
                    configuration,
                    workspace_path,
                    conversation_attachments,
                )
                .await
            })
        }),
    })
}

/// Builds the hidden backing tool used by the semantic `speech_synthesis` fallback alias.
/// 构建语义 `speech_synthesis` 回退别名使用的隐藏承载工具。
fn configured_speech_synthesis_candidate(
    configuration: PersistedAiToolConfiguration,
    workspace_root: Option<PathBuf>,
) -> Option<VulcanToolCandidate> {
    if !has_exact_bound_target(&configuration, AiToolKind::SpeechSynthesis) {
        return None;
    }
    let workspace_path = vulcan_runtime_assets::native_host_path_text_from_path(&workspace_root?);

    Some(VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: SPEECH_SYNTHESIS_HOST_BINDING_TOOL_ID.to_string(),
            name: SPEECH_SYNTHESIS_HOST_BINDING_TOOL_ID.to_string(),
            description: generated_media_tool_description(
                "Synthesize speech through the one text-to-speech provider selected in the host configuration.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: configured_speech_synthesis_parameters_schema(&configuration),
        ai_visible: false,
        plan_policy: VulcanPlanToolPolicy::HeuristicText(String::from(
            "speech synthesize text to speech voice audio narration",
        )),
        request_streaming: None,
        invoke: std::sync::Arc::new(move |args, _session_id| {
            let configuration = configuration.clone();
            let workspace_path = workspace_path.clone();
            Box::pin(async move {
                execute_configured_speech_synthesis(args, configuration, workspace_path).await
            })
        }),
    })
}

/// Builds the hidden backing tool used by the semantic `music_generation` fallback alias.
/// 构建语义 `music_generation` 回退别名使用的隐藏承载工具。
fn configured_music_generation_candidate(
    configuration: PersistedAiToolConfiguration,
    workspace_root: Option<PathBuf>,
) -> Option<VulcanToolCandidate> {
    if !has_exact_bound_target(&configuration, AiToolKind::MusicGeneration) {
        return None;
    }
    let workspace_path = vulcan_runtime_assets::native_host_path_text_from_path(&workspace_root?);

    Some(VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: MUSIC_GENERATION_HOST_BINDING_TOOL_ID.to_string(),
            name: MUSIC_GENERATION_HOST_BINDING_TOOL_ID.to_string(),
            description: generated_media_tool_description(
                "Generate songs or instrumental music through the one music-generation provider selected in the host configuration.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: configured_music_generation_parameters_schema(&configuration),
        ai_visible: false,
        plan_policy: VulcanPlanToolPolicy::HeuristicText(String::from(
            "generate music song bgm lyrics instrumental",
        )),
        request_streaming: None,
        invoke: std::sync::Arc::new(move |args, _session_id| {
            let configuration = configuration.clone();
            let workspace_path = workspace_path.clone();
            Box::pin(async move {
                execute_configured_music_generation(args, configuration, workspace_path).await
            })
        }),
    })
}

/// Builds the hidden backing tool used by the semantic `music_cover` fallback alias.
/// 构建语义 `music_cover` 回退别名使用的隐藏承载工具。
fn configured_music_cover_candidate(
    configuration: PersistedAiToolConfiguration,
    workspace_root: Option<PathBuf>,
    conversation_attachments: Vec<ConversationAttachment>,
) -> Option<VulcanToolCandidate> {
    if !has_exact_bound_target(&configuration, AiToolKind::MusicCover) {
        return None;
    }
    let workspace_path = vulcan_runtime_assets::native_host_path_text_from_path(&workspace_root?);
    let audio_attachments = conversation_attachments
        .into_iter()
        .filter(|attachment| attachment.kind == ConversationAttachmentKind::Audio)
        .collect::<Vec<_>>();
    Some(VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: MUSIC_COVER_HOST_BINDING_TOOL_ID.to_string(),
            name: MUSIC_COVER_HOST_BINDING_TOOL_ID.to_string(),
            description: generated_media_tool_description(
                "Create a music cover from reference audio through the one music-cover provider selected in the host configuration.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: configured_music_cover_parameters_schema(
            &configuration,
            &audio_attachments,
        ),
        ai_visible: false,
        plan_policy: VulcanPlanToolPolicy::HeuristicText(String::from(
            "music cover remix rearrange reference audio song",
        )),
        request_streaming: None,
        invoke: std::sync::Arc::new(move |args, _session_id| {
            let configuration = configuration.clone();
            let workspace_path = workspace_path.clone();
            let conversation_attachments = audio_attachments.clone();
            Box::pin(async move {
                execute_configured_music_cover(
                    args,
                    configuration,
                    workspace_path,
                    conversation_attachments,
                )
                .await
            })
        }),
    })
}

/// Appends semantic AI host bindings and removes their concrete backing tool from direct model exposure.
/// 追加语义 AI 宿主绑定，并移除其具体承载工具的直接暴露，避免模型同时看到两套等价工具。
pub(super) fn append_ai_tool_host_binding_aliases(
    candidates: &mut Vec<VulcanToolCandidate>,
    aliases: Vec<AiToolHostBindingAlias>,
) {
    for alias in aliases {
        let host_tool_id = alias.host_tool_id.trim();
        if host_tool_id.is_empty() {
            continue;
        }

        let Some(target_index) = candidates
            .iter()
            .position(|candidate| candidate_matches_ai_host_binding_id(candidate, host_tool_id))
        else {
            continue;
        };
        let target = candidates.remove(target_index);
        candidates.push(build_ai_tool_host_binding_candidate(alias.kind, target));
    }
}

/// Builds one semantic AI candidate bound to an already registered host tool candidate.
/// 根据一条已注册宿主工具候选项构建一条语义 AI 绑定候选项。
fn build_ai_tool_host_binding_candidate(
    kind: AiToolKind,
    target: VulcanToolCandidate,
) -> VulcanToolCandidate {
    let semantic_tool_name = kind.key().to_string();
    let backing_tool_name = target.descriptor.name.clone();
    let semantic_description = if target.descriptor.description.trim().is_empty() {
        format!(
            "Semantic tool `{}` is explicitly bound to host tool `{}`.",
            kind.key(),
            backing_tool_name
        )
    } else {
        target.descriptor.description.clone()
    };
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: semantic_tool_name.clone(),
            name: semantic_tool_name.clone(),
            description: semantic_description.clone(),
            help_text: target.descriptor.help_text,
            source: VulcanToolSourceKind::AiHostBinding,
        },
        parameters_schema: target.parameters_schema,
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::HeuristicText(format!(
            "{} {} {}",
            semantic_tool_name, backing_tool_name, semantic_description
        )),
        request_streaming: target.request_streaming,
        invoke: target.invoke,
    }
}

/// Returns whether one host candidate matches the explicitly configured host-tool identifier.
/// 返回单条宿主候选工具是否匹配显式配置的宿主工具标识。
fn candidate_matches_ai_host_binding_id(
    candidate: &VulcanToolCandidate,
    host_tool_id: &str,
) -> bool {
    let host_binding_public_id = build_direct_public_tool_name(host_tool_id);
    let direct_id = build_direct_public_tool_name(&candidate.descriptor.id);
    let direct_name = build_direct_public_tool_name(&candidate.descriptor.name);
    let mcp_id = build_prefixed_public_tool_name("mcp", &[&candidate.descriptor.id]);
    let mcp_name = build_prefixed_public_tool_name("mcp", &[&candidate.descriptor.name]);

    host_tool_id == candidate.descriptor.id
        || host_tool_id == candidate.descriptor.name
        || host_tool_id == direct_id
        || host_tool_id == direct_name
        || host_binding_public_id == direct_id
        || host_binding_public_id == direct_name
        || host_tool_id == mcp_id
        || host_tool_id == mcp_name
        || host_binding_public_id == mcp_id
        || host_binding_public_id == mcp_name
}
