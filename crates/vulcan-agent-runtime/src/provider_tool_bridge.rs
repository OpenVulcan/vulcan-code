//! Per-turn provider tool registration and host-side output staging.
//! 每回合供应商工具注册与宿主侧输出暂存。

use crate::attachment_asset_index::{
    discard_unreferenced_staged_attachment, retain_generated_attachments_for_session,
};
use crate::attachment_assets::{
    attachment_descriptor_from_mime_type, stage_attachment_bytes, stage_attachment_file_streaming,
};
use crate::media_understanding::{
    build_model_replacement_media_candidate, direct_media_understanding_description,
    media_understanding_input_schema, media_understanding_tool_id,
    project_media_understanding_arguments, render_media_understanding_provider_result,
    resolve_media_understanding_resource_arguments,
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use vulcan_core::types::ConversationAttachment;
use vulcan_provider_adapters::{
    AvailableProviderToolTarget, ManifestSourceLayer, MediaUnderstandingBackendSelection,
    PersistedAiConfiguration, ProviderToolCapabilityKind, ProviderToolExecutionResult,
    ProviderToolNormalizedOutput, ProviderToolResourceDownloadPolicy, ProviderToolTargetSelection,
    PublicAiRuntime,
};
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolHostResult,
    VulcanToolInvocationResult, VulcanToolSourceKind,
};

/// Maximum generated image or generic file size accepted by the host.
/// 宿主接受的生成图片或通用文件最大字节数。
const PROVIDER_TOOL_DEFAULT_OUTPUT_BYTE_LIMIT: usize = 50 * 1024 * 1024;
/// Maximum generated audio size accepted by the host.
/// 宿主接受的生成音频最大字节数。
const PROVIDER_TOOL_AUDIO_OUTPUT_BYTE_LIMIT: usize = 100 * 1024 * 1024;
/// Maximum generated video size accepted by the host.
/// 宿主接受的生成视频最大字节数。
const PROVIDER_TOOL_VIDEO_OUTPUT_BYTE_LIMIT: usize = 512 * 1024 * 1024;
/// Maximum wall-clock time allowed for one provider resource download.
/// 单个供应商资源下载允许的最大墙钟时间。
const PROVIDER_TOOL_RESOURCE_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// Stable host-result kind carrying exact managed generated attachments.
/// 携带精确托管生成附件的稳定宿主结果类型。
pub(crate) const GENERATED_ATTACHMENTS_HOST_RESULT_KIND: &str = "generated_attachments";

/// Stable model-extension capabilities exposed independently from selected targets.
/// 独立于已选目标稳定暴露的模型扩展能力。
const MODEL_EXTENSION_CAPABILITIES: [ProviderToolCapabilityKind; 9] = [
    ProviderToolCapabilityKind::ImageGenerate,
    ProviderToolCapabilityKind::ImageEdit,
    ProviderToolCapabilityKind::VideoGenerate,
    ProviderToolCapabilityKind::VideoEdit,
    ProviderToolCapabilityKind::VideoExtend,
    ProviderToolCapabilityKind::SpeechSynthesize,
    ProviderToolCapabilityKind::SpeechTranscribe,
    ProviderToolCapabilityKind::MusicGenerate,
    ProviderToolCapabilityKind::MusicCover,
];

/// Stable model-facing lookup tool identifier.
/// 稳定的面向模型查询工具标识。
const MODEL_EXTENSION_PARAMETERS_TOOL_ID: &str = "get_model_extension_parameters";

/// Marks owned asynchronous provider work for cancellation when one tool future is dropped.
/// 当工具 Future 被丢弃时，为其拥有的异步供应商任务记录取消意图。
struct ProviderTaskCancellationOnDrop {
    /// Runtime that owns durable provider-task recovery state.
    /// 持有供应商任务持久恢复状态的运行时。
    runtime: Arc<PublicAiRuntime>,
    /// Exact conversation identity attached to the tool invocation.
    /// 附加到工具调用的精确会话身份。
    owner_session_id: Option<String>,
    /// Whether dropping this guard must record cancellation intent.
    /// 丢弃此守卫时是否必须记录取消意图。
    armed: bool,
}

impl ProviderTaskCancellationOnDrop {
    /// Creates an armed guard for one provider-tool invocation.
    /// 为一次供应商工具调用创建已启用守卫。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` owns task recovery and `owner_session_id` identifies the conversation.
    /// `runtime` 持有任务恢复状态，`owner_session_id` 标识会话。
    ///
    /// # Returns
    /// 返回值
    /// An armed cancellation guard.
    /// 返回已启用的取消守卫。
    fn new(runtime: Arc<PublicAiRuntime>, owner_session_id: Option<String>) -> Self {
        Self {
            runtime,
            owner_session_id,
            armed: true,
        }
    }

    /// Disarms cancellation after the invocation reaches a normal result.
    /// 在调用正常得到结果后解除取消。
    fn disarm(&mut self) {
        self.armed = false;
    }

    /// Completes one normally returned invocation without treating its error as owner cancellation.
    /// 完成一次正常返回的调用，不把其错误视为所有者取消。
    ///
    /// # Parameters
    /// 参数
    /// `result` is the terminal value returned by the exact provider-tool invocation.
    /// `result` 是精确供应商工具调用返回的终态值。
    ///
    /// # Returns
    /// 返回值
    /// Returns the original success or failure after disarming owner-wide cancellation.
    /// 在解除所有者级取消后原样返回成功或失败。
    fn complete<ResultType, ErrorType>(
        &mut self,
        result: Result<ResultType, ErrorType>,
    ) -> Result<ResultType, ErrorType> {
        self.disarm();
        result
    }
}

impl Drop for ProviderTaskCancellationOnDrop {
    /// Persists cancellation intent synchronously before the interrupted future disappears.
    /// 在被中断 Future 消失前同步持久化取消意图。
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let Some(owner_session_id) = self.owner_session_id.as_deref() else {
            return;
        };
        if let Err(error) = self
            .runtime
            .request_provider_task_cancellation_for_owner(owner_session_id)
        {
            eprintln!(
                "[provider-tool] failed to persist provider-task cancellation for owner `{owner_session_id}`: {error}"
            );
        }
    }
}

/// Builds direct provider tools and the invariant model-extension facade set.
/// 构建指定直连供应商工具与不变的模型扩展外壳集合。
///
/// # Parameters
/// 参数
/// `runtime`, `configuration`, and `workspace_root` are frozen for this execution turn.
/// `runtime`、`configuration` 与 `workspace_root` 在当前执行回合保持冻结。
///
/// # Returns
/// 返回值
/// Direct target-native tools plus one lookup tool and nine stable extension executors.
/// 返回目标原生直连工具、一个查询工具与九个稳定扩展执行工具。
pub(crate) fn build_provider_tool_candidates(
    runtime: Arc<PublicAiRuntime>,
    mut configuration: PersistedAiConfiguration,
    workspace_root: Option<PathBuf>,
    session_selections: &Option<
        std::collections::BTreeMap<
            String,
            vulcan_workbench_model::SessionProviderToolTargetSelection,
        >,
    >,
    session_enabled_capabilities: &Option<BTreeSet<String>>,
    session_extension_target_sets: &Option<
        BTreeMap<String, BTreeSet<vulcan_workbench_model::SessionProviderToolTargetSelection>>,
    >,
    session_media_understanding_selections: &Option<
        BTreeMap<String, vulcan_workbench_model::SessionMediaUnderstandingSelection>,
    >,
    suppress_web_search: bool,
    media_resource_runtime: Option<Arc<dyn crate::media_resource_runtime::MediaResourceRuntime>>,
) -> Result<Vec<VulcanToolCandidate>, String> {
    apply_session_provider_tool_selections(
        &mut configuration,
        session_selections,
        session_enabled_capabilities,
        session_extension_target_sets,
        session_media_understanding_selections,
    )?;
    validate_enabled_media_understanding_backends(&configuration)?;
    let available = runtime.available_provider_tool_targets(&configuration)?;
    let mut candidates = Vec::new();
    for (capability, selection) in &configuration.provider_tool_selections {
        if media_understanding_tool_id(*capability).is_some() {
            return Err(format!(
                "Media-understanding capability `{capability:?}` cannot use the legacy direct-selection store; select its exact session route."
            ));
        }
        if !capability.is_direct_selection() {
            return Err(format!(
                "Provider tool capability `{capability:?}` cannot use the direct-selection store."
            ));
        }
        if provider_tool_capability_is_suppressed(*capability, suppress_web_search) {
            // Native route search is registered independently; retain persisted external selection but hide it for this turn.
            // 原生路由搜索独立注册；保留持久化外部选择，但本回合不暴露该工具。
            continue;
        }
        if !configuration
            .provider_tool_enabled_capabilities
            .contains(capability)
        {
            continue;
        }
        let Some(selected) = available
            .iter()
            .find(|candidate| {
                candidate.connection_id == selection.connection_id
                    && candidate.target.target_id == selection.target_id
                    && candidate.target.contract.capability == *capability
            })
            .cloned()
        else {
            return Err(format!(
                "Provider tool capability `{:?}` retains unavailable target `{}` through account `{}`; select a valid model and account before sending.",
                capability, selection.target_id, selection.connection_id
            ));
        };
        candidates.push(build_direct_provider_tool_candidate(
            Arc::clone(&runtime),
            configuration.clone(),
            workspace_root.clone(),
            selected,
            media_resource_runtime.clone(),
        )?);
    }
    // mediaUnderstandingSelections is a closed provider-tool or model-replacement union that
    // never changes the three AI-facing tool schemas.
    // mediaUnderstandingSelections 是封闭的供应商工具或模型代替联合类型，绝不改变三个 AI 可见工具 Schema。
    for (capability, backend) in configuration.media_understanding_selections.clone() {
        if media_understanding_tool_id(capability).is_none() {
            return Err(format!(
                "Non-media capability `{capability:?}` cannot use the media-understanding backend store."
            ));
        }
        if !configuration
            .provider_tool_enabled_capabilities
            .contains(&capability)
        {
            continue;
        }
        match backend {
            MediaUnderstandingBackendSelection::ProviderTool { selection } => {
                // selected is the exact currently available credential target retained by settings.
                // selected 是设置保留的当前精确可用凭据目标。
                let selected = available
                    .iter()
                    .find(|candidate| {
                        candidate.connection_id == selection.connection_id
                            && candidate.target.target_id == selection.target_id
                            && candidate.target.contract.capability == capability
                    })
                    .cloned()
                    .ok_or_else(|| {
                        format!(
                            "Media-understanding capability `{capability:?}` retains unavailable target `{}` through account `{}`; select a valid model and account before sending.",
                            selection.target_id, selection.connection_id
                        )
                    })?;
                candidates.push(build_direct_provider_tool_candidate(
                    Arc::clone(&runtime),
                    configuration.clone(),
                    workspace_root.clone(),
                    selected,
                    media_resource_runtime.clone(),
                )?);
            }
            MediaUnderstandingBackendSelection::ModelReplacement {
                selection,
                reasoning_effort,
            } => {
                // The exact session route is validated below against the current executable catalog;
                // the global candidate switch owns only new-session system defaults.
                // 下方会依据当前可执行目录校验精确会话路由；全局候选开关只负责新会话系统默认值。
                candidates.push(build_model_replacement_media_candidate(
                    Arc::clone(&runtime),
                    configuration.clone(),
                    workspace_root.clone(),
                    capability,
                    selection,
                    reasoning_effort,
                    media_resource_runtime.clone(),
                )?);
            }
        }
    }
    let extension_catalog = Arc::new(build_selected_model_extension_catalog(
        &configuration,
        &available,
    )?);
    candidates.push(build_model_extension_parameters_candidate(Arc::clone(
        &extension_catalog,
    )));
    for capability in MODEL_EXTENSION_CAPABILITIES {
        candidates.push(build_model_extension_executor_candidate(
            Arc::clone(&runtime),
            configuration.clone(),
            workspace_root.clone(),
            Arc::clone(&extension_catalog),
            capability,
            media_resource_runtime.clone(),
        ));
    }
    Ok(candidates)
}

