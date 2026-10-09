//! Native media reading and replay for the current conversation model.
//! 当前会话模型的原生媒体读取与重放。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use vulcan_ai_contract::{ResourceKind, ResourceReference};
use vulcan_core::types::ConversationAttachmentKind;
use vulcan_provider_adapters::{
    PersistedAiConfiguration, ProviderToolCapabilityKind, ResolvedPublicAiSelection,
};

mod budget;
use crate::image_conversion;
mod projection;
#[cfg(test)]
mod tests;
mod tool;

pub(crate) use budget::budget_snapshot;
pub use projection::project_native_media_messages;
pub(crate) use tool::build_read_media_candidate;

/// Durable marker owned by the host; model text is never parsed as a media instruction.
/// 宿主持有的持久标记；绝不把模型正文解析为媒体指令。
const MARKER: &str = "$vulcan_native_media_input";
/// Current durable native-input format version shared by its producer and validator.
/// 产出者及校验器共享的当前持久原生输入格式版本。
const ENVELOPE_VERSION: u8 = 1;

/// One immutable media result; uploading and reading occur only during the explicit tool call.
/// 一份不可变媒体结果；上传与读取仅在显式工具调用期间发生。
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeMediaEnvelope {
    /// Exact envelope version, independent of session metadata schemas.
    /// 精确信封版本，独立于会话元数据结构。
    #[serde(rename = "$vulcan_native_media_input")]
    version: u8,
    /// Safe explanation kept separate from binary content and private references.
    /// 与二进制内容及私有引用分离的安全说明。
    text: String,
    /// Prepared bytes or provider reference, never a path the encoder must reopen.
    /// 已准备字节或供应商引用，绝非编码器需要重新打开的路径。
    resource: ResourceReference,
    /// Source byte length used for exact replay admission.
    /// 精确重放准入使用的来源字节数。
    byte_len: u64,
    /// Original upload ownership; inline content remains portable between capable models.
    /// 原上传归属；内联内容可在具备能力的模型间移植。
    upload: Option<NativeUploadOrigin>,
}

/// Frozen ownership and lifetime of a tool-produced upload reference.
/// 工具产出上传引用的冻结归属与生命周期。
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeUploadOrigin {
    /// Exact account, model, protocol and route revision from tool execution.
    /// 工具执行时的精确账号、模型、协议及路由修订。
    route: vulcan_workbench_model::SessionProviderRouteLeaseIdentity,
    /// Calling session, retained even for provider-wide references.
    /// 调用会话，即使是供应商全局引用也保留。
    session_id: String,
    /// Original upload contract fingerprint.
    /// 原上传契约指纹。
    contract_fingerprint: String,
    /// Original expiry, using the existing no-expiry integer sentinel.
    /// 原过期时间，使用既有无过期整数哨兵。
    expires_at_unix_millis: u64,
}

/// Parses host-marked `text` from direct execution or the V1 controller's exact model_output wrapper.
/// 从直接执行或V1控制器精确model_output包装器中解析宿主标记的`text`。
/// Returns no result for ordinary text and errors for malformed marked envelopes; never searches nested objects.
/// 普通文本返回空，畸形标记信封返回错误；绝不搜索嵌套对象。
fn parse_envelope(text: &str) -> Result<Option<NativeMediaEnvelope>, String> {
    // Ordinary tool text is not required to be JSON.
    // 普通工具正文不要求为JSON。
    let Ok(mut value) = serde_json::from_str::<Value>(text) else {
        return Ok(None);
    };
    if value.get(MARKER).is_none() {
        // V1 controller projection serializes the whole completion payload; direct execution stores its model output.
        // V1控制器投影序列化完整完成载荷；直接执行保存其中的模型输出。
        let Some(model_output) = value.get("model_output").and_then(Value::as_str) else {
            return Ok(None);
        };
        // Unwrap exactly the documented owner once; unrelated JSON remains ordinary tool text.
        // 仅解包已定义的唯一归属一次；无关JSON仍是普通工具正文。
        let Ok(envelope_value) = serde_json::from_str::<Value>(model_output) else {
            return Ok(None);
        };
        value = envelope_value;
    }
    if value.get(MARKER).is_none() {
        return Ok(None);
    }
    // Deserialize a closed shape rather than searching candidate field names.
    // 反序列化封闭结构，不搜索候选字段名。
    let envelope: NativeMediaEnvelope = serde_json::from_value(value)
        .map_err(|error| format!("Invalid native media result: {error}"))?;
    if envelope.version != ENVELOPE_VERSION
        || envelope.byte_len == 0
        || envelope.resource.local_path.is_some()
    {
        return Err(String::from(
            "Invalid native media result version, size or private source path.",
        ));
    }
    Ok(Some(envelope))
}

