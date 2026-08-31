//! Frozen input-resource constraints and request-wide inline-media budgets.
//! 冻结的输入资源约束与请求级内联媒体预算。

use crate::agent_runtime::provider_attachment::model_visible_attachment_field;
use base64::Engine;
use std::collections::BTreeMap;
use vulcan_ai_contract::{
    AiMessage, ContentItem, ResourceKind, ResourceReference, ResourceSourceKind,
};
use vulcan_core::types::CoreError as VulcanCoreError;
use vulcan_provider_adapters::{
    InputResourceConstraint, ProviderExecutionClient, ProviderExecutionRoute,
    ResourceReferencePolicy, UploadReferenceEncoding,
};

/// Maximum aggregate Base64 payload bytes allowed for host-inline resources in one request.
/// 单次请求中宿主内联资源允许使用的 Base64 聚合载荷最大字节数。
const HOST_INLINE_RESOURCE_BASE64_BUDGET_BYTES: u64 = 2 * 1024 * 1024;

/// Tracks request-wide encoded media consumption before Provider request serialization.
/// 在 Provider 请求序列化前跟踪请求级编码媒体消耗。
#[derive(Debug, Default)]
pub(super) struct InlineResourceProjectionBudget {
    /// Request-wide native resource counts keyed by canonical modality.
    /// 按规范模态索引的请求级原生资源数量。
    retained_resource_counts: BTreeMap<ResourceKind, u64>,
    /// Aggregate Base64 bytes already committed by earlier canonical resources.
    /// 先前规范资源已经占用的 Base64 聚合字节数。
    used_base64_bytes: u64,
}

impl InlineResourceProjectionBudget {
    /// Reserves the exact Base64 expansion for one local resource.
    /// 为一个本地资源保留精确的 Base64 膨胀字节数。
    ///
    /// # Parameters
    /// 参数
    /// `resource_kind` and `max_count` freeze the route count contract; `raw_byte_len` is the
    /// optional verified local payload byte count when Base64 projection is required.
    /// `resource_kind` 与 `max_count` 冻结路线数量契约；`raw_byte_len` 是需要 Base64 投影时
    /// 可选的已验证本地载荷字节数。
    ///
    /// # Returns
    /// 返回值
    /// Success after both budgets are atomically committed, or the exact request-wide limit name.
    /// 两项预算原子提交后返回成功，否则返回精确的请求级限制名称。
    fn reserve(
        &mut self,
        resource_kind: ResourceKind,
        max_count: Option<u64>,
        raw_byte_len: Option<u64>,
    ) -> Result<(), &'static str> {
        let current_count = self
            .retained_resource_counts
            .get(&resource_kind)
            .copied()
            .unwrap_or_default();
        let Some(next_count) = current_count.checked_add(1) else {
            return Err("the frozen route request-wide resource-count limit");
        };
        if max_count.is_some_and(|limit| next_count > limit) {
            return Err("the frozen route request-wide resource-count limit");
        }
        // encodedByteLen uses the closed four-by-three Base64 expansion without approximation.
        // encodedByteLen 使用封闭的四比三 Base64 膨胀公式，不采用近似值。
        let encoded_byte_len = raw_byte_len
            .unwrap_or_default()
            .saturating_add(2)
            .checked_div(3)
            .unwrap_or_default()
            .saturating_mul(4);
        let Some(next_used) = self.used_base64_bytes.checked_add(encoded_byte_len) else {
            return Err("the request-wide host inline-media budget");
        };
        if next_used > HOST_INLINE_RESOURCE_BASE64_BUDGET_BYTES {
            return Err("the request-wide host inline-media budget");
        }
        self.retained_resource_counts
            .insert(resource_kind, next_count);
        self.used_base64_bytes = next_used;
        Ok(())
    }

    /// Reports whether the request-wide route count is already exhausted without mutating budget state.
    /// 在不修改预算状态的情况下报告请求级路线数量是否已经耗尽。
    ///
    /// # Parameters
    /// 参数
    /// `resource_kind` selects one modality and `max_count` is the exact frozen route bound.
    /// `resource_kind` 选择一种模态，`max_count` 是精确冻结路线边界。
    ///
    /// # Returns
    /// 返回值
    /// `true` only when another native resource would exceed the declared request maximum.
    /// 仅当再加入一个原生资源会超过声明的请求上限时返回 `true`。
    fn count_limit_reached(&self, resource_kind: ResourceKind, max_count: Option<u64>) -> bool {
        max_count.is_some_and(|limit| {
            self.retained_resource_counts
                .get(&resource_kind)
                .copied()
                .unwrap_or_default()
                >= limit
        })
    }
}