/// Validates that every enabled fixed media facade has one exact effective backend route.
/// 校验每个已启用固定媒体外壳均具有一条精确生效后端路由。
///
/// # Parameters
/// 参数
/// `configuration` is the per-turn configuration after session enable switches are applied.
/// `configuration` 是应用会话启用开关后的当前回合配置。
///
/// # Returns
/// 返回值
/// Success when every enabled media capability has one backend, or an explicit configuration error.
/// 每个已启用媒体能力均具有一个后端时返回成功，否则返回明确配置错误。
fn validate_enabled_media_understanding_backends(
    configuration: &PersistedAiConfiguration,
) -> Result<(), String> {
    for capability in [
        ProviderToolCapabilityKind::ImageUnderstand,
        ProviderToolCapabilityKind::VideoUnderstand,
        ProviderToolCapabilityKind::AudioUnderstand,
    ] {
        if configuration
            .provider_tool_enabled_capabilities
            .contains(&capability)
            && !configuration
                .media_understanding_selections
                .contains_key(&capability)
        {
            return Err(format!(
                "Media-understanding capability `{capability:?}` is enabled but has no exact session backend route."
            ));
        }
    }
    Ok(())
}

/// Builds the exact selected native target catalog behind stable extension tools.
/// 构建稳定扩展工具背后的精确已选原生目标目录。
///
/// # Parameters
/// 参数
/// `configuration` owns selected target sets and `available` is the frozen credential-eligible
/// target snapshot for this execution.
/// `configuration` 持有已选目标集合，`available` 是当前执行冻结的凭据合格目标快照。
///
/// # Returns
/// 返回值
/// System-manifest targets keyed by extension capability, with duplicate public ids rejected.
/// 返回按扩展能力索引的系统 Manifest 目标，并拒绝重复公开标识。
fn build_selected_model_extension_catalog(
    configuration: &PersistedAiConfiguration,
    available: &[AvailableProviderToolTarget],
) -> Result<BTreeMap<ProviderToolCapabilityKind, Vec<AvailableProviderToolTarget>>, String> {
    let mut catalog = BTreeMap::new();
    for (capability, selections) in &configuration.provider_extension_target_sets {
        if !capability.is_model_extension() {
            return Err(format!(
                "Provider tool capability `{capability:?}` cannot use the model-extension store."
            ));
        }
        let mut public_model_ids = BTreeSet::new();
        let mut selected_targets = Vec::new();
        for selection in selections {
            let Some(target) = available.iter().find(|candidate| {
                candidate.connection_id == selection.connection_id
                    && candidate.target.target_id == selection.target_id
                    && candidate.target.contract.capability == *capability
            }) else {
                // Stale selections remain visible in settings diagnostics but do not block an
                // otherwise valid conversation or enter the model-visible extension catalog.
                // 过期选择保留给设置诊断，但不会阻塞其他有效会话，也不会进入模型可见扩展目录。
                continue;
            };
            if target.manifest_source_layer == ManifestSourceLayer::Custom {
                continue;
            }
            if target.target.contract.extension_profile.is_none() {
                return Err(format!(
                    "Native model-extension target `{}` has no reviewed extension profile.",
                    target.target.target_id
                ));
            }
            let public_model_id = provider_tool_public_model_id(target);
            if !public_model_ids.insert(public_model_id.clone()) {
                return Err(format!(
                    "Model-extension capability `{capability:?}` selects duplicate public model `{public_model_id}` through multiple accounts."
                ));
            }
            selected_targets.push(target.clone());
        }
        selected_targets.sort_by(|left, right| {
            provider_tool_public_model_id(left).cmp(&provider_tool_public_model_id(right))
        });
        if !selected_targets.is_empty() {
            catalog.insert(*capability, selected_targets);
        }
    }
    Ok(catalog)
}

/// Builds one stable public model identifier without exposing account identity.
/// 构建一个不暴露账号身份的稳定公开模型标识。
///
/// # Parameters
/// 参数
/// `target` is one frozen system-manifest target paired with an account.
/// `target` 是一个与账号配对的冻结系统 Manifest 目标。
///
/// # Returns
/// 返回值
/// The exact `provider_id/upstream_target_id` identifier.
/// 返回精确的 `provider_id/upstream_target_id` 标识。
fn provider_tool_public_model_id(target: &AvailableProviderToolTarget) -> String {
    format!(
        "{}/{}",
        target.target.provider_id, target.target.upstream_target_id
    )
}

/// Builds the invariant model-extension parameter lookup candidate.
/// 构建不变的模型扩展参数查询候选工具。
///
/// # Parameters
/// 参数
/// `catalog` is the frozen selected system-target catalog for this execution.
/// `catalog` 是当前执行冻结的已选系统目标目录。
///
/// # Returns
/// 返回值
/// One stable AI-visible lookup tool whose result may vary without changing its schema.
/// 返回一个结果可变化但 Schema 不变的稳定 AI 可见查询工具。
fn build_model_extension_parameters_candidate(
    catalog: Arc<BTreeMap<ProviderToolCapabilityKind, Vec<AvailableProviderToolTarget>>>,
) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from(MODEL_EXTENSION_PARAMETERS_TOOL_ID),
            name: String::from(MODEL_EXTENSION_PARAMETERS_TOOL_ID),
            description: String::from(
                "MUST be called before any model-extension tool. Returns the currently enabled native models, reviewed capabilities, constraints, and exact args schema for one extension capability. Never invent a model id or model-specific argument.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::AiHostBinding,
        },
        parameters_schema: model_extension_parameters_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Visible,
        request_streaming: None,
        invoke: Arc::new(move |arguments, _owner_session_id| {
            let catalog = Arc::clone(&catalog);
            Box::pin(async move { render_model_extension_parameters(catalog.as_ref(), arguments) })
        }),
    }
}

/// Builds one invariant executor candidate for an exact extension capability.
/// 为一项精确扩展能力构建一个不变执行候选工具。
///
/// # Parameters
/// 参数
/// Runtime, configuration, workspace, catalog, and capability are frozen into the invocation.
/// 运行时、配置、工作区、目录与能力被冻结进调用。
///
/// # Returns
/// 返回值
/// One stable executor accepting only `model` and `args`.
/// 返回一个只接受 `model` 与 `args` 的稳定执行工具。
fn build_model_extension_executor_candidate(
    runtime: Arc<PublicAiRuntime>,
    configuration: PersistedAiConfiguration,
    workspace_root: Option<PathBuf>,
    catalog: Arc<BTreeMap<ProviderToolCapabilityKind, Vec<AvailableProviderToolTarget>>>,
    capability: ProviderToolCapabilityKind,
    media_resource_runtime: Option<Arc<dyn crate::media_resource_runtime::MediaResourceRuntime>>,
) -> VulcanToolCandidate {
    let tool_id = model_extension_tool_id(capability);
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from(tool_id),
            name: String::from(tool_id),
            description: model_extension_tool_description(capability),
            help_text: None,
            source: VulcanToolSourceKind::AiHostBinding,
        },
        parameters_schema: model_extension_executor_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Visible,
        request_streaming: None,
        invoke: Arc::new(move |arguments, context| {
            let runtime = Arc::clone(&runtime);
            let configuration = configuration.clone();
            let workspace_root = workspace_root.clone();
            let catalog = Arc::clone(&catalog);
            // Keep the host resource authority from this execution, independent of model-supplied arguments.
            // 保持来自本执行的宿主资源权威，独立于模型提供参数。
            let media_resource_runtime = media_resource_runtime.clone();
            Box::pin(async move {
                let owner_session_id = Some(context.session_id.clone());
                let (model, provider_arguments) = parse_model_extension_invocation(arguments)?;
                let selected = catalog
                    .get(&capability)
                    .and_then(|targets| {
                        targets
                            .iter()
                            .find(|target| provider_tool_public_model_id(target) == model)
                    })
                    .cloned()
                    .ok_or_else(|| {
                        format!(
                            "Model `{model}` is not currently enabled for `{tool_id}`. Call `{MODEL_EXTENSION_PARAMETERS_TOOL_ID}` again before retrying."
                        )
                    })?;
                if selected.manifest_source_layer == ManifestSourceLayer::Custom {
                    return Err(format!(
                        "Custom provider model `{model}` cannot execute through model extensions."
                    ));
                }
                let selection = ProviderToolTargetSelection {
                    connection_id: selected.connection_id.clone(),
                    target_id: selected.target.target_id.clone(),
                };
                let mut cancellation_guard = ProviderTaskCancellationOnDrop::new(
                    Arc::clone(&runtime),
                    owner_session_id.clone(),
                );
                // Resolve only bindings from the runtime's one frozen operation, with this exact invoking session.
                // 只使用本精确发起会话解析运行时唯一冻结操作中的绑定。
                let source_resolver =
                    crate::media_resource_runtime::SessionProviderToolMediaSourceResolver {
                        runtime: media_resource_runtime.as_deref(),
                        workspace_root: workspace_root.as_deref(),
                        session_id: &context.session_id,
                    };
                let execution_result = runtime
                    .execute_provider_tool_target_with_media_sources(
                        &configuration,
                        capability,
                        &selection,
                        provider_arguments,
                        owner_session_id,
                        vulcan_provider_adapters::ProviderToolResourceContext {
                            workspace_root: workspace_root.as_deref(),
                            media_sources: &[],
                            source_resolver: Some(&source_resolver),
                        },
                    )
                    .await;
                // A normally returned provider error belongs only to this tool. Disarm before
                // propagating it so the drop guard cannot cancel asynchronous sibling tools.
                // 正常返回的 Provider 错误只属于当前工具。传播错误前先解除守卫，避免析构守卫取消
                // 异步兄弟工具。
                let result = cancellation_guard.complete(execution_result)?;
                // rendered uses one canonical media result shape for understanding capabilities.
                // rendered 为理解能力使用统一规范媒体结果形态。
                if media_understanding_tool_id(capability).is_some() {
                    render_media_understanding_provider_result(capability, result)
                } else {
                    render_provider_tool_result(
                        workspace_root.as_deref(),
                        context.session_id.as_str(),
                        result,
                    )
                    .await
                }
            })
        }),
    }
}

