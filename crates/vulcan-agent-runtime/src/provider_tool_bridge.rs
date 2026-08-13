//! Per-turn provider tool registration and host-side output staging.
//! 每回合供应商工具注册与宿主侧输出暂存。

use crate::attachment_assets::{
    attachment_descriptor_from_mime_type, stage_attachment_bytes, stage_attachment_file_streaming,
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use vulcan_provider_adapters::{
    AvailableProviderToolTarget, ManifestSourceLayer, PersistedAiConfiguration,
    ProviderToolCapabilityKind, ProviderToolExecutionResult, ProviderToolNormalizedOutput,
    ProviderToolResourceDownloadPolicy, ProviderToolTargetSelection, PublicAiRuntime,
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
    session_extension_target_sets: &Option<
        BTreeMap<String, BTreeSet<vulcan_workbench_model::SessionProviderToolTargetSelection>>,
    >,
    suppress_web_search: bool,
) -> Result<Vec<VulcanToolCandidate>, String> {
    apply_session_provider_tool_selections(
        &mut configuration,
        session_selections,
        session_extension_target_sets,
    )?;
    let available = runtime.available_provider_tool_targets(&configuration)?;
    let mut candidates = Vec::new();
    for (capability, selection) in &configuration.provider_tool_selections {
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
        ));
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
        ));
    }
    Ok(candidates)
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
        invoke: Arc::new(move |arguments, owner_session_id| {
            let runtime = Arc::clone(&runtime);
            let configuration = configuration.clone();
            let workspace_root = workspace_root.clone();
            let catalog = Arc::clone(&catalog);
            Box::pin(async move {
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
                let result = runtime
                    .execute_provider_tool_target_for_owner_in_workspace(
                        &configuration,
                        capability,
                        &selection,
                        provider_arguments,
                        owner_session_id,
                        workspace_root.as_deref(),
                    )
                    .await?;
                let rendered = render_provider_tool_result(workspace_root.as_deref(), result).await;
                cancellation_guard.disarm();
                rendered
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
        "Executes the `{}` model extension. You MUST call `{MODEL_EXTENSION_PARAMETERS_TOOL_ID}` for this capability immediately before use, then pass one returned model id and args conforming exactly to that model's schema. Never guess or reuse stale model-specific arguments.",
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
            "provider_id": target.target.provider_id,
            "model_id": target.target.upstream_target_id,
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
    if let Some(requested_model) = requested_model.as_deref() {
        if models.is_empty() {
            return Err(format!(
                "Model `{requested_model}` is not currently enabled for capability `{capability_name}`."
            ));
        }
    }
    let payload = json!({
        "kind": "model_extension_parameters",
        "capability": capability_name,
        "models": models,
        "instruction": if catalog.contains_key(&capability) {
            "Choose one returned model and pass args matching its exact args_schema."
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
    session_extension_target_sets: &Option<
        BTreeMap<String, BTreeSet<vulcan_workbench_model::SessionProviderToolTargetSelection>>,
    >,
) -> Result<(), String> {
    if let Some(session_selections) = session_selections {
        configuration.provider_tool_selections = session_selections
            .iter()
            .map(|(capability, selection)| {
                let capability: ProviderToolCapabilityKind =
                    serde_json::from_value(Value::String(capability.clone())).map_err(|error| {
                        format!(
                            "Session provider tool capability `{capability}` is invalid: {error}"
                        )
                    })?;
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
        configuration.provider_tool_enabled_capabilities = configuration
            .provider_tool_selections
            .keys()
            .copied()
            .collect();
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
/// An AI-host binding with the selected model's unmodified input schema.
/// 返回使用选定模型未修改输入 Schema 的 AI 宿主绑定。
fn build_direct_provider_tool_candidate(
    runtime: Arc<PublicAiRuntime>,
    configuration: PersistedAiConfiguration,
    workspace_root: Option<PathBuf>,
    selected: AvailableProviderToolTarget,
) -> VulcanToolCandidate {
    let contract = selected.target.contract.clone();
    let capability = contract.capability;
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: contract.tool_id.clone(),
            name: contract.tool_id,
            description: contract.description,
            help_text: None,
            source: VulcanToolSourceKind::AiHostBinding,
        },
        parameters_schema: contract.input_schema,
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Visible,
        request_streaming: None,
        invoke: Arc::new(move |arguments, owner_session_id| {
            let runtime = Arc::clone(&runtime);
            let configuration = configuration.clone();
            let workspace_root = workspace_root.clone();
            Box::pin(async move {
                let mut cancellation_guard = ProviderTaskCancellationOnDrop::new(
                    Arc::clone(&runtime),
                    owner_session_id.clone(),
                );
                let result = runtime
                    .execute_selected_provider_tool_for_owner_in_workspace(
                        &configuration,
                        capability,
                        arguments,
                        owner_session_id,
                        workspace_root.as_deref(),
                    )
                    .await?;
                let rendered = render_provider_tool_result(workspace_root.as_deref(), result).await;
                cancellation_guard.disarm();
                rendered
            })
        }),
    }
}

/// Stages unified provider resources and builds model/card output.
/// 暂存统一供应商资源并构建模型与卡片输出。
///
/// # Parameters
/// 参数
/// `workspace_root` owns managed attachments and `result` is provider-normalized.
/// `workspace_root` 持有托管附件，`result` 已由供应商归一化。
///
/// # Returns
/// 返回值
/// A model-visible local-file result with generated-attachment host metadata.
/// 返回包含生成附件宿主元数据的模型可见本地文件结果。
async fn render_provider_tool_result(
    workspace_root: Option<&Path>,
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
            render_provider_resource_result(workspace_root, result.upstream_response_id, resources)
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
            let resources = stage_provider_resources(workspace_root, composite.resources).await?;
            let payload = json!({
                "kind": "composite",
                "composite": {
                    "text": composite.text,
                    "resources": resources,
                    "embeddings": composite.embeddings,
                    "reranks": composite.reranks,
                    "searches": composite.searches,
                    "extracts": composite.extracts
                },
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
    }
}

/// Stages unified provider resources and builds model/card output.
/// 暂存统一供应商资源并构建模型与卡片输出。
///
/// # Parameters
/// 参数
/// `workspace_root`, `upstream_response_id`, and `resources` are provider-normalized.
/// `workspace_root`、`upstream_response_id` 与 `resources` 已由供应商归一化。
///
/// # Returns
/// 返回值
/// A model-visible local-file result with generated-attachment host metadata.
/// 返回包含生成附件宿主元数据的模型可见本地文件结果。
async fn render_provider_resource_result(
    workspace_root: Option<&Path>,
    upstream_response_id: Option<String>,
    resources: Vec<vulcan_provider_adapters::ProviderToolGeneratedResource>,
) -> Result<VulcanToolInvocationResult, String> {
    let outputs = stage_provider_resources(workspace_root, resources).await?;
    let payload = json!({
        "outputs": outputs,
        "upstream_response_id": upstream_response_id
    });
    let local_files = provider_resource_local_files(outputs.as_slice());
    Ok(VulcanToolInvocationResult::from_model_output(json!({
        "resources": {
            "local_files": local_files
        }
    }))
    .with_host_result(VulcanToolHostResult {
        kind: String::from("generated_attachments"),
        payload,
    }))
}

/// Stages normalized provider resources and returns unified local descriptors.
/// 暂存归一化供应商资源并返回统一本地描述。
///
/// # Parameters
/// 参数
/// `workspace_root` owns managed attachments and `resources` are provider-normalized.
/// `workspace_root` 持有托管附件，`resources` 已由供应商归一化。
///
/// # Returns
/// 返回值
/// Ordered local descriptors suitable for model output and host cards.
/// 返回适用于模型输出与宿主卡片的有序本地描述。
async fn stage_provider_resources(
    workspace_root: Option<&Path>,
    resources: Vec<vulcan_provider_adapters::ProviderToolGeneratedResource>,
) -> Result<Vec<Value>, String> {
    if resources.is_empty() {
        return Ok(Vec::new());
    }
    let workspace_root = workspace_root.ok_or_else(|| {
        String::from("Provider tools that generate resources require an active workspace root.")
    })?;
    let workspace_text = vulcan_runtime_assets::native_host_path_text_from_path(workspace_root);
    let mut outputs = Vec::with_capacity(resources.len());
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
        outputs.push(json!({
            "kind": resource.kind,
            "file_name": attachment.file_name,
            "local_path": attachment.local_path,
            "preview_local_path": attachment.preview_local_path,
            "mime_type": attachment.mime_type
        }));
    }
    Ok(outputs)
}

/// Collects local file paths from exact normalized resource descriptors.
/// 从精确的归一化资源描述中收集本地文件路径。
///
/// # Parameters
/// 参数
/// `resources` are descriptors produced by `stage_provider_resources`.
/// `resources` 是由 `stage_provider_resources` 产生的描述。
///
/// # Returns
/// 返回值
/// Ordered local paths registered with the tool invocation result.
/// 返回注册到工具调用结果中的有序本地路径。
fn provider_resource_local_files(resources: &[Value]) -> Vec<String> {
    resources
        .iter()
        .filter_map(|output| output.get("local_path").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
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
    let parsed_url = download_policy.validate_url(download_url, "Provider resource URL")?;
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
        MODEL_EXTENSION_CAPABILITIES, apply_session_provider_tool_selections,
        build_selected_model_extension_catalog, download_provider_resource_to_temp,
        model_extension_executor_schema, model_extension_parameters_schema,
        model_extension_tool_id, render_provider_tool_result,
    };
    use serde_json::json;
    use std::collections::BTreeSet;
    use vulcan_provider_adapters::{
        AvailableProviderToolTarget, ManifestSourceLayer, ProviderToolCapabilityKind,
        ProviderToolCompositeOutput, ProviderToolEmbeddingOutput, ProviderToolEmbeddingVector,
        ProviderToolExecutionResult, ProviderToolNormalizedOutput, ProviderToolRerankOutput,
        ProviderToolRerankResult, ProviderToolResourceDownloadPolicy, ProviderToolResourceEncoding,
        ProviderToolTarget, ProviderToolTargetSelection,
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
    ) -> AvailableProviderToolTarget {
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
                "tool_id": "image_generate",
                "description": "Generate one test image.",
                "capability": "image_generate",
                "input_schema": {
                    "type": "object",
                    "properties": {"prompt": {"type": "string"}},
                    "required": ["prompt"],
                    "additionalProperties": false
                },
                "extension_profile": {"summary": "Reviewed native image model."},
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
        );
        // customTarget must remain excluded even when a forged persisted selection references it.
        // customTarget 即使被伪造持久化选择引用也必须保持排除。
        let custom_target = available_extension_target(
            "custom",
            "image-model",
            "custom-image",
            "custom-account",
            ManifestSourceLayer::Custom,
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
            &None,
        )
        .expect("apply explicit empty override");

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
            &Some(Default::default()),
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
            "Provider resource URL is outside the declared scheme, default port or exact operation origin, and host suffixes."
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
            "Provider resource URL is outside the declared scheme, default port or exact operation origin, and host suffixes."
        );
    }

    /// Verifies embedding output remains typed in both model and host payloads.
    /// 验证向量输出在模型与宿主载荷中均保持强类型。
    #[tokio::test]
    async fn embedding_output_preserves_vector_indexes_and_values() {
        let result = render_provider_tool_result(
            None,
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