/// Resolves every local message resource into an immutable request-scoped URL or data URL.
/// 将每个本地消息资源解析为不可变的请求作用域原生 URL 或 Data URL。
///
/// # Parameters
/// 参数
/// `message` is the public message being prepared, `route` fixes the selected protocol,
/// and `model_resource_kinds` is the exact discovered model capability snapshot.
/// `message` 是正在准备的公共消息，`route` 固定所选协议，
/// `model_resource_kinds` 是精确发现的模型能力快照。
///
/// # Returns
/// 返回值
/// Returns success after every resource is validated and resolved, or a controlled
/// pre-dispatch error before any provider request is sent.
/// 在每个资源完成校验与解析后返回成功；否则在发送任何供应商请求前返回受控错误。
pub(super) async fn resolve_public_message_resources(
    message: &mut AiMessage,
    route: &ProviderExecutionRoute,
    model_id: &str,
    model_resource_kinds: &[ResourceKind],
    client: &ProviderExecutionClient,
    session_scope_id: Option<&str>,
    inline_resource_budget: &mut InlineResourceProjectionBudget,
) -> Result<(), VulcanCoreError> {
    for item in &mut message.content {
        let ContentItem::Resource { resource } = item else {
            continue;
        };
        if let Some(reason) = resource_projection_unavailability_reason(
            route,
            model_id,
            model_resource_kinds,
            resource,
        ) {
            *item = ContentItem::Text {
                text: resource_projection_budget_fallback_text(resource, reason.as_str(), None),
            };
            continue;
        }
        let constraint = input_resource_constraint(route, resource.kind);
        let max_count = constraint.and_then(|constraint| constraint.max_count);
        if inline_resource_budget.count_limit_reached(resource.kind, max_count) {
            *item = ContentItem::Text {
                text: resource_projection_budget_fallback_text(
                    resource,
                    "the frozen route request-wide resource-count limit",
                    None,
                ),
            };
            continue;
        }
        if resource.url.is_some() {
            // inlineRawByteLen closes the aggregate-budget gap for canonical data URLs that were
            // already materialized before this final transport pass.
            // inlineRawByteLen 为在最终传输阶段前已物化的规范 Data URL 补齐聚合预算边界。
            let inline_raw_byte_len = match inline_data_resource_raw_byte_len(resource) {
                Ok(byte_len) => byte_len,
                Err(reason) => {
                    *item = ContentItem::Text {
                        text: resource_projection_budget_fallback_text(resource, reason, None),
                    };
                    continue;
                }
            };
            if let (Some(byte_limit), Some(byte_len)) = (
                constraint.and_then(|constraint| constraint.max_bytes),
                inline_raw_byte_len,
            ) && byte_len > byte_limit
            {
                *item = ContentItem::Text {
                    text: resource_projection_budget_fallback_text(
                        resource,
                        "the frozen route byte limit",
                        Some(byte_len),
                    ),
                };
                continue;
            }
            if let Err(reason) =
                inline_resource_budget.reserve(resource.kind, max_count, inline_raw_byte_len)
            {
                *item = ContentItem::Text {
                    text: resource_projection_budget_fallback_text(
                        resource,
                        reason,
                        inline_raw_byte_len,
                    ),
                };
            }
            continue;
        }
        let Some(local_path) = resource.local_path.as_deref() else {
            *item = ContentItem::Text {
                text: resource_projection_budget_fallback_text(
                    resource,
                    "the managed resource has neither a local path nor a remote reference",
                    None,
                ),
            };
            continue;
        };
        let Some(media_type) = resource.media_type.as_deref() else {
            *item = ContentItem::Text {
                text: resource_projection_budget_fallback_text(
                    resource,
                    "the managed resource has no verified MIME type",
                    None,
                ),
            };
            continue;
        };
        if let Some(byte_limit) = constraint.and_then(|constraint| constraint.max_bytes) {
            let Ok(metadata) = tokio::fs::metadata(local_path).await else {
                *item = ContentItem::Text {
                    text: resource_projection_budget_fallback_text(
                        resource,
                        "the managed resource is no longer readable",
                        None,
                    ),
                };
                continue;
            };
            if metadata.len() > byte_limit {
                *item = ContentItem::Text {
                    text: resource_projection_budget_fallback_text(
                        resource,
                        "the frozen route byte limit",
                        Some(metadata.len()),
                    ),
                };
                continue;
            }
        }
        if let Some(prepared_reference) = client
            .prepared_media_reference(
                route,
                std::path::Path::new(local_path),
                resource.kind,
                media_type,
                session_scope_id,
                None,
            )
            .await
        {
            match inline_resource_budget.reserve(resource.kind, max_count, None) {
                Ok(()) => apply_prepared_media_reference(resource, prepared_reference),
                Err(reason) => {
                    *item = ContentItem::Text {
                        text: resource_projection_budget_fallback_text(resource, reason, None),
                    };
                }
            }
            continue;
        }
        let remote_reference_required = constraint.is_some_and(|constraint| {
            constraint.reference_policy == ResourceReferencePolicy::RemoteOnly
        });
        // Native video and every remote-only resource must have a prepared provider reference before projection.
        // 原生视频与所有仅远程资源在协议投影前都必须拥有已准备的供应商引用。
        if resource.kind == ResourceKind::Video || remote_reference_required {
            *item = ContentItem::Text {
                text: resource_projection_budget_fallback_text(
                    resource,
                    "a required Provider reference was unavailable",
                    None,
                ),
            };
            continue;
        }
        let Ok(bytes) = tokio::fs::read(local_path).await else {
            *item = ContentItem::Text {
                text: resource_projection_budget_fallback_text(
                    resource,
                    "the managed resource is no longer readable",
                    None,
                ),
            };
            continue;
        };
        if let Some(byte_limit) = constraint.and_then(|constraint| constraint.max_bytes)
            && u64::try_from(bytes.len()).unwrap_or(u64::MAX) > byte_limit
        {
            *item = ContentItem::Text {
                text: resource_projection_budget_fallback_text(
                    resource,
                    "the resource changed and exceeded the frozen route byte limit",
                    Some(u64::try_from(bytes.len()).unwrap_or(u64::MAX)),
                ),
            };
            continue;
        }
        let raw_byte_len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if let Err(reason) =
            inline_resource_budget.reserve(resource.kind, max_count, Some(raw_byte_len))
        {
            *item = ContentItem::Text {
                text: resource_projection_budget_fallback_text(
                    resource,
                    reason,
                    Some(raw_byte_len),
                ),
            };
            continue;
        }
        resource.url = Some(format!(
            "data:{media_type};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ));
    }
    Ok(())
}

/// Applies one cached provider reference according to its frozen Manifest encoding.
/// 根据冻结的 Manifest 编码应用一条已缓存供应商引用。
///
/// # Parameters
/// 参数
/// `resource` is the request-local resource projection and `prepared` carries the exact reference
/// string plus its closed upload-channel encoding.
/// `resource` 是单次请求局部资源投影；`prepared` 携带精确引用字符串及其封闭上传通道编码。
///
/// # Returns
/// 返回值
/// This function has no return value and rewrites only the request-local resource source fields.
/// 本函数无返回值，且只改写单次请求局部资源的来源字段。
fn apply_prepared_media_reference(
    resource: &mut ResourceReference,
    prepared: vulcan_provider_adapters::PreparedMediaReferenceSnapshot,
) {
    match prepared.reference_encoding {
        UploadReferenceEncoding::ProviderFileId => {
            resource.id = prepared.provider_reference;
            resource.source_kind = ResourceSourceKind::ProviderFileId;
            resource.url = None;
        }
        UploadReferenceEncoding::ResponseUrl | UploadReferenceEncoding::Template => {
            resource.source_kind = ResourceSourceKind::RemoteUrl;
            resource.url = Some(prepared.provider_reference);
        }
    }
}

/// Decodes the exact raw byte length of an already materialized inline Data URL.
/// 解码一个已物化内联 Data URL 的精确原始字节长度。
///
/// # Parameters
/// 参数
/// `resource` is the request-local resource whose source-kind contract controls interpretation.
/// `resource` 是单次请求局部资源，其来源类型契约决定解释方式。
///
/// # Returns
/// 返回值
/// `Some` raw bytes for inline data, `None` for non-inline URLs, or a controlled malformed-data reason.
/// 内联数据返回 `Some` 原始字节数，非内联 URL 返回 `None`，畸形数据返回受控原因。
fn inline_data_resource_raw_byte_len(
    resource: &ResourceReference,
) -> Result<Option<u64>, &'static str> {
    if resource.source_kind != ResourceSourceKind::InlineData {
        return Ok(None);
    }
    let encoded = resource
        .url
        .as_deref()
        .and_then(|value| value.strip_prefix("data:"))
        .and_then(|value| value.split_once(";base64,"))
        .map(|(_, encoded)| encoded)
        .ok_or("the inline resource is not a canonical Base64 Data URL")?;
    if u64::try_from(encoded.len()).unwrap_or(u64::MAX) > HOST_INLINE_RESOURCE_BASE64_BUDGET_BYTES {
        return Err("the request-wide host inline-media budget");
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "the inline resource contains invalid Base64 data")?;
    Ok(Some(u64::try_from(decoded.len()).unwrap_or(u64::MAX)))
}

/// Builds an AI-safe metadata-only replacement for one request-local resource projection.
/// 为单次请求局部资源投影构建对 AI 安全的纯元数据替代文本。
///
/// # Parameters
/// 参数
/// `resource` supplies only provider-neutral identity and MIME facts, `reason` names the exact
/// host boundary, and `byte_len` carries the verified local size when available.
/// `resource` 只提供供应商无关身份与 MIME 事实，`reason` 命名精确宿主边界，`byte_len`
/// 在可用时携带已验证本地大小。
///
/// # Returns
/// 返回值
/// A deterministic line that never exposes the managed absolute path or Provider reference.
/// 返回一条确定性文本，绝不暴露托管绝对路径或供应商引用。
fn resource_projection_budget_fallback_text(
    resource: &ResourceReference,
    reason: &str,
    byte_len: Option<u64>,
) -> String {
    // resourceLocator must not derive a model-visible locator from a Provider file id. A missing
    // canonical locator is represented explicitly instead of echoing transport-only identity.
    // resourceLocator 绝不能从供应商文件标识派生模型可见定位符；缺失规范定位符时显式标记不可用，
    // 而不是回显仅用于传输的身份。
    let resource_locator = resource
        .resource_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            if resource.source_kind == ResourceSourceKind::ProviderFileId {
                String::from("attachment://unavailable")
            } else {
                format!("attachment://{}", resource.id)
            }
        });
    // provider file ids are transport-only secrets; a degraded model-visible line falls back to
    // the disclosed canonical locator instead of echoing the internal Provider reference.
    // 供应商文件标识仅用于传输；降级后的模型可见文本改用已公开的规范定位符，绝不回显内部供应商引用。
    let model_resource_id = if resource.source_kind == ResourceSourceKind::ProviderFileId {
        resource
            .resource_path
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("unavailable")
    } else {
        resource.id.as_str()
    };
    let resource_id = model_visible_attachment_field(model_resource_id);
    let resource_locator = model_visible_attachment_field(&resource_locator);
    let file_name =
        model_visible_attachment_field(resource.file_name.as_deref().unwrap_or("unavailable"));
    let media_type = model_visible_attachment_field(
        resource
            .media_type
            .as_deref()
            .unwrap_or("application/octet-stream"),
    );
    let reason = model_visible_attachment_field(reason);
    let byte_len = byte_len
        .map(|value| value.to_string())
        .unwrap_or_else(|| String::from("unknown"));
    format!(
        "ATTACHMENT: id={resource_id}; resource_path={resource_locator}; file_name={file_name}; mime_type={media_type}; byte_len={byte_len}. NATIVE_DELIVERY: metadata_only (reason={reason}). CONTENT_VISIBILITY: resource bytes were not provided; do not infer or guess the attachment contents. Use the disclosed resource locator with an eligible registered understanding tool only when required by the user's actual intent.",
    )
}