/// Returns the stable public tool identifier for one model-extension capability.
/// 返回一项模型扩展能力的稳定公开工具标识。
///
/// # Parameters
/// 参数
/// `capability` must be one of the nine closed model-extension capabilities.
/// `capability` 必须是九项封闭模型扩展能力之一。
///
/// # Returns
/// 返回值
/// The invariant tool identifier.
/// 返回不变的工具标识。
fn model_extension_tool_id(capability: ProviderToolCapabilityKind) -> &'static str {
    match capability {
        ProviderToolCapabilityKind::ImageGenerate => "image_generate",
        ProviderToolCapabilityKind::ImageEdit => "image_edit",
        ProviderToolCapabilityKind::VideoGenerate => "video_generate",
        ProviderToolCapabilityKind::VideoEdit => "video_edit",
        ProviderToolCapabilityKind::VideoExtend => "video_extend",
        ProviderToolCapabilityKind::SpeechSynthesize => "speech_synthesize",
        ProviderToolCapabilityKind::SpeechTranscribe => "speech_transcribe",
        ProviderToolCapabilityKind::MusicGenerate => "music_generate",
        ProviderToolCapabilityKind::MusicCover => "music_cover",
        _ => unreachable!("non-extension capability reached extension tool id resolution"),
    }
}

/// Builds the invariant English description for one model-extension executor.
/// 为一项模型扩展执行工具构建不变的英文说明。
///
/// # Parameters
/// 参数
/// `capability` identifies the exact stable executor.
/// `capability` 标识精确的稳定执行工具。
///
/// # Returns
/// 返回值
/// A model-facing description that mandates parameter discovery before invocation.
/// 返回要求调用前先查询参数的面向模型说明。
fn model_extension_tool_description(capability: ProviderToolCapabilityKind) -> String {
    format!(
        "Executes the `{}` model extension. You MUST call `{MODEL_EXTENSION_PARAMETERS_TOOL_ID}` for this capability immediately before use, then pass one returned model id and args conforming exactly to that model's schema. Never guess or reuse stale model-specific arguments. Declared media-source fields can use a canonical project-resource://ID, an owned attachment://ID from this conversation, or a readable local file path. Relative paths resolve from the active workspace; absolute and project-external paths are accepted until filesystem permissions are enabled. The host resolves only declared media bindings and applies independent extension size limits; an oversized or unsupported file returns an explicit error, not an address-only understanding result.",
        model_extension_tool_id(capability)
    )
}

/// Returns the invariant JSON Schema for model-extension parameter discovery.
/// 返回模型扩展参数查询工具的不变 JSON Schema。
///
/// # Returns
/// 返回值
/// One strict object schema independent from selected providers and models.
/// 返回一份独立于已选供应商与模型的严格对象 Schema。
fn model_extension_parameters_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "capability": {
                "type": "string",
                "enum": [
                    "image_generate",
                    "image_edit",
                    "video_generate",
                    "video_edit",
                    "video_extend",
                    "speech_synthesize",
                    "speech_transcribe",
                    "music_generate",
                    "music_cover"
                ],
                "description": "Exact model-extension capability to inspect."
            },
            "model": {
                "type": "string",
                "minLength": 3,
                "description": "Optional exact provider/model identifier returned by an earlier list response."
            }
        },
        "required": ["capability"],
        "additionalProperties": false
    })
}

/// Returns the invariant JSON Schema shared by every model-extension executor.
/// 返回每个模型扩展执行工具共享的不变 JSON Schema。
///
/// # Returns
/// 返回值
/// One strict outer object carrying an exact public model id and model-native argument object.
/// 返回携带精确公开模型标识与模型原生参数对象的严格外层对象。
fn model_extension_executor_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "model": {
                "type": "string",
                "minLength": 3,
                "description": "Exact provider/model identifier returned by get_model_extension_parameters."
            },
            "args": {
                "type": "object",
                "description": "Exact model-native arguments returned by get_model_extension_parameters."
            }
        },
        "required": ["model", "args"],
        "additionalProperties": false
    })
}

/// Renders the selected model-extension contracts for one exact capability.
/// 渲染一项精确能力的已选模型扩展契约。
///
/// # Parameters
/// 参数
/// `catalog` is the frozen selected native-target catalog and `arguments` is the strict lookup request.
/// `catalog` 是冻结的已选原生目标目录，`arguments` 是严格查询请求。
///
/// # Returns
/// 返回值
/// Structured model-visible JSON containing reviewed profiles and exact target schemas.
/// 返回包含已审核说明与精确目标 Schema 的模型可见结构化 JSON。
fn render_model_extension_parameters(
    catalog: &BTreeMap<ProviderToolCapabilityKind, Vec<AvailableProviderToolTarget>>,
    arguments: Value,
) -> Result<VulcanToolInvocationResult, String> {
    let object = arguments.as_object().ok_or_else(|| {
        String::from("Model-extension parameter lookup arguments must be a JSON object.")
    })?;
    if let Some(unknown) = object
        .keys()
        .find(|key| key.as_str() != "capability" && key.as_str() != "model")
    {
        return Err(format!(
            "Model-extension parameter lookup contains unknown field `{unknown}`."
        ));
    }
    let capability_name = object
        .get("capability")
        .and_then(Value::as_str)
        .ok_or_else(|| String::from("Field `capability` must be a string."))?;
    let capability: ProviderToolCapabilityKind =
        serde_json::from_value(Value::String(capability_name.to_string())).map_err(|_| {
            format!("Capability `{capability_name}` is not a registered model extension.")
        })?;
    if !capability.is_model_extension() {
        return Err(format!(
            "Capability `{capability_name}` is not a registered model extension."
        ));
    }
    let requested_model = object
        .get("model")
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| String::from("Field `model` must be a string when present."))
        })
        .transpose()?;
    let mut models = Vec::new();
    for target in catalog.get(&capability).into_iter().flatten() {
        let model = provider_tool_public_model_id(target);
        if requested_model
            .as_deref()
            .is_some_and(|requested| requested != model)
        {
            continue;
        }
        let profile = target
            .target
            .contract
            .extension_profile
            .as_ref()
            .ok_or_else(|| {
                format!(
                    "Native model-extension target `{}` has no reviewed extension profile.",
                    target.target.target_id
                )
            })?;
        models.push(json!({
            "model": model,
            "display_name": target.target.display_name,
            "description": profile.summary,
            "characteristics": profile.characteristics,
            "input_modes": profile.input_modes,
            "resolution_policy": profile.resolution_policy,
            "duration_policy": profile.duration_policy,
            "format_policy": profile.format_policy,
            "constraints": profile.constraints,
            "limitations": profile.limitations,
            "evidence_refs": profile.evidence_refs,
            "args_schema": target.target.contract.input_schema
        }));
    }
    if let Some(requested_model) = requested_model.as_deref()
        && models.is_empty()
    {
        return Err(format!(
            "Model `{requested_model}` is not currently enabled for capability `{capability_name}`."
        ));
    }
    let payload = json!({
        "kind": "model_extension_parameters",
        "capability": capability_name,
        "models": models,
        "instruction": if catalog.contains_key(&capability) {
            "Copy one returned `model` field verbatim into the executor call; it is the only valid callable model identifier. Pass `args` matching that model's exact `args_schema`. In declared media-source fields, use a canonical project-resource://ID, an owned attachment://ID, or a readable local file path. Relative paths resolve from the active workspace; absolute and project-external paths are accepted until filesystem permissions are enabled. The host resolves the exact declared source before provider dispatch. Do not infer alternate model or source fields."
        } else {
            "No native model is currently enabled for this extension capability."
        }
    });
    Ok(
        VulcanToolInvocationResult::from_model_output(payload.clone()).with_host_result(
            VulcanToolHostResult {
                kind: String::from("model_extension_parameters"),
                payload,
            },
        ),
    )
}

/// Parses one stable executor invocation without accepting ambiguous fallback fields.
/// 解析一次稳定执行工具调用，且不接受模糊回退字段。
///
/// # Parameters
/// 参数
/// `arguments` is the model-provided stable facade input.
/// `arguments` 是模型提供的稳定外壳输入。
///
/// # Returns
/// 返回值
/// The exact public model id and model-native JSON argument object.
/// 返回精确公开模型标识与模型原生 JSON 参数对象。
fn parse_model_extension_invocation(arguments: Value) -> Result<(String, Value), String> {
    let object = arguments
        .as_object()
        .ok_or_else(|| String::from("Model-extension invocation must be a JSON object."))?;
    if let Some(unknown) = object
        .keys()
        .find(|key| key.as_str() != "model" && key.as_str() != "args")
    {
        return Err(format!(
            "Model-extension invocation contains unknown field `{unknown}`."
        ));
    }
    let model = object
        .get("model")
        .and_then(Value::as_str)
        .filter(|value| value.contains('/') && !value.starts_with('/') && !value.ends_with('/'))
        .ok_or_else(|| String::from("Field `model` must be one exact provider/model identifier."))?
        .to_string();
    let provider_arguments = object
        .get("args")
        .filter(|value| value.is_object())
        .cloned()
        .ok_or_else(|| String::from("Field `args` must be a JSON object."))?;
    Ok((model, provider_arguments))
}

/// Reports whether one configured provider-tool capability is hidden for the current route.
/// 判断当前路由是否需要隐藏某项已配置的供应商工具能力。
fn provider_tool_capability_is_suppressed(
    capability: vulcan_provider_adapters::ProviderToolCapabilityKind,
    suppress_web_search: bool,
) -> bool {
    suppress_web_search
        && capability == vulcan_provider_adapters::ProviderToolCapabilityKind::WebSearch
}