/// Returns a safe preview of exact `text`, or no preview when it is an ordinary tool result.
/// 返回精确`text`的安全预览；普通工具结果不返回预览。
pub(crate) fn text_preview(text: &str) -> Result<Option<String>, String> {
    Ok(parse_envelope(text)?.map(|envelope| envelope.text))
}

/// Appends `notice` without losing the binary envelope; returns none for ordinary output.
/// 追加`notice`而不丢失二进制信封；普通输出返回空。
pub(crate) fn append_notice(text: &str, notice: &str) -> Result<Option<String>, String> {
    // Preserve all durable source and ownership facts while updating explanatory text.
    // 更新说明文字时保留所有持久来源及归属事实。
    let Some(mut envelope) = parse_envelope(text)? else {
        return Ok(None);
    };
    envelope.text.push_str("\n\n");
    envelope.text.push_str(notice);
    serde_json::to_string(&envelope)
        .map(Some)
        .map_err(|error| error.to_string())
}

/// Extracts a native envelope from the exact V1 `model_output` field, leaving other payloads unchanged.
/// 从精确V1`model_output`字段提取原生信封，其他载荷保持不变。
pub fn v1_native_media_content(output: &Value) -> Result<Option<Value>, String> {
    // V1 completion payloads have a single documented model-output owner.
    // V1完成载荷的模型输出具有唯一已定义归属。
    let Some(text) = output.get("model_output").and_then(Value::as_str) else {
        return Ok(None);
    };
    Ok(parse_envelope(text)?.map(|_| Value::String(text.to_string())))
}

/// Retires marked native media `output` recorded before a compaction to its address-only text.
/// 将压缩之前记录的已标记原生媒体`output`退役为仅地址文字。
/// Returns none for ordinary tool output and errors for malformed marked envelopes.
/// 普通工具输出返回空，畸形标记信封返回错误。
pub fn compacted_native_media_content(output: &Value) -> Result<Option<Value>, String> {
    let Some(text) = output.as_str() else {
        return Ok(None);
    };
    Ok(parse_envelope(text)?.map(|envelope| {
        Value::String(projection::unattached_media_text(
            vulcan_provider_adapters::MediaDeliveryRejection::CompactedHistory,
            &envelope.resource,
        ))
    }))
}

/// Maps supported host `kind` to the provider-neutral resource kind; generic files have no media reader.
/// 将支持的宿主`kind`映射为供应商无关资源类型；通用文件不属于媒体读取器。
fn resource_kind(kind: ConversationAttachmentKind) -> Option<ResourceKind> {
    match kind {
        ConversationAttachmentKind::Image => Some(ResourceKind::Image),
        ConversationAttachmentKind::Audio => Some(ResourceKind::Audio),
        ConversationAttachmentKind::Video => Some(ResourceKind::Video),
        ConversationAttachmentKind::Pdf => Some(ResourceKind::File),
        ConversationAttachmentKind::File => None,
    }
}

/// Tests exact `kind` and `mime` against frozen `selection`; returns actual local input support.
/// 根据冻结`selection`检查精确`kind`与`mime`；返回实际本地输入支持情况。
fn supports_mime(selection: &ResolvedPublicAiSelection, kind: ResourceKind, mime: &str) -> bool {
    // Model declarations, MIME restrictions and the actual encoder must agree.
    // 模型声明、MIME限制及实际编码器必须一致。
    let facts = vulcan_provider_adapters::MediaDeliveryTarget {
        profile_id: &selection.route.protocol_route.protocol_profile_id,
        model_inputs: &selection.model.capabilities.input_resources,
        constraint: selection
            .route
            .protocol_route
            .input_resource_constraints
            .iter()
            .find(|constraint| constraint.resource_kind == kind),
        upload: None,
    }
    .facts(
        kind,
        mime,
        1,
        vulcan_provider_adapters::MediaDeliveryOrigin::NewIntent,
        1,
    );
    facts.target_supported && facts.inline_supported && facts.target_max_bytes != Some(0)
}