/// Explains why one resource cannot use native projection on the selected route.
/// 说明一个资源为何无法在所选路由上使用原生投影。
///
/// # Parameters
/// 参数
/// `route` identifies the provider and selected protocol, `model_resource_kinds` contains
/// exact model declarations, and `resource` is the requested resource.
/// `route` 标识供应商与所选协议，`model_resource_kinds` 包含精确模型声明，
/// `resource` 是请求使用的资源。
///
/// # Returns
/// 返回值
/// Returns the exact Provider-neutral degradation reason, or `None` when native projection is eligible.
/// 返回精确且供应商无关的降级原因；可使用原生投影时返回 `None`。
fn resource_projection_unavailability_reason(
    route: &ProviderExecutionRoute,
    model_id: &str,
    model_resource_kinds: &[ResourceKind],
    resource: &ResourceReference,
) -> Option<String> {
    if !model_resource_kinds.contains(&resource.kind) {
        return Some(format!(
            "model `{model_id}` does not declare {:?} input",
            resource.kind
        ));
    }
    let protocol = route.protocol_route.protocol_profile_id.as_str();
    let protocol_supports = protocol_profile_supports_resource_kind(protocol, resource.kind);
    if !protocol_supports {
        return Some(format!(
            "protocol profile `{protocol}` has no registered {:?} input projector",
            resource.kind
        ));
    }
    if let Some(media_type) = resource.media_type.as_deref()
        && !vulcan_ai_protocols::protocol_profile_supports_local_media_type(
            protocol,
            resource.kind,
            media_type,
        )
    {
        return Some(format!(
            "protocol profile `{protocol}` has no registered {:?} projector for MIME type `{media_type}`",
            resource.kind
        ));
    }
    if let Some(constraint) = input_resource_constraint(route, resource.kind)
        && let Some(reason) = frozen_resource_constraint_unavailability_reason(constraint, resource)
    {
        return Some(reason);
    }
    None
}