/// Applies an explicit session override to the per-turn provider configuration.
/// 将显式会话覆盖应用到当前回合的供应商配置。
fn apply_session_provider_tool_selections(
    configuration: &mut PersistedAiConfiguration,
    session_selections: &Option<
        std::collections::BTreeMap<
            String,
            vulcan_workbench_model::SessionProviderToolTargetSelection,
        >,
    >,
    session_enabled_capabilities: &Option<BTreeSet<String>>,
    session_extension_target_sets: &Option<
        BTreeMap<String, BTreeSet<vulcan_workbench_model::SessionProviderToolTargetSelection>>,
    >,
    session_media_understanding_selections: &Option<
        BTreeMap<String, vulcan_workbench_model::SessionMediaUnderstandingSelection>,
    >,
) -> Result<(), String> {
    match (session_selections, session_enabled_capabilities) {
        (Some(session_selections), Some(session_enabled_capabilities)) => {
            configuration.provider_tool_selections = session_selections
            .iter()
            .map(|(capability, selection)| {
                let capability: ProviderToolCapabilityKind =
                    serde_json::from_value(Value::String(capability.clone())).map_err(|error| {
                        format!(
                            "Session provider tool capability `{capability}` is invalid: {error}"
                        )
                    })?;
                if media_understanding_tool_id(capability).is_some() {
                    return Err(format!(
                        "Session media-understanding capability `{capability:?}` cannot use the direct target store; use the exact media route store."
                    ));
                }
                if !capability.is_direct_selection() {
                    return Err(format!(
                        "Session capability `{capability:?}` cannot use the direct-selection store."
                    ));
                }
                Ok((
                    capability,
                    ProviderToolTargetSelection {
                        connection_id: selection.connection_id.clone(),
                        target_id: selection.target_id.clone(),
                    },
                ))
            })
            .collect::<Result<_, String>>()?;
            configuration.provider_tool_enabled_capabilities = session_enabled_capabilities
            .iter()
            .map(|capability| {
                let capability: ProviderToolCapabilityKind =
                    serde_json::from_value(Value::String(capability.clone())).map_err(|error| {
                        format!(
                            "Session provider tool capability `{capability}` is invalid: {error}"
                        )
                    })?;
                if !capability.is_direct_selection() {
                    return Err(format!(
                        "Session capability `{capability:?}` cannot use the direct enabled-capability store."
                    ));
                }
                Ok(capability)
            })
            .collect::<Result<_, String>>()?;
        }
        (None, None) => {}
        _ => {
            return Err(String::from(
                "Session provider tool selections and enabled capabilities must be supplied together.",
            ));
        }
    }
    if let Some(session_media_understanding_selections) = session_media_understanding_selections {
        configuration.media_understanding_selections = session_media_understanding_selections
            .iter()
            .map(|(capability, selection)| {
                let capability: ProviderToolCapabilityKind =
                    serde_json::from_value(Value::String(capability.clone())).map_err(|error| {
                        format!(
                            "Session media-understanding capability `{capability}` is invalid: {error}"
                        )
                    })?;
                if media_understanding_tool_id(capability).is_none() {
                    return Err(format!(
                        "Session capability `{capability:?}` cannot use the media-understanding route store."
                    ));
                }
                let selection = match selection {
                    vulcan_workbench_model::SessionMediaUnderstandingSelection::ProviderTool {
                        selection,
                    } => MediaUnderstandingBackendSelection::ProviderTool {
                        selection: ProviderToolTargetSelection {
                            connection_id: selection.connection_id.clone(),
                            target_id: selection.target_id.clone(),
                        },
                    },
                    vulcan_workbench_model::SessionMediaUnderstandingSelection::ModelReplacement {
                        selection,
                        reasoning_effort,
                    } => MediaUnderstandingBackendSelection::ModelReplacement {
                        selection: vulcan_provider_adapters::ActiveAiModelSelection {
                            provider_id: selection.provider_id.clone(),
                            model_id: selection.model_id.clone(),
                            connection_id: selection.connection_id.clone(),
                        },
                        reasoning_effort: reasoning_effort.map(Into::into),
                    },
                };
                Ok((capability, selection))
            })
            .collect::<Result<_, String>>()?;
    }
    if let Some(session_extension_target_sets) = session_extension_target_sets {
        configuration.provider_extension_target_sets = session_extension_target_sets
            .iter()
            .map(|(capability, selections)| {
                let capability: ProviderToolCapabilityKind =
                    serde_json::from_value(Value::String(capability.clone())).map_err(|error| {
                        format!(
                            "Session model-extension capability `{capability}` is invalid: {error}"
                        )
                    })?;
                if !capability.is_model_extension() {
                    return Err(format!(
                        "Session capability `{capability:?}` cannot use the model-extension store."
                    ));
                }
                let selections = selections
                    .iter()
                    .map(|selection| ProviderToolTargetSelection {
                        connection_id: selection.connection_id.clone(),
                        target_id: selection.target_id.clone(),
                    })
                    .collect();
                Ok((capability, selections))
            })
            .collect::<Result<_, String>>()?;
    }
    Ok(())
}

/// Builds one immutable candidate for one exact selected target.
/// 为一个精确选定目标构建一个不可变候选项。
///
/// # Parameters
/// 参数
/// The runtime, configuration, workspace, and target are frozen into the invocation closure.
/// 运行时、配置、工作区与目标会冻结到调用闭包中。
///
/// # Returns
/// 返回值
/// An AI-host binding with an invariant media schema or the selected non-media target schema.
/// 返回采用不变媒体 Schema 或所选非媒体目标 Schema 的 AI 宿主绑定。
fn build_direct_provider_tool_candidate(
    runtime: Arc<PublicAiRuntime>,
    configuration: PersistedAiConfiguration,
    workspace_root: Option<PathBuf>,
    selected: AvailableProviderToolTarget,
    media_resource_runtime: Option<Arc<dyn crate::media_resource_runtime::MediaResourceRuntime>>,
) -> Result<VulcanToolCandidate, String> {
    // targetSelection freezes the exact target and credential independently from mutable defaults.
    // targetSelection 独立于可变默认值冻结精确目标与凭据。
    let target_selection = ProviderToolTargetSelection {
        connection_id: selected.connection_id.clone(),
        target_id: selected.target.target_id.clone(),
    };
    // contract remains the internal provider-native execution contract.
    // contract 保持为内部供应商原生执行契约。
    let contract = selected.target.contract.clone();
    // capability chooses whether the AI-facing facade must remain media-invariant.
    // capability 选择 AI 可见外壳是否必须保持媒体不变。
    let capability = contract.capability;
    // toolId never changes when an understanding backend or provider target changes.
    // toolId 在理解后端或供应商目标切换时永不变化。
    let tool_id = media_understanding_tool_id(capability)
        .map(str::to_owned)
        .unwrap_or_else(|| contract.tool_id.clone());
    // parametersSchema hides provider and model controls for every understanding tool.
    // parametersSchema 为全部理解工具隐藏供应商与模型控件。
    let parameters_schema = media_understanding_input_schema(capability)
        .unwrap_or_else(|| contract.input_schema.clone());
    // mediaInputProjection is mandatory internal configuration for a fixed media facade.
    // mediaInputProjection 是固定媒体外壳必需的内部配置。
    let media_input_projection = contract.media_input_projection.clone();
    // description preserves provider-authored facts and derives formats and limits only from the
    // exact operation and upload channels used by this selection.
    // description 保留供应商编写的事实，并只从当前选择使用的精确操作与上传通道派生格式和限制。
    let description = if media_understanding_tool_id(capability).is_some() {
        let profile = runtime
            .provider_tool_media_input_profile(&configuration, capability, &target_selection)?
            .ok_or_else(|| {
                format!(
                    "Media-understanding target `{}` has no media input profile.",
                    selected.target.target_id
                )
            })?;
        direct_media_understanding_description(capability, contract.description.as_str(), &profile)?
    } else {
        contract.description.clone()
    };
    Ok(VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: tool_id.clone(),
            name: tool_id,
            description,
            help_text: None,
            source: VulcanToolSourceKind::AiHostBinding,
        },
        parameters_schema,
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Visible,
        request_streaming: None,
        invoke: Arc::new(move |arguments, context| {
            let runtime = Arc::clone(&runtime);
            let configuration = configuration.clone();
            let workspace_root = workspace_root.clone();
            let target_selection = target_selection.clone();
            let media_input_projection = media_input_projection.clone();
            // The exact host resource resolver follows the invoking session, not a model-selected project.
            // 精确宿主资源解析器遵循发起会话，而非模型选择的项目。
            let media_resource_runtime = media_resource_runtime.clone();
            Box::pin(async move {
                // invoking_session_id remains available after provider dispatch so generated outputs can bind to the same owner.
                // invoking_session_id 在供应商分派后继续保留，使生成输出可以绑定到相同所有者。
                let invoking_session_id = context.session_id;
                let owner_session_id = Some(invoking_session_id.clone());
                let mut cancellation_guard = ProviderTaskCancellationOnDrop::new(
                    Arc::clone(&runtime),
                    owner_session_id.clone(),
                );
                // Resolved sources travel outside JSON while arguments retain only their safe locators.
                // 解析来源在JSON之外传递，参数只保留其安全定位符。
                let (resolved_arguments, media_sources) =
                    if media_understanding_tool_id(capability).is_some() {
                        // Fixed media tools own one explicit resource, including its original filename and digest.
                        // 固定媒体工具持有一个显式资源，包括其原始文件名及摘要。
                        let (resolved, sources) = resolve_media_understanding_resource_arguments(
                            workspace_root.as_deref(),
                            owner_session_id.as_deref(),
                            capability,
                            arguments,
                            media_resource_runtime.as_deref(),
                        )
                        .await?;
                        (resolved, sources)
                    } else {
                        (arguments, Vec::new())
                    };
                // projectedArguments maps the fixed facade only through reviewed target metadata.
                // projectedArguments 仅通过已审核目标元数据映射固定外壳。
                let projected_arguments = if media_understanding_tool_id(capability).is_some() {
                    project_media_understanding_arguments(
                        capability,
                        media_input_projection.as_ref().ok_or_else(|| {
                            format!(
                                "Media-understanding target `{}` has no explicit input projection.",
                                target_selection.target_id
                            )
                        })?,
                        resolved_arguments,
                    )?
                } else {
                    resolved_arguments
                };
                let result = runtime
                    .execute_provider_tool_target_with_media_sources(
                        &configuration,
                        capability,
                        &target_selection,
                        projected_arguments,
                        owner_session_id,
                        vulcan_provider_adapters::ProviderToolResourceContext {
                            workspace_root: workspace_root.as_deref(),
                            media_sources: &media_sources,
                            source_resolver: None,
                        },
                    )
                    .await?;
                // rendered uses one canonical media result shape for understanding capabilities.
                // rendered 为理解能力使用统一规范媒体结果形态。
                let rendered = if media_understanding_tool_id(capability).is_some() {
                    render_media_understanding_provider_result(capability, result)
                } else {
                    render_provider_tool_result(
                        workspace_root.as_deref(),
                        invoking_session_id.as_str(),
                        result,
                    )
                    .await
                };
                cancellation_guard.disarm();
                rendered
            })
        }),
    })
}