/// Lists native MIME types admitted by `selection`, reusing the host's canonical format registry.
/// 复用宿主规范格式注册表，列出`selection`准入的原生MIME类型。
/// Model-specific constraints already override provider-route defaults in the frozen selection; explicit MIME lists never union with defaults.
/// 冻结选择中的模型专属约束已覆盖供应商路由默认值；显式MIME列表绝不与默认值取并集。
fn supported_mimes(selection: &ResolvedPublicAiSelection) -> Vec<&'static str> {
    // PDF is the file modality's explicit document format, not arbitrary binary input.
    // PDF是文件模态的明确文档格式，不是任意二进制输入。
    let mut mimes = Vec::new();
    for kind in [
        ConversationAttachmentKind::Image,
        ConversationAttachmentKind::Audio,
        ConversationAttachmentKind::Video,
        ConversationAttachmentKind::Pdf,
    ] {
        // Only the four explicit media kinds enter this loop.
        // 仅四种明确媒体类型进入本循环。
        let resource = resource_kind(kind.clone()).expect("explicit media kind");
        // PDF has one canonical MIME; the other modalities use the shared closed registry.
        // PDF具有唯一规范MIME；其他模态使用共享封闭注册表。
        let registered = if kind == ConversationAttachmentKind::Pdf {
            &["application/pdf"][..]
        } else {
            crate::attachment_assets::registered_media_attachment_mime_types(kind.clone())
        };
        mimes.extend(
            registered
                .iter()
                .copied()
                .filter(|mime| supports_mime(selection, resource, mime)),
        );
        if kind == ConversationAttachmentKind::Image {
            // Extra decodable formats become native only through an explicit independent MIME declaration.
            // 额外可解码格式仅通过显式独立MIME声明成为原生格式。
            if let Some(constraint) = selection
                .route
                .protocol_route
                .input_resource_constraints
                .iter()
                .find(|constraint| constraint.resource_kind == resource)
            {
                for codec in image_conversion::CODECS {
                    if !mimes.contains(&codec.mime)
                        && constraint
                            .accepted_mime_types
                            .iter()
                            .any(|mime| mime.eq_ignore_ascii_case(codec.mime))
                        && supports_mime(selection, resource, codec.mime)
                    {
                        mimes.push(codec.mime);
                    }
                }
            }
        }
    }
    mimes
}

/// Removes redundant external understanding from the turn-local `configuration` for exact `selection`.
/// 根据精确`selection`从回合局部`configuration`移除重复的外部理解能力。
/// Both provider tools and replacement models are controlled by the same enabled-capability gate.
/// 供应商工具与替代模型均由相同启用能力闸门控制。
pub(crate) fn suppress_external_understanding(
    configuration: &mut PersistedAiConfiguration,
    selection: &ResolvedPublicAiSelection,
) {
    // Reuse the same MIME admission as the reader exposed to the model.
    // 复用暴露给模型的读取器所使用的相同MIME准入。
    let mimes = supported_mimes(selection);
    for (capability, prefix) in [
        (ProviderToolCapabilityKind::ImageUnderstand, "image/"),
        (ProviderToolCapabilityKind::AudioUnderstand, "audio/"),
        (ProviderToolCapabilityKind::VideoUnderstand, "video/"),
    ] {
        if mimes.iter().any(|mime| mime.starts_with(prefix)) {
            configuration
                .provider_tool_enabled_capabilities
                .remove(&capability);
        }
    }
}

/// Returns host wall-clock milliseconds for original upload lifetime checks, failing on invalid host time.
/// 返回原上传生命周期检查使用的宿主毫秒时间；宿主时间无效时失败。
fn now_millis() -> Result<u64, String> {
    u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis(),
    )
    .map_err(|error| error.to_string())
}
