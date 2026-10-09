//! Side-effect-free native media projection shared by V1 dispatch, capture and compaction.
//! V1分派、捕获及压缩共享的无副作用原生媒体投影。

use super::*;
use crate::agent_runtime::events::ProviderRequestMediaContext;
use base64::Engine;
use vulcan_ai_contract::{AiMessage, ContentItem, MessageRole, ResourceSourceKind};
use vulcan_provider_adapters::{
    MediaDeliveryDecision, MediaDeliveryOrigin, MediaDeliveryPlanner, MediaDeliveryPurpose,
    MediaDeliveryTarget,
};

/// Projects native tool envelopes in `messages` using exact frozen `context`; no file or network I/O occurs.
/// 使用精确冻结`context`投影`messages`中的原生工具信封；不执行文件或网络IO。
/// Keeps user-owned media intact and spends only its remaining encoded budget on tool-owned content.
/// 保持用户持有的媒体不变，仅将其剩余编码预算用于工具持有内容。
/// Returns ordered messages with media after the complete consecutive tool-result batch, or a validation error.
/// 返回完整连续工具结果批次之后附带媒体的有序消息，或校验错误。
pub fn project_native_media_messages(
    messages: &mut Vec<AiMessage>,
    context: Option<&ProviderRequestMediaContext>,
) -> Result<(), String> {
    // Find exact marked tool outputs before requiring a media execution in text-only fixtures.
    // 在纯文本夹具中要求媒体执行之前，先查找精确标记的工具输出。
    let mut has_media = false;
    for message in messages.iter() {
        for item in &message.content {
            if let ContentItem::ToolResult { result } = item
                && let Some(text) = result.content.as_str()
                && parse_envelope(text)?.is_some()
            {
                has_media = true;
            }
        }
    }
    if !has_media {
        return Ok(());
    }
    // Native output cannot be sent under guessed default model capabilities.
    // 原生输出不能在猜测的默认模型能力下发送。
    let context = context.ok_or("Native media tool results require the frozen model execution.")?;
    // User attachments already have authoritative first-delivery decisions; tool media cannot displace them.
    // 用户附件已有权威首次投递决定；工具媒体不能挤占它们。
    let mut policy = context.execution.media_delivery_policy;
    // Count already prepared inline resources without allocating another decoded copy.
    // 计算已准备内联资源大小，不再分配解码副本。
    let mut reserved = 0u64;
    for message in messages.iter() {
        for item in &message.content {
            if let ContentItem::Resource { resource } = item
                && resource.source_kind == ResourceSourceKind::InlineData
            {
                reserved = reserved
                    .checked_add(inline_data(resource)?.len() as u64)
                    .ok_or("Native media encoded budget overflow.")?;
            }
        }
    }
    policy.conversation.max_total_encoded_bytes = policy
        .conversation
        .max_total_encoded_bytes
        .saturating_sub(reserved);
    // Tool results share one request-local allocation in canonical order.
    // 工具结果按规范顺序共享一份请求局部分配量。
    let mut planner = MediaDeliveryPlanner::new(policy, MediaDeliveryPurpose::Conversation)?;
    // Build an atomic replacement so a malformed later envelope leaves the original messages untouched.
    // 构建原子替换，使后续畸形信封不会留下部分修改的原消息。
    let mut projected = Vec::with_capacity(messages.len());
    // Media is delayed until every adjacent result has satisfied its original tool call.
    // 媒体延迟到所有相邻结果均满足原工具调用之后。
    let mut pending = Vec::new();
    // Per-message modality counts cover all media merged into the synthetic user message.
    // 逐消息模态数量覆盖合并到合成用户消息中的全部媒体。
    let mut counts = std::collections::BTreeMap::<ResourceKind, u64>::new();
    for mut message in messages.clone() {
        if message.role != MessageRole::Tool {
            flush_media(&mut projected, &mut pending);
            counts.clear();
        }
        for item in &mut message.content {
            if let ContentItem::ToolResult { result } = item
                && let Some(text) = result.content.as_str()
                && let Some(envelope) = parse_envelope(text)?
            {
                if result.is_error {
                    result.content =
                        Value::String("Media tool failed; content was not attached.".into());
                    continue;
                }
                // Each native item retains a visible association with its original provider call id.
                // 每项原生内容保留与原供应商调用ID的可见关联。
                let content = project_envelope(
                    &envelope,
                    context,
                    &mut planner,
                    counts.get(&envelope.resource.kind).copied().unwrap_or(0) + 1,
                )?;
                match content {
                    ContentItem::Resource { .. } => {
                        *counts.entry(envelope.resource.kind).or_default() += 1;
                        pending.push(ContentItem::Text {
                            text: format!(
                                "Media from tool call {}: {}",
                                result.call_id, envelope.text
                            ),
                        });
                        pending.push(content);
                        result.content = Value::String("Media read completed. Its content follows this tool-result batch for your own understanding.".into());
                    }
                    ContentItem::Text { text } => result.content = Value::String(text),
                    _ => return Err("Invalid native media projection result.".into()),
                }
            }
        }
        projected.push(message);
    }
    flush_media(&mut projected, &mut pending);
    *messages = projected;
    Ok(())
}