/// Stages unified provider resources and builds model/card output.
/// 暂存统一供应商资源并构建模型与卡片输出。
///
/// # Parameters
/// 参数
/// `workspace_root` owns managed attachments, `session_id` identifies their exact conversation owner,
/// and `result` is provider-normalized.
/// `workspace_root` 持有托管附件，`session_id` 标识其精确会话所有者，`result` 已由供应商归一化。
///
/// # Returns
/// 返回值
/// A model-visible local-file result with generated-attachment host metadata.
/// 返回包含生成附件宿主元数据的模型可见本地文件结果。
async fn render_provider_tool_result(
    workspace_root: Option<&Path>,
    session_id: &str,
    result: ProviderToolExecutionResult,
) -> Result<VulcanToolInvocationResult, String> {
    match result.output {
        ProviderToolNormalizedOutput::Text(text) => {
            let payload = json!({
                "kind": "text",
                "text": text,
                "upstream_response_id": result.upstream_response_id
            });
            Ok(
                VulcanToolInvocationResult::from_model_output(payload.clone()).with_host_result(
                    VulcanToolHostResult {
                        kind: String::from("provider_tool_output"),
                        payload,
                    },
                ),
            )
        }
        ProviderToolNormalizedOutput::Resources(resources) => {
            render_provider_resource_result(
                workspace_root,
                session_id,
                result.upstream_response_id,
                resources,
            )
            .await
        }
        ProviderToolNormalizedOutput::Search(search) => {
            let payload = json!({
                "kind": "search",
                "search": search,
                "upstream_response_id": result.upstream_response_id
            });
            Ok(
                VulcanToolInvocationResult::from_model_output(payload.clone()).with_host_result(
                    VulcanToolHostResult {
                        kind: String::from("provider_tool_output"),
                        payload,
                    },
                ),
            )
        }
        ProviderToolNormalizedOutput::Extract(extract) => {
            let payload = json!({
                "kind": "extract",
                "extract": extract,
                "upstream_response_id": result.upstream_response_id
            });
            Ok(
                VulcanToolInvocationResult::from_model_output(payload.clone()).with_host_result(
                    VulcanToolHostResult {
                        kind: String::from("provider_tool_output"),
                        payload,
                    },
                ),
            )
        }
        ProviderToolNormalizedOutput::Embedding(embedding) => {
            let payload = json!({
                "kind": "embedding",
                "embedding": embedding,
                "upstream_response_id": result.upstream_response_id
            });
            Ok(
                VulcanToolInvocationResult::from_model_output(payload.clone()).with_host_result(
                    VulcanToolHostResult {
                        kind: String::from("provider_tool_output"),
                        payload,
                    },
                ),
            )
        }
        ProviderToolNormalizedOutput::Rerank(rerank) => {
            let payload = json!({
                "kind": "rerank",
                "rerank": rerank,
                "upstream_response_id": result.upstream_response_id
            });
            Ok(
                VulcanToolInvocationResult::from_model_output(payload.clone()).with_host_result(
                    VulcanToolHostResult {
                        kind: String::from("provider_tool_output"),
                        payload,
                    },
                ),
            )
        }
        ProviderToolNormalizedOutput::Composite(composite) => {
            let resources =
                stage_provider_resources(workspace_root, session_id, composite.resources).await?;
            let has_generated_resources = !resources.host_resources.is_empty();
            let model_payload = json!({
                "kind": "composite",
                "composite": {
                    "text": composite.text.clone(),
                    "resources": resources.model_resources,
                    "embeddings": composite.embeddings.clone(),
                    "reranks": composite.reranks.clone(),
                    "searches": composite.searches.clone(),
                    "extracts": composite.extracts.clone()
                },
                "upstream_response_id": result.upstream_response_id.clone()
            });
            let host_result = if has_generated_resources {
                VulcanToolHostResult {
                    kind: String::from(GENERATED_ATTACHMENTS_HOST_RESULT_KIND),
                    payload: json!({
                        "outputs": resources.host_resources,
                        "upstream_response_id": result.upstream_response_id
                    }),
                }
            } else {
                VulcanToolHostResult {
                    kind: String::from("provider_tool_output"),
                    payload: json!({
                        "kind": "composite",
                        "composite": {
                            "text": composite.text,
                            "resources": resources.host_resources,
                            "embeddings": composite.embeddings,
                            "reranks": composite.reranks,
                            "searches": composite.searches,
                            "extracts": composite.extracts
                        },
                        "upstream_response_id": result.upstream_response_id
                    }),
                }
            };
            Ok(VulcanToolInvocationResult::from_model_output(model_payload)
                .with_host_result(host_result))
        }
    }
}

/// Separates model-visible managed resource locators from host-private local descriptors.
/// 将模型可见的托管资源定位信息与宿主私有本地描述分离。
#[derive(Debug)]
struct StagedProviderResources {
    /// Resource descriptors safe to return to the model.
    /// 可安全返回给模型的资源描述。
    model_resources: Vec<Value>,
    /// Full canonical attachment descriptors retained only by the host.
    /// 仅由宿主保留的完整规范附件描述。
    host_resources: Vec<ConversationAttachment>,
}

/// Stages unified provider resources and builds model/card output.
/// 暂存统一供应商资源并构建模型与卡片输出。
///
/// # Parameters
/// 参数
/// `workspace_root` owns managed attachments, `session_id` identifies their exact conversation owner,
/// and `upstream_response_id` plus `resources` are provider-normalized.
/// `workspace_root` 持有托管附件，`session_id` 标识其精确会话所有者，
/// `upstream_response_id` 与 `resources` 已由供应商归一化。
///
/// # Returns
/// 返回值
/// A model-visible managed-resource result with generated-attachment host metadata.
/// 返回包含生成附件宿主元数据的模型可见托管资源结果。
async fn render_provider_resource_result(
    workspace_root: Option<&Path>,
    session_id: &str,
    upstream_response_id: Option<String>,
    resources: Vec<vulcan_provider_adapters::ProviderToolGeneratedResource>,
) -> Result<VulcanToolInvocationResult, String> {
    let resources = stage_provider_resources(workspace_root, session_id, resources).await?;
    let payload = json!({
        "outputs": resources.host_resources,
        "upstream_response_id": upstream_response_id
    });
    Ok(VulcanToolInvocationResult::from_model_output(json!({
        "resources": resources.model_resources
    }))
    .with_host_result(VulcanToolHostResult {
        kind: String::from(GENERATED_ATTACHMENTS_HOST_RESULT_KIND),
        payload,
    }))
}

/// Stages normalized provider resources and returns unified local descriptors.
/// 暂存归一化供应商资源并返回统一本地描述。
///
/// # Parameters
/// 参数
/// `workspace_root` owns managed attachments, `session_id` identifies their exact conversation owner,
/// and `resources` are provider-normalized.
/// `workspace_root` 持有托管附件，`session_id` 标识其精确会话所有者，`resources` 已由供应商归一化。
///
/// # Returns
/// 返回值
/// Ordered public and private descriptors for model output and host cards.
/// 返回分别用于模型输出与宿主卡片的有序公开及私有描述。
async fn stage_provider_resources(
    workspace_root: Option<&Path>,
    session_id: &str,
    resources: Vec<vulcan_provider_adapters::ProviderToolGeneratedResource>,
) -> Result<StagedProviderResources, String> {
    if resources.is_empty() {
        return Ok(StagedProviderResources {
            model_resources: Vec::new(),
            host_resources: Vec::new(),
        });
    }
    let workspace_root = workspace_root.ok_or_else(|| {
        String::from("Provider tools that generate resources require an active workspace root.")
    })?;
    let workspace_text = vulcan_runtime_assets::native_host_path_text_from_path(workspace_root);
    let mut model_resources = Vec::with_capacity(resources.len());
    let mut host_resources = Vec::with_capacity(resources.len());
    // staging_result keeps every staged item private until the complete batch has passed validation.
    // staging_result 在整批通过校验前保持每个暂存项不可披露。
    let staging_result: Result<(), String> = async {
        for resource in resources {
            let descriptor = attachment_descriptor_from_mime_type(resource.mime_type.as_str())
                .ok_or_else(|| {
                    format!(
                        "Provider tool resource `{}` has unsupported MIME type `{}`.",
                        resource.output_id, resource.mime_type
                    )
                })?;
            let suggested_name = format!("{}.{}", resource.output_id, descriptor.extension);
            let attachment = match (
                resource.bytes,
                resource.download_url.as_deref(),
                resource.download_policy.as_ref(),
            ) {
                (Some(bytes), None, None) => stage_attachment_bytes(
                    workspace_text.as_str(),
                    &descriptor,
                    suggested_name.as_str(),
                    None,
                    bytes.as_slice(),
                )?,
                (None, Some(download_url), Some(download_policy)) => {
                    let temporary_path = download_provider_resource_to_temp(
                        download_url,
                        resource.kind.as_str(),
                        download_policy,
                    )
                    .await?;
                    stage_attachment_file_streaming(
                        workspace_text.as_str(),
                        &descriptor,
                        suggested_name.as_str(),
                        temporary_path.as_ref(),
                    )?
                }
                _ => {
                    return Err(format!(
                        "Provider tool resource `{}` has an invalid carrier.",
                        resource.output_id
                    ));
                }
            };
            // The attachment enters the rollback set before any post-staging invariant is checked.
            // attachment 在任何暂存后不变量校验之前进入回滚集合。
            host_resources.push(attachment);
            let attachment = host_resources.last().ok_or_else(|| {
                String::from("Staged provider resource batch lost its last item.")
            })?;
            let asset_id = attachment.asset_id.as_deref().ok_or_else(|| {
                String::from("Staged provider resource is missing its managed asset identifier.")
            })?;
            attachment.resource_path.as_deref().ok_or_else(|| {
                String::from("Staged provider resource is missing its managed resource path.")
            })?;
            // resource_path is the opaque conversation-owned locator disclosed to the model.
            // resource_path 是向模型披露的不透明会话所有定位符。
            let resource_path = format!("attachment://{asset_id}");
            model_resources.push(json!({
                "kind": resource.kind,
                "asset_id": asset_id,
                "resource_path": resource_path,
                "file_name": attachment.file_name.as_str(),
                "mime_type": attachment.mime_type.as_str(),
                "byte_len": attachment.byte_len
            }));
        }
        Ok(())
    }
    .await;
    if let Err(error) = staging_result {
        return Err(rollback_staged_provider_resources(
            workspace_text.as_str(),
            host_resources.as_slice(),
            error,
        ));
    }
    // Ownership is committed before the locator becomes model-visible, so the next tool call can resolve it immediately.
    // 所有权在定位符对模型可见前完成提交，使下一次工具调用可以立即解析该定位符。
    if let Err(error) = retain_generated_attachments_for_session(
        workspace_text.as_str(),
        session_id,
        host_resources.as_slice(),
    ) {
        return Err(rollback_staged_provider_resources(
            workspace_text.as_str(),
            host_resources.as_slice(),
            error,
        ));
    }
    Ok(StagedProviderResources {
        model_resources,
        host_resources,
    })
}

/// Rolls back every uncommitted resource in one failed provider-output batch.
/// 回滚一次失败供应商输出批次中的全部未提交资源。
///
/// # Parameters
/// 参数
/// `workspace_path` owns the project-scoped attachment index, `attachments` is the exact staged
/// batch, and `primary_error` is the failure that aborted staging or ownership retention.
/// `workspace_path` 持有项目级附件索引，`attachments` 是精确暂存批次，`primary_error` 是中止暂存或
/// 所有权保留的失败。
///
/// # Returns
/// 返回值
/// The primary failure, augmented with every rollback failure without hiding the original cause.
/// 返回原始失败，并在不隐藏根因的前提下附加每项回滚失败。
fn rollback_staged_provider_resources(
    workspace_path: &str,
    attachments: &[ConversationAttachment],
    primary_error: String,
) -> String {
    // rollback_errors retains exact cleanup failures while reverse order mirrors resource creation.
    // rollback_errors 保留精确清理失败，逆序处理则与资源创建顺序相反。
    let mut rollback_errors = Vec::new();
    for attachment in attachments.iter().rev() {
        match discard_unreferenced_staged_attachment(workspace_path, attachment) {
            Ok(true) => {}
            Ok(false) => rollback_errors.push(format!(
                "staged asset `{}` was not discarded",
                attachment.asset_id.as_deref().unwrap_or("missing")
            )),
            Err(error) => rollback_errors.push(error),
        }
    }
    if rollback_errors.is_empty() {
        primary_error
    } else {
        format!(
            "{primary_error} Provider resource batch rollback also failed: {}",
            rollback_errors.join("; ")
        )
    }
}

