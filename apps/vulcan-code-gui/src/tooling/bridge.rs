//! GUI-side bridge that adapts native, LuaSkills, and MCP runtime sources into the standalone VulcanToolsManager crate.
//! 将原生工具、LuaSkills 与 MCP 运行时来源适配到独立 VulcanToolsManager crate 的 GUI 侧桥接层。

use crate::agent_runtime::ai_tool_router::{AiToolRouteKind, resolve_ai_tool_plan};
use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
use crate::agent_runtime::question_runtime::{QuestionRuntime, build_question_tool_candidate};
use crate::agent_runtime::task_runtime::{TaskRuntime, build_task_tool_candidate};
use crate::image_understanding_compat::{
    configured_understand_image_parameters_schema, execute_configured_understand_image,
};
use crate::luaskills_runtime::{
    LUASKILL_SID_PARAMETER_NAME, LUASKILLS_HOST_MANAGED_SID_PREFIX, LuaSkillsRuntimeHandle,
    PWD_PARAMETER_NAME, build_luaskills_tool_invocation_context, call_luaskills_tool_entry,
    luaskills_entry_args_schema, render_luaskills_invocation_result_text,
};
use crate::mcp::McpRuntimeToolBinding;
use crate::media_generation_compat::{
    configured_image_generation_parameters_schema, configured_music_cover_parameters_schema,
    configured_music_generation_parameters_schema, configured_speech_synthesis_parameters_schema,
    configured_video_generation_parameters_schema, execute_configured_image_generation,
    execute_configured_music_cover, execute_configured_music_generation,
    execute_configured_speech_synthesis, execute_configured_video_generation,
};
use crate::project_temp_maintenance;
use crate::provider_config::{
    AiToolKind, IMAGE_GENERATION_PROVIDER_CHAIN_TOOL_ID, MUSIC_COVER_PROVIDER_CHAIN_TOOL_ID,
    MUSIC_GENERATION_PROVIDER_CHAIN_TOOL_ID, PersistedAiToolConfiguration,
    SPEECH_SYNTHESIS_PROVIDER_CHAIN_TOOL_ID, UNDERSTAND_IMAGE_PROVIDER_CHAIN_TOOL_ID,
    VIDEO_GENERATION_PROVIDER_CHAIN_TOOL_ID, WEB_SEARCH_PROVIDER_CHAIN_TOOL_ID,
};
use crate::tooling::{
    NativeToolCatalog, NativeToolDescriptor, NativeToolExecutionRequest,
    allocate_unique_public_tool_name, build_direct_public_tool_name,
    build_prefixed_public_tool_name,
};
use crate::web_search_compat::{
    configured_web_search_parameters_schema, execute_configured_web_search,
};
use crate::workspace_path::native_workspace_path_text_from_path;
use luaskills::RuntimeEntryDescriptor;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
use vulcan_luaskills_prompt::{
    LUASKILL_HELP_TOOL_NAME, LuaSkillPromptEntry, render_luaskill_help_output,
};
use vulcan_tools_luaskills_call::{
    CALL_LUASKILL_TOOL_NAME, LuaSkillCallEntryView, call_luaskill_args_schema,
    parse_call_luaskill_request, resolve_call_luaskill_entry_by_name,
    restore_call_luaskill_args_for_entry,
};
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolHostResult,
    VulcanToolInvocationResult, VulcanToolSourceKind, VulcanToolsManager,
};

/// Shared description suffix that explains how generated-media tools expose export controls and follow-up file paths to the model.
/// 生成媒体工具共享的描述后缀，用于向模型说明导出控制项以及可复用的后续文件路径。
const GENERATED_MEDIA_OUTPUT_DESCRIPTION_SUFFIX: &str = " Optional `output_dir` exports a caller-chosen copy. Optional `output_file_stem` suggests the file name without extension. Generated results return `outputs[*].local_path` for follow-up tool calls.";

/// One semantic AI-tool fallback alias requested by the provider/model router.
/// Provider/Model 路由器请求的一条语义 AI 工具回退别名。
#[derive(Debug, Clone)]
pub struct AiToolFallbackAlias {
    /// Stable semantic AI-specialized tool kind exposed to the model.
    /// 暴露给模型的稳定 AI 专用语义工具类型。
    pub kind: AiToolKind,
    /// Concrete host tool identifier that should back the semantic alias.
    /// 承载该语义别名的具体宿主工具标识。
    pub fallback_tool_id: String,
}

