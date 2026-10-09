//! Exact target projection for the private typed media envelope.
//! 私有强类型媒体封装的精确目标投影。

use crate::{
    ANTHROPIC_MESSAGES_PROFILE_ID, ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
    CLAUDE_CODE_MESSAGES_PROFILE_ID, EncodedWireRequest, GEMINI_GENERATE_CONTENT_PROFILE_ID,
    GEMINI_INTERACTIONS_PROFILE_ID, GOOGLE_CODE_ASSIST_PROFILE_ID, INTERNAL_RESOURCE_PART_TYPE,
    InternalResponsesRequest, LlmCoreError, OPENAI_CHAT_PROFILE_ID,
    OPENAI_CODEX_RESPONSES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID, ProviderRouteContext,
    UpstreamProtocol, converted_protocol_headers, gemini_generate_content_path,
};
use serde::Deserialize;
use serde_json::Value;
use vulcan_ai_contract::{ResourceKind, ResourceReference, ResourceSourceKind};

#[cfg(test)]
mod tests;

/// Exact private resource shape; unknown envelope fields cannot become protocol controls.
/// 精确私有资源结构；未知封装字段不能成为协议控制项。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InternalResourcePart {
    /// Closed internal tag, validated before materialization.
    /// 封闭内部标签，在物化前校验。
    #[serde(rename = "type")]
    part_type: String,
    /// Exact index into the immutable request-local typed resource table.
    /// 不可变请求局部强类型资源表中的精确索引。
    resource_index: usize,
}

/// Explicit target profile available only to the internal conversion entry.
/// 仅内部转换入口可使用的显式目标 Profile。
#[derive(Clone, Copy)]
pub(crate) struct InternalMediaProjector<'a> {
    /// Frozen exact profile, including distinctions within one protocol family.
    /// 冻结的精确 Profile，包括同一协议族内的差异。
    profile_id: &'a str,
    /// Canonical resources owned by this exact internal request, not looked up from mutable state.
    /// 此精确内部请求持有的规范资源，不从可变状态查询。
    resources: &'a [ResourceReference],
}

impl<'a> InternalMediaProjector<'a> {
    /// Validates `profile_id` against `upstream`; returns no guessed protocol or fallback.
    /// 根据 `upstream` 校验 `profile_id`，不返回猜测协议或回退目标。
    /// `resources` is the immutable table owned by this exact internal request.
    /// `resources` 是此精确内部请求持有的不可变资源表。
    fn new(
        profile_id: &'a str,
        upstream: UpstreamProtocol,
        resources: &'a [ResourceReference],
    ) -> Result<Self, LlmCoreError> {
        // Profile identity, not a provider name or a broad capability flag, selects the mapping.
        // 映射由 Profile 身份选择，而不是供应商名称或宽泛能力标记。
        let matches = match upstream {
            UpstreamProtocol::OpenAiResponses => matches!(
                profile_id,
                OPENAI_RESPONSES_PROFILE_ID | OPENAI_CODEX_RESPONSES_PROFILE_ID
            ),
            UpstreamProtocol::OpenAiChatCompletions => profile_id == OPENAI_CHAT_PROFILE_ID,
            UpstreamProtocol::AnthropicMessages => matches!(
                profile_id,
                ANTHROPIC_MESSAGES_PROFILE_ID
                    | ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
                    | CLAUDE_CODE_MESSAGES_PROFILE_ID
            ),
            UpstreamProtocol::GeminiGenerateContent => {
                profile_id == GEMINI_GENERATE_CONTENT_PROFILE_ID
            }
            UpstreamProtocol::GeminiInteractions => profile_id == GEMINI_INTERACTIONS_PROFILE_ID,
            UpstreamProtocol::GoogleCodeAssist => profile_id == GOOGLE_CODE_ASSIST_PROFILE_ID,
        };
        if !matches {
            return Err(LlmCoreError::new(format!(
                "Protocol profile `{profile_id}` does not match the selected internal request target."
            )));
        }
        Ok(Self {
            profile_id,
            resources,
        })
    }