/// Downloads one bounded HTTPS provider resource into an automatically cleaned temporary file.
/// 将一个有界 HTTPS 供应商资源下载到会自动清理的临时文件。
///
/// # Parameters
/// 参数
/// `download_url` is the provider-normalized locator, `resource_kind` selects the host byte limit,
/// and `download_policy` carries the frozen manifest network authority.
/// `download_url` 是供应商归一化后的定位符，`resource_kind` 选择宿主字节上限，
/// `download_policy` 携带冻结的 Manifest 网络权限。
///
/// # Returns
/// 返回值
/// A temporary path containing non-empty content below the configured limit.
/// 返回包含低于配置上限的非空内容的临时路径。
async fn download_provider_resource_to_temp(
    download_url: &str,
    resource_kind: &str,
    download_policy: &ProviderToolResourceDownloadPolicy,
) -> Result<tempfile::TempPath, String> {
    // The host revalidates the frozen adapter authority immediately before network access.
    // 宿主在网络访问前立即复验适配器冻结的权限。
    let parsed_url = download_policy.validate_url(download_url, "Provider resource")?;
    let byte_limit = match resource_kind {
        "audio" => PROVIDER_TOOL_AUDIO_OUTPUT_BYTE_LIMIT,
        "video" => PROVIDER_TOOL_VIDEO_OUTPUT_BYTE_LIMIT,
        _ => PROVIDER_TOOL_DEFAULT_OUTPUT_BYTE_LIMIT,
    };
    // Redirects are disabled because the provider URL was validated before this request and a redirect could otherwise bypass that boundary.
    // 禁止重定向，因为供应商 URL 已在请求前校验；否则重定向可能绕过该边界。
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(PROVIDER_TOOL_RESOURCE_DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|error| {
            format!("Provider resource download client could not be created: {error}")
        })?;
    let response = client
        .get(parsed_url)
        .send()
        .await
        .map_err(|error| format!("Provider resource download failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Provider resource download failed with HTTP {}.",
            response.status().as_u16()
        ));
    }
    if response
        .content_length()
        .is_some_and(|content_length| content_length > byte_limit as u64)
    {
        return Err(String::from(
            "Provider resource exceeds the host limit for its declared kind.",
        ));
    }
    let temporary_file = tempfile::NamedTempFile::new().map_err(|error| {
        format!("Provider resource temporary file could not be created: {error}")
    })?;
    let temporary_path = temporary_file.into_temp_path();
    let temporary_path_ref: &Path = temporary_path.as_ref();
    let mut file = tokio::fs::File::create(temporary_path_ref)
        .await
        .map_err(|error| {
            format!("Provider resource temporary file could not be opened: {error}")
        })?;
    let mut stream = response.bytes_stream();
    let mut downloaded_bytes = 0_usize;
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|error| format!("Provider resource body could not be read: {error}"))?;
        downloaded_bytes = downloaded_bytes.saturating_add(chunk.len());
        if downloaded_bytes > byte_limit {
            return Err(String::from(
                "Provider resource exceeds the host limit for its declared kind.",
            ));
        }
        file.write_all(chunk.as_ref()).await.map_err(|error| {
            format!("Provider resource temporary file could not be written: {error}")
        })?;
    }
    if downloaded_bytes == 0 {
        return Err(String::from("Provider resource cannot be empty."));
    }
    file.flush().await.map_err(|error| {
        format!("Provider resource temporary file could not be flushed: {error}")
    })?;
    file.sync_all().await.map_err(|error| {
        format!("Provider resource temporary file could not be synchronized: {error}")
    })?;
    Ok(temporary_path)
}

#[cfg(test)]
mod tests {
    use super::{
        MODEL_EXTENSION_CAPABILITIES, ProviderTaskCancellationOnDrop,
        apply_session_provider_tool_selections, build_selected_model_extension_catalog,
        download_provider_resource_to_temp, model_extension_executor_schema,
        model_extension_parameters_schema, model_extension_tool_id,
        render_model_extension_parameters, render_provider_resource_result,
        render_provider_tool_result, stage_provider_resources,
        validate_enabled_media_understanding_backends,
    };
    use serde_json::json;
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;
    use vulcan_ai_contract::ReasoningEffort;
    use vulcan_core::types::ConversationAttachmentKind;
    use vulcan_provider_adapters::{
        AvailableProviderToolTarget, ManifestSourceLayer, ProviderToolCapabilityKind,
        ProviderToolCompositeOutput, ProviderToolEmbeddingOutput, ProviderToolEmbeddingVector,
        ProviderToolExecutionResult, ProviderToolGeneratedResource, ProviderToolNormalizedOutput,
        ProviderToolRerankOutput, ProviderToolRerankResult, ProviderToolResourceDownloadPolicy,
        ProviderToolResourceEncoding, ProviderToolTarget, ProviderToolTargetSelection,
        PublicAiRuntime,
    };

    /// Builds one minimal model-extension target for bridge catalog tests.
    /// 为 bridge 目录测试构建一个最小模型扩展目标。
    ///
    /// # Parameters
    /// 参数
    /// The identifiers and source layer define the exact target and account provenance.
    /// 各标识与来源层定义精确目标及账号溯源。
    ///
    /// # Returns
    /// 返回值
    /// One frozen available target with a reviewed profile and closed schema.
    /// 返回一条带已审核说明与封闭 Schema 的冻结可用目标。
    fn available_extension_target(
        provider_id: &str,
        model_id: &str,
        target_id: &str,
        connection_id: &str,
        source_layer: ManifestSourceLayer,
        capability: ProviderToolCapabilityKind,
    ) -> AvailableProviderToolTarget {
        let tool_id = model_extension_tool_id(capability);
        let capability_value =
            serde_json::to_value(capability).expect("test provider capability should serialize");
        // target is decoded through the same public wire shape used by compiled manifests.
        // target 通过与已编译 Manifest 相同的公共 Wire 结构解码。
        let target: ProviderToolTarget = serde_json::from_value(json!({
            "target_id": target_id,
            "provider_id": provider_id,
            "service_entry_id": "public-api",
            "credential_profile_ids": ["api-key"],
            "upstream_target_id": model_id,
            "display_name": model_id,
            "contract": {
                "tool_id": tool_id,
                "description": "Execute one test model extension.",
                "capability": capability_value,
                "input_schema": {
                    "type": "object",
                    "properties": {"prompt": {"type": "string"}},
                    "required": ["prompt"],
                    "additionalProperties": false
                },
                "extension_profile": {"summary": "Reviewed native extension model."},
                "output": {
                    "kind": "text",
                    "source_path": "/text",
                    "normalized_result_path": "text"
                }
            },
            "execution": {
                "endpoint_path": "/images/generations",
                "authentication": "bearer_authorization",
                "adapter": "generic_json",
                "execution_kind": "http_json_sync"
            }
        }))
        .expect("test provider target should decode");
        AvailableProviderToolTarget {
            connection_id: connection_id.to_string(),
            connection_display_name: connection_id.to_string(),
            manifest_source_layer: source_layer,
            target,
        }
    }

    /// Verifies the stable extension facade never embeds selected model schemas in tool identity.
    /// 验证稳定扩展外壳绝不会把已选模型 Schema 嵌入工具身份。
    #[test]
    fn model_extension_facade_ids_and_schemas_are_invariant() {
        // expectedIds is the closed generation-capability executor inventory.
        // expectedIds 是生成类能力执行工具的封闭清单。
        let expected_ids = [
            "image_generate",
            "image_edit",
            "video_generate",
            "video_edit",
            "video_extend",
            "speech_synthesize",
            "speech_transcribe",
            "music_generate",
            "music_cover",
        ];
        // actualIds depends only on the capability enum and never on user selection.
        // actualIds 仅依赖能力枚举，绝不依赖用户选择。
        let actual_ids = MODEL_EXTENSION_CAPABILITIES.map(model_extension_tool_id);
        assert_eq!(actual_ids, expected_ids);
        assert_eq!(
            model_extension_executor_schema(),
            json!({
                "type": "object",
                "properties": {
                    "model": {
                        "type": "string",
                        "minLength": 3,
                        "description": "Exact provider/model identifier returned by get_model_extension_parameters."
                    },
                    "args": {
                        "type": "object",
                        "description": "Exact model-native arguments returned by get_model_extension_parameters."
                    }
                },
                "required": ["model", "args"],
                "additionalProperties": false
            })
        );
        assert_eq!(
            model_extension_parameters_schema()["required"],
            json!(["capability"])
        );
    }