/// Reports whether one protocol profile has any native projector for a resource modality.
/// 报告一个协议 Profile 是否拥有某种资源模态的原生投影器。
///
/// # Parameters
/// 参数
/// `protocol` is the frozen protocol profile and `kind` is the canonical resource modality.
/// `protocol` 是冻结协议 Profile，`kind` 是规范资源模态。
///
/// # Returns
/// 返回值
/// `true` only for modality/profile pairs implemented by the protocol registry; generic files
/// always remain metadata-only until a concrete file projector is registered.
/// 仅协议注册表已实现的模态/Profile 组合返回 `true`；通用文件在注册具体投影器前始终保持纯元数据。
fn protocol_profile_supports_resource_kind(protocol: &str, kind: ResourceKind) -> bool {
    match protocol {
        vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID => matches!(
            kind,
            ResourceKind::Image | ResourceKind::Video | ResourceKind::Audio
        ),
        vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID
        | vulcan_ai_protocols::OPENAI_CODEX_RESPONSES_PROFILE_ID => kind == ResourceKind::Image,
        vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID
        | vulcan_ai_protocols::CLAUDE_CODE_MESSAGES_PROFILE_ID => kind == ResourceKind::Image,
        vulcan_ai_protocols::ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID => {
            matches!(kind, ResourceKind::Image | ResourceKind::Video)
        }
        vulcan_ai_protocols::GEMINI_GENERATE_CONTENT_PROFILE_ID
        | vulcan_ai_protocols::GEMINI_INTERACTIONS_PROFILE_ID
        | vulcan_ai_protocols::GOOGLE_CODE_ASSIST_PROFILE_ID => matches!(
            kind,
            ResourceKind::Image | ResourceKind::Video | ResourceKind::Audio
        ),
        _ => false,
    }
}