    /// Converts private `part` only in a user input position; returns an exact native media block.
    /// 仅在用户输入位置转换私有 `part`，返回精确原生媒体块。
    pub(crate) fn project(self, part: &Value, role: &str) -> Result<Value, LlmCoreError> {
        if role != "user" {
            return Err(LlmCoreError::new(
                "Internal media resources are only allowed in user input positions.",
            ));
        }
        // Typed decoding rejects malformed private envelopes before any target encoding.
        // 强类型解码在目标编码之前拒绝格式错误的私有封装。
        let envelope: InternalResourcePart =
            serde_json::from_value(part.clone()).map_err(|error| {
                LlmCoreError::new(format!("Invalid internal media envelope: {error}"))
            })?;
        if envelope.part_type != INTERNAL_RESOURCE_PART_TYPE {
            return Err(LlmCoreError::new(
                "Internal resource tag does not match its envelope.",
            ));
        }
        // Only the immutable owner table may resolve an internal index; missing entries are errors.
        // 仅不可变所有者表能解析内部索引；缺失条目属于错误。
        let resource = self.resources.get(envelope.resource_index).ok_or_else(|| {
            LlmCoreError::new("Internal media index is outside its request-owned resource table.")
        })?;
        self.project_resource(resource)
    }

    /// Encodes verified `resource` for this profile without changing its media kind or source identity.
    /// 为本 Profile 编码已验证 `resource`，不改变其媒体类型或来源身份。
    /// Returns an exact block or an explicit unsupported source/MIME/protocol error.
    /// 返回精确内容块，或明确的不支持来源、MIME、协议错误。
    fn project_resource(self, resource: &ResourceReference) -> Result<Value, LlmCoreError> {
        validate_resource_source(resource)?;
        match self.profile_id {
            OPENAI_RESPONSES_PROFILE_ID | OPENAI_CODEX_RESPONSES_PROFILE_ID => {
                crate::openai::encode_responses_resource(resource).map_err(protocol_error)
            }
            OPENAI_CHAT_PROFILE_ID => {
                crate::openai::encode_chat_resource(resource).map_err(protocol_error)
            }
            ANTHROPIC_MESSAGES_PROFILE_ID
            | ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
            | CLAUDE_CODE_MESSAGES_PROFILE_ID => {
                // Only the standard image file source is registered; documents and videos keep
                // rejecting provider file ids because no Anthropic route consumes them yet.
                // 仅注册了标准图片文件来源；文档与视频仍拒绝供应商文件 ID，因为尚无 Anthropic
                // 路由消费它们。
                if resource.source_kind == ResourceSourceKind::ProviderFileId
                    && resource.kind != ResourceKind::Image
                {
                    return Err(LlmCoreError::new(
                        "Anthropic media cannot consume a bare provider file id for this resource kind.",
                    ));
                }
                match resource.kind {
                    ResourceKind::Image => {
                        crate::anthropic::encode_anthropic_image(resource).map_err(protocol_error)
                    }
                    ResourceKind::File => {
                        // Reuse the existing document mapping while preserving file_id versus URL semantics.
                        // 复用既有文档映射，同时保留 file_id 与 URL 的语义差异。
                        let file = crate::openai::encode_responses_resource(resource)
                            .map_err(protocol_error)?;
                        crate::responses_to_anthropic::anthropic_document_block(&file)
                    }
                    ResourceKind::Video
                        if self.profile_id == ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID =>
                    {
                        crate::anthropic::encode_anthropic_video(resource).map_err(protocol_error)
                    }
                    ResourceKind::Audio | ResourceKind::Video => Err(LlmCoreError::new(format!(
                        "Protocol `{}` has no registered {:?} input projector.",
                        self.profile_id, resource.kind
                    ))),
                }
            }
            GEMINI_GENERATE_CONTENT_PROFILE_ID
            | GOOGLE_CODE_ASSIST_PROFILE_ID
            | GEMINI_INTERACTIONS_PROFILE_ID => {
                if resource.source_kind == ResourceSourceKind::ProviderFileId {
                    return Err(LlmCoreError::new(
                        "Gemini media requires a file URI rather than a bare provider file id.",
                    ));
                }
                if self.profile_id == GEMINI_INTERACTIONS_PROFILE_ID
                    && !resource
                        .url
                        .as_deref()
                        .is_some_and(|url| url.starts_with("data:"))
                {
                    return Err(LlmCoreError::new(
                        "Gemini Interactions has no registered remote media input projector.",
                    ));
                }
                if resource
                    .url
                    .as_deref()
                    .is_some_and(|url| !url.starts_with("data:"))
                    && resource.media_type.is_none()
                {
                    return Err(LlmCoreError::new(
                        "Gemini remote media requires an explicit MIME type.",
                    ));
                }
                // Interactions subsequently transforms this exact inlineData part into its content shape.
                // Interactions 随后将此精确 inlineData Part 转换为自身内容结构。
                crate::gemini::encode_gemini_resource(resource).map_err(protocol_error)
            }
            _ => Err(LlmCoreError::new(
                "The frozen internal media projector is not registered.",
            )),
        }
    }
}

