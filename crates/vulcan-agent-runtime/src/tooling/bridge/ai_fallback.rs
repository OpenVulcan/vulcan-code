//! AI-tool fallback backing candidate registration helpers.
//! AI 工具回退后端候选注册辅助逻辑。

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

/// Builds one AI-facing description for a generated-media host tool so semantic aliases can clearly understand export controls and follow-up path reuse.
/// 构建一条面向 AI 的生成媒体宿主工具描述，使语义别名能够清楚理解导出控制项与后续路径复用方式。
fn generated_media_tool_description(base_description: &str, target_summary: &str) -> String {
    format!(
        "{base_description} Available `target` values: {target_summary}.{GENERATED_MEDIA_OUTPUT_DESCRIPTION_SUFFIX}"
    )
}

/// Builds the hidden backing tool used by the semantic `web_search` fallback alias.
/// 构建语义 `web_search` 回退别名使用的隐藏承载工具。
fn configured_web_search_candidate(
    configuration: PersistedAiToolConfiguration,
) -> Option<VulcanToolCandidate> {
    if !configuration.has_usable_targets_for_kind(AiToolKind::WebSearch) {
        return None;
    }

    let target_summary = std::iter::once(String::from("auto"))
        .chain(configuration.usable_targets_for_kind(AiToolKind::WebSearch))
        .collect::<Vec<_>>()
        .join(", ");

    Some(VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: WEB_SEARCH_PROVIDER_CHAIN_TOOL_ID.to_string(),
            name: WEB_SEARCH_PROVIDER_CHAIN_TOOL_ID.to_string(),
            description: format!(
                "Execute web search through the user-configured external search target chain. Available `target` values: {}. When `auto` is provided, targets are tried in the current default order.",
                target_summary
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
    if !configuration.has_usable_targets_for_kind(AiToolKind::UnderstandImage) {
        return None;
    }

    let image_attachments = conversation_attachments
        .into_iter()
        .filter(|attachment| attachment.kind == ConversationAttachmentKind::Image)
        .collect::<Vec<_>>();

    let target_summary = std::iter::once(String::from("auto"))
        .chain(configuration.usable_targets_for_kind(AiToolKind::UnderstandImage))
        .collect::<Vec<_>>()
        .join(", ");
    Some(VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: UNDERSTAND_IMAGE_PROVIDER_CHAIN_TOOL_ID.to_string(),
            name: UNDERSTAND_IMAGE_PROVIDER_CHAIN_TOOL_ID.to_string(),
            description: format!(
                "Read image attachments from the current session through the user-configured image-understanding target chain. Available `target` values: {}. Use a resource id for pasted or uploaded attachments, or a full local path for path-based images.",
                target_summary
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
    if !configuration.has_usable_targets_for_kind(AiToolKind::ImageGeneration) {
        return None;
    }
    let workspace_path = vulcan_runtime_assets::native_host_path_text_from_path(&workspace_root?);
    let target_summary = std::iter::once(String::from("auto"))
        .chain(configuration.usable_targets_for_kind(AiToolKind::ImageGeneration))
        .collect::<Vec<_>>()
        .join(", ");

    Some(VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: IMAGE_GENERATION_PROVIDER_CHAIN_TOOL_ID.to_string(),
            name: IMAGE_GENERATION_PROVIDER_CHAIN_TOOL_ID.to_string(),
            description: generated_media_tool_description(
                "Generate images through the user-configured image-generation target chain.",
                &target_summary,
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
    if !configuration.has_usable_targets_for_kind(AiToolKind::VideoGeneration) {
        return None;
    }
    let workspace_path = vulcan_runtime_assets::native_host_path_text_from_path(&workspace_root?);
    let target_summary = std::iter::once(String::from("auto"))
        .chain(configuration.usable_targets_for_kind(AiToolKind::VideoGeneration))
        .collect::<Vec<_>>()
        .join(", ");

    Some(VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: VIDEO_GENERATION_PROVIDER_CHAIN_TOOL_ID.to_string(),
            name: VIDEO_GENERATION_PROVIDER_CHAIN_TOOL_ID.to_string(),
            description: generated_media_tool_description(
                "Generate videos through the user-configured video-generation target chain.",
                &target_summary,
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
    if !configuration.has_usable_targets_for_kind(AiToolKind::SpeechSynthesis) {
        return None;
    }
    let workspace_path = vulcan_runtime_assets::native_host_path_text_from_path(&workspace_root?);
    let target_summary = std::iter::once(String::from("auto"))
        .chain(configuration.usable_targets_for_kind(AiToolKind::SpeechSynthesis))
        .collect::<Vec<_>>()
        .join(", ");

    Some(VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: SPEECH_SYNTHESIS_PROVIDER_CHAIN_TOOL_ID.to_string(),
            name: SPEECH_SYNTHESIS_PROVIDER_CHAIN_TOOL_ID.to_string(),
            description: generated_media_tool_description(
                "Synthesize speech through the user-configured text-to-speech target chain.",
                &target_summary,
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
    if !configuration.has_usable_targets_for_kind(AiToolKind::MusicGeneration) {
        return None;
    }
    let workspace_path = vulcan_runtime_assets::native_host_path_text_from_path(&workspace_root?);
    let target_summary = std::iter::once(String::from("auto"))
        .chain(configuration.usable_targets_for_kind(AiToolKind::MusicGeneration))
        .collect::<Vec<_>>()
        .join(", ");

    Some(VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: MUSIC_GENERATION_PROVIDER_CHAIN_TOOL_ID.to_string(),
            name: MUSIC_GENERATION_PROVIDER_CHAIN_TOOL_ID.to_string(),
            description: generated_media_tool_description(
                "Generate songs or instrumental music through the user-configured music-generation target chain.",
                &target_summary,
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
    if !configuration.has_usable_targets_for_kind(AiToolKind::MusicCover) {
        return None;
    }
    let workspace_path = vulcan_runtime_assets::native_host_path_text_from_path(&workspace_root?);
    let audio_attachments = conversation_attachments
        .into_iter()
        .filter(|attachment| attachment.kind == ConversationAttachmentKind::Audio)
        .collect::<Vec<_>>();
    let target_summary = std::iter::once(String::from("auto"))
        .chain(configuration.usable_targets_for_kind(AiToolKind::MusicCover))
        .collect::<Vec<_>>()
        .join(", ");

    Some(VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: MUSIC_COVER_PROVIDER_CHAIN_TOOL_ID.to_string(),
            name: MUSIC_COVER_PROVIDER_CHAIN_TOOL_ID.to_string(),
            description: generated_media_tool_description(
                "Create a music cover from reference audio through the user-configured music-cover target chain.",
                &target_summary,
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

/// Appends semantic AI fallback aliases and removes their concrete backing tool from direct model exposure.
/// 追加语义 AI 回退别名，并移除其具体承载工具的直接暴露，避免模型同时看到两套等价工具。
pub(super) fn append_ai_tool_fallback_aliases(
    candidates: &mut Vec<VulcanToolCandidate>,
    aliases: Vec<AiToolFallbackAlias>,
) {
    for alias in aliases {
        let fallback_tool_id = alias.fallback_tool_id.trim();
        if fallback_tool_id.is_empty() {
            continue;
        }

        let Some(target_index) = candidates
            .iter()
            .position(|candidate| candidate_matches_ai_fallback_id(candidate, fallback_tool_id))
        else {
            continue;
        };
        let target = candidates.remove(target_index);
        candidates.push(build_ai_tool_fallback_candidate(alias.kind, target));
    }
}

/// Builds one semantic AI fallback candidate backed by an already registered host tool candidate.
/// 根据一条已注册宿主工具候选项构建一条语义 AI 回退候选项。
fn build_ai_tool_fallback_candidate(
    kind: AiToolKind,
    target: VulcanToolCandidate,
) -> VulcanToolCandidate {
    let semantic_tool_name = kind.key().to_string();
    let backing_tool_name = target.descriptor.name.clone();
    let semantic_description = if target.descriptor.description.trim().is_empty() {
        format!(
            "Semantic tool `{}` falls back to host tool `{}`.",
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
            source: VulcanToolSourceKind::AiFallback,
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

/// Returns whether one host candidate can satisfy the configured fallback tool identifier.
/// 返回单条宿主候选工具是否可以匹配配置中的回退工具标识。
fn candidate_matches_ai_fallback_id(
    candidate: &VulcanToolCandidate,
    fallback_tool_id: &str,
) -> bool {
    let fallback_public_id = build_direct_public_tool_name(fallback_tool_id);
    let direct_id = build_direct_public_tool_name(&candidate.descriptor.id);
    let direct_name = build_direct_public_tool_name(&candidate.descriptor.name);
    let mcp_id = build_prefixed_public_tool_name("mcp", &[&candidate.descriptor.id]);
    let mcp_name = build_prefixed_public_tool_name("mcp", &[&candidate.descriptor.name]);

    fallback_tool_id == candidate.descriptor.id
        || fallback_tool_id == candidate.descriptor.name
        || fallback_tool_id == direct_id
        || fallback_tool_id == direct_name
        || fallback_public_id == direct_id
        || fallback_public_id == direct_name
        || fallback_tool_id == mcp_id
        || fallback_tool_id == mcp_name
        || fallback_public_id == mcp_id
        || fallback_public_id == mcp_name
}
