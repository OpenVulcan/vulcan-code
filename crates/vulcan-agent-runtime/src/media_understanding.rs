//! Fixed-schema image, video, and audio understanding backends.
//! 固定 Schema 的图片、视频与音频理解后端。

use crate::attachment_assets::{
    attachment_descriptor_from_path, registered_media_attachment_mime_types, streaming_file_sha256,
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use vulcan_ai_contract::{
    AiExecutionRequest, AiMessage, AiOperation, AiStreamEvent, ContentItem, MessageRole,
    ModelCapabilities, ParallelToolCallPolicy, PromptCachePolicy, ReasoningControlMode,
    ReasoningEffort, ReasoningOutputPolicy, ResourceKind, ResourceReference, ResourceSourceKind,
    StructuredOutputPolicy, ToolChoicePolicy,
};
use vulcan_core::types::ConversationAttachmentKind;
use vulcan_provider_adapters::{
    ActiveAiModelSelection, InputResourceConstraint, MediaUploadCancellation,
    PersistedAiConfiguration, ProviderToolCapabilityKind, ProviderToolExecutionResult,
    ProviderToolMediaInputProfile, ProviderToolMediaInputProjection, ProviderToolNormalizedOutput,
    PublicAiRuntime, ResolvedPublicAiSelection,
};
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolInvocationResult,
    VulcanToolOrigin, VulcanToolSourceKind,
};

/// Maximum UTF-8 byte length accepted for the canonical answer.
/// 规范回答允许的最大 UTF-8 字节长度。
const MEDIA_UNDERSTANDING_ANSWER_BYTE_LIMIT: usize = 256 * 1024;
/// Maximum number of timeline or observation segments accepted from one backend.
/// 单个后端结果允许的时间线或观察分段最大数量。
const MEDIA_UNDERSTANDING_SEGMENT_LIMIT: usize = 512;
/// Maximum UTF-8 byte length accepted for one segment body.
/// 单个分段正文允许的最大 UTF-8 字节长度。
const MEDIA_UNDERSTANDING_SEGMENT_BYTE_LIMIT: usize = 32 * 1024;
/// Maximum warning count accepted from one backend.
/// 单个后端结果允许的警告最大数量。
const MEDIA_UNDERSTANDING_WARNING_LIMIT: usize = 64;
/// Maximum UTF-8 byte length accepted for one warning.
/// 单条警告允许的最大 UTF-8 字节长度。
const MEDIA_UNDERSTANDING_WARNING_BYTE_LIMIT: usize = 4 * 1024;
/// Maximum UTF-8 byte length accepted for one complete replacement-model answer.
/// 单份模型替代理解完整回答允许的最大 UTF-8 字节长度。
const MEDIA_UNDERSTANDING_OUTPUT_DOCUMENT_BYTE_LIMIT: usize = 32 * 1024 * 1024;

/// Returns the stable AI-facing tool id for one media-understanding capability.
/// 返回一项媒体理解能力稳定的 AI 可见工具标识。
pub(crate) fn media_understanding_tool_id(
    capability: ProviderToolCapabilityKind,
) -> Option<&'static str> {
    match capability {
        ProviderToolCapabilityKind::ImageUnderstand => Some("image_understand"),
        ProviderToolCapabilityKind::VideoUnderstand => Some("video_understand"),
        ProviderToolCapabilityKind::AudioUnderstand => Some("audio_understand"),
        _ => None,
    }
}

/// Builds the invariant model-facing input schema for one media-understanding capability.
/// 为一项媒体理解能力构建不变的模型可见输入 Schema。
pub(crate) fn media_understanding_input_schema(
    capability: ProviderToolCapabilityKind,
) -> Option<Value> {
    // pathArgument is the one stable media-specific path field exposed to AI.
    // pathArgument 是向 AI 暴露的唯一稳定媒体专属路径字段。
    let path_argument = media_understanding_path_argument(capability)?;
    Some(json!({
        "type": "object",
        "properties": {
            (path_argument): {
                "type": "string",
                "minLength": 1,
                "description": "Exact project-resource:// id, owned attachment:// id, or readable local file path. Relative paths resolve from the active workspace; absolute and project-external paths are accepted until filesystem permissions are enabled. The file extension must match this understanding tool's media type."
            },
            "prompt": {
                "type": "string",
                "minLength": 1,
                "description": "The exact question or analysis instruction for the media."
            }
        },
        "required": [path_argument, "prompt"],
        "additionalProperties": false
    }))
}

/// Projects the fixed media facade into one explicitly configured provider-native argument object.
/// 将固定媒体外壳投影为一个显式配置的供应商原生参数对象。
pub(crate) fn project_media_understanding_arguments(
    capability: ProviderToolCapabilityKind,
    projection: &ProviderToolMediaInputProjection,
    arguments: Value,
) -> Result<Value, String> {
    // pathArgument is fixed by media semantics and cannot be supplied by provider configuration.
    // pathArgument 由媒体语义固定，不能由供应商配置提供。
    let path_argument = media_understanding_path_argument(capability)
        .ok_or_else(|| format!("Capability `{capability:?}` is not media understanding."))?;
    // source must match the invariant public schema exactly.
    // source 必须精确匹配不变公开 Schema。
    let source = arguments
        .as_object()
        .ok_or_else(|| String::from("Media-understanding arguments must be a JSON object."))?;
    if source.len() != 2 || !source.contains_key(path_argument) || !source.contains_key("prompt") {
        return Err(String::from(
            "Media-understanding arguments must contain exactly one media path and `prompt`.",
        ));
    }
    // pathValue is the validated non-empty media reference.
    // pathValue 是经过校验的非空媒体引用。
    let path_value = source
        .get(path_argument)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("Media-understanding argument `{path_argument}` is required."))?;
    // promptValue is the validated non-empty caller instruction.
    // promptValue 是经过校验的非空调用方指令。
    let prompt_value = source
        .get("prompt")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| String::from("Media-understanding argument `prompt` is required."))?;
    if projection.path_field.trim().is_empty()
        || projection.prompt_field.trim().is_empty()
        || projection.path_field == projection.prompt_field
        || projection
            .fixed_arguments
            .contains_key(projection.path_field.as_str())
        || projection
            .fixed_arguments
            .contains_key(projection.prompt_field.as_str())
    {
        return Err(String::from(
            "Media-understanding target carries an invalid explicit input projection.",
        ));
    }
    // target starts from reviewed fixed provider arguments only.
    // target 仅从已审核供应商固定参数开始构建。
    let mut target = projection
        .fixed_arguments
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<serde_json::Map<_, _>>();
    // projectedPath preserves scalar or explicit single-item-array target shape.
    // projectedPath 保留标量或显式单项数组目标形态。
    let projected_path = if projection.path_as_single_item_array {
        Value::Array(vec![Value::String(path_value.to_string())])
    } else {
        Value::String(path_value.to_string())
    };
    target.insert(projection.path_field.clone(), projected_path);
    target.insert(
        projection.prompt_field.clone(),
        Value::String(prompt_value.to_string()),
    );
    Ok(Value::Object(target))
}