/// Resolves semantic fallback aliases from one provider/model blueprint.
/// 从单个 Provider/Model 蓝图解析语义回退别名。
pub fn resolve_ai_tool_fallback_aliases(
    blueprint: &ProviderRuntimeBlueprint,
) -> Vec<AiToolFallbackAlias> {
    resolve_ai_tool_plan(blueprint)
        .decisions
        .into_iter()
        .filter(|decision| decision.route == AiToolRouteKind::HostFallback)
        .filter_map(|decision| {
            decision
                .fallback_tool_id
                .map(|fallback_tool_id| AiToolFallbackAlias {
                    kind: decision.kind,
                    fallback_tool_id,
                })
        })
        .collect()
}

/// Builds one standalone host-owned tools manager from the current GUI runtime surfaces.
/// 根据当前 GUI 运行时工具面构建独立宿主自有工具管理器。
pub fn build_vulcan_tools_manager(
    task_runtime: Arc<TaskRuntime>,
    question_runtime: Arc<QuestionRuntime>,
    native_catalog: NativeToolCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    workspace_root: Option<PathBuf>,
    conversation_attachments: Vec<ConversationAttachment>,
    candidate_native_tool_ids: Vec<String>,
    candidate_luaskills_tools: Vec<RuntimeEntryDescriptor>,
    candidate_luaskills_dispatch_tools: Vec<RuntimeEntryDescriptor>,
    candidate_mcp_tools: Vec<McpRuntimeToolBinding>,
    ai_tool_configuration: PersistedAiToolConfiguration,
    ai_tool_fallback_aliases: Vec<AiToolFallbackAlias>,
    enabled_prompt_skill_names: Vec<String>,
    prompt_skill_user_root: Option<PathBuf>,
    implicit_luaskill_prompt_entries: Vec<LuaSkillPromptEntry>,
) -> VulcanToolsManager {
    let mut candidates = Vec::new();
    let mut occupied_public_tool_ids = BTreeSet::new();

    let task_candidate = build_task_tool_candidate(task_runtime);
    occupied_public_tool_ids.insert(task_candidate.descriptor.id.clone());
    candidates.push(task_candidate);
    let ask_candidate = build_question_tool_candidate(question_runtime, "ask");
    occupied_public_tool_ids.insert(ask_candidate.descriptor.id.clone());
    candidates.push(ask_candidate);

    for tool_id in candidate_native_tool_ids {
        let Some(definition) = native_catalog.definition(&tool_id) else {
            continue;
        };
        let descriptor = native_descriptor_to_vulcan(&definition.descriptor);
        occupied_public_tool_ids.insert(descriptor.id.clone());
        let catalog = native_catalog.clone();
        let tool_id = definition.descriptor.id.clone();
        let workspace_root_for_context = workspace_root.clone();
        let enabled_prompt_skill_names_for_context = enabled_prompt_skill_names.clone();
        let prompt_skill_user_root_for_context = prompt_skill_user_root.clone();
        candidates.push(VulcanToolCandidate {
            descriptor,
            parameters_schema: definition.args_schema.clone(),
            ai_visible: definition.descriptor.ai_visible,
            plan_policy: native_plan_policy(&definition.descriptor),
            invoke: std::sync::Arc::new(move |args, session_id| {
                let catalog = catalog.clone();
                let tool_id = tool_id.clone();
                let workspace_root = workspace_root_for_context.clone();
                let prompt_skill_user_root = prompt_skill_user_root_for_context.clone();
                let enabled_prompt_skill_names = enabled_prompt_skill_names_for_context.clone();
                Box::pin(async move {
                    catalog
                        .invoke(
                            &tool_id,
                            NativeToolExecutionRequest {
                                arguments: args,
                                session_id: session_id
                                    .as_deref()
                                    .and_then(|value| Uuid::parse_str(value).ok()),
                                spill_root: workspace_root.as_deref().and_then(|path| {
                                    project_temp_maintenance::ensure_project_temp_dir(path).ok()
                                }),
                                workspace_root,
                                enabled_prompt_skill_names,
                                prompt_skill_user_root,
                            },
                        )
                        .await
                        .map_err(|error| error.to_string())
                })
            }),
        });
    }

    if !candidate_luaskills_dispatch_tools.is_empty() {
        let luaskill_help_candidate =
            build_luaskills_help_candidate(implicit_luaskill_prompt_entries);
        occupied_public_tool_ids.insert(luaskill_help_candidate.descriptor.id.clone());
        candidates.push(luaskill_help_candidate);
        let dispatcher_candidate = build_luaskills_dispatch_candidate(
            luaskills_runtime.clone(),
            workspace_root.clone(),
            candidate_luaskills_dispatch_tools,
        );
        occupied_public_tool_ids.insert(dispatcher_candidate.descriptor.id.clone());
        candidates.push(dispatcher_candidate);
    }

    let mut candidate_luaskills_tools = candidate_luaskills_tools;
    candidate_luaskills_tools.sort_by(|left, right| left.canonical_name.cmp(&right.canonical_name));

    for entry in candidate_luaskills_tools {
        let descriptor = luaskills_descriptor_to_vulcan(&entry, &mut occupied_public_tool_ids);
        let host_managed_pwd_available = workspace_root
            .as_deref()
            .is_some_and(|path| !path.as_os_str().is_empty());
        let parameters_schema =
            luaskills_entry_args_schema(&entry, true, host_managed_pwd_available);
        let canonical_name = entry.canonical_name.clone();
        let skill_id = entry.skill_id.clone();
        let entry_declares_luaskill_sid = entry_declares_luaskill_sid(&entry);
        let entry_declares_pwd = entry_declares_pwd(&entry);
        let runtime = luaskills_runtime.clone();
        let workspace_root = workspace_root.clone();
        candidates.push(VulcanToolCandidate {
            descriptor,
            parameters_schema,
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::HeuristicText(canonical_name.clone()),
            invoke: std::sync::Arc::new(move |args, session_id| {
                let runtime = runtime.clone();
                let canonical_name = canonical_name.clone();
                let skill_id = skill_id.clone();
                let workspace_root = workspace_root.clone();
                Box::pin(async move {
                    let Some(handle) = runtime else {
                        return Err(format!(
                            "The LuaSkills runtime is currently unavailable, so tool `{}` cannot be invoked.",
                            canonical_name
                        ));
                    };
                    let args = inject_host_managed_luaskill_sid_if_needed(
                        args,
                        entry_declares_luaskill_sid,
                        session_id.as_deref(),
                        &canonical_name,
                    )?;
                    let args = inject_host_managed_pwd_if_needed(
                        args,
                        entry_declares_pwd,
                        workspace_root.as_deref(),
                        &canonical_name,
                    )?;
                    let parsed_session_id = session_id
                        .as_deref()
                        .and_then(|value| Uuid::parse_str(value).ok());
                    let invocation_context = build_luaskills_tool_invocation_context(
                        parsed_session_id,
                        "host-tool",
                        &canonical_name,
                    );
                    let call_handle = handle.clone();
                    let result = call_luaskills_tool_entry(
                        call_handle,
                        canonical_name.clone(),
                        args,
                        Some(invocation_context),
                    )
                    .await
                    .map_err(|error| {
                        format!("LuaSkills tool `{}` failed: {}.", canonical_name, error)
                    })?;
                    let model_output = Value::String(render_luaskills_invocation_result_text(
                        &handle,
                        &canonical_name,
                        Some(&skill_id),
                        &result,
                        workspace_root.as_deref(),
                    ));
                    let invocation_result = if let Some(host_result) = result.host_result.as_ref() {
                        VulcanToolInvocationResult::from_model_output(model_output)
                            .with_host_result(VulcanToolHostResult {
                                kind: host_result.kind.clone(),
                                payload: host_result.payload.clone(),
                            })
                    } else {
                        VulcanToolInvocationResult::from_model_output(model_output)
                    };
                    Ok(invocation_result)
                })
            }),
        });
    }

    let mut candidate_mcp_tools = candidate_mcp_tools;
    candidate_mcp_tools.sort_by(|left, right| left.id().cmp(right.id()));

    for tool in candidate_mcp_tools {
        let descriptor = mcp_tool_binding_to_vulcan(&tool);
        occupied_public_tool_ids.insert(descriptor.id.clone());
        let parameters_schema = tool.args_schema().clone();
        let tool_name = tool.name().to_string();
        candidates.push(VulcanToolCandidate {
            descriptor,
            parameters_schema,
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::HeuristicText(tool_name.clone()),
            invoke: std::sync::Arc::new(move |args, _session_id| {
                let tool = tool.clone();
                Box::pin(async move {
                    tool.invoke(args)
                        .await
                        .map(VulcanToolInvocationResult::from_model_output)
                })
            }),
        });
    }

    append_configured_ai_tool_backing_candidates(
        &mut candidates,
        ai_tool_configuration,
        workspace_root,
        conversation_attachments,
    );
    append_ai_tool_fallback_aliases(&mut candidates, ai_tool_fallback_aliases);

    VulcanToolsManager::new(candidates)
}