    /// Verifies custom manifests and stale selections never enter the model-visible extension catalog.
    /// 验证自定义 Manifest 与过期选择绝不会进入模型可见扩展目录。
    #[test]
    fn model_extension_catalog_accepts_only_selected_native_targets() {
        // nativeTarget is the sole bundled contract permitted to reach the model.
        // nativeTarget 是唯一允许到达模型的内置契约。
        let native_target = available_extension_target(
            "native",
            "image-model",
            "native-image",
            "native-account",
            ManifestSourceLayer::Bundled,
            ProviderToolCapabilityKind::ImageGenerate,
        );
        // customTarget must remain excluded even when a forged persisted selection references it.
        // customTarget 即使被伪造持久化选择引用也必须保持排除。
        let custom_target = available_extension_target(
            "custom",
            "image-model",
            "custom-image",
            "custom-account",
            ManifestSourceLayer::Custom,
            ProviderToolCapabilityKind::ImageGenerate,
        );
        // configuration owns one native, one custom, and one stale selection.
        // configuration 持有一条原生、一条自定义与一条过期选择。
        let mut configuration = vulcan_provider_adapters::PersistedAiConfiguration::default();
        configuration.provider_extension_target_sets.insert(
            ProviderToolCapabilityKind::ImageGenerate,
            BTreeSet::from([
                ProviderToolTargetSelection {
                    connection_id: String::from("native-account"),
                    target_id: String::from("native-image"),
                },
                ProviderToolTargetSelection {
                    connection_id: String::from("custom-account"),
                    target_id: String::from("custom-image"),
                },
                ProviderToolTargetSelection {
                    connection_id: String::from("stale-account"),
                    target_id: String::from("stale-image"),
                },
            ]),
        );
        // catalog is the exact model-visible result after provenance and availability filtering.
        // catalog 是经过溯源与可用性过滤后的精确模型可见结果。
        let catalog =
            build_selected_model_extension_catalog(&configuration, &[native_target, custom_target])
                .expect("native extension catalog should build");
        // selected contains only the bundled native model.
        // selected 仅包含内置原生模型。
        let selected = catalog
            .get(&ProviderToolCapabilityKind::ImageGenerate)
            .expect("native image capability should remain");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].target.provider_id, "native");
    }

    /// Speech and video discovery expose only their canonical callable model identifiers.
    /// 语音与视频参数发现仅暴露各自的规范可调用模型标识。
    #[test]
    fn speech_and_video_parameters_expose_only_canonical_callable_models() {
        let cases = [
            (
                ProviderToolCapabilityKind::SpeechSynthesize,
                "speech_synthesize",
                "speech-2.8-hd",
                "cn.speech-2.8-hd",
            ),
            (
                ProviderToolCapabilityKind::VideoGenerate,
                "video_generate",
                "MiniMax-Hailuo-2.3",
                "cn.MiniMax-Hailuo-2.3",
            ),
        ];

        for (capability, capability_name, model_id, target_id) in cases {
            let target = available_extension_target(
                "minimax",
                model_id,
                target_id,
                "minimax-account",
                ManifestSourceLayer::Bundled,
                capability,
            );
            let catalog = BTreeMap::from([(capability, vec![target])]);
            let result = render_model_extension_parameters(
                &catalog,
                json!({ "capability": capability_name }),
            )
            .expect("selected MiniMax extension parameters should render");
            let model = &result.model_output["models"][0];

            assert_eq!(model["model"], format!("minimax/{model_id}"));
            assert!(model.get("provider_id").is_none());
            assert!(model.get("model_id").is_none());
            assert!(
                result.model_output["instruction"]
                    .as_str()
                    .expect("instruction must be text")
                    .contains("only valid callable model identifier")
            );
        }
    }

    /// A normally returned provider failure cannot arm owner-wide sibling cancellation.
    /// 正常返回的 Provider 失败不能触发所有者级兄弟工具取消。
    #[test]
    fn normal_provider_failure_disarms_owner_cancellation_guard() {
        let runtime = Arc::new(PublicAiRuntime::unavailable().expect("test runtime should build"));
        let mut guard = ProviderTaskCancellationOnDrop::new(
            runtime,
            Some(String::from("parallel-tool-session")),
        );

        let result: Result<(), String> =
            guard.complete(Err(String::from("one tool failed normally")));

        assert_eq!(result, Err(String::from("one tool failed normally")));
        assert!(!guard.armed);
    }

    /// Verifies Agent Runtime production input materialization has no provider-named branch.
    /// 验证 Agent Runtime 生产输入物化不存在供应商命名分支。
    ///
    /// The release gate performs the same repository-wide check; this focused R0 guard keeps the
    /// bridge invariant independently runnable with the crate tests.
    /// 发布门禁会执行同类仓库级检查；本聚焦 R0 守卫使 bridge 不变量可随 crate 测试独立运行。
    #[test]
    fn agent_runtime_has_no_provider_named_input_materialization() {
        // source includes this module so the production prefix is cut at the first test boundary.
        // source 包含当前测试模块，因此在首个测试边界处截取生产前缀。
        let source = include_str!("provider_tool_bridge.rs");
        // production contains only the bridge code that ships in non-test builds.
        // production 只包含非测试构建实际发布的 bridge 代码。
        let production = source
            .split_once("#[cfg(test)]")
            .map(|(production, _)| production)
            .expect("provider tool bridge must retain one explicit test boundary");
        // forbidden_fragments are assembled so the guard strings cannot satisfy themselves.
        // forbidden_fragments 通过分段组装，避免守卫字符串自身命中检查。
        let forbidden_fragments = [
            ["materialize_", "provider_tool_inputs"].concat(),
            ["Ali", "baba"].concat(),
            ["Mini", "Max"].concat(),
            ["Tavi", "ly"].concat(),
            ["ProviderTool", "AdapterKind"].concat(),
        ];
        for forbidden in forbidden_fragments {
            assert!(
                !production.contains(forbidden.as_str()),
                "Agent provider-tool bridge restored forbidden production input authority `{forbidden}`"
            );
        }
    }

    /// Verifies an explicit empty session selection disables every global provider tool.
    /// 验证显式空会话选择会禁用全部全局供应商工具。
    #[test]
    fn explicit_empty_session_selection_clears_global_targets() {
        let mut configuration = vulcan_provider_adapters::PersistedAiConfiguration::default();
        configuration.provider_tool_selections.insert(
            vulcan_provider_adapters::ProviderToolCapabilityKind::WebSearch,
            vulcan_provider_adapters::ProviderToolTargetSelection {
                connection_id: String::from("connection"),
                target_id: String::from("target"),
            },
        );

        apply_session_provider_tool_selections(
            &mut configuration,
            &Some(Default::default()),
            &Some(Default::default()),
            &None,
            &None,
        )
        .expect("apply explicit empty override");

        assert!(configuration.provider_tool_selections.is_empty());
        assert!(configuration.provider_tool_enabled_capabilities.is_empty());
    }

    /// Verifies an enabled media capability survives without a legacy session target selection.
    /// 验证已启用媒体能力无需旧会话目标选择也能保留。
    #[test]
    fn session_media_capability_is_independent_from_direct_target_selection() {
        // configuration starts from empty global switches so only the explicit session override is observable.
        // configuration 从空全局开关开始，使测试仅观察显式会话覆盖。
        let mut configuration = vulcan_provider_adapters::PersistedAiConfiguration::default();

        apply_session_provider_tool_selections(
            &mut configuration,
            &Some(Default::default()),
            &Some(BTreeSet::from([String::from("video_understand")])),
            &None,
            &None,
        )
        .expect("apply media capability override");

        assert!(configuration.provider_tool_selections.is_empty());
        assert_eq!(
            configuration.provider_tool_enabled_capabilities,
            BTreeSet::from([ProviderToolCapabilityKind::VideoUnderstand])
        );
    }

    /// Verifies one exact session media route replaces the global default completely.
    /// 验证一条精确会话媒体路由会完整替换全局默认值。
    #[test]
    fn session_media_selection_owns_the_requested_backend_route() {
        let capability = ProviderToolCapabilityKind::VideoUnderstand;
        let replacement_selection = vulcan_provider_adapters::ActiveAiModelSelection {
            provider_id: String::from("replacement-provider"),
            model_id: String::from("replacement-video-model"),
            connection_id: String::from("replacement-account"),
        };
        let mut configuration = vulcan_provider_adapters::PersistedAiConfiguration::default();
        let media_selections = BTreeMap::from([(
            String::from("video_understand"),
            vulcan_workbench_model::SessionMediaUnderstandingSelection::ModelReplacement {
                selection: vulcan_workbench_model::SessionMediaUnderstandingModelSelection {
                    provider_id: replacement_selection.provider_id.clone(),
                    model_id: replacement_selection.model_id.clone(),
                    connection_id: replacement_selection.connection_id.clone(),
                },
                reasoning_effort: Some(
                    vulcan_workbench_model::SessionMediaUnderstandingReasoningEffort::High,
                ),
            },
        )]);

        apply_session_provider_tool_selections(
            &mut configuration,
            &Some(Default::default()),
            &Some(BTreeSet::from([String::from("video_understand")])),
            &None,
            &Some(media_selections),
        )
        .expect("session replacement mode should select its stored route");

        assert_eq!(
            configuration
                .media_understanding_selections
                .get(&capability),
            Some(
                &vulcan_provider_adapters::MediaUnderstandingBackendSelection::ModelReplacement {
                    selection: replacement_selection,
                    reasoning_effort: Some(ReasoningEffort::High),
                }
            )
        );
        assert_eq!(
            configuration.provider_tool_enabled_capabilities,
            BTreeSet::from([capability])
        );
        assert!(configuration.provider_tool_selections.is_empty());
    }

    /// Verifies provider-tool execution never reintroduces the system-default multimodal candidate gate.
    /// 验证供应商工具执行绝不会重新引入系统默认多模态候选门禁。
    #[test]
    fn session_media_execution_has_no_global_candidate_switch_gate() {
        // source includes tests, so production isolates the exact code compiled outside this module.
        // source 包含测试，因此 production 隔离当前模块之外实际编译的精确代码。
        let source = include_str!("provider_tool_bridge.rs");
        // production ends at the stable test boundary and cannot match this guard's own fragments.
        // production 截止于稳定测试边界，不会命中当前守卫自身的分段字符串。
        let production = source
            .split_once("#[cfg(test)]")
            .map(|(production, _)| production)
            .expect("provider tool bridge must retain one explicit test boundary");
        // forbidden_gate names the global system-default predicate without embedding it literally.
        // forbidden_gate 通过分段命名全局系统默认谓词，避免字面量自命中。
        let forbidden_gate = ["multimodal_understanding_", "candidate_", "is_enabled"].concat();

        assert!(
            !production.contains(forbidden_gate.as_str()),
            "session media execution must validate only its exact frozen route"
        );
    }

    /// Verifies a media switch without its required exact route fails before tool exposure.
    /// 验证缺少必需精确路由的媒体开关会在工具暴露前失败。
    #[test]
    fn enabled_media_capability_requires_exact_backend_route() {
        // configuration simulates a session switch whose exact route is missing.
        // configuration 模拟缺少精确路由的会话开关。
        let mut configuration = vulcan_provider_adapters::PersistedAiConfiguration::default();
        configuration
            .provider_tool_enabled_capabilities
            .insert(ProviderToolCapabilityKind::VideoUnderstand);

        let error = validate_enabled_media_understanding_backends(&configuration)
            .expect_err("enabled video understanding must require one exact route");

        assert!(error.contains("VideoUnderstand"), "{error}");
        assert!(error.contains("exact session backend route"), "{error}");
    }

    /// Verifies a legacy session media target cannot create a second backend authority.
    /// 验证旧版会话媒体目标无法形成第二个后端权威。
    #[test]
    fn session_media_target_selection_is_rejected() {
        // configuration starts without any direct target so the attempted session write is isolated.
        // configuration 从不含直连目标的状态开始，以隔离本次会话写入尝试。
        let mut configuration = vulcan_provider_adapters::PersistedAiConfiguration::default();
        // selections imitates one stale pre-schema-18 session route.
        // selections 模拟一条过期的 Schema 18 之前会话路由。
        let selections = std::collections::BTreeMap::from([(
            String::from("video_understand"),
            vulcan_workbench_model::SessionProviderToolTargetSelection {
                connection_id: String::from("legacy-account"),
                target_id: String::from("legacy-video-target"),
            },
        )]);
        // enabledCapabilities keeps the paired session override structurally complete.
        // enabledCapabilities 使配对的会话覆盖在结构上保持完整。
        let enabled_capabilities = BTreeSet::from([String::from("video_understand")]);

        // error proves the stale session route is rejected before it can mutate configuration.
        // error 证明过期会话路由会在修改配置之前被拒绝。
        let error = apply_session_provider_tool_selections(
            &mut configuration,
            &Some(selections),
            &Some(enabled_capabilities),
            &None,
            &None,
        )
        .expect_err("legacy session media target must fail closed");

        assert!(
            error.contains("cannot use the direct target store"),
            "{error}"
        );
        assert!(configuration.provider_tool_selections.is_empty());
    }

    /// Verifies an explicit empty session extension set disables every global extension model.
    /// 验证显式空会话扩展集合会禁用全部全局扩展模型。
    #[test]
    fn explicit_empty_session_extension_set_clears_global_models() {
        // configuration starts with one globally selected image-generation extension target.
        // configuration 初始持有一个全局已选图片生成扩展目标。
        let mut configuration = vulcan_provider_adapters::PersistedAiConfiguration::default();
        configuration.provider_extension_target_sets.insert(
            ProviderToolCapabilityKind::ImageGenerate,
            BTreeSet::from([ProviderToolTargetSelection {
                connection_id: String::from("connection"),
                target_id: String::from("target"),
            }]),
        );

        apply_session_provider_tool_selections(
            &mut configuration,
            &None,
            &None,
            &Some(Default::default()),
            &None,
        )
        .expect("apply explicit empty extension override");

        assert!(configuration.provider_extension_target_sets.is_empty());
    }

    /// Verifies the shared resource boundary rejects insecure provider URLs before networking.
    /// 验证共享资源边界会在发起网络请求前拒绝不安全的供应商 URL。
    #[tokio::test]
    async fn provider_resource_download_rejects_insecure_url() {
        // The HTTPS-only policy is the exact authority under test.
        // 仅 HTTPS 策略是本测试使用的精确权限。
        let policy = ProviderToolResourceDownloadPolicy {
            encoding: ProviderToolResourceEncoding::HttpsUrl,
            host_suffixes: vec![String::from("example.com")],
        };
        let error =
            download_provider_resource_to_temp("http://example.com/output.png", "image", &policy)
                .await
                .expect_err("insecure URL must be rejected");
        assert_eq!(
            error,
            "Provider resource URL host `example.com` is outside the declared scheme, default port or exact operation origin, and host suffixes."
        );
    }

    /// Verifies a syntactically valid HTTPS URL cannot escape its manifest-owned host authority.
    /// 验证语法有效的 HTTPS URL 无法逃逸 Manifest 持有的宿主权限。
    #[tokio::test]
    async fn provider_resource_download_rejects_undeclared_host() {
        // The exact host matcher intentionally excludes every subdomain and alternate domain.
        // 精确宿主匹配项刻意排除全部子域与其他域名。
        let policy = ProviderToolResourceDownloadPolicy {
            encoding: ProviderToolResourceEncoding::HttpsUrl,
            host_suffixes: vec![String::from("example.com")],
        };
        let error = download_provider_resource_to_temp(
            "https://assets.example.org/output.png",
            "image",
            &policy,
        )
        .await
        .expect_err("undeclared host must be rejected before networking");
        assert_eq!(
            error,
            "Provider resource URL host `assets.example.org` is outside the declared scheme, default port or exact operation origin, and host suffixes."
        );
    }

    /// Verifies embedding output remains typed in both model and host payloads.
    /// 验证向量输出在模型与宿主载荷中均保持强类型。
    #[tokio::test]
    async fn embedding_output_preserves_vector_indexes_and_values() {
        let result = render_provider_tool_result(
            None,
            "embedding-session",
            ProviderToolExecutionResult {
                upstream_response_id: Some(String::from("embedding-response")),
                output: ProviderToolNormalizedOutput::Embedding(ProviderToolEmbeddingOutput {
                    model: Some(String::from("embedding-model")),
                    vectors: vec![ProviderToolEmbeddingVector {
                        index: 2,
                        values: vec![0.25, -0.5],
                    }],
                }),
            },
        )
        .await
        .expect("embedding output");

        assert_eq!(result.model_output["kind"], "embedding");
        assert_eq!(result.model_output["embedding"]["vectors"][0]["index"], 2);
        assert_eq!(
            result.host_result.as_ref().expect("host output").payload["embedding"]["vectors"][0]["values"],
            json!([0.25, -0.5])
        );
    }

    /// Verifies a resource-free composite output does not require a workspace.
    /// 验证不含资源的复合输出不要求工作区。
    #[tokio::test]
    async fn resource_free_composite_output_keeps_each_semantic_kind() {
        let result = render_provider_tool_result(
            None,
            "composite-session",
            ProviderToolExecutionResult {
                upstream_response_id: None,
                output: ProviderToolNormalizedOutput::Composite(ProviderToolCompositeOutput {
                    text: vec![String::from("summary")],
                    resources: Vec::new(),
                    embeddings: Vec::new(),
                    reranks: vec![ProviderToolRerankOutput {
                        model: None,
                        results: vec![ProviderToolRerankResult {
                            index: 1,
                            score: 0.75,
                            document: Some(String::from("candidate")),
                        }],
                    }],
                    searches: Vec::new(),
                    extracts: Vec::new(),
                }),
            },
        )
        .await
        .expect("composite output");

        assert_eq!(result.model_output["kind"], "composite");
        assert_eq!(result.model_output["composite"]["text"], json!(["summary"]));
        assert_eq!(
            result.model_output["composite"]["reranks"][0]["results"][0]["score"],
            0.75
        );
    }

    /// Verifies model-visible generated resources never expose host-private absolute paths.
    /// 验证模型可见的生成资源绝不暴露宿主私有绝对路径。
    #[tokio::test]
    async fn generated_resource_output_separates_public_locator_from_private_host_path() {
        let workspace = tempfile::tempdir().expect("temporary workspace");
        // session_id is the exact owner expected to resolve the generated resource.
        // session_id 是预期能够解析生成资源的精确所有者。
        let session_id = "generated-resource-session";
        let result = render_provider_resource_result(
            Some(workspace.path()),
            session_id,
            Some(String::from("generated-response")),
            vec![ProviderToolGeneratedResource {
                output_id: String::from("generated-image"),
                kind: String::from("image"),
                mime_type: String::from("image/png"),
                bytes: Some(vec![0x89, b'P', b'N', b'G']),
                download_url: None,
                download_policy: None,
            }],
        )
        .await
        .expect("generated resource output");

        let model_resource = &result.model_output["resources"][0];
        let model_text = serde_json::to_string(&result.model_output).expect("model output json");
        let host_output = &result
            .host_result
            .as_ref()
            .expect("generated attachment host result")
            .payload["outputs"][0];

        assert!(model_resource["asset_id"].as_str().is_some());
        // asset_id is the stable managed identity returned without any private path.
        // asset_id 是不携带任何私有路径返回的稳定托管身份。
        let asset_id = model_resource["asset_id"].as_str().expect("model asset id");
        // resource_path is the exact model-visible locator reused by understanding tools.
        // resource_path 是理解工具复用的精确模型可见定位符。
        let resource_path = model_resource["resource_path"]
            .as_str()
            .expect("model resource locator");
        assert_eq!(resource_path, format!("attachment://{asset_id}"));
        assert!(model_resource.get("local_path").is_none());
        assert!(model_resource.get("preview_local_path").is_none());
        assert!(!model_text.contains(workspace.path().to_string_lossy().as_ref()));
        assert!(host_output["local_path"].as_str().is_some());
        assert_eq!(host_output["asset_id"], model_resource["asset_id"]);
        assert!(
            host_output["resource_path"]
                .as_str()
                .is_some_and(|value| value.starts_with(".vulcan/code/attachments/images/"))
        );
        // workspace_text preserves the native path contract used by the attachment index.
        // workspace_text 保留附件索引使用的原生路径契约。
        let workspace_text =
            vulcan_runtime_assets::native_host_path_text_from_path(workspace.path());
        // resolved proves the invoking session can immediately reuse the generated attachment.
        // resolved 证明发起会话可以立即复用生成附件。
        let resolved = crate::attachment_asset_index::resolve_owned_attachment_resource(
            workspace_text.as_str(),
            session_id,
            resource_path,
            ConversationAttachmentKind::Image,
        )
        .expect("generated resource owned by invoking session");
        assert_eq!(resolved.asset_id.as_deref(), Some(asset_id));
        // foreign_error proves the same opaque locator cannot cross the conversation boundary.
        // foreign_error 证明相同不透明定位符无法跨越会话边界。
        let foreign_error = crate::attachment_asset_index::resolve_owned_attachment_resource(
            workspace_text.as_str(),
            "other-session",
            resource_path,
            ConversationAttachmentKind::Image,
        )
        .expect_err("generated resource must remain conversation-scoped");
        assert_eq!(
            foreign_error,
            "Attachment resource is not owned by the invoking conversation."
        );
    }

    /// Verifies a later resource validation failure removes every earlier staged file and index record.
    /// 验证后续资源校验失败会移除此前全部暂存文件与索引记录。
    #[tokio::test]
    async fn generated_resource_batch_rolls_back_all_prior_staging() {
        // workspace isolates the managed attachment tree from user projects.
        // workspace 将托管附件树与用户项目隔离。
        let workspace = tempfile::tempdir().expect("temporary workspace");
        // resources places one valid item before the deterministic unsupported-MIME failure.
        // resources 在确定性的 MIME 不支持失败前放置一个有效项。
        let resources = vec![
            ProviderToolGeneratedResource {
                output_id: String::from("first-image"),
                kind: String::from("image"),
                mime_type: String::from("image/png"),
                bytes: Some(vec![0x89, b'P', b'N', b'G']),
                download_url: None,
                download_policy: None,
            },
            ProviderToolGeneratedResource {
                output_id: String::from("invalid-output"),
                kind: String::from("file"),
                mime_type: String::from("application/x-unsupported-provider-output"),
                bytes: Some(vec![1, 2, 3]),
                download_url: None,
                download_policy: None,
            },
        ];

        let error = stage_provider_resources(Some(workspace.path()), "batch-owner", resources)
            .await
            .expect_err("the unsupported second resource must fail the complete batch");

        assert!(error.contains("unsupported MIME type"), "{error}");
        // asset_root is the exact project-scoped directory that must contain no staged payload.
        // asset_root 是不得残留暂存载荷的精确项目级目录。
        let asset_root = crate::attachment_assets::resolve_managed_attachment_root(
            workspace.path().to_string_lossy().as_ref(),
        );
        // image_directory is created by the first stage but must be empty after rollback.
        // image_directory 由首项暂存创建，但回滚后必须为空。
        let image_directory = asset_root.join("images");
        assert_eq!(
            std::fs::read_dir(&image_directory)
                .expect("staged image directory should remain readable")
                .count(),
            0
        );
        // index_document is the durable authority and must publish an empty asset set.
        // index_document 是持久权威，必须发布空资产集合。
        let index_document: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(asset_root.join("index").join("assets.json"))
                .expect("attachment index should remain readable"),
        )
        .expect("attachment index should remain valid json");
        assert_eq!(index_document["assets"], json!([]));
    }

    /// Verifies ownership-retention failure rolls back the complete staged resource batch.
    /// 验证所有权保留失败会回滚完整的暂存资源批次。
    #[tokio::test]
    async fn generated_resource_batch_rolls_back_when_owner_commit_fails() {
        // workspace isolates the retention-failure transaction.
        // workspace 隔离所有权保留失败事务。
        let workspace = tempfile::tempdir().expect("temporary workspace");
        // resources contains one valid output so the empty owner fails only at final commit.
        // resources 包含一个有效输出，使空所有者只在最终提交阶段失败。
        let resources = vec![ProviderToolGeneratedResource {
            output_id: String::from("uncommitted-image"),
            kind: String::from("image"),
            mime_type: String::from("image/png"),
            bytes: Some(vec![0x89, b'P', b'N', b'G']),
            download_url: None,
            download_policy: None,
        }];

        let error = stage_provider_resources(Some(workspace.path()), "   ", resources)
            .await
            .expect_err("empty owner must fail the atomic ownership commit");

        assert!(error.contains("invoking conversation id"), "{error}");
        // asset_root contains only an empty durable index after rollback.
        // asset_root 在回滚后只包含空的持久索引。
        let asset_root = crate::attachment_assets::resolve_managed_attachment_root(
            workspace.path().to_string_lossy().as_ref(),
        );
        let index_document: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(asset_root.join("index").join("assets.json"))
                .expect("attachment index should remain readable"),
        )
        .expect("attachment index should remain valid json");
        assert_eq!(index_document["assets"], json!([]));
    }

    /// Verifies native web search suppresses only the external search capability.
    /// 验证原生网页搜索只抑制外部搜索能力。
    #[test]
    fn native_web_search_suppression_preserves_web_extract_and_other_tools() {
        use super::provider_tool_capability_is_suppressed;
        use vulcan_provider_adapters::ProviderToolCapabilityKind;

        assert!(provider_tool_capability_is_suppressed(
            ProviderToolCapabilityKind::WebSearch,
            true
        ));
        assert!(!provider_tool_capability_is_suppressed(
            ProviderToolCapabilityKind::WebExtract,
            true
        ));
        assert!(!provider_tool_capability_is_suppressed(
            ProviderToolCapabilityKind::MediaAnalyze,
            true
        ));
        assert!(!provider_tool_capability_is_suppressed(
            ProviderToolCapabilityKind::WebSearch,
            false
        ));
    }
}