/// Builds one fixed-schema media tool backed by an ordinary conversation model.
/// 构建一个由普通会话模型驱动的固定 Schema 媒体工具。
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_model_replacement_media_candidate(
    runtime: Arc<PublicAiRuntime>,
    configuration: PersistedAiConfiguration,
    workspace_root: Option<PathBuf>,
    capability: ProviderToolCapabilityKind,
    selection: ActiveAiModelSelection,
    reasoning_effort: Option<ReasoningEffort>,
    media_resource_runtime: Option<Arc<dyn crate::media_resource_runtime::MediaResourceRuntime>>,
) -> Result<VulcanToolCandidate, String> {
    // toolId is determined only by media semantics and never by the replacement model.
    // toolId 仅由媒体语义决定，绝不由替代模型决定。
    let tool_id = media_understanding_tool_id(capability)
        .ok_or_else(|| format!("Capability `{capability:?}` is not media understanding."))?
        .to_string();
    // parametersSchema contains only the media path and prompt.
    // parametersSchema 仅包含媒体路径与提示词。
    let parameters_schema = media_understanding_input_schema(capability)
        .ok_or_else(|| format!("Capability `{capability:?}` has no fixed media schema."))?;
    // resolvedSelection freezes the exact protocol route and effective model capabilities.
    // resolvedSelection 冻结精确协议路由与生效模型能力。
    let resolved_selection = runtime.resolve_connection_model_selection(
        &configuration,
        selection.connection_id.as_str(),
        selection.provider_id.as_str(),
        selection.model_id.as_str(),
    )?;
    // routeSupport closes over both model declarations and the exact frozen protocol encoder.
    // routeSupport 同时封闭校验模型声明与精确冻结的协议编码器。
    if !model_replacement_route_supports_media_understanding(
        &resolved_selection.model.capabilities,
        resolved_selection
            .route
            .protocol_route
            .protocol_profile_id
            .as_str(),
        capability,
    ) {
        return Err(format!(
            "Provider `{}` model `{}` no longer satisfies the media-understanding route contract.",
            selection.provider_id, selection.model_id
        ));
    }
    if let Some(reasoning_effort) = reasoning_effort {
        validate_media_replacement_reasoning(
            resolved_selection.model.capabilities.reasoning_control,
            &resolved_selection.model.capabilities.reasoning_efforts,
            reasoning_effort,
        )?;
    }
    // description combines the invariant facade with exact frozen route constraints.
    // description 将不变外壳与精确冻结路线约束合并。
    let description = media_understanding_description(
        capability,
        resolved_selection
            .route
            .protocol_route
            .protocol_profile_id
            .as_str(),
        resolved_selection
            .route
            .protocol_route
            .input_resource_constraints
            .as_slice(),
    )?;
    let origin = VulcanToolOrigin::new(
        VulcanToolSourceKind::AiHostBinding,
        selection.provider_id.clone(),
        tool_id.clone(),
    );
    Ok(VulcanToolCandidate {
        descriptor: VulcanToolDescriptor::new(tool_id.clone(), tool_id, description, None, origin),
        parameters_schema,
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Visible,
        request_streaming: None,
        invoke: Arc::new(move |arguments, context| {
            // invocationRuntime owns one isolated physical model execution.
            // invocationRuntime 持有一次隔离的真实模型执行。
            let invocation_runtime = Arc::clone(&runtime);
            // invocationSelection retains the immutable account route selected in settings.
            // invocationSelection 保留设置中选择的不可变账号路由。
            let invocation_selection = resolved_selection.clone();
            // invocationWorkspace remains the only local filesystem boundary.
            // invocationWorkspace 保持为唯一本地文件系统边界。
            let invocation_workspace = workspace_root.clone();
            // The host port retains exact project identity across root and child tool invocations.
            // 宿主端口跨根与子工具调用保留精确项目身份。
            let invocation_resources = media_resource_runtime.clone();
            Box::pin(async move {
                execute_model_replacement_media_understanding(
                    invocation_runtime,
                    invocation_workspace.as_deref(),
                    capability,
                    invocation_selection,
                    reasoning_effort,
                    arguments,
                    Some(context.session_id),
                    invocation_resources.as_deref(),
                )
                .await
            })
        }),
    })
}

/// Executes one model-replacement media-understanding invocation end to end.
/// 端到端执行一次模型代替媒体理解调用。
#[allow(clippy::too_many_arguments)]
async fn execute_model_replacement_media_understanding(
    runtime: Arc<PublicAiRuntime>,
    workspace_root: Option<&Path>,
    capability: ProviderToolCapabilityKind,
    resolved_selection: ResolvedPublicAiSelection,
    reasoning_effort: Option<ReasoningEffort>,
    arguments: Value,
    owner_session_id: Option<String>,
    media_resource_runtime: Option<&dyn crate::media_resource_runtime::MediaResourceRuntime>,
) -> Result<VulcanToolInvocationResult, String> {
    // pathArgument is the only accepted media path field for this capability.
    // pathArgument 是当前能力唯一接受的媒体路径字段。
    let path_argument = media_understanding_path_argument(capability)
        .ok_or_else(|| format!("Capability `{capability:?}` has no media path argument."))?;
    // argumentObject must match the fixed schema before any file access occurs.
    // argumentObject 必须在任何文件访问前匹配固定 Schema。
    let argument_object = arguments
        .as_object()
        .ok_or_else(|| String::from("Media-understanding arguments must be a JSON object."))?;
    if argument_object.len() != 2
        || !argument_object.contains_key(path_argument)
        || !argument_object.contains_key("prompt")
    {
        return Err(String::from(
            "Media-understanding arguments must contain exactly one media path and `prompt`.",
        ));
    }
    // mediaReference is the exact AI-supplied local path value.
    // mediaReference 是 AI 提供的精确本地路径值。
    let media_reference = argument_object
        .get(path_argument)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("Media-understanding argument `{path_argument}` is required."))?;
    // prompt is the exact user question or analysis instruction.
    // prompt 是精确的用户问题或分析指令。
    let prompt = argument_object
        .get("prompt")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| String::from("Media-understanding argument `prompt` is required."))?;
    // attachment resolves either an owned locator or an explicit readable host path.
    // attachment 解析归当前会话持有的定位符或显式可读宿主路径。
    let attachment = resolve_media_understanding_attachment(
        workspace_root,
        owner_session_id.as_deref(),
        capability,
        media_reference,
        media_resource_runtime,
    )
    .await?;
    let media_path = PathBuf::from(attachment.local_path.as_str());
    // resourceKind is the exact provider-neutral media category.
    // resourceKind 是精确的供应商无关媒体类别。
    let resource_kind = media_understanding_resource_kind(capability)
        .ok_or_else(|| format!("Capability `{capability:?}` has no media resource kind."))?;
    // mimeType is the canonical staged MIME and must also agree with the closed host format registry.
    // mimeType 是规范暂存 MIME，且必须同时与封闭宿主格式注册表一致。
    let registered_mime_type =
        media_understanding_mime_type(resource_kind, Path::new(&attachment.file_name))?;
    if registered_mime_type != attachment.mime_type {
        return Err(String::from(
            "Canonical attachment MIME no longer matches the registered media format.",
        ));
    }
    let mime_type = attachment.mime_type.as_str();
    // Both asset and project resolvers return exact immutable source metadata; absence cannot be guessed from the path.
    // 资产与项目解析器均返回精确不可变源元数据；缺失时不能从路径猜测。
    let expected_source_identity = vulcan_provider_adapters::PreparedMediaSourceIdentity {
        byte_len: attachment
            .byte_len
            .ok_or_else(|| String::from("Media resource has no canonical byte length."))?,
        content_sha256: attachment
            .source_sha256
            .clone()
            .ok_or_else(|| String::from("Media resource has no canonical content digest."))?,
    };
    // The runtime jointly validates upload reference and inline capabilities before any preparation.
    // 运行时在任何准备之前联合验证上传引用与内联能力。
    // cancellation connects preparation to this invocation lifetime.
    // cancellation 将准备过程与本次调用生命周期连接起来。
    let cancellation = MediaUploadCancellation::new();
    // uploadOperationId is stable per media kind and independent from the selected model.
    // uploadOperationId 按媒体类型保持稳定，且与所选模型无关。
    let upload_operation_id = format!(
        "media-understanding.model-replacement.{}",
        media_understanding_tool_id(capability)
            .ok_or_else(|| format!("Capability `{capability:?}` has no media tool id."))?
    );
    // preparedReference is immutable request-owned content, so protocol encoding never reopens the path.
    // preparedReference 是不可变的请求自有内容，因此协议编码绝不重新打开该路径。
    let prepared_media = runtime
        .prepare_and_remember_local_media(vulcan_provider_adapters::PrepareLocalMediaRequest {
            file_name: &attachment.file_name,
            expected_source_identity: &expected_source_identity,
            selection: &resolved_selection,
            owner_session_id: owner_session_id.as_deref(),
            operation_id: upload_operation_id.as_str(),
            local_path: &media_path,
            kind: resource_kind,
            mime_type,
            progress: None,
            cancellation: &cancellation,
        })
        .await?;
    // mediaId is stable only for this physical execution and never leaks a local path.
    // mediaId 仅在本次真实执行内稳定，绝不泄露本地路径。
    let media_id = format!("media-understanding:{}", uuid::Uuid::new_v4());
    // resource is replayed in both the user message and request resource inventory.
    // resource 同时进入用户消息与请求资源清单。
    // preparedRouteFields preserve Provider file ids, remote URLs, and inline Data URLs in their
    // dedicated wire channels instead of inferring semantics from string shape.
    // preparedRouteFields 将供应商文件标识、远程 URL 与内联 Data URL 保留在各自专用 Wire 通道，
    // 不根据字符串形态推断语义。
    let (resource_id, source_kind, resource_url) = match prepared_media.reference_encoding {
        Some(vulcan_provider_adapters::UploadReferenceEncoding::ProviderFileId) => (
            prepared_media.reference,
            ResourceSourceKind::ProviderFileId,
            None,
        ),
        Some(
            vulcan_provider_adapters::UploadReferenceEncoding::ResponseUrl
            | vulcan_provider_adapters::UploadReferenceEncoding::Template,
        ) => (
            media_id,
            ResourceSourceKind::RemoteUrl,
            Some(prepared_media.reference),
        ),
        None => (
            media_id,
            ResourceSourceKind::InlineData,
            Some(prepared_media.reference),
        ),
    };
    let resource = ResourceReference {
        id: resource_id,
        kind: resource_kind,
        media_type: Some(mime_type.to_string()),
        source_kind,
        file_name: Some(attachment.file_name.clone()),
        image_detail: None,
        resource_path: attachment.resource_path.clone(),
        local_path: None,
        url: resource_url,
    };
    // modelPrompt asks for one visible answer; the host owns the canonical result envelope and
    // never delegates its internal JSON contract to a conversation model.
    // modelPrompt 请求一个可见回答；规范结果信封由宿主持有，绝不把内部 JSON 契约委托给会话模型。
    let model_prompt = format!(
        "Act as a media-understanding backend. Answer the user instruction directly from the attached media. Return only the answer text without a JSON or Markdown wrapper.\n\nUser instruction: {}",
        prompt.trim()
    );
    // request contains one actual media input and keeps the owning session scope so a configured
    // provider session header also applies to image, video, and audio understanding requests.
    // request 包含一个真实媒体输入，并保留所属会话作用域，使已配置的供应商会话请求头同样
    // 应用于图片、视频与音频理解请求。
    let request = AiExecutionRequest {
        operation: AiOperation::ConversationRespond,
        model_id: resolved_selection.model.id.clone(),
        cache_scope_id: owner_session_id.clone(),
        continuation: None,
        messages: vec![AiMessage {
            role: MessageRole::User,
            content: vec![
                ContentItem::Text { text: model_prompt },
                ContentItem::Resource {
                    resource: resource.clone(),
                },
            ],
            provider_context_items: Vec::new(),
        }],
        tools: Vec::new(),
        native_tools: Vec::new(),
        tool_choice: ToolChoicePolicy::Disabled,
        parallel_tool_calls: ParallelToolCallPolicy::Disabled,
        structured_output: StructuredOutputPolicy::None,
        stop_sequences: Vec::new(),
        prompt_cache: PromptCachePolicy::default(),
        reasoning_effort,
        reasoning_output: media_replacement_reasoning_output_policy(reasoning_effort),
        model_parameters: BTreeMap::new(),
        resources: vec![resource],
        output_token_limit: None,
    };
    // executionId is unique per physical replacement-model request.
    // executionId 对每次真实替代模型请求唯一。
    let execution_id = format!("media-understanding-{}", uuid::Uuid::new_v4());
    // stream is the normalized exact-route provider response.
    // stream 是归一化后的精确路由供应商响应。
    let mut stream = runtime
        .execute_internal_llm(&resolved_selection, request, execution_id)
        .await
        .map_err(|failure| failure.to_string())?;
    // outputText accumulates the sole result channel declared by the replacement request.
    // outputText 累积替代请求声明的唯一结果通道。
    let mut output_text = String::new();
    // completed distinguishes terminal completion from a truncated stream.
    // completed 区分终态完成与截断流。
    let mut completed = false;
    while let Some(next_event) = stream.next().await {
        // event is one normalized provider item or a controlled transport failure.
        // event 是一项归一化供应商事件或受控传输失败。
        let event = next_event.map_err(|failure| failure.to_string())?;
        match event {
            AiStreamEvent::ResponseItemStarted { item, .. } => match item {
                ContentItem::StructuredJson { .. } => {
                    return Err(String::from(
                        "Media-understanding replacement returned structured output for a plain-text request.",
                    ));
                }
                ContentItem::Text { text } => {
                    append_bounded_media_output_text(&mut output_text, text.as_str())?
                }
                _ => {}
            },
            AiStreamEvent::ResponseTextDelta { delta, .. } => {
                append_bounded_media_output_text(&mut output_text, delta.as_str())?
            }
            AiStreamEvent::ResponseCompleted => {
                completed = true;
                break;
            }
            AiStreamEvent::ResponseFailed { error } => {
                return Err(format!(
                    "Media-understanding replacement model failed: {}",
                    serde_json::to_string(&error)
                        .unwrap_or_else(|_| String::from("provider error"))
                ));
            }
            AiStreamEvent::ResponseCancelled => {
                return Err(String::from(
                    "Media-understanding replacement model was cancelled.",
                ));
            }
            _ => {}
        }
    }
    if !completed {
        return Err(String::from(
            "Media-understanding replacement stream ended before terminal completion.",
        ));
    }
    // payload canonicalizes the model's plain answer in the host-owned result contract.
    // payload 在宿主持有的结果契约中规范化模型的纯文本回答。
    let payload = finalize_media_understanding_output(output_text.as_str(), capability)?;
    validate_media_understanding_payload(&payload, capability)?;
    media_understanding_invocation_result(payload)
}