/// Explains why `resource` was not attached for `reason`, keeping only its source address.
/// 说明`resource`因`reason`未被附加，仅保留其来源地址。
pub(super) fn unattached_media_text(
    reason: vulcan_provider_adapters::MediaDeliveryRejection,
    resource: &ResourceReference,
) -> String {
    format!(
        "Media content was not attached: {}. Source: {}. Use an available understanding tool or read the source again only when the current model supports it.",
        reason.explanation(),
        resource.resource_path.as_deref().unwrap_or("unknown")
    )
}

/// Flushes `pending` media into `messages` after its complete tool batch; empty batches add no message.
/// 在完整工具批次之后将`pending`媒体写入`messages`；空批次不添加消息。
fn flush_media(messages: &mut Vec<AiMessage>, pending: &mut Vec<ContentItem>) {
    if !pending.is_empty() {
        messages.push(AiMessage {
            role: MessageRole::User,
            content: std::mem::take(pending),
            provider_context_items: Vec::new(),
        });
    }
}

/// Extracts Base64 from exact inline `resource`, rejecting mismatched MIME and malformed framing.
/// 从精确内联`resource`提取Base64，拒绝MIME不匹配或封装损坏。
fn inline_data(resource: &ResourceReference) -> Result<&str, String> {
    // Inline resources have an explicit MIME and data-URL channel.
    // 内联资源具有明确MIME及数据URL通道。
    let mime = resource
        .media_type
        .as_deref()
        .ok_or("Native inline media has no MIME type.")?;
    resource
        .url
        .as_deref()
        .and_then(|url| url.strip_prefix(&format!("data:{mime};base64,")))
        .ok_or("Native inline media has invalid data framing.".into())
}