/// Appends host-owned backing candidates for configured API-native compatibility tools.
/// 为已配置的 API 原生兼容工具追加宿主侧承载候选项。
fn append_configured_ai_tool_backing_candidates(
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
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: configured_web_search_parameters_schema(&configuration),
        ai_visible: false,
        plan_policy: VulcanPlanToolPolicy::HeuristicText(String::from(
            "web search browse internet search news docs",
        )),
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
fn configured_understand_image_candidate(
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
    if image_attachments.is_empty() {
        return None;
    }

    let target_summary = std::iter::once(String::from("auto"))
        .chain(configuration.usable_targets_for_kind(AiToolKind::UnderstandImage))
        .collect::<Vec<_>>()
        .join(", ");
    let image_summary = image_attachments
        .iter()
        .map(|attachment| attachment.file_name.clone())
        .collect::<Vec<_>>()
        .join(", ");

    Some(VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: UNDERSTAND_IMAGE_PROVIDER_CHAIN_TOOL_ID.to_string(),
            name: UNDERSTAND_IMAGE_PROVIDER_CHAIN_TOOL_ID.to_string(),
            description: format!(
                "Read image attachments from the current session through the user-configured image-understanding target chain. Available `target` values: {}. Available images: {}.",
                target_summary, image_summary
            ),
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
    let workspace_path = workspace_root?.to_string_lossy().into_owned();
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
    let workspace_path = workspace_root?.to_string_lossy().into_owned();
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
    let workspace_path = workspace_root?.to_string_lossy().into_owned();
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
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: configured_speech_synthesis_parameters_schema(&configuration),
        ai_visible: false,
        plan_policy: VulcanPlanToolPolicy::HeuristicText(String::from(
            "speech synthesize text to speech voice audio narration",
        )),
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
    let workspace_path = workspace_root?.to_string_lossy().into_owned();
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
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: configured_music_generation_parameters_schema(&configuration),
        ai_visible: false,
        plan_policy: VulcanPlanToolPolicy::HeuristicText(String::from(
            "generate music song bgm lyrics instrumental",
        )),
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
    let workspace_path = workspace_root?.to_string_lossy().into_owned();
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

/// Converts one native tool descriptor into the unified host-facing tool descriptor.
/// 将单条原生工具描述转换为统一宿主侧工具描述。
fn native_descriptor_to_vulcan(descriptor: &NativeToolDescriptor) -> VulcanToolDescriptor {
    VulcanToolDescriptor {
        id: descriptor.id.clone(),
        name: descriptor.name.clone(),
        description: descriptor.description.clone(),
        source: VulcanToolSourceKind::Native,
    }
}

/// Converts one LuaSkills runtime entry into the unified host-facing tool descriptor.
/// 将单条 LuaSkills 运行时入口转换为统一宿主侧工具描述。
fn luaskills_descriptor_to_vulcan(
    entry: &RuntimeEntryDescriptor,
    occupied_public_tool_ids: &mut BTreeSet<String>,
) -> VulcanToolDescriptor {
    let preferred_public_id = build_direct_public_tool_name(&entry.canonical_name);
    let public_id = if occupied_public_tool_ids.contains(&preferred_public_id) {
        allocate_unique_public_tool_name(
            &build_prefixed_public_tool_name("ls", &[&entry.canonical_name]),
            occupied_public_tool_ids,
        )
    } else {
        allocate_unique_public_tool_name(&preferred_public_id, occupied_public_tool_ids)
    };

    VulcanToolDescriptor {
        id: public_id.clone(),
        name: public_id,
        description: entry.description.clone(),
        source: VulcanToolSourceKind::LuaSkills,
    }
}

/// Builds the AI-visible LuaSkill help tool paired with the implicit dispatcher.
/// 构建与隐式调度器配对的 AI 可见 LuaSkill 帮助工具。
fn build_luaskills_help_candidate(entries: Vec<LuaSkillPromptEntry>) -> VulcanToolCandidate {
    let entries = Arc::new(entries);
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from(LUASKILL_HELP_TOOL_NAME),
            name: String::from(LUASKILL_HELP_TOOL_NAME),
            description: String::from(
                "Return full prompt help and structured Markdown argument schema help for one enabled implicit LuaSkills entry by complete canonical name.",
            ),
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: luaskill_help_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::HeuristicText(String::from(
            "luaskill implicit LuaSkills help and Markdown schema",
        )),
        invoke: Arc::new(move |args, _session_id| {
            let entries = Arc::clone(&entries);
            Box::pin(async move { execute_luaskills_help_tool(entries, args).await })
        }),
    }
}

/// Builds the AI-visible LuaSkills dispatcher used for host-side implicit entry calls.
/// 构建 AI 可见的 LuaSkills 调度器，用于宿主侧隐式入口调用。
fn build_luaskills_dispatch_candidate(
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    workspace_root: Option<PathBuf>,
    mut dispatch_entries: Vec<RuntimeEntryDescriptor>,
) -> VulcanToolCandidate {
    dispatch_entries.sort_by(|left, right| {
        left.skill_id
            .cmp(&right.skill_id)
            .then_with(|| left.local_name.cmp(&right.local_name))
            .then_with(|| left.canonical_name.cmp(&right.canonical_name))
    });
    let dispatch_entries = Arc::new(dispatch_entries);
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from(CALL_LUASKILL_TOOL_NAME),
            name: String::from(CALL_LUASKILL_TOOL_NAME),
            description: String::from(
                "Call one enabled LuaSkills entry by its complete canonical tool name with compact argument pairs. Use this dispatcher for implicit LuaSkills entries that are not registered as direct function tools.",
            ),
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: call_luaskill_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::HeuristicText(String::from(
            "call_luaskill LuaSkills implicit entry dispatcher",
        )),
        invoke: Arc::new(move |args, session_id| {
            let runtime = luaskills_runtime.clone();
            let workspace_root = workspace_root.clone();
            let dispatch_entries = Arc::clone(&dispatch_entries);
            Box::pin(async move {
                execute_luaskills_dispatch_tool(
                    runtime,
                    workspace_root,
                    dispatch_entries,
                    args,
                    session_id,
                )
                .await
            })
        }),
    }
}

/// Builds the compact JSON schema accepted by the `luaskill` help tool.
/// 构建 `luaskill` 帮助工具接受的紧凑 JSON Schema。
fn luaskill_help_args_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "name": {
                "type": "string",
                "description": "Complete canonical LuaSkills tool name, such as vulcan-codekit-ast-tree."
            }
        },
        "required": ["name"]
    })
}