/// Appends one text event while enforcing the whole-document byte ceiling before allocation.
/// 在分配前执行整份文档字节上限，并追加一个文本事件。
///
/// # Parameters
/// 参数
/// `output` owns prior UTF-8 answer fragments and `fragment` is the next normalized stream event.
/// `output` 持有此前 UTF-8 回答分片，`fragment` 是下一项归一化流事件。
///
/// # Returns
/// 返回值
/// Success after a bounded append, or an explicit host-limit failure.
/// 有界追加后返回成功；超过宿主上限时返回明确错误。
fn append_bounded_media_output_text(output: &mut String, fragment: &str) -> Result<(), String> {
    if output
        .len()
        .checked_add(fragment.len())
        .is_none_or(|length| length > MEDIA_UNDERSTANDING_OUTPUT_DOCUMENT_BYTE_LIMIT)
    {
        return Err(String::from(
            "Media-understanding replacement text output exceeds the host byte limit.",
        ));
    }
    output.push_str(fragment);
    Ok(())
}

/// Wraps one replacement-model text answer in the canonical host result.
/// 将一份替代模型文本回答包装为规范宿主结果。
///
/// # Parameters
/// 参数
/// `output_text` is the accumulated text channel and `capability` fixes the canonical media kind.
/// `output_text` 是累积文本通道，`capability` 固定规范媒体类型。
///
/// # Returns
/// 返回值
/// One canonical JSON value, or an explicit error when the model returned no answer text.
/// 返回一个规范 JSON 值；模型没有返回回答文本时返回明确错误。
fn finalize_media_understanding_output(
    output_text: &str,
    capability: ProviderToolCapabilityKind,
) -> Result<Value, String> {
    let trimmed_output = output_text.trim();
    if trimmed_output.is_empty() {
        return Err(String::from(
            "Media-understanding replacement returned no answer text.",
        ));
    }
    Ok(json!({
        "schema_version": "vulcan.media-understanding.v1",
        "media_kind": media_understanding_media_label(capability)?,
        "answer": trimmed_output,
        "segments": [],
        "warnings": []
    }))
}

/// Renders one provider-specific response into the canonical model-neutral shape.
/// 将一项供应商专用响应渲染为规范模型无关形态。
pub(crate) fn render_media_understanding_provider_result(
    capability: ProviderToolCapabilityKind,
    result: ProviderToolExecutionResult,
) -> Result<VulcanToolInvocationResult, String> {
    // answer is the provider-returned visible understanding text.
    // answer 是供应商返回的可见理解文本。
    let answer = match result.output {
        ProviderToolNormalizedOutput::Text(text) => text,
        _ => {
            return Err(String::from(
                "Media-understanding provider target returned a non-text normalized output.",
            ));
        }
    };
    if answer.trim().is_empty() {
        return Err(String::from(
            "Media-understanding provider target returned empty text.",
        ));
    }
    // payload is identical across provider-specific and model-replacement modes.
    // payload 在供应商专用与模型代替模式间保持一致。
    let payload = json!({
        "schema_version": "vulcan.media-understanding.v1",
        "media_kind": media_understanding_media_label(capability)?,
        "answer": answer,
        "segments": [],
        "warnings": []
    });
    validate_media_understanding_payload(&payload, capability)?;
    media_understanding_invocation_result(payload)
}