/// Explains one frozen route-constraint mismatch without dispatching or mutating canonical state.
/// 说明一项冻结路由约束不匹配的原因，不进行分派，也不修改规范状态。
///
/// # Parameters
/// 参数
/// `constraint` is the immutable route declaration and `resource` is the request-local projection candidate.
/// `constraint` 是不可变路由声明，`resource` 是单次请求局部投影候选。
///
/// # Returns
/// 返回值
/// A deterministic metadata-only reason, or `None` when the frozen constraint admits the candidate.
/// 返回确定性的纯元数据降级原因；冻结约束允许候选资源时返回 `None`。
fn frozen_resource_constraint_unavailability_reason(
    constraint: &InputResourceConstraint,
    resource: &ResourceReference,
) -> Option<String> {
    if !constraint.accepted_mime_types.is_empty()
        && !resource.media_type.as_ref().is_some_and(|media_type| {
            constraint
                .accepted_mime_types
                .iter()
                .any(|accepted| accepted.eq_ignore_ascii_case(media_type))
        })
    {
        return Some(format!(
            "the frozen {:?} input constraint does not accept this MIME type",
            resource.kind
        ));
    }
    let source_admitted = match constraint.reference_policy {
        ResourceReferencePolicy::Any => true,
        ResourceReferencePolicy::LocalOnly => {
            resource.local_path.is_some()
                && (resource.url.is_none()
                    || resource.source_kind == ResourceSourceKind::HostResolved)
        }
        ResourceReferencePolicy::RemoteOnly => {
            (resource.url.is_some() && resource.local_path.is_none())
                || (resource.local_path.is_some() && !constraint.upload_channel_refs.is_empty())
        }
    };
    (!source_admitted).then(|| {
        format!(
            "the resource source violates the frozen {:?} reference policy",
            constraint.reference_policy
        )
    })
}

/// Returns the frozen constraint for one resource kind on the selected route.
/// 返回所选路由上一种资源类别的冻结约束。
///
/// # Parameters
/// 参数
/// `route` is the immutable execution route and `resource_kind` selects one closed kind.
/// `route` 是不可变执行路由，`resource_kind` 选择一种封闭类别。
///
/// # Returns
/// 返回值
/// The exact frozen constraint, or `None` when the route accepts the projector baseline.
/// 返回精确冻结约束；路由接受投影器基线时返回 `None`。
fn input_resource_constraint(
    route: &ProviderExecutionRoute,
    resource_kind: ResourceKind,
) -> Option<&InputResourceConstraint> {
    route
        .protocol_route
        .input_resource_constraints
        .iter()
        .find(|constraint| constraint.resource_kind == resource_kind)
}

#[cfg(test)]
mod tests;