/// Executes the LuaSkill help lookup tool from one implicit dispatch snapshot.
/// 基于单份隐式调度快照执行 LuaSkill 帮助查询工具。
async fn execute_luaskills_help_tool(
    entries: Arc<Vec<LuaSkillPromptEntry>>,
    args: Value,
) -> Result<VulcanToolInvocationResult, String> {
    let Value::Object(mut object) = args else {
        return Err(String::from("`luaskill` arguments must be a JSON object."));
    };
    let name = object
        .remove("name")
        .and_then(|value| value.as_str().map(str::trim).map(str::to_string))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("`luaskill.name` must be a non-empty string."))?;
    let output = render_luaskill_help_output(&name, &entries)?;
    Ok(VulcanToolInvocationResult::from_model_output(
        Value::String(output),
    ))
}

/// Executes one LuaSkills entry through the compact host-side dispatcher.
/// 通过紧凑的宿主侧调度器执行单个 LuaSkills 入口。
async fn execute_luaskills_dispatch_tool(
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    workspace_root: Option<PathBuf>,
    dispatch_entries: Arc<Vec<RuntimeEntryDescriptor>>,
    args: Value,
    session_id: Option<String>,
) -> Result<VulcanToolInvocationResult, String> {
    let Some(handle) = luaskills_runtime else {
        return Err(String::from(
            "The LuaSkills runtime is currently unavailable, so `call_luaskill` cannot invoke entries.",
        ));
    };
    let request = parse_call_luaskill_request(args)?;
    let entry_views = dispatch_entries
        .iter()
        .map(luaskills_entry_to_call_view)
        .collect::<Vec<_>>();
    let Some(entry_view) = resolve_call_luaskill_entry_by_name(&entry_views, &request.name) else {
        return Err(format!(
            "LuaSkills entry `{}` is not available for host dispatch. It may be disabled, not installed, or not enabled in the current runtime.",
            request.name
        ));
    };
    let Some(entry) = dispatch_entries
        .iter()
        .find(|entry| entry.canonical_name == entry_view.canonical_name)
    else {
        return Err(format!(
            "LuaSkills entry `{}` disappeared from the dispatch snapshot.",
            request.name
        ));
    };

    let request_args = restore_call_luaskill_args_for_entry(request.args, entry_view)?;
    let entry_declares_luaskill_sid = entry_declares_luaskill_sid(entry);
    let entry_declares_pwd = entry_declares_pwd(entry);
    let call_args = inject_host_managed_luaskill_sid_if_needed(
        request_args,
        entry_declares_luaskill_sid,
        session_id.as_deref(),
        &entry.canonical_name,
    )?;
    let call_args = inject_host_managed_pwd_if_needed(
        call_args,
        entry_declares_pwd,
        workspace_root.as_deref(),
        &entry.canonical_name,
    )?;
    let parsed_session_id = session_id
        .as_deref()
        .and_then(|value| Uuid::parse_str(value).ok());
    let invocation_context = build_luaskills_tool_invocation_context(
        parsed_session_id,
        "call-luaskill",
        &entry.canonical_name,
    );
    let result = call_luaskills_tool_entry(
        handle.clone(),
        entry.canonical_name.clone(),
        call_args,
        Some(invocation_context),
    )
    .await
    .map_err(|error| {
        format!(
            "LuaSkills entry `{}` in skill `{}` failed: {}.",
            entry.local_name, entry.skill_id, error
        )
    })?;
    let model_output = Value::String(render_luaskills_invocation_result_text(
        &handle,
        &entry.canonical_name,
        Some(&entry.skill_id),
        &result,
        workspace_root.as_deref(),
    ));
    let invocation_result = if let Some(host_result) = result.host_result.as_ref() {
        VulcanToolInvocationResult::from_model_output(model_output).with_host_result(
            VulcanToolHostResult {
                kind: host_result.kind.clone(),
                payload: host_result.payload.clone(),
            },
        )
    } else {
        VulcanToolInvocationResult::from_model_output(model_output)
    };
    Ok(invocation_result)
}