mod result_markdown;

/// Projects a validated canonical payload into one body shared by model and host display.
/// 将已校验规范载荷投影为模型与宿主展示共享的一份正文。
/// Takes the canonical value; returns the readable result or an explicit projection error.
/// 接收规范值，返回可读结果或明确的投影错误。
fn media_understanding_invocation_result(
    payload: Value,
) -> Result<VulcanToolInvocationResult, String> {
    Ok(VulcanToolInvocationResult::from_model_output(
        Value::String(result_markdown::render(payload)?),
    ))
}

/// Validates the canonical result without accepting provider-specific alternate fields.
/// 校验规范结果且不接受供应商专属替代字段。
fn validate_media_understanding_payload(
    payload: &Value,
    capability: ProviderToolCapabilityKind,
) -> Result<(), String> {
    // object is the only accepted top-level JSON shape.
    // object 是唯一接受的顶层 JSON 形态。
    let object = payload
        .as_object()
        .ok_or_else(|| String::from("Media-understanding result must be a JSON object."))?;
    // allowedKeys closes the result contract against backend-specific fields.
    // allowedKeys 关闭结果契约，不接受后端专属字段。
    let allowed_keys = BTreeSet::from([
        "schema_version",
        "media_kind",
        "answer",
        "segments",
        "warnings",
    ]);
    if object
        .keys()
        .any(|key| !allowed_keys.contains(key.as_str()))
    {
        return Err(String::from(
            "Media-understanding result contains an undeclared field.",
        ));
    }
    if object.get("schema_version").and_then(Value::as_str) != Some("vulcan.media-understanding.v1")
        || object.get("media_kind").and_then(Value::as_str)
            != Some(media_understanding_media_label(capability)?)
    {
        return Err(String::from(
            "Media-understanding result kind does not match the selected capability.",
        ));
    }
    let answer = object
        .get("answer")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| String::from("Media-understanding result requires a non-empty answer."))?;
    if answer.len() > MEDIA_UNDERSTANDING_ANSWER_BYTE_LIMIT {
        return Err(String::from(
            "Media-understanding result answer exceeds the host byte limit.",
        ));
    }
    // segments is a bounded closed timeline shared by image, video, and audio results.
    // segments 是图片、视频与音频结果共享的有界封闭时间线。
    let segments = object
        .get("segments")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            String::from("Media-understanding result requires `segments` as an array.")
        })?;
    if segments.len() > MEDIA_UNDERSTANDING_SEGMENT_LIMIT {
        return Err(String::from(
            "Media-understanding result contains too many segments.",
        ));
    }
    for segment in segments {
        // segmentObject rejects backend-specific timeline fields.
        // segmentObject 拒绝后端专属时间线字段。
        let segment_object = segment
            .as_object()
            .ok_or_else(|| String::from("Media-understanding result segments must be objects."))?;
        if segment_object.len() != 3
            || !segment_object.contains_key("start_ms")
            || !segment_object.contains_key("end_ms")
            || !segment_object.contains_key("content")
        {
            return Err(String::from(
                "Media-understanding result segment fields do not match the canonical contract.",
            ));
        }
        // startMs and endMs accept either null or one non-negative integer.
        // startMs 与 endMs 仅接受空值或非负整数。
        let start_ms = optional_non_negative_milliseconds(segment_object.get("start_ms"))?;
        let end_ms = optional_non_negative_milliseconds(segment_object.get("end_ms"))?;
        if start_ms.zip(end_ms).is_some_and(|(start, end)| end < start) {
            return Err(String::from(
                "Media-understanding result segment end_ms cannot precede start_ms.",
            ));
        }
        let content = segment_object
            .get("content")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                String::from("Media-understanding result segment content must be non-empty.")
            })?;
        if content.len() > MEDIA_UNDERSTANDING_SEGMENT_BYTE_LIMIT {
            return Err(String::from(
                "Media-understanding result segment content exceeds the host byte limit.",
            ));
        }
    }
    // warnings is a bounded string array and never carries provider-native objects.
    // warnings 是有界字符串数组，绝不携带供应商原生对象。
    let warnings = object
        .get("warnings")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            String::from("Media-understanding result requires `warnings` as an array.")
        })?;
    if warnings.len() > MEDIA_UNDERSTANDING_WARNING_LIMIT
        || warnings.iter().any(|warning| {
            warning
                .as_str()
                .is_none_or(|value| value.len() > MEDIA_UNDERSTANDING_WARNING_BYTE_LIMIT)
        })
    {
        return Err(String::from(
            "Media-understanding result warnings exceed the host shape or byte limits.",
        ));
    }
    Ok(())
}

/// Parses one required nullable non-negative millisecond field.
/// 解析一个必需、可为空且非负的毫秒字段。
///
/// # Parameters
/// 参数
/// `value` is the exact segment field retained by the canonical result object.
/// `value` 是规范结果对象保留的精确分段字段。
///
/// # Returns
/// 返回值
/// `None` for JSON null, one unsigned integer for a timestamp, or a controlled validation error.
/// JSON 空值返回 `None`，时间戳返回无符号整数，其他情况返回受控校验错误。
fn optional_non_negative_milliseconds(value: Option<&Value>) -> Result<Option<u64>, String> {
    match value {
        Some(Value::Null) => Ok(None),
        Some(value) => value.as_u64().map(Some).ok_or_else(|| {
            String::from(
                "Media-understanding result segment timestamps must be null or non-negative integers.",
            )
        }),
        None => Err(String::from(
            "Media-understanding result segment is missing a timestamp field.",
        )),
    }
}

/// Returns the exact resource kind for one understanding capability.
/// 返回一项理解能力的精确资源类型。
fn media_understanding_resource_kind(
    capability: ProviderToolCapabilityKind,
) -> Option<ResourceKind> {
    match capability {
        ProviderToolCapabilityKind::ImageUnderstand => Some(ResourceKind::Image),
        ProviderToolCapabilityKind::VideoUnderstand => Some(ResourceKind::Video),
        ProviderToolCapabilityKind::AudioUnderstand => Some(ResourceKind::Audio),
        _ => None,
    }
}

/// Returns the canonical attachment kind for one media-understanding capability.
/// 返回一项媒体理解能力对应的规范附件类型。
///
/// # Parameters
/// 参数
/// `capability` is the exact provider-neutral understanding capability.
/// `capability` 是精确的供应商无关理解能力。
///
/// # Returns
/// 返回值
/// The matching attachment modality, or an explicit error for non-understanding capabilities.
/// 返回匹配的附件模态；非理解能力返回显式错误。
fn media_understanding_attachment_kind(
    capability: ProviderToolCapabilityKind,
) -> Result<ConversationAttachmentKind, String> {
    match capability {
        ProviderToolCapabilityKind::ImageUnderstand => Ok(ConversationAttachmentKind::Image),
        ProviderToolCapabilityKind::VideoUnderstand => Ok(ConversationAttachmentKind::Video),
        ProviderToolCapabilityKind::AudioUnderstand => Ok(ConversationAttachmentKind::Audio),
        _ => Err(format!(
            "Capability `{capability:?}` is not media understanding."
        )),
    }
}

/// Reports whether one frozen model and protocol route can execute a replacement media tool.
/// 报告一条冻结的模型与协议路由能否执行媒体替代理解工具。
///
/// # Parameters
/// 参数
/// `capabilities` is the effective model contract, `protocol_profile_id` is the frozen encoder,
/// and `capability` selects the exact media kind.
/// `capabilities` 是生效模型契约，`protocol_profile_id` 是冻结编码器，`capability` 选择精确媒体类型。
///
/// # Returns
/// 返回值
/// `true` only when conversation execution, media input, and the protocol encoder all exist.
/// 仅当会话执行、媒体输入与协议编码器全部存在时返回 `true`。
fn model_replacement_route_supports_media_understanding(
    capabilities: &ModelCapabilities,
    protocol_profile_id: &str,
    capability: ProviderToolCapabilityKind,
) -> bool {
    // attachmentKind binds the semantic facade to the same protocol encoder registry used by normal attachments.
    // attachmentKind 将语义外壳绑定到普通附件使用的同一协议编码器注册表。
    let attachment_kind = match media_understanding_attachment_kind(capability) {
        Ok(kind) => kind,
        Err(_) => return false,
    };
    let Some(resource_kind) = media_understanding_resource_kind(capability) else {
        return false;
    };
    capabilities
        .operations
        .contains(&AiOperation::ConversationRespond)
        && capabilities.input_resources.contains(&resource_kind)
        && crate::provider_input_normalizer::protocol_profile_supports_attachment_kind(
            protocol_profile_id,
            attachment_kind,
        )
}

