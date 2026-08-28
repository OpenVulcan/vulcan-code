//! Fixed-schema image, video, and audio understanding backends.
//! 固定 Schema 的图片、视频与音频理解后端。

use crate::attachment_assets::{
    attachment_descriptor_from_path, registered_media_attachment_mime_types,
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
    StructuredOutputCapability, StructuredOutputPolicy, ToolChoicePolicy,
};
use vulcan_core::types::ConversationAttachmentKind;
use vulcan_provider_adapters::{
    ActiveAiModelSelection, InputResourceConstraint, MediaUploadCancellation,
    PersistedAiConfiguration, ProviderToolCapabilityKind, ProviderToolExecutionResult,
    ProviderToolMediaInputProfile, ProviderToolMediaInputProjection, ProviderToolNormalizedOutput,
    PublicAiRuntime, ResolvedPublicAiSelection,
};
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolHostResult,
    VulcanToolInvocationResult, VulcanToolSourceKind,
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
/// Maximum serialized byte length accepted for one complete replacement-model document.
/// 单份模型替代理解完整文档允许的最大序列化字节长度。
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
                "description": "Canonical project-relative attachment resource path or attachment:// resource id disclosed in the conversation. Absolute paths are not accepted."
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
            // invocationRuntime owns one isolated physical model execution.
            // invocationRuntime 持有一次隔离的真实模型执行。
            let invocation_runtime = Arc::clone(&runtime);
            // invocationSelection retains the immutable account route selected in settings.
            // invocationSelection 保留设置中选择的不可变账号路由。
            let invocation_selection = resolved_selection.clone();
            // invocationWorkspace remains the only local filesystem boundary.
            // invocationWorkspace 保持为唯一本地文件系统边界。
            let invocation_workspace = workspace_root.clone();
            Box::pin(async move {
                execute_model_replacement_media_understanding(
                    invocation_runtime,
                    invocation_workspace.as_deref(),
                    capability,
                    invocation_selection,
                    reasoning_effort,
                    arguments,
                    Some(context.session_id),
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
    // attachment resolves only a canonical resource owned by the invoking conversation.
    // attachment 只解析由发起调用会话持有的规范资源。
    let attachment = resolve_media_understanding_attachment(
        workspace_root,
        owner_session_id.as_deref(),
        capability,
        media_reference,
    )
    .await?;
    let media_path = PathBuf::from(attachment.local_path.as_str());
    // resourceKind is the exact provider-neutral media category.
    // resourceKind 是精确的供应商无关媒体类别。
    let resource_kind = media_understanding_resource_kind(capability)
        .ok_or_else(|| format!("Capability `{capability:?}` has no media resource kind."))?;
    // mimeType is the canonical staged MIME and must also agree with the closed host format registry.
    // mimeType 是规范暂存 MIME，且必须同时与封闭宿主格式注册表一致。
    let registered_mime_type = media_understanding_mime_type(resource_kind, &media_path)?;
    if registered_mime_type != attachment.mime_type {
        return Err(String::from(
            "Canonical attachment MIME no longer matches the registered media format.",
        ));
    }
    let mime_type = attachment.mime_type.as_str();
    // protocolProfileId is the exact immutable encoder selected with the replacement account route.
    // protocolProfileId 是随替代账号路由选定的精确不可变编码器。
    let protocol_profile_id = resolved_selection
        .route
        .protocol_route
        .protocol_profile_id
        .as_str();
    if !vulcan_ai_protocols::protocol_profile_supports_local_media_type(
        protocol_profile_id,
        resource_kind,
        mime_type,
    ) {
        return Err(format!(
            "Replacement protocol profile `{protocol_profile_id}` cannot encode `{mime_type}` as `{resource_kind:?}`; media upload was not started."
        ));
    }
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
        file_name: media_path
            .file_name()
            .and_then(|value| value.to_str())
            .map(str::to_owned),
        image_detail: None,
        resource_path: attachment.resource_path.clone(),
        local_path: None,
        url: resource_url,
    };
    // outputSchema is the invariant canonical media-understanding result contract.
    // outputSchema 是不变的规范媒体理解结果契约。
    let output_schema = media_understanding_output_schema(capability)?;
    // structuredOutput uses native JSON Schema when available and otherwise JSON Object mode.
    // structuredOutput 在可用时使用原生 JSON Schema，否则使用 JSON Object 模式。
    let structured_output = match resolved_selection.model.capabilities.structured_output {
        StructuredOutputCapability::JsonSchema => StructuredOutputPolicy::JsonSchema {
            name: String::from("media_understanding_result"),
            description: Some(String::from(
                "Canonical image, video, or audio understanding result.",
            )),
            schema: output_schema.clone(),
            strict: true,
        },
        StructuredOutputCapability::JsonObject => StructuredOutputPolicy::JsonObject,
        StructuredOutputCapability::Unsupported => {
            return Err(String::from(
                "The selected replacement model no longer supports stable structured output.",
            ));
        }
    };
    // modelPrompt requires the same schema when only JSON Object mode is available.
    // modelPrompt 在仅有 JSON Object 模式时仍要求相同 Schema。
    let model_prompt = format!(
        "Act as a media-understanding backend. Return only one JSON object matching this exact schema: {}\n\nUser instruction: {}",
        serde_json::to_string(&output_schema).map_err(|error| error.to_string())?,
        prompt.trim()
    );
    // request contains one actual media input and never exposes backend controls to the caller.
    // request 包含一个真实媒体输入，绝不向调用方暴露后端控件。
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
        structured_output,
        stop_sequences: Vec::new(),
        prompt_cache: PromptCachePolicy::default(),
        reasoning_effort,
        reasoning_output: ReasoningOutputPolicy::Hidden,
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
    // outputText accumulates JSON text when no StructuredJson item is emitted.
    // outputText 在未产生 StructuredJson 项时累积 JSON 文本。
    let mut output_text = String::new();
    // structuredValue retains the sole complete structured item and rejects duplicates immediately.
    // structuredValue 保留唯一完整结构化项，并立即拒绝重复项。
    let mut structured_value = None;
    // completed distinguishes terminal completion from a truncated stream.
    // completed 区分终态完成与截断流。
    let mut completed = false;
    while let Some(next_event) = stream.next().await {
        // event is one normalized provider item or a controlled transport failure.
        // event 是一项归一化供应商事件或受控传输失败。
        let event = next_event.map_err(|failure| failure.to_string())?;
        match event {
            AiStreamEvent::ResponseItemStarted { item, .. } => match item {
                ContentItem::StructuredJson { value } => {
                    record_structured_media_output(&mut structured_value, value)?
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
    // payload accepts exactly one native structured item or one textual JSON document, never both.
    // payload 仅接受一个原生结构化项或一个文本 JSON 文档，绝不同时接受两种通道。
    let payload = finalize_media_understanding_output(structured_value, output_text.as_str())?;
    validate_media_understanding_payload(&payload, capability)?;
    Ok(media_understanding_invocation_result(payload))
}

/// Records the sole native structured result emitted by a replacement-model stream.
/// 记录替代模型流发出的唯一原生结构化结果。
///
/// # Parameters
/// 参数
/// `slot` owns the previously observed structured item and `value` is the newly emitted item.
/// `slot` 持有此前观察到的结构化项，`value` 是新发出的结构化项。
///
/// # Returns
/// 返回值
/// `Ok(())` for the first item, or an explicit error when the stream emits multiple JSON values.
/// 首个结构化项返回 `Ok(())`；流发出多个 JSON 值时返回明确错误。
fn record_structured_media_output(slot: &mut Option<Value>, value: Value) -> Result<(), String> {
    if slot.is_some() {
        return Err(String::from(
            "Media-understanding replacement returned multiple structured JSON items.",
        ));
    }
    // serializedLength applies the same whole-document ceiling to native structured events.
    // serializedLength 对原生结构化事件应用相同的整份文档上限。
    let serialized_length = serde_json::to_vec(&value)
        .map_err(|error| format!("Media-understanding structured output is invalid: {error}"))?
        .len();
    if serialized_length > MEDIA_UNDERSTANDING_OUTPUT_DOCUMENT_BYTE_LIMIT {
        return Err(String::from(
            "Media-understanding replacement structured output exceeds the host byte limit.",
        ));
    }
    *slot = Some(value);
    Ok(())
}

/// Appends one text event while enforcing the whole-document byte ceiling before allocation.
/// 在分配前执行整份文档字节上限，并追加一个文本事件。
///
/// # Parameters
/// 参数
/// `output` owns prior UTF-8 JSON fragments and `fragment` is the next normalized stream event.
/// `output` 持有此前 UTF-8 JSON 分片，`fragment` 是下一项归一化流事件。
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

/// Finalizes exactly one replacement-model result channel into a JSON value.
/// 将唯一的替代模型结果通道终结为 JSON 值。
///
/// # Parameters
/// 参数
/// `structured_value` is the optional native item and `output_text` is the accumulated text channel.
/// `structured_value` 是可选原生结构化项，`output_text` 是累积文本通道。
///
/// # Returns
/// 返回值
/// One JSON value, or an explicit error for mixed channels or invalid textual JSON.
/// 返回一个 JSON 值；混合通道或无效文本 JSON 时返回明确错误。
fn finalize_media_understanding_output(
    structured_value: Option<Value>,
    output_text: &str,
) -> Result<Value, String> {
    if let Some(value) = structured_value {
        if !output_text.trim().is_empty() {
            return Err(String::from(
                "Media-understanding replacement mixed structured JSON with text output.",
            ));
        }
        return Ok(value);
    }
    serde_json::from_str(output_text.trim())
        .map_err(|error| format!("Media-understanding replacement returned invalid JSON: {error}"))
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
    Ok(media_understanding_invocation_result(payload))
}

/// Wraps one validated canonical payload for model and host consumers.
/// 为模型与宿主消费者包装一项已校验规范载荷。
fn media_understanding_invocation_result(payload: Value) -> VulcanToolInvocationResult {
    VulcanToolInvocationResult::from_model_output(payload.clone()).with_host_result(
        VulcanToolHostResult {
            kind: String::from("media_understanding_output"),
            payload,
        },
    )
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

/// Builds the invariant structured result schema for one media capability.
/// 为一项媒体能力构建不变的结构化结果 Schema。
fn media_understanding_output_schema(
    capability: ProviderToolCapabilityKind,
) -> Result<Value, String> {
    // mediaKind is the only allowed semantic media label.
    // mediaKind 是唯一允许的语义媒体标签。
    let media_kind = media_understanding_media_label(capability)?;
    Ok(json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "schema_version": { "type": "string", "enum": ["vulcan.media-understanding.v1"] },
            "media_kind": { "type": "string", "enum": [media_kind] },
            "answer": {
                "type": "string",
                "minLength": 1,
                "maxLength": MEDIA_UNDERSTANDING_ANSWER_BYTE_LIMIT
            },
            "segments": {
                "type": "array",
                "maxItems": MEDIA_UNDERSTANDING_SEGMENT_LIMIT,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "start_ms": { "type": ["integer", "null"], "minimum": 0 },
                        "end_ms": { "type": ["integer", "null"], "minimum": 0 },
                        "content": {
                            "type": "string",
                            "minLength": 1,
                            "maxLength": MEDIA_UNDERSTANDING_SEGMENT_BYTE_LIMIT
                        }
                    },
                    "required": ["start_ms", "end_ms", "content"]
                }
            },
            "warnings": {
                "type": "array",
                "maxItems": MEDIA_UNDERSTANDING_WARNING_LIMIT,
                "items": {
                    "type": "string",
                    "maxLength": MEDIA_UNDERSTANDING_WARNING_BYTE_LIMIT
                }
            }
        },
        "required": ["schema_version", "media_kind", "answer", "segments", "warnings"]
    }))
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
/// `true` only when conversation execution, media input, protocol encoding, and stable JSON output all exist.
/// 仅当会话执行、媒体输入、协议编码与稳定 JSON 输出全部存在时返回 `true`。
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
        && crate::provider_input_normalizer::protocol_profile_supports_structured_output(
            protocol_profile_id,
            capabilities.structured_output,
        )
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
        "Understand exactly one {media_label} using the configured media-understanding backend. Use only a canonical project-relative resource path or `attachment://` id disclosed in this conversation; absolute paths are rejected. Accepted source MIME types: {mime_description}.{byte_description}{count_description} The host never calls this tool automatically; preserve the user's exact analysis intent in `prompt`."
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
        "{} Use exactly one conversation-owned canonical project-relative resource path or `attachment://` id; absolute paths and Provider file ids are rejected. {} The host never invokes this understanding tool automatically, so preserve the user's exact analysis intent.",
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

/// Resolves one AI-visible resource locator into an owned canonical media attachment.
/// 将一个 AI 可见资源定位符解析为归当前会话所有的规范媒体附件。
///
/// # Parameters
/// 参数
/// `workspace_root` owns the managed asset index, `owner_session_id` is the invoking conversation,
/// `capability` fixes the expected modality, and `reference` is the disclosed safe locator.
/// `workspace_root` 持有托管资产索引，`owner_session_id` 是发起调用的会话，`capability`
/// 固定预期模态，`reference` 是已告知 AI 的安全定位符。
///
/// # Returns
/// 返回值
/// The integrity-checked canonical attachment without exposing its provider reference to the model.
/// 返回通过完整性校验的规范附件，且不向模型暴露供应商引用。
pub(crate) async fn resolve_media_understanding_attachment(
    workspace_root: Option<&Path>,
    owner_session_id: Option<&str>,
    capability: ProviderToolCapabilityKind,
    reference: &str,
) -> Result<vulcan_core::types::ConversationAttachment, String> {
    let workspace_root = workspace_root.ok_or_else(|| {
        String::from("Media-understanding resources require an active workspace root.")
    })?;
    let owner_session_id = owner_session_id.ok_or_else(|| {
        String::from("Media-understanding resources require an owning conversation id.")
    })?;
    let expected_kind = media_understanding_attachment_kind(capability)?;
    let workspace_path = vulcan_runtime_assets::native_host_path_text_from_path(workspace_root);
    let owner_session_id = owner_session_id.to_string();
    let reference = reference.to_string();
    tokio::task::spawn_blocking(move || {
        crate::attachment_asset_index::resolve_owned_attachment_resource(
            workspace_path.as_str(),
            owner_session_id.as_str(),
            reference.as_str(),
            expected_kind,
        )
    })
    .await
    .map_err(|error| format!("Media-understanding resource resolver task failed: {error}"))?
}

/// Rewrites the safe AI-visible locator in one media-tool argument object to its internal local path.
/// 将媒体工具参数对象中的安全 AI 可见定位符改写为内部本地路径。
///
/// # Parameters
/// 参数
/// `workspace_root`, `owner_session_id`, and `capability` define the ownership boundary; `arguments`
/// is the exact fixed-schema model call.
/// `workspace_root`、`owner_session_id` 与 `capability` 定义所有权边界；`arguments` 是精确的
/// 固定 Schema 模型调用。
///
/// # Returns
/// 返回值
/// A request-local argument copy containing an internal path for the provider adapter only.
/// 返回仅供 Provider Adapter 使用、包含内部路径的单次请求局部参数副本。
pub(crate) async fn resolve_media_understanding_resource_arguments(
    workspace_root: Option<&Path>,
    owner_session_id: Option<&str>,
    capability: ProviderToolCapabilityKind,
    arguments: Value,
) -> Result<Value, String> {
    let path_argument = media_understanding_path_argument(capability)
        .ok_or_else(|| format!("Capability `{capability:?}` has no media path argument."))?;
    let reference = arguments
        .as_object()
        .and_then(|object| object.get(path_argument))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Media-understanding argument `{path_argument}` is required."))?;
    let attachment = resolve_media_understanding_attachment(
        workspace_root,
        owner_session_id,
        capability,
        reference,
    )
    .await?;
    let mut resolved = arguments;
    resolved
        .as_object_mut()
        .ok_or_else(|| String::from("Media-understanding arguments must be a JSON object."))?
        .insert(
            path_argument.to_string(),
            Value::String(attachment.local_path),
        );
    Ok(resolved)
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

    /// Rejects multiple native structured values instead of silently accepting the last one.
    /// 拒绝多个原生结构化值，而不是静默接受最后一个。
    #[test]
    fn replacement_result_rejects_multiple_structured_items() {
        // slot represents the single result channel after its first complete value.
        // slot 表示已经接收首个完整值的单一结果通道。
        let mut slot = Some(json!({ "answer": "first" }));

        assert!(record_structured_media_output(&mut slot, json!({ "answer": "second" })).is_err());
    }

    /// Rejects a response that combines native structured JSON with non-empty text.
    /// 拒绝同时包含原生结构化 JSON 与非空文本的响应。
    #[test]
    fn replacement_result_rejects_mixed_output_channels() {
        assert!(
            finalize_media_understanding_output(
                Some(json!({ "answer": "structured" })),
                r#"{"answer":"text"}"#,
            )
            .is_err()
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

    /// Verifies replacement admission intersects model declarations with the frozen protocol encoder.
    /// 验证替代理解准入会把模型声明与冻结协议编码器取交集。
    #[test]
    fn replacement_route_requires_protocol_media_encoder() {
        // capabilities deliberately declare video so the protocol is the only differing gate.
        // capabilities 有意声明视频能力，使协议成为唯一差异门槛。
        let capabilities = ModelCapabilities {
            operations: vec![AiOperation::ConversationRespond],
            input_resources: vec![ResourceKind::Video],
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