/// Converts one LuaSkills runtime entry into the shared dispatcher entry view.
/// 将单个 LuaSkills 运行时入口转换为共享调度器入口视图。
fn luaskills_entry_to_call_view(entry: &RuntimeEntryDescriptor) -> LuaSkillCallEntryView<'_> {
    LuaSkillCallEntryView {
        canonical_name: &entry.canonical_name,
        input_schema: &entry.input_schema,
    }
}

/// Converts one MCP runtime binding into the unified host-facing tool descriptor.
/// 将单条 MCP 运行时工具绑定转换为统一宿主侧工具描述。
fn mcp_tool_binding_to_vulcan(tool: &McpRuntimeToolBinding) -> VulcanToolDescriptor {
    VulcanToolDescriptor {
        id: tool.id().to_string(),
        name: tool.name().to_string(),
        description: tool.description().to_string(),
        source: VulcanToolSourceKind::Mcp,
    }
}

/// Converts one native-tool Plan availability flag into the standalone manager Plan policy.
/// 将单条原生工具的 Plan 可用标记转换为独立管理器的 Plan 策略。
fn native_plan_policy(descriptor: &NativeToolDescriptor) -> VulcanPlanToolPolicy {
    if descriptor.available_in_plan {
        VulcanPlanToolPolicy::Visible
    } else {
        VulcanPlanToolPolicy::Hidden
    }
}