/// Returns the fixed path argument for one understanding capability.
/// 返回一项理解能力的固定路径参数。
fn media_understanding_path_argument(
    capability: ProviderToolCapabilityKind,
) -> Option<&'static str> {
    match capability {
        ProviderToolCapabilityKind::ImageUnderstand => Some("image_path"),
        ProviderToolCapabilityKind::VideoUnderstand => Some("video_path"),
        ProviderToolCapabilityKind::AudioUnderstand => Some("audio_path"),
        _ => None,
    }
}

/// Returns the stable lower-case media label for one understanding capability.
/// 返回一项理解能力稳定的小写媒体标签。
fn media_understanding_media_label(
    capability: ProviderToolCapabilityKind,
) -> Result<&'static str, String> {
    match capability {
        ProviderToolCapabilityKind::ImageUnderstand => Ok("image"),
        ProviderToolCapabilityKind::VideoUnderstand => Ok("video"),
        ProviderToolCapabilityKind::AudioUnderstand => Ok("audio"),
        _ => Err(format!(
            "Capability `{capability:?}` is not media understanding."
        )),
    }
}

/// Returns the provider-neutral AI-facing description for one media tool and frozen route.
/// 返回一项媒体工具及冻结路线的供应商无关 AI 可见说明。
///
/// # Parameters
/// 参数
/// `capability` fixes the media kind, `protocol_profile_id` names the exact encoder, and
/// `constraints` contains only reviewed route limits.
/// `capability` 固定媒体类型，`protocol_profile_id` 命名精确编码器，`constraints`
/// 只包含已审核的路线限制。
///
/// # Returns
/// 返回值
/// A deterministic description that never invents absent MIME or size claims.
/// 返回一条确定性说明，绝不虚构缺失的 MIME 或大小声明。
fn media_understanding_description(
    capability: ProviderToolCapabilityKind,
    protocol_profile_id: &str,
    constraints: &[InputResourceConstraint],
) -> Result<String, String> {
    let media_label = media_understanding_media_label(capability)?;
    let resource_kind = media_understanding_resource_kind(capability)
        .ok_or_else(|| format!("Capability `{capability:?}` is not media understanding."))?;
    let constraint = constraints
        .iter()
        .find(|constraint| constraint.resource_kind == resource_kind);
    // acceptedMimeTypes is the exact intersection of the host resolver, protocol encoder, and
    // optional frozen route declaration; no provider capability is inferred from its name.
    // acceptedMimeTypes 是宿主解析器、协议编码器与可选冻结路线声明的精确交集；不会根据供应商名称推断能力。
    let accepted_mime_types =
        registered_media_attachment_mime_types(media_understanding_attachment_kind(capability)?)
            .iter()
            .copied()
            .filter(|mime_type| {
                vulcan_ai_protocols::protocol_profile_supports_local_media_type(
                    protocol_profile_id,
                    resource_kind,
                    mime_type,
                )
            })
            .filter(|mime_type| {
                constraint.is_none_or(|constraint| {
                    constraint.accepted_mime_types.is_empty()
                        || constraint
                            .accepted_mime_types
                            .iter()
                            .any(|accepted| accepted.eq_ignore_ascii_case(mime_type))
                })
            })
            .collect::<Vec<_>>();
    let mime_description = if accepted_mime_types.is_empty() {
        String::from("none in the host media format registry")
    } else {
        accepted_mime_types.join(", ")
    };
    let byte_description = constraint
        .and_then(|constraint| constraint.max_bytes)
        .map(|max_bytes| format!(" Maximum source size: {max_bytes} bytes."))
        .unwrap_or_default();
    let count_description = constraint
        .and_then(|constraint| constraint.max_count)
        .map(|max_count| format!(" Frozen route count limit: {max_count}."))
        .unwrap_or_default();
    Ok(format!(
        "Understand exactly one {media_label} using the configured media-understanding backend. Use a canonical `project-resource://` id, an owned `attachment://` id, or a readable local file path. Relative paths resolve from the active workspace; absolute and project-external paths are accepted until filesystem permissions are enabled. Accepted source MIME types: {mime_description}.{byte_description}{count_description} The host never calls this tool automatically; preserve the user's exact analysis intent in `prompt`."
    ))
}

/// Builds one direct provider-understanding description from exact operation and channel limits.
/// 根据精确操作与通道限制构建一条直连供应商理解工具说明。
///
/// # Parameters
/// 参数
/// `capability` fixes the media kind, `base_description` retains reviewed provider facts, and
/// `profile` contains the frozen non-secret upload-route intersections.
/// `capability` 固定媒体类型，`base_description` 保留已审核供应商事实，`profile` 包含冻结的
/// 非秘密上传路线交集。
///
/// # Returns
/// 返回值
/// One deterministic AI-facing description listing every actually usable host MIME and route limit.
/// 返回一条确定性的 AI 可见说明，列出每项实际可用宿主 MIME 与路线限制。
pub(crate) fn direct_media_understanding_description(
    capability: ProviderToolCapabilityKind,
    base_description: &str,
    profile: &ProviderToolMediaInputProfile,
) -> Result<String, String> {
    let attachment_kind = media_understanding_attachment_kind(capability)?;
    let resource_kind = media_understanding_resource_kind(capability)
        .ok_or_else(|| format!("Capability `{capability:?}` is not media understanding."))?;
    if profile.resource_kind != resource_kind {
        return Err(String::from(
            "Provider media input profile does not match the understanding capability.",
        ));
    }
    let registered = registered_media_attachment_mime_types(attachment_kind);
    let mut route_descriptions = Vec::with_capacity(profile.routes.len());
    for (index, route) in profile.routes.iter().enumerate() {
        // accepted retains only formats that both the canonical attachment resolver and exact
        // provider-tool route can consume.
        // accepted 仅保留规范附件解析器与精确供应商工具路线均可消费的格式。
        let accepted = registered
            .iter()
            .copied()
            .filter(|mime_type| {
                route.accepted_mime_types.is_empty()
                    || route
                        .accepted_mime_types
                        .iter()
                        .any(|accepted| accepted.eq_ignore_ascii_case(mime_type))
            })
            .collect::<Vec<_>>();
        if accepted.is_empty() {
            return Err(format!(
                "Provider media route {} has no MIME type shared with the host registry.",
                index + 1
            ));
        }
        let max_bytes = route
            .max_bytes
            .map(|value| format!(" maximum source size {value} bytes"))
            .unwrap_or_else(|| String::from(" no declared source-size maximum"));
        let max_count = route
            .max_count
            .map(|value| format!(" maximum source count {value}"))
            .unwrap_or_else(|| String::from(" no declared source-count maximum"));
        route_descriptions.push(format!(
            "Route {} accepts {};{};{}.",
            index + 1,
            accepted.join(", "),
            max_bytes,
            max_count
        ));
    }
    Ok(format!(
        "{} Use exactly one canonical `project-resource://` id, owned `attachment://` id, or readable local file path. Relative paths resolve from the active workspace; absolute and project-external paths are accepted until filesystem permissions are enabled. Provider file ids are rejected. {} The host never invokes this understanding tool automatically, so preserve the user's exact analysis intent.",
        base_description.trim(),
        route_descriptions.join(" ")
    ))
}

/// Validates one reasoning level against the exact replacement route.
/// 针对精确替代路由校验一个思考档位。
fn validate_media_replacement_reasoning(
    control: ReasoningControlMode,
    supported: &[ReasoningEffort],
    requested: ReasoningEffort,
) -> Result<(), String> {
    // valid applies the same closed semantics as ordinary configured inference.
    // valid 应用与普通已配置推理相同的封闭语义。
    let valid = match control {
        ReasoningControlMode::FixedOff => requested == ReasoningEffort::None,
        ReasoningControlMode::FixedOn => {
            requested != ReasoningEffort::None && supported.contains(&requested)
        }
        ReasoningControlMode::Toggle
        | ReasoningControlMode::EffortSelection
        | ReasoningControlMode::ProviderAdaptive => supported.contains(&requested),
        ReasoningControlMode::Unknown => false,
    };
    valid.then_some(()).ok_or_else(|| {
        format!(
            "Reasoning effort `{}` is not declared by the selected replacement model route.",
            requested.as_str()
        )
    })
}

