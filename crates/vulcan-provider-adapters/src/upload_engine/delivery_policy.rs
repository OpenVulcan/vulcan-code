//! Upload primitive classification shared by conversation and independent extension preparation.
//! 会话与独立扩展准备共用的上传原语分类。

use super::*;

/// Executable transport kind; an inline primitive is not a real provider upload.
/// 可执行传输类型；内联原语不是真实供应商上传。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UploadChannelDeliveryKind {
    /// Local bytes are encoded in the provider request.
    /// 本地字节编码进供应商请求。
    Inline,
    /// A registered network primitive creates a provider-owned resource.
    /// 已注册网络原语创建供应商持有的资源。
    ProviderUpload,
    /// An already remote, validated URL is passed through without downloading or uploading it.
    /// 已远程化且已校验的 URL 直接传递，不下载也不上传。
    RemoteReference,
}

/// Classifies executable `channel` semantics; unsupported or missing primitives return errors.
/// 分类 `channel` 的可执行语义；不支持或缺失的原语返回错误。
pub(crate) fn upload_channel_delivery_kind(
    channel: &crate::UploadChannelDeclaration,
) -> Result<UploadChannelDeliveryKind, String> {
    match channel.primitive {
        Some(UploadPrimitiveKind::InlineDataUrl | UploadPrimitiveKind::InlineBase64) => {
            Ok(UploadChannelDeliveryKind::Inline)
        }
        Some(UploadPrimitiveKind::MultipartFile | UploadPrimitiveKind::ProviderFileCreate) => {
            Ok(UploadChannelDeliveryKind::ProviderUpload)
        }
        Some(UploadPrimitiveKind::SignedObjectUpload)
            if channel.signing_primitive
                == Some(crate::UploadSigningPrimitiveKind::TemporaryOssPolicy) =>
        {
            Ok(UploadChannelDeliveryKind::ProviderUpload)
        }
        Some(UploadPrimitiveKind::RemoteUrlPassthrough) => {
            Ok(UploadChannelDeliveryKind::RemoteReference)
        }
        _ => Err(format!(
            "Upload channel `{}` has no registered executable primitive for its declared signing contract.",
            channel.upload_channel_id
        )),
    }
}