/// Rejects source-field contradictions before any protocol can accidentally reinterpret a resource.
/// 在任意协议可能意外重新解释资源之前拒绝来源字段矛盾。
/// `resource` is the typed canonical input; success preserves its declared source unchanged.
/// `resource` 是强类型规范输入；成功时保持其声明来源不变。
fn validate_resource_source(resource: &ResourceReference) -> Result<(), LlmCoreError> {
    if let Some(mime) = resource.media_type.as_deref() {
        validate_resource_mime_kind(resource.kind, mime)?;
    }
    if resource.source_kind == ResourceSourceKind::ProviderFileId {
        if resource.id.trim().is_empty() || resource.url.is_some() {
            return Err(LlmCoreError::new(
                "Provider file-id media requires a non-empty id and no conflicting URL.",
            ));
        }
        return Ok(());
    }
    // The canonical source owns one exact URL; local paths are never encoded into the external block.
    // 规范来源持有一个精确 URL；本地路径绝不会被编码到外部内容块。
    let url = resource
        .url
        .as_deref()
        .filter(|url| !url.trim().is_empty())
        .ok_or_else(|| {
            LlmCoreError::new("Media must be prepared before target protocol projection.")
        })?;
    if resource.source_kind == ResourceSourceKind::RemoteUrl && url.starts_with("data:") {
        return Err(LlmCoreError::new(
            "Remote media source cannot contain an inline data URL.",
        ));
    }
    if resource.source_kind == ResourceSourceKind::InlineData && !url.starts_with("data:") {
        return Err(LlmCoreError::new(
            "Inline media source requires a Base64 data URL.",
        ));
    }
    if let Some(body) = url.strip_prefix("data:") {
        // Data URLs explicitly own their MIME and encoding; an optional typed MIME must agree.
        // Data URL 显式持有 MIME 与编码方式；可选强类型 MIME 必须与其一致。
        let (metadata, _) = body
            .split_once(',')
            .ok_or_else(|| LlmCoreError::new("Media data URL has no payload separator."))?;
        // Only Base64 is a registered internal inline representation.
        // 内部内联表示仅注册了 Base64。
        let mime = metadata
            .strip_suffix(";base64")
            .filter(|mime| !mime.is_empty())
            .ok_or_else(|| {
                LlmCoreError::new("Media data URL requires an explicit MIME and Base64 encoding.")
            })?;
        validate_resource_mime_kind(resource.kind, mime)?;
        if resource
            .media_type
            .as_deref()
            .is_some_and(|declared| !declared.eq_ignore_ascii_case(mime))
        {
            return Err(LlmCoreError::new(
                "Media MIME disagrees with its Base64 data URL.",
            ));
        }
    }
    Ok(())
}

/// Checks declared `kind` against known `mime`; returns an error instead of relabelling unsupported media.
/// 对照已知 `mime` 检查声明的 `kind`；不匹配时返回错误，不重新标记不支持的媒体。
fn validate_resource_mime_kind(kind: ResourceKind, mime: &str) -> Result<(), LlmCoreError> {
    if mime.trim().is_empty() {
        return Err(LlmCoreError::new(
            "A declared media MIME type cannot be empty.",
        ));
    }
    // MIME families are case-insensitive; this check does not infer missing MIME information.
    // MIME 族不区分大小写；此检查不会推断缺失的 MIME 信息。
    let normalized = mime.to_ascii_lowercase();
    // Files may carry documents or opaque bytes, but cannot impersonate known image/audio/video content.
    // 文件可携带文档或不透明字节，但不能冒充已知图片、音频或视频内容。
    let matches = match kind {
        ResourceKind::Image => normalized.starts_with("image/"),
        ResourceKind::Audio => normalized.starts_with("audio/"),
        ResourceKind::Video => normalized.starts_with("video/"),
        ResourceKind::File => !["image/", "audio/", "video/"]
            .iter()
            .any(|prefix| normalized.starts_with(prefix)),
    };
    if matches {
        Ok(())
    } else {
        Err(LlmCoreError::new(format!(
            "Media kind {kind:?} disagrees with MIME `{mime}`."
        )))
    }
}