/// Selects reasoning-output control only when the replacement request activates reasoning.
/// 仅在替代请求实际启用推理时选择推理输出控制。
///
/// # Parameters
/// 参数
/// `reasoning_effort` is the explicit provider-neutral effort selected for this media capability.
/// `reasoning_effort` 是为当前媒体能力显式选择的供应商无关推理档位。
///
/// # Returns
/// 返回值
/// Hidden reasoning for an active effort, otherwise the exact protocol profile default.
/// 推理已启用时隐藏推理输出，否则使用精确协议 Profile 的默认行为。
fn media_replacement_reasoning_output_policy(
    reasoning_effort: Option<ReasoningEffort>,
) -> ReasoningOutputPolicy {
    match reasoning_effort {
        Some(ReasoningEffort::Adaptive)
        | Some(ReasoningEffort::Minimal)
        | Some(ReasoningEffort::Low)
        | Some(ReasoningEffort::Medium)
        | Some(ReasoningEffort::High)
        | Some(ReasoningEffort::XHigh)
        | Some(ReasoningEffort::Max)
        | Some(ReasoningEffort::Ultra) => ReasoningOutputPolicy::Hidden,
        Some(ReasoningEffort::None) | None => ReasoningOutputPolicy::ProviderDefault,
    }
}

/// Resolves one AI-visible resource locator or local path into a canonical media attachment.
/// 将一个 AI 可见资源定位符或本地路径解析为规范媒体附件。
///
/// # Parameters
/// 参数
/// `workspace_root` owns the managed asset index and resolves relative paths,
/// `owner_session_id` is required only by project or attachment locators, `capability` fixes the
/// expected modality, and `reference` is the exact disclosed locator or path.
/// `workspace_root` 持有托管资产索引并解析相对路径，`owner_session_id` 仅供项目或附件定位符
/// 使用，`capability` 固定预期模态，`reference` 是已告知 AI 的精确定位符或路径。
///
/// # Returns
/// 返回值
/// The integrity-checked canonical attachment. Direct paths are read from the host only and are
/// replaced by the selected provider delivery channel before dispatch.
/// 返回通过完整性校验的规范附件。直接路径仅由宿主读取，并在分派前由所选供应商投递通道替换。
pub(crate) async fn resolve_media_understanding_attachment(
    workspace_root: Option<&Path>,
    owner_session_id: Option<&str>,
    capability: ProviderToolCapabilityKind,
    reference: &str,
    media_resource_runtime: Option<&dyn crate::media_resource_runtime::MediaResourceRuntime>,
) -> Result<vulcan_core::types::ConversationAttachment, String> {
    // Project resource locators use the V1 session authority; legacy asset locators retain their explicit legacy resolver.
    // 项目资源定位符使用V1会话权威；旧资产定位符保留其明确的旧解析器。
    if let Some(resource_id) =
        vulcan_workspace_store::workspace_store::parse_project_resource_locator(reference)?
    {
        let owner = owner_session_id.ok_or_else(|| {
            String::from("Project media resources require an owning conversation id.")
        })?;
        let runtime = media_resource_runtime.ok_or_else(|| {
            String::from("Project media resource resolution is unavailable for this invocation.")
        })?;
        return runtime
            .resolve_project_media(
                owner,
                resource_id,
                media_understanding_attachment_kind(capability)?,
            )
            .await;
    }
    let expected_kind = media_understanding_attachment_kind(capability)?;
    if reference.starts_with("attachment:") {
        let workspace_root = workspace_root.ok_or_else(|| {
            String::from("Managed media attachments require an active workspace root.")
        })?;
        let owner_session_id = owner_session_id.ok_or_else(|| {
            String::from("Managed media attachments require an owning conversation id.")
        })?;
        let workspace_path = vulcan_runtime_assets::native_host_path_text_from_path(workspace_root);
        let owner_session_id = owner_session_id.to_string();
        let reference = reference.to_string();
        return tokio::task::spawn_blocking(move || {
            crate::attachment_asset_index::resolve_owned_attachment_resource_metadata_for_kind(
                workspace_path.as_str(),
                owner_session_id.as_str(),
                reference.as_str(),
                expected_kind,
            )
        })
        .await
        .map_err(|error| format!("Media-understanding resource resolver task failed: {error}"))?;
    }
    resolve_direct_media_understanding_attachment(workspace_root, capability, reference).await
}

/// Resolves one explicit local path without applying attachment ownership semantics.
/// 在不应用附件所有权语义的情况下解析一条显式本地路径。
///
/// # Parameters
/// 参数
/// `workspace_root` is used only as the base for relative paths, `capability` fixes the required
/// media kind, and `reference` may address any readable host file.
/// `workspace_root` 仅作为相对路径的基准，`capability` 固定所需媒体类型，`reference` 可指向
/// 任意可读宿主文件。
///
/// # Returns
/// 返回值
/// A canonical attachment carrying exact file metadata and a content digest for safe delivery.
/// 返回携带精确文件元数据及内容摘要、可供安全投递的规范附件。
async fn resolve_direct_media_understanding_attachment(
    workspace_root: Option<&Path>,
    capability: ProviderToolCapabilityKind,
    reference: &str,
) -> Result<vulcan_core::types::ConversationAttachment, String> {
    // The shared provider-tool resolver intentionally treats the workspace as a relative-path base,
    // not an authorization boundary; future permissions must gate the canonical target separately.
    // 共享 Provider 工具解析器有意仅将工作区视为相对路径基准，而非授权边界；未来权限系统
    // 必须单独门控规范目标。
    let local_path =
        vulcan_provider_adapters::resolve_provider_tool_local_path(workspace_root, reference)
            .await?;
    let metadata = tokio::fs::metadata(&local_path)
        .await
        .map_err(|_| String::from("Media-understanding source could not be inspected."))?;
    if !metadata.is_file() {
        return Err(String::from(
            "Media-understanding source must be a regular file.",
        ));
    }
    let resource_kind = media_understanding_resource_kind(capability)
        .ok_or_else(|| format!("Capability `{capability:?}` is not media understanding."))?;
    let mime_type = media_understanding_mime_type(resource_kind, &local_path)?;
    let file_name = local_path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("Media-understanding source has no valid file name."))?
        .to_string();
    let digest_path = local_path.clone();
    let source_sha256 = tokio::task::spawn_blocking(move || streaming_file_sha256(&digest_path))
        .await
        .map_err(|_| String::from("Media-understanding digest task failed."))?
        .map_err(|_| String::from("Media-understanding source could not be read."))?;
    Ok(vulcan_core::types::ConversationAttachment {
        kind: media_understanding_attachment_kind(capability)?,
        file_name,
        local_path: vulcan_runtime_assets::native_host_path_text_from_path(&local_path),
        resource_path: None,
        source_path: Some(reference.to_string()),
        asset_id: None,
        byte_len: Some(metadata.len()),
        source_sha256: Some(source_sha256),
        delivery_mode: None,
        preview_local_path: None,
        mime_type: mime_type.to_string(),
    })
}

/// Resolves the safe AI-visible locator while carrying its private source metadata outside tool arguments.
/// 解析安全AI可见定位符，同时在工具参数之外传递其私有来源元数据。
///
/// # Parameters
/// 参数
/// `workspace_root` supplies relative-path context, `owner_session_id` supplies locator ownership,
/// `capability` fixes the modality, and `arguments` is the exact fixed-schema model call.
/// `workspace_root` 提供相对路径上下文，`owner_session_id` 提供定位符所有权，`capability`
/// 固定模态，`arguments` 是精确的固定 Schema 模型调用。
///
/// # Returns
/// 返回值
/// Canonical arguments plus an exact host source; private paths and source proofs never enter provider JSON.
/// 返回规范参数及精确宿主来源；私有路径和来源证据绝不进入供应商JSON。
pub(crate) async fn resolve_media_understanding_resource_arguments(
    workspace_root: Option<&Path>,
    owner_session_id: Option<&str>,
    capability: ProviderToolCapabilityKind,
    arguments: Value,
    media_resource_runtime: Option<&dyn crate::media_resource_runtime::MediaResourceRuntime>,
) -> Result<
    (
        Value,
        Vec<vulcan_provider_adapters::ProviderToolMediaSource>,
    ),
    String,