/// Admits one `envelope` under frozen `context`, shared `planner` and synthetic-message `count`.
/// 在冻结`context`、共享`planner`及合成消息`count`下准入一份`envelope`。
/// Returns its native resource or explicit unsent-content text; never reopens or reuploads historical sources.
/// 返回原生资源或明确未发送内容文字；绝不重新打开或上传历史来源。
fn project_envelope(
    envelope: &NativeMediaEnvelope,
    context: &ProviderRequestMediaContext,
    planner: &mut MediaDeliveryPlanner,
    count: u64,
) -> Result<ContentItem, String> {
    // The execution is the same authority used by the final wire encoder.
    // 执行对象与最终线上编码器使用相同权威。
    let execution = &context.execution;
    // Require canonical MIME metadata for every persisted representation.
    // 每种持久表示均要求规范MIME元数据。
    let resource = &envelope.resource;
    // MIME cannot be guessed from private upload URLs or opaque provider ids.
    // 不能从私有上传URL或不透明供应商ID猜测MIME。
    let mime = resource
        .media_type
        .as_deref()
        .ok_or("Native media result has no MIME type.")?;
    // Enforce the same closed host registry at replay as at tool execution.
    // 重放时执行与工具执行时相同的封闭宿主注册表。
    let descriptor = crate::attachment_assets::attachment_descriptor_from_mime_type(mime)
        .ok_or("Native media result has an unregistered MIME type.")?;
    if resource_kind(descriptor.kind) != Some(resource.kind) {
        return Err("Native media kind and MIME disagree.".into());
    }
    // Resolve the currently frozen upload contract even for historical inline hard-limit validation.
    // 即使是历史内联硬限制校验，也解析当前冻结上传契约。
    let upload = execution.client.conversation_media_upload_contract(
        &execution.route,
        resource.kind,
        mime,
    )?;
    // Original representation determines replay; current soft upload preferences never rewrite it.
    // 原表示决定重放；当前软上传偏好绝不改写它。
    let origin = match (resource.source_kind, &envelope.upload) {
        (ResourceSourceKind::InlineData, None) => {
            // Bound decoded allocations by the same shared hard ceiling as fresh media admission.
            // 使用与新媒体准入相同的共享硬上限限制解码分配。
            let data = inline_data(resource)?;
            if data.len() as u64 > vulcan_provider_adapters::MEDIA_INLINE_ENCODED_HARD_LIMIT_BYTES
                || Some(data.len() as u64)
                    != vulcan_provider_adapters::media_base64_encoded_len(envelope.byte_len)
            {
                return Err("Native inline media size is invalid.".into());
            }
            if base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|_| "Native inline media is not valid Base64.")?
                .len() as u64
                != envelope.byte_len
            {
                return Err("Native inline media size differs from its original content.".into());
            }
            MediaDeliveryOrigin::HistoricalInline
        }
        (ResourceSourceKind::ProviderFileId | ResourceSourceKind::RemoteUrl, Some(original)) => {
            // Provider-upload references belong to one account, route, model and calling session.
            // 供应商上传引用归属于一个账号、路线、模型及调用会话。
            let matches = original.route.provider_id == execution.route.provider.id
                && original.route.connection_id == execution.route.connection.id
                && original.route.model_id == execution.model.id
                && original.route.protocol_profile_id
                    == execution.route.protocol_route.protocol_profile_id
                && original.route.route_revision == execution.frozen_route_revision()
                && original.session_id == context.session_scope_id;
            // Reference fields are selected by their declared encoding, never guessed by shape.
            // 引用字段由声明编码选择，绝不按形状猜测。
            let reference = if resource.source_kind == ResourceSourceKind::ProviderFileId {
                resource.id.as_str()
            } else {
                resource
                    .url
                    .as_deref()
                    .ok_or("Native uploaded media has no URL.")?
            };
            // Replay may validate an original reference but cannot refresh or upload it.
            // 重放可校验原引用，但不能刷新或上传。
            let usable = original.expires_at_unix_millis > now_millis()?
                && upload.as_ref().is_some_and(|contract| {
                    contract.contract_fingerprint == original.contract_fingerprint
                        && vulcan_provider_adapters::media_upload_source_kind(
                            contract.reference_encoding,
                        ) == resource.source_kind
                })
                && execution
                    .client
                    .validate_conversation_media_reference(
                        &execution.route,
                        resource.kind,
                        mime,
                        reference,
                    )
                    .is_ok();
            MediaDeliveryOrigin::HistoricalUpload {
                original_target_matches: matches,
                reference_usable: usable,
            }
        }
        _ => {
            return Err("Native media representation does not match its original delivery.".into());
        }
    };
    // Reapply model, protocol, MIME, hard-size, count and total encoded-budget constraints.
    // 重新执行模型、协议、MIME、硬大小、数量及总编码预算限制。
    let decision = planner.plan(
        MediaDeliveryTarget {
            profile_id: &execution.route.protocol_route.protocol_profile_id,
            model_inputs: &execution.model.capabilities.input_resources,
            constraint: execution
                .route
                .protocol_route
                .input_resource_constraints
                .iter()
                .find(|constraint| constraint.resource_kind == resource.kind),
            upload: upload.as_ref(),
        }
        .facts(resource.kind, mime, envelope.byte_len, origin, count),
    )?;
    match decision {
        MediaDeliveryDecision::Inline { .. } | MediaDeliveryDecision::RestoreOriginalUpload => {
            Ok(ContentItem::Resource {
                resource: resource.clone(),
            })
        }
        MediaDeliveryDecision::Attachment { reason } | MediaDeliveryDecision::Reject { reason } => {
            Ok(ContentItem::Text {
                text: unattached_media_text(reason, resource),
            })
        }
        MediaDeliveryDecision::Upload => {
            Err("Historical tool media cannot acquire a new upload.".into())
        }
    }
}