/// Preserves the exact protocol error while converting it to the core error type.
/// 转换为核心错误类型时保留精确协议错误。
fn protocol_error(error: crate::ProtocolConversionError) -> LlmCoreError {
    LlmCoreError::new(error.to_string())
}

/// Projects private `part` through an explicitly supplied projector; public converters pass `None`.
/// 通过显式提供的投影器投影私有 `part`；公共转换器传入 `None`。
/// Returns native media for `role`, or rejects untrusted private input instead of passing it through.
/// 返回 `role` 对应的原生媒体，或拒绝不可信私有输入而非透传。
pub(crate) fn project_internal_part(
    part: &Value,
    role: &str,
    projector: Option<InternalMediaProjector<'_>>,
) -> Result<Value, LlmCoreError> {
    projector
        .ok_or_else(|| {
            LlmCoreError::new(
                "Private media envelopes are not accepted by standard Responses converters.",
            )
        })?
        .project(part, role)
}

/// Converts one typed internal `request` to the exact `profile_id` selected by `context`.
/// 将一个强类型内部 `request` 转换为 `context` 选定的精确 `profile_id`。
/// Returns a wire request only after every private media part has a target mapping; no protocol fallback occurs.
/// 仅在全部私有媒体项都有目标映射后返回线路请求；不执行协议回退。
pub fn project_internal_request_for_provider(
    request: InternalResponsesRequest,
    context: &ProviderRouteContext,
    profile_id: &str,
) -> Result<EncodedWireRequest, LlmCoreError> {
    // Consume the internal owner; it is never serialized or sent as a wire request.
    // 消费内部所有者；内部对象绝不直接序列化或发送为线路请求。
    let (body, baseline_headers, resources, replay_sidecar) = request.into_parts();
    // Exact profile validation precedes body conversion and therefore precedes any transport work.
    // 精确 Profile 校验先于正文转换，因此也先于任意传输工作。
    let projector = InternalMediaProjector::new(profile_id, context.upstream_protocol, &resources)?;
    if profile_id != OPENAI_CODEX_RESPONSES_PROFILE_ID && body.get("previous_response_id").is_some()
    {
        return Err(LlmCoreError::new(
            "A Codex continuation must be rebuilt from complete history before selecting another protocol.",
        ));
    }
    // The canonical model supplies only the declared dynamic Gemini path, not protocol selection.
    // 规范模型仅提供已声明的 Gemini 动态路径，不参与协议选择。
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .ok_or_else(|| LlmCoreError::new("Internal request is missing its exact model."))?
        .to_string();
    // Every target is projected independently, including standard Responses.
    // 每个目标均独立投影，包括标准 Responses。
    // declaredChatSidecar hands the captured attribution only to the converter that declared it.
    // declaredChatSidecar 只把已捕获的归属交给声明它的转换器。
    let declared_chat_sidecar = matches!(
        context.reasoning_replay_contract,
        Some(vulcan_ai_contract::ReasoningReplayContract::DeepseekChatReasoningContentV1)
    )
    .then_some(&replay_sidecar);
    // declaredAnthropicSidecar is the same one-converter ownership rule for the Anthropic target.
    // declaredAnthropicSidecar 是 Anthropic 目标上同一条“只交付声明者”规则。
    let declared_anthropic_sidecar = matches!(
        context.reasoning_replay_contract,
        Some(vulcan_ai_contract::ReasoningReplayContract::DeepseekAnthropicThinkingV1)
    )
    .then_some(&replay_sidecar);
    let projected = match context.upstream_protocol {
        UpstreamProtocol::OpenAiResponses => {
            // Only the declared plaintext target may recover unsigned canonical reasoning.
            // 只有声明明文契约的目标才能恢复无签名的规范推理。
            let body = if context.reasoning_replay_contract
                == Some(vulcan_ai_contract::ReasoningReplayContract::DeepseekResponsesPlaintextV1)
            {
                crate::responses_plaintext_replay::project_plaintext_reasoning(
                    body,
                    &replay_sidecar,
                )?
            } else {
                body
            };
            project_standard_responses(body, projector)?
        }
        UpstreamProtocol::OpenAiChatCompletions => {
            crate::responses_to_chat::responses_to_chat_completions_with_media(
                body,
                Some(projector),
                declared_chat_sidecar,
            )?
        }
        UpstreamProtocol::AnthropicMessages => {
            crate::responses_to_anthropic::responses_to_anthropic_messages_with_media(
                body,
                context.anthropic_reasoning_mode,
                Some(projector),
                declared_anthropic_sidecar,
            )?
        }
        UpstreamProtocol::GeminiGenerateContent => {
            crate::responses_to_gemini::responses_to_gemini_generate_content_with_media(
                body,
                false,
                Some(projector),
            )?
        }
        UpstreamProtocol::GeminiInteractions => {
            crate::responses_to_gemini::responses_to_gemini_interactions_with_media(
                body,
                Some(projector),
            )?
        }
        UpstreamProtocol::GoogleCodeAssist => {
            crate::responses_to_gemini::responses_to_gemini_generate_content_with_media(
                body,
                true,
                Some(projector),
            )?
        }
    };
    // Paths match the registered protocol encoders; frozen provider endpoint overrides remain external.
    // 路径与已注册协议编码器一致；冻结供应商端点覆盖仍由外层处理。
    let path = match profile_id {
        OPENAI_CODEX_RESPONSES_PROFILE_ID => String::from("/responses"),
        OPENAI_RESPONSES_PROFILE_ID => String::from("/v1/responses"),
        OPENAI_CHAT_PROFILE_ID => String::from("/v1/chat/completions"),
        ANTHROPIC_MESSAGES_PROFILE_ID
        | ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
        | CLAUDE_CODE_MESSAGES_PROFILE_ID => String::from("/v1/messages"),
        GEMINI_GENERATE_CONTENT_PROFILE_ID => gemini_generate_content_path(&model),
        GEMINI_INTERACTIONS_PROFILE_ID => String::from("/interactions"),
        GOOGLE_CODE_ASSIST_PROFILE_ID => String::from("/v1internal:streamGenerateContent?alt=sse"),
        _ => {
            return Err(LlmCoreError::new(
                "Unregistered internal request target path.",
            ));
        }
    };
    crate::validate_target_media_projection(profile_id, &projected)?;
    Ok(EncodedWireRequest {
        path,
        headers: if profile_id == OPENAI_CODEX_RESPONSES_PROFILE_ID {
            baseline_headers
        } else {
            converted_protocol_headers(context.upstream_protocol)
        },
        body: projected,
    })
}