> {
    let path_argument = media_understanding_path_argument(capability)
        .ok_or_else(|| format!("Capability `{capability:?}` has no media path argument."))?;
    let reference = arguments
        .as_object()
        .and_then(|object| object.get(path_argument))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Media-understanding argument `{path_argument}` is required."))?;
    // A declared HTTPS source remains in operation arguments so the frozen manifest upload binding
    // can validate its passthrough primitive and host allowlist before provider dispatch.
    // 已声明的 HTTPS 来源保留在操作参数中，使冻结 Manifest 上传绑定能在供应商分派前校验直通原语与宿主白名单。
    if media_understanding_reference_is_remote_url(reference)? {
        return Ok((arguments, Vec::new()));
    }
    let attachment = resolve_media_understanding_attachment(
        workspace_root,
        owner_session_id,
        capability,
        reference,
        media_resource_runtime,
    )
    .await?;
    // The host proof binds the exact normalized argument to its catalog identity, not its digest filename.
    // 宿主证据将精确规范化参数绑定到目录身份，而非摘要文件名。
    let source = vulcan_provider_adapters::ProviderToolMediaSource {
        reference: reference.to_string(),
        local_path: PathBuf::from(attachment.local_path),
        file_name: attachment.file_name,
        mime_type: attachment.mime_type,
        source_identity: vulcan_provider_adapters::PreparedMediaSourceIdentity {
            byte_len: attachment.byte_len.ok_or_else(|| {
                String::from("Resolved media source has no registered byte length.")
            })?,
            content_sha256: attachment
                .source_sha256
                .ok_or_else(|| String::from("Resolved media source has no registered digest."))?,
        },
    };
    // Keep only the safe canonical locator in the operation argument projection.
    // 操作参数投影中只保留安全规范定位符。
    let mut resolved = arguments;
    resolved
        .as_object_mut()
        .ok_or_else(|| String::from("Media-understanding arguments must be a JSON object."))?
        .insert(
            path_argument.to_string(),
            Value::String(source.reference.clone()),
        );
    Ok((resolved, vec![source]))
}

/// Detects HTTP(S) media inputs before any local-path canonicalization.
/// 在执行任何本地路径规范化之前识别 HTTP(S) 媒体输入。
///
/// # Parameters
/// 参数
/// `reference` is the exact fixed-facade media string supplied by the AI.
/// `reference` 是 AI 向固定媒体外壳提供的精确媒体字符串。
///
/// # Returns
/// 返回值
/// `true` for a syntactically valid HTTPS URL that must be authorized by the frozen manifest
/// upload route, `false` for local or host-owned locators, and an explicit error for HTTP URLs.
/// 对必须由冻结 Manifest 上传路线授权的合法 HTTPS URL 返回 `true`，对本地或宿主持有定位符返回
/// `false`，对 HTTP URL 返回明确错误。
fn media_understanding_reference_is_remote_url(reference: &str) -> Result<bool, String> {
    // normalized_prefix performs only case-insensitive scheme classification; the URL parser remains authoritative.
    // normalized_prefix 只执行不区分大小写的 scheme 分类；URL 解析器仍是权威。
    let normalized_prefix = reference.to_ascii_lowercase();
    if !normalized_prefix.starts_with("http://") && !normalized_prefix.starts_with("https://") {
        return Ok(false);
    }
    // parsed_url rejects malformed network references before they can reach path handling.
    // parsed_url 在异常网络引用进入路径处理前拒绝它们。
    let parsed_url = reqwest::Url::parse(reference)
        .map_err(|_| String::from("Media-understanding source is not a valid HTTP(S) URL."))?;
    if parsed_url.scheme() != "https" {
        return Err(String::from(
            "Media-understanding remote sources require a manifest-declared HTTPS URL passthrough route; HTTP is not supported.",
        ));
    }
    Ok(true)
}