/// Resolves the first applicable real upload in `channel_refs`, preserving declared order and hard limits.
/// 在 `channel_refs` 中解析首个适用真实上传，保留声明顺序及硬限制。
/// `channels`, `resource` and `credential_role` are frozen manifest/source facts, never provider-name guesses.
/// `channels`、`resource` 与 `credential_role` 是冻结清单及来源事实，不能根据供应商名称猜测。
/// Returns no upload for inline-only or MIME-inapplicable declarations; malformed or unauthorized uploads fail.
/// 对仅内联或 MIME 不适用声明返回无上传；格式错误或未授权上传直接失败。
pub(crate) fn resolve_real_upload_channel<'a>(
    channels: &'a [crate::UploadChannelDeclaration],
    channel_refs: &[String],
    resource: &UploadResource,
    credential_role: Option<&str>,
) -> Result<Option<&'a crate::UploadChannelDeclaration>, String> {
    // Resolve every referenced identity before selection so malformed declarations cannot be hidden later.
    // 选择前解析全部引用身份，防止后续格式错误声明被隐藏。
    let referenced = channel_refs
        .iter()
        .map(|reference| {
            channels
                .iter()
                .find(|channel| &channel.upload_channel_id == reference)
                .ok_or_else(|| {
                    format!("Operation references unknown upload channel `{reference}`.")
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    // A denied real upload is not reclassified as absence to enable inline fallback.
    // 被拒绝的真实上传不能重新归类为不存在，从而启用内联回退。
    let mut denied_upload = false;
    for channel in referenced {
        if !channel.accepted_mime_types.is_empty()
            && !channel.accepted_mime_types.contains(&resource.mime_type)
        {
            continue;
        }
        // Primitive identity comes from the registered executor, not from a non-empty channel array.
        // 原语身份来自已注册执行器，不来自非空通道数组。
        let kind = upload_channel_delivery_kind(channel)?;
        if kind != UploadChannelDeliveryKind::ProviderUpload {
            continue;
        }
        if channel
            .credential_role
            .as_deref()
            .is_some_and(|required| Some(required) != credential_role)
        {
            denied_upload = true;
            continue;
        }
        return Ok(Some(channel));
    }
    if denied_upload {
        return Err(String::from(
            "The declared real media upload is not authorized for the frozen credential role.",
        ));
    }
    Ok(None)
}

/// Selects a declared `kind` without inventing bindings or changing channel order to evade limits.
/// 选择已声明 `kind`，不虚构绑定，也不改变通道顺序以绕过限制。
/// Returns the first authorized MIME-compatible declaration or no matching channel.
/// 返回首个已授权且 MIME 兼容的声明，或无匹配通道。
fn resolve_channel_of_kind<'a>(
    channels: &'a [crate::UploadChannelDeclaration],
    references: &[String],
    resource: &UploadResource,
    credential_role: Option<&str>,
    kind: UploadChannelDeliveryKind,
) -> Result<Option<&'a crate::UploadChannelDeclaration>, String> {
    for reference in references {
        // Manifest references, not runtime candidates or provider names, own channel identity.
        // 通道身份由清单引用持有，不来自运行时候选或供应商名称。
        let channel = channels
            .iter()
            .find(|channel| &channel.upload_channel_id == reference)
            .ok_or_else(|| format!("Operation references unknown upload channel `{reference}`."))?;
        if (!channel.accepted_mime_types.is_empty()
            && !channel.accepted_mime_types.contains(&resource.mime_type))
            || channel
                .credential_role
                .as_deref()
                .is_some_and(|required| Some(required) != credential_role)
        {
            continue;
        }
        if upload_channel_delivery_kind(channel)? == kind {
            return Ok(Some(channel));
        }
    }
    Ok(None)
}

/// Plans a fresh operation `resource` using frozen bindings and the request-wide `planner`.
/// 使用冻结绑定与请求级 `planner` 规划新的操作 `resource`。
/// `channels`, `references` and `credential_role` define exact allowed transports and consumer shapes.
/// `channels`、`references` 与 `credential_role` 定义精确允许的传输及消费形态。
/// Returns one executable channel; rejects oversized extensions before decoding, reads or network effects.
/// 返回一个可执行通道；在解码、读取或网络副作用前拒绝过大的扩展输入。
pub(super) fn resolve_operation_delivery_channel<'a>(
    channels: &'a [crate::UploadChannelDeclaration],
    references: &[String],
    resource: &UploadResource,
    credential_role: Option<&str>,
    planner: &mut crate::MediaDeliveryPlanner,
    input_resources: &[vulcan_ai_contract::ResourceKind],
) -> Result<&'a crate::UploadChannelDeclaration, String> {
    // The owning operation declares allowed modalities independently of transport capabilities.
    // 所属操作独立于传输能力声明允许的模态。
    let kind = operation_resource_kind(&resource.mime_type)?;
    if !input_resources.contains(&kind) {
        return Err(String::from(
            "Media extension cannot process this file: its modality is not declared by the selected operation.",
        ));
    }
    // An already remote source has no verified raw byte length and cannot be treated as zero-byte inline data.
    // 已远程化来源没有已验证原始字节长度，不能当作零字节内联数据。
    if let Some(url) = resource.remote_url.as_deref() {
        // Explicit passthrough retains its closed host allowlist without downloading or reuploading.
        // 显式直通保留封闭宿主允许列表，不下载也不重新上传。
        let remote = resolve_channel_of_kind(
            channels,
            references,
            resource,
            credential_role,
            UploadChannelDeliveryKind::RemoteReference,
        )?
        .ok_or_else(|| {
            String::from("Remote operation media requires a declared URL passthrough channel.")
        })?;
        validate_upload_resource(remote, resource)?;
        validate_remote_url_passthrough(remote, url)?;
        return Ok(remote);
    }
    // Real upload is selected first, regardless of the order of any inline declarations.
    // 无论内联声明的排列顺序如何，都先选择真实上传。
    let real = resolve_real_upload_channel(channels, references, resource, credential_role)?;
    // An inline consumer must be explicitly declared by this exact operation binding.
    // 内联消费方必须由此精确操作绑定显式声明。
    let inline = resolve_channel_of_kind(
        channels,
        references,
        resource,
        credential_role,
        UploadChannelDeliveryKind::Inline,
    )?;
    if real.is_none() && inline.is_none() {
        return Err(format!(
            "No declared upload channel accepts MIME type `{}` and credential role `{}` for the referenced channels.",
            resource.mime_type,
            credential_role.unwrap_or("none")
        ));
    }
    // Per-inline-interface limits may select an available upload, but never a larger second upload interface.
    // 各内联接口限制可以让策略选用可用上传，但绝不能改选更大的第二上传接口。
    let inline_supported = inline.is_some_and(|channel| {
        channel
            .max_bytes
            .is_none_or(|limit| resource.byte_len <= limit)
            && channel
                .max_count
                .is_none_or(|limit| resource.item_count <= limit)
    });
    // A count breach on the real interface cannot be bypassed through an inline preference.
    // 真实接口的数量限制不能通过内联偏好绕过。
    if real.is_some_and(|channel| {
        channel
            .max_count
            .is_some_and(|limit| resource.item_count > limit)
    }) {
        return Err(String::from(
            "Media extension cannot process these files: the selected upload interface item limit is exceeded.",
        ));
    }
    // The pure planner owns raw thresholds and aggregate encoded reservations for all bindings.
    // 纯规划器持有全部绑定的原始阈值与累计编码预留量。
    let decision = planner.plan(crate::MediaDeliveryFacts {
        kind,
        raw_bytes: resource.byte_len,
        source_available: resource.byte_len != 0,
        target_supported: real.is_some() || inline.is_some(),
        target_max_bytes: if real.is_none() {
            inline.and_then(|channel| channel.max_bytes)
        } else {
            None
        },
        inline_supported,
        upload: match real {
            Some(channel) => crate::MediaUploadAvailability::Available {
                max_bytes: channel.max_bytes,
            },
            None => crate::MediaUploadAvailability::Absent,
        },
        origin: crate::MediaDeliveryOrigin::NewIntent,
    })?;
    // Materialize exactly the planned form; missing channels are explicit internal contract failures.
    // 精确物化规划形态；缺失通道属于明确内部契约失败。
    let channel = match decision {
        crate::MediaDeliveryDecision::Inline { .. } => inline.ok_or_else(|| {
            String::from("Planned operation inline input lost its declared consumer.")
        })?,
        crate::MediaDeliveryDecision::Upload => {
            real.ok_or_else(|| String::from("Planned operation upload lost its declared channel."))?
        }
        crate::MediaDeliveryDecision::Reject { reason } => {
            return Err(format!(
                "Media extension cannot understand/process this file ({} bytes): {}. Native content was not sent.",
                resource.byte_len,
                reason.explanation()
            ));
        }
        crate::MediaDeliveryDecision::Attachment { .. }
        | crate::MediaDeliveryDecision::RestoreOriginalUpload => {
            return Err(String::from(
                "Operation media planning returned a conversation-only action.",
            ));
        }
    };
    // Validate the real contract even when inline was preferred; malformed uploads are not 'absent'.
    // 即使偏好内联也校验真实契约；格式错误的上传不能视为“不存在”。
    if let Some(real) = real {
        validate_upload_resource(real, resource)?;
    }
    validate_upload_resource(channel, resource)?;
    Ok(channel)
}

/// Resolves the canonical operation `mime` family without turning known media into files.
/// 解析规范操作 `mime` 族，不把已知媒体转换为文件。
/// Returns a closed resource kind or an explicit malformed-MIME error.
/// 返回封闭资源类型，或明确的 MIME 格式错误。
fn operation_resource_kind(mime: &str) -> Result<vulcan_ai_contract::ResourceKind, String> {
    // MIME families are case-insensitive even when a Data URL supplies noncanonical casing.
    // 即使 Data URL 使用非规范大小写，MIME 族也不区分大小写。
    let normalized = mime.to_ascii_lowercase();
    match normalized.split_once('/') {
        Some((family, subtype)) if !family.is_empty() && !subtype.is_empty() => Ok(match family {
            "image" => vulcan_ai_contract::ResourceKind::Image,
            "audio" => vulcan_ai_contract::ResourceKind::Audio,
            "video" => vulcan_ai_contract::ResourceKind::Video,
            _ => vulcan_ai_contract::ResourceKind::File,
        }),
        _ => Err(String::from(
            "Operation media MIME must contain a non-empty type and subtype.",
        )),
    }
}