/// Returns whether one LuaSkills runtime entry declares the reserved managed identity field.
/// 返回单个 LuaSkills 运行时入口是否声明了保留托管身份字段。
fn entry_declares_luaskill_sid(entry: &RuntimeEntryDescriptor) -> bool {
    entry_declares_reserved_parameter(entry, LUASKILL_SID_PARAMETER_NAME)
}

/// Returns whether one LuaSkills runtime entry declares the reserved managed project-path field.
/// 返回单个 LuaSkills 运行时入口是否声明了保留托管项目路径字段。
fn entry_declares_pwd(entry: &RuntimeEntryDescriptor) -> bool {
    entry_declares_reserved_parameter(entry, PWD_PARAMETER_NAME)
}

/// Returns whether one LuaSkills runtime entry declares one reserved parameter name in either schema properties or legacy parameter descriptors.
/// 返回单个 LuaSkills 运行时入口是否在 schema properties 或旧版参数描述中声明了某个保留参数名。
fn entry_declares_reserved_parameter(entry: &RuntimeEntryDescriptor, parameter_name: &str) -> bool {
    entry
        .input_schema
        .get("properties")
        .and_then(Value::as_object)
        .is_some_and(|properties| properties.contains_key(parameter_name))
        || entry
            .parameters
            .iter()
            .any(|parameter| parameter.name == parameter_name)
}

/// Appends semantic AI fallback aliases and removes their concrete backing tool from direct model exposure.
/// 追加语义 AI 回退别名，并移除其具体承载工具的直接暴露，避免模型同时看到两套等价工具。
fn append_ai_tool_fallback_aliases(
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
            source: VulcanToolSourceKind::AiFallback,
        },
        parameters_schema: target.parameters_schema,
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::HeuristicText(format!(
            "{} {} {}",
            semantic_tool_name, backing_tool_name, semantic_description
        )),
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