/// Projects the private media parts of `body` through the strict Responses resource encoder.
/// 通过严格 Responses 资源编码器投影 `body` 的私有媒体项。
/// Returns the standard body, or rejects unsupported types without leaking private resource fields.
/// 返回标准正文，或拒绝不支持的类型，不泄漏私有资源字段。
fn project_standard_responses(
    mut body: Value,
    projector: InternalMediaProjector<'_>,
) -> Result<Value, LlmCoreError> {
    // Standard Responses must not inherit Codex-only client identity; the Codex target omits output limits.
    // 标准 Responses 不能继承 Codex 专用客户端身份；Codex 目标则省略输出限制。
    let object = body
        .as_object_mut()
        .ok_or_else(|| LlmCoreError::new("Internal Responses body must be an object."))?;
    if projector.profile_id == OPENAI_CODEX_RESPONSES_PROFILE_ID {
        object.remove("max_output_tokens");
    } else {
        object.remove("client_metadata");
        object.remove("prompt_cache_key");
    }
    // The shared internal encoder always emits an input array; absence is corruption, not another shape.
    // 共享内部编码器始终输出输入数组；缺失属于损坏，不是另一种结构。
    let input = body
        .get_mut("input")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            LlmCoreError::new("Internal Responses request is missing its input array.")
        })?;
    for item in input {
        // Role belongs to the canonical input item, never to the resource payload.
        // 角色属于规范输入项，绝不从资源载荷取得。
        let role = item
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if let Some(content) = item.get_mut("content").and_then(Value::as_array_mut) {
            for part in content {
                if part.get("type").and_then(Value::as_str) == Some(INTERNAL_RESOURCE_PART_TYPE) {
                    *part = projector.project(part, &role)?;
                }
            }
        }
    }
    Ok(body)
}