/// Resolves one exact MIME type from the shared host attachment format registry.
/// 从共享宿主附件格式注册表解析一个精确 MIME 类型。
fn media_understanding_mime_type(kind: ResourceKind, path: &Path) -> Result<&'static str, String> {
    // descriptor comes from the same closed registry used by browser staging and local-path attachments.
    // descriptor 来自浏览器暂存与本地路径附件共同使用的同一封闭注册表。
    let descriptor = attachment_descriptor_from_path(path).ok_or_else(|| {
        format!(
            "Extension for media-understanding source `{}` is not registered.",
            path.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("media")
        )
    })?;
    // expectedKind prevents ambiguous or mislabeled paths from crossing modality boundaries.
    // expectedKind 防止存在歧义或错误标注的路径跨越模态边界。
    let expected_kind = match kind {
        ResourceKind::Image => ConversationAttachmentKind::Image,
        ResourceKind::Video => ConversationAttachmentKind::Video,
        ResourceKind::Audio => ConversationAttachmentKind::Audio,
        ResourceKind::File => {
            return Err(String::from(
                "Media-understanding MIME resolution does not accept generic files.",
            ));
        }
    };
    if descriptor.kind != expected_kind {
        return Err(format!(
            "Media-understanding source format `{}` does not match media kind `{kind:?}`.",
            descriptor.mime_type
        ));
    }
    Ok(descriptor.mime_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_ai_contract::StructuredOutputCapability;

    mod project_resources;

    /// Verifies that backend switching cannot change any media-understanding input schema.
    /// 验证后端切换无法改变任何媒体理解输入 Schema。
    #[test]
    fn fixed_media_input_schemas_expose_only_path_and_prompt() {
        for (capability, path_argument) in [
            (ProviderToolCapabilityKind::ImageUnderstand, "image_path"),
            (ProviderToolCapabilityKind::VideoUnderstand, "video_path"),
            (ProviderToolCapabilityKind::AudioUnderstand, "audio_path"),
        ] {
            // schema is the public contract shared by every backend mode.
            // schema 是全部后端模式共享的公开契约。
            let schema = media_understanding_input_schema(capability).expect("media schema");
            // properties is the exact model-visible parameter set.
            // properties 是精确模型可见参数集合。
            let properties = schema["properties"].as_object().expect("properties");
            assert_eq!(properties.len(), 2);
            assert!(properties.contains_key(path_argument));
            assert!(properties.contains_key("prompt"));
            assert!(
                properties[path_argument]["description"]
                    .as_str()
                    .is_some_and(|description| description.contains("project-external paths"))
            );
            assert_eq!(schema["additionalProperties"], Value::Bool(false));
        }
    }

    /// Verifies media MIME resolution shares the attachment registry and rejects cross-kind ambiguity.
    /// 验证媒体 MIME 解析共享附件注册表，并拒绝跨类型歧义。
    #[test]
    fn media_mime_resolution_uses_shared_attachment_registry() {
        assert_eq!(
            media_understanding_mime_type(ResourceKind::Video, Path::new("clip.m4v"))
                .expect("M4V should resolve for video"),
            "video/x-m4v"
        );
        assert_eq!(
            media_understanding_mime_type(ResourceKind::Audio, Path::new("voice.opus"))
                .expect("Opus should resolve for audio"),
            "audio/opus"
        );
        assert!(
            media_understanding_mime_type(ResourceKind::Audio, Path::new("ambiguous.mp4")).is_err()
        );
    }

    /// Rejects a replacement result that tries to add backend-specific fields.
    /// 拒绝尝试增加后端专属字段的替代理解结果。
    #[test]
    fn canonical_media_result_rejects_backend_specific_fields() {
        // payload contains one forbidden provider-specific model field.
        // payload 包含一个禁止的供应商专属模型字段。
        let payload = json!({
            "schema_version": "vulcan.media-understanding.v1",
            "media_kind": "video",
            "answer": "summary",
            "segments": [],
            "warnings": [],
            "model": "hidden-backend"
        });
        assert!(
            validate_media_understanding_payload(
                &payload,
                ProviderToolCapabilityKind::VideoUnderstand,
            )
            .is_err()
        );
    }

    /// Rejects invalid timeline ordering even when the provider returns valid JSON.
    /// 即使供应商返回有效 JSON，也拒绝无效时间线顺序。
    #[test]
    fn canonical_media_result_rejects_reversed_segment_time() {
        let payload = json!({
            "schema_version": "vulcan.media-understanding.v1",
            "media_kind": "audio",
            "answer": "transcript summary",
            "segments": [{
                "start_ms": 2000,
                "end_ms": 1000,
                "content": "out of order"
            }],
            "warnings": []
        });

        assert!(
            validate_media_understanding_payload(
                &payload,
                ProviderToolCapabilityKind::AudioUnderstand,
            )
            .is_err()
        );
    }

    /// Wraps plain answer text in the host-owned media-understanding contract.
    /// 将纯文本回答包装到宿主持有的媒体理解契约中。
    #[test]
    fn replacement_plain_text_is_wrapped_by_the_host_contract() {
        let payload = finalize_media_understanding_output(
            "  Two birds appear near the center of the frame.  ",
            ProviderToolCapabilityKind::VideoUnderstand,
        )
        .expect("plain-text replacement output should be canonicalized");

        assert_eq!(payload["schema_version"], "vulcan.media-understanding.v1");
        assert_eq!(payload["media_kind"], "video");
        assert_eq!(
            payload["answer"],
            "Two birds appear near the center of the frame."
        );
        assert_eq!(payload["segments"], json!([]));
        assert_eq!(payload["warnings"], json!([]));
    }

    /// Preserves JSON-looking model text as answer text instead of interpreting an undeclared schema.
    /// 将看似 JSON 的模型文本保留为回答文本，而不是解释未声明的 Schema。
    #[test]
    fn replacement_json_looking_text_remains_answer_text() {
        // payload proves JSON-looking characters stay inside the host-owned answer field.
        // payload 证明看似 JSON 的字符仍保留在宿主持有的回答字段中。
        let payload = finalize_media_understanding_output(
            r#"{"summary":"A bird crosses the frame."}"#,
            ProviderToolCapabilityKind::VideoUnderstand,
        )
        .expect("JSON-looking text is still a valid plain-text answer");

        assert_eq!(
            payload["answer"],
            r#"{"summary":"A bird crosses the frame."}"#
        );
    }

    /// Rejects terminal completion without an answer instead of reporting a JSON parser location.
    /// 拒绝没有回答的终态完成，而不是报告 JSON 解析器位置。
    #[test]
    fn replacement_empty_output_reports_the_missing_result_channel() {
        assert_eq!(
            finalize_media_understanding_output("  ", ProviderToolCapabilityKind::VideoUnderstand,)
                .expect_err("empty replacement output must fail"),
            "Media-understanding replacement returned no answer text."
        );
    }

    /// Rejects the first text fragment that would exceed the whole-document host ceiling.
    /// 拒绝首个会使整份文档超过宿主上限的文本分片。
    #[test]
    fn replacement_text_output_is_bounded_during_streaming() {
        // output starts exactly at the allowed boundary so the next non-empty byte must fail.
        // output 从允许边界精确开始，因此下一个非空字节必须失败。
        let mut output = "x".repeat(MEDIA_UNDERSTANDING_OUTPUT_DOCUMENT_BYTE_LIMIT);

        append_bounded_media_output_text(&mut output, "")
            .expect("an empty boundary fragment should remain valid");
        assert!(append_bounded_media_output_text(&mut output, "x").is_err());
        assert_eq!(output.len(), MEDIA_UNDERSTANDING_OUTPUT_DOCUMENT_BYTE_LIMIT);
    }

    /// Verifies internal media requests control reasoning output only for an active effort.
    /// 验证内部媒体请求仅在推理档位实际启用时控制推理输出。
    #[test]
    fn replacement_reasoning_output_requires_an_active_effort() {
        assert_eq!(
            media_replacement_reasoning_output_policy(None),
            ReasoningOutputPolicy::ProviderDefault
        );
        assert_eq!(
            media_replacement_reasoning_output_policy(Some(ReasoningEffort::None)),
            ReasoningOutputPolicy::ProviderDefault
        );
        assert_eq!(
            media_replacement_reasoning_output_policy(Some(ReasoningEffort::Adaptive)),
            ReasoningOutputPolicy::Hidden
        );
    }

    /// Verifies replacement admission intersects model declarations with the frozen protocol encoder.
    /// 验证替代理解准入会把模型声明与冻结协议编码器取交集。
    #[test]
    fn replacement_route_requires_protocol_media_encoder() {
        // capabilities deliberately declare video so the protocol is the only differing gate.
        // capabilities 有意声明视频能力，使协议成为唯一差异门槛。
        let capabilities = ModelCapabilities {
            operations: vec![AiOperation::ConversationRespond],
            input_resources: vec![ResourceKind::Video],
            structured_output: StructuredOutputCapability::Unsupported,
            ..ModelCapabilities::default()
        };

        assert!(model_replacement_route_supports_media_understanding(
            &capabilities,
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,
            ProviderToolCapabilityKind::VideoUnderstand,
        ));
        assert!(!model_replacement_route_supports_media_understanding(
            &capabilities,
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID,
            ProviderToolCapabilityKind::VideoUnderstand,
        ));
    }

    /// Verifies AI-facing tool documentation exposes the exact host, protocol, and route MIME intersection.
    /// 验证 AI 可见工具说明会公布宿主、协议与路线三者的精确 MIME 交集。
    #[test]
    fn media_description_lists_exact_frozen_route_mime_types() {
        let constraints = vec![InputResourceConstraint {
            resource_kind: ResourceKind::Image,
            accepted_mime_types: vec![
                String::from("image/png"),
                String::from("image/webp"),
                String::from("image/svg+xml"),
            ],
            max_bytes: Some(1_048_576),
            max_count: Some(2),
            reference_policy: Default::default(),
            upload_channel_refs: Vec::new(),
        }];

        let description = media_understanding_description(
            ProviderToolCapabilityKind::ImageUnderstand,
            vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID,
            constraints.as_slice(),
        )
        .expect("image understanding description");

        assert!(description.contains("Accepted source MIME types: image/png, image/webp."));
        assert!(!description.contains("image/svg+xml"));
        assert!(description.contains("Maximum source size: 1048576 bytes."));
        assert!(description.contains("Frozen route count limit: 2."));
        assert!(description.contains("absolute and project-external paths are accepted"));
    }

    /// Verifies direct-provider documentation intersects host formats and keeps per-route limits.
    /// 验证直连供应商说明会取宿主格式交集，并保留逐路线限制。
    #[test]
    fn direct_media_description_lists_exact_operation_route_constraints() {
        let profile = ProviderToolMediaInputProfile {
            resource_kind: ResourceKind::Video,
            routes: vec![
                vulcan_provider_adapters::ProviderToolMediaInputRouteConstraint {
                    upload_channel_id: String::from("video-a"),
                    accepted_mime_types: vec![
                        String::from("video/mp4"),
                        String::from("application/octet-stream"),
                    ],
                    max_bytes: Some(8_388_608),
                    max_count: Some(1),
                },
            ],
        };

        let description = direct_media_understanding_description(
            ProviderToolCapabilityKind::VideoUnderstand,
            "Understand one video.",
            &profile,
        )
        .expect("direct media description");

        assert!(description.contains("Route 1 accepts video/mp4;"));
        assert!(!description.contains("application/octet-stream"));
        assert!(description.contains("maximum source size 8388608 bytes"));
        assert!(description.contains("maximum source count 1"));
        assert!(description.contains("Provider file ids are rejected"));
        assert!(description.contains("absolute and project-external paths are accepted"));
    }

    /// Verifies model replacement accepts every extended video container recognized for Kimi uploads.
    /// 验证模型替代路径接受 Kimi 上传识别的全部扩展视频容器。
    #[test]
    fn replacement_mime_map_covers_extended_video_containers() {
        for (extension, expected_mime) in [
            ("mpeg", "video/mpeg"),
            ("mpg", "video/mpeg"),
            ("flv", "video/x-flv"),
            ("wmv", "video/x-ms-wmv"),
            ("3gp", "video/3gpp"),
            ("3gpp", "video/3gpp"),
            ("m4v", "video/x-m4v"),
            ("3g2", "video/3gpp2"),
        ] {
            // path supplies only the extension because MIME resolution must not inspect file bytes here.
            // path 仅提供扩展名，因为此处 MIME 解析不得读取文件字节。
            let path = PathBuf::from(format!("sample.{extension}"));
            assert_eq!(
                media_understanding_mime_type(ResourceKind::Video, path.as_path())
                    .expect("extended video MIME should resolve"),
                expected_mime
            );
        }
    }
}