/// Inject one host-managed LuaSkills SID when the entry declares it and the caller did not provide it.
/// 当入口声明该字段且调用方未提供时，注入一份宿主管理 LuaSkills SID。
fn inject_host_managed_luaskill_sid_if_needed(
    args: Value,
    entry_declares_luaskill_sid: bool,
    session_id: Option<&str>,
    tool_name: &str,
) -> Result<Value, String> {
    if !entry_declares_luaskill_sid {
        return Ok(args);
    }

    let stable_session_id = session_id.ok_or_else(|| {
        format!(
            "LuaSkills tool `{tool_name}` requires a host-managed LUASKILL_SID, but the current session identifier is unavailable."
        )
    })?;

    let mut object = match args {
        Value::Object(map) => map,
        _ => {
            return Err(format!(
                "LuaSkills tool `{tool_name}` expects a JSON object so a host-managed LUASKILL_SID can be injected."
            ));
        }
    };

    if !object.contains_key(LUASKILL_SID_PARAMETER_NAME) {
        object.insert(
            String::from(LUASKILL_SID_PARAMETER_NAME),
            Value::String(format!(
                "{LUASKILLS_HOST_MANAGED_SID_PREFIX}{stable_session_id}"
            )),
        );
    }

    Ok(Value::Object(object))
}

/// Inject one host-managed workspace path when the entry declares `PWD`, the host can resolve a current project directory, and the caller did not provide one.
/// 当入口声明 `PWD`、宿主能够解析当前项目目录且调用方未提供时，注入一份宿主管理工作区路径。
fn inject_host_managed_pwd_if_needed(
    args: Value,
    entry_declares_pwd: bool,
    workspace_root: Option<&std::path::Path>,
    tool_name: &str,
) -> Result<Value, String> {
    if !entry_declares_pwd {
        return Ok(args);
    }

    let Some(workspace_root) = workspace_root.filter(|path| !path.as_os_str().is_empty()) else {
        return Ok(args);
    };

    let mut object = match args {
        Value::Object(map) => map,
        _ => {
            return Err(format!(
                "LuaSkills tool `{tool_name}` expects a JSON object so a host-managed PWD can be injected."
            ));
        }
    };

    if !object.contains_key(PWD_PARAMETER_NAME) {
        object.insert(
            String::from(PWD_PARAMETER_NAME),
            Value::String(native_workspace_path_text_from_path(workspace_root)),
        );
    }

    Ok(Value::Object(object))
}

#[cfg(test)]
mod tests {
    //! GUI bridge tests for semantic AI fallback aliasing.
    //! 面向语义 AI 回退别名的 GUI 桥接层测试。

    use super::{
        AiToolFallbackAlias, append_ai_tool_fallback_aliases, build_luaskills_dispatch_candidate,
        build_luaskills_help_candidate, inject_host_managed_pwd_if_needed,
        luaskills_entry_to_call_view,
    };
    use crate::luaskills_runtime::PWD_PARAMETER_NAME;
    use crate::provider_config::AiToolKind;
    use luaskills::RuntimeEntryDescriptor;
    use serde_json::json;
    use std::path::Path;
    use std::sync::Arc;
    use vulcan_luaskills_prompt::{LUASKILL_HELP_TOOL_NAME, LuaSkillPromptEntry};
    use vulcan_tools_luaskills_call::CALL_LUASKILL_TOOL_NAME;
    use vulcan_tools_manager::{
        VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor,
        VulcanToolInvocationResult, VulcanToolSourceKind, VulcanToolsManager,
    };

    /// Builds one inert host-tool candidate used by fallback alias tests.
    /// 构建一条供回退别名测试使用的惰性宿主工具候选项。
    fn test_candidate(id: &str, source: VulcanToolSourceKind) -> VulcanToolCandidate {
        VulcanToolCandidate {
            descriptor: VulcanToolDescriptor {
                id: id.to_string(),
                name: id.to_string(),
                description: String::from("Search the web"),
                source,
            },
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            }),
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::Visible,
            invoke: Arc::new(|_args, _session_id| {
                Box::pin(async { Ok(VulcanToolInvocationResult::from_model_output(json!("ok"))) })
            }),
        }
    }

    /// Verifies a configured host fallback is exposed as the semantic AI tool and the backing tool is not duplicated.
    /// 验证已配置的宿主回退会以语义 AI 工具暴露，并且不会重复暴露底层承载工具。
    #[test]
    fn ai_tool_fallback_alias_exposes_semantic_name_without_backing_duplicate() {
        let mut candidates = vec![test_candidate("brave-search", VulcanToolSourceKind::Mcp)];

        append_ai_tool_fallback_aliases(
            &mut candidates,
            vec![AiToolFallbackAlias {
                kind: AiToolKind::WebSearch,
                fallback_tool_id: String::from("mcp_brave_search"),
            }],
        );

        let resolved = VulcanToolsManager::new(candidates).resolve_for_execution_mode("Build");
        assert_eq!(resolved.descriptors.len(), 1);
        assert_eq!(resolved.descriptors[0].id, "web_search");
        assert_eq!(resolved.descriptors[0].description, "Search the web");
        assert_eq!(
            resolved.descriptors[0].source,
            VulcanToolSourceKind::AiFallback
        );
        assert_eq!(
            resolved.definitions[0].parameters_schema["properties"]["query"]["type"],
            "string"
        );
    }

    /// Verifies host-managed PWD injection uses the native workspace-path format expected by LuaSkills host conventions.
    /// 验证宿主管理 PWD 注入会使用 LuaSkills 宿主公约要求的原生工作区路径格式。
    #[test]
    fn host_managed_pwd_injection_uses_native_workspace_path_text() {
        let workspace_root = if cfg!(windows) {
            Path::new("C:/workspace/demo")
        } else {
            Path::new("/tmp/workspace/demo")
        };
        let expected_pwd = if cfg!(windows) {
            r"C:\workspace\demo"
        } else {
            "/tmp/workspace/demo"
        };

        let injected = inject_host_managed_pwd_if_needed(
            json!({ "query": "demo" }),
            true,
            Some(workspace_root),
            "vulcan-codekit-run",
        )
        .expect("pwd injection should succeed");

        assert_eq!(injected[PWD_PARAMETER_NAME], expected_pwd);
    }

    /// Verifies host-managed PWD injection preserves an explicit caller-supplied path instead of overwriting it.
    /// 验证宿主管理 PWD 注入会保留调用方显式提供的路径，而不是强行覆盖。
    #[test]
    fn host_managed_pwd_injection_preserves_explicit_value() {
        let injected = inject_host_managed_pwd_if_needed(
            json!({ "PWD": "/caller/path", "query": "demo" }),
            true,
            Some(Path::new("/tmp/workspace/demo")),
            "vulcan-codekit-run",
        )
        .expect("pwd injection should preserve explicit value");

        assert_eq!(injected[PWD_PARAMETER_NAME], "/caller/path");
    }

    /// Verifies implicit LuaSkills expose help and call tools as one paired tool group.
    /// 验证隐式 LuaSkills 会以帮助与调用工具成组暴露。
    #[test]
    fn implicit_luaskills_expose_help_and_call_tool_pair() {
        let entry = RuntimeEntryDescriptor {
            canonical_name: String::from("vulcan-codekit-ast-tree"),
            skill_id: String::from("vulcan-codekit"),
            local_name: String::from("ast-tree"),
            root_name: String::from("USER"),
            skill_dir: String::from(r"C:\skills\vulcan-codekit"),
            description: String::from("Inspect source tree structure."),
            parameters: Vec::new(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "paths": { "type": "string" }
                },
                "required": ["paths"]
            }),
        };
        let manager = VulcanToolsManager::new(vec![
            build_luaskills_help_candidate(vec![LuaSkillPromptEntry::new(
                "vulcan-codekit-ast-tree",
                "Inspect source tree structure.",
            )]),
            build_luaskills_dispatch_candidate(None, None, vec![entry]),
        ]);

        let resolved = manager.resolve_for_execution_mode("Build");
        let ids = resolved
            .descriptors
            .iter()
            .map(|descriptor| descriptor.id.as_str())
            .collect::<Vec<_>>();

        assert!(ids.contains(&LUASKILL_HELP_TOOL_NAME));
        assert!(ids.contains(&CALL_LUASKILL_TOOL_NAME));
    }

    /// Verifies implicit LuaSkills dispatch uses LuaSkills canonical names rather than public tool aliases.
    /// 验证隐式 LuaSkills 调度使用 LuaSkills canonical 名称，而不是 public tool 别名。
    #[test]
    fn implicit_luaskills_dispatch_uses_canonical_name() {
        let input_schema = json!({ "type": "object" });
        let entry = RuntimeEntryDescriptor {
            canonical_name: String::from("vulcan-codekit-ast-tree"),
            skill_id: String::from("vulcan-codekit"),
            local_name: String::from("ast-tree"),
            root_name: String::from("USER"),
            skill_dir: String::from(r"C:\skills\vulcan-codekit"),
            description: String::from("Inspect source tree structure."),
            parameters: Vec::new(),
            input_schema,
        };

        let view = luaskills_entry_to_call_view(&entry);

        assert_eq!(view.canonical_name, "vulcan-codekit-ast-tree");
    }
}
