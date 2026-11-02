//! Manifest-driven generic upload engine.
//! Manifest 驱动的通用上传引擎。
//!
//! The engine materializes one canonical input resource (local file, inline bytes, or remote
//! URL) through a declared `UploadChannelDeclaration`, selecting the channel by reference
//! instead of by provider-named enumeration (plan §R9.5). Every bounded limit is enforced
//! before any network call: MIME allowlist, byte size, item count, explicit HTTPS host, and
//! safe absolute endpoint path.
//! 引擎通过声明的 `UploadChannelDeclaration` 物化一个规范输入资源（本地文件、内联字节或
//! 远端 URL），按引用选择通道而不是按供应商命名枚举（方案 §R9.5）。所有有界限制都在
//! 任何网络调用之前强制：MIME 允许列表、字节大小、条目数量、显式 HTTPS 宿主与安全绝对
//! 端点路径。

use crate::{
    CredentialExecutionSecret, CredentialProfile, MediaUploadCancellation, MediaUploadFailure,
    MediaUploadProgressCallback, MediaUploadStage, ServiceEntry, UploadPrimitiveKind,
    UploadReferenceEncoding, UploadResponseIdEncoding,
};
use async_stream::stream;
use base64::Engine;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::Deserialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

mod delivery_policy;
mod file_create_body;
mod inputs;
mod openai_codex;
use delivery_policy::resolve_operation_delivery_channel;
pub(crate) use delivery_policy::{
    UploadChannelDeliveryKind, resolve_real_upload_channel, upload_channel_delivery_kind,
};
pub(crate) use inputs::operation_upload_inputs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::AsyncReadExt;

/// One canonical input resource to be materialized through an upload channel.
/// 一个需要经上传通道物化的规范输入资源。
#[derive(Debug, Clone)]
pub(crate) struct UploadResource {
    /// Optional immutable source proof supplied by the host resource owner, never by AI arguments.
    /// 由宿主资源所有者而非AI参数提供的可选不可变来源证据。
    pub source_identity: Option<crate::PreparedMediaSourceIdentity>,
    /// Original catalog filename when the physical entity is digest-addressed.
    /// 物理实体使用摘要寻址时的原目录文件名。
    pub file_name: Option<String>,
    /// Local file path when the resource is host-backed.
    /// 资源由宿主文件承载时的本地路径。
    pub local_path: Option<PathBuf>,
    /// Inline bytes when the resource is provided directly.
    /// 资源直接提供时的内联字节。
    pub inline_bytes: Option<Vec<u8>>,
    /// Exact resource MIME type.
    /// 精确资源 MIME 类型。
    pub mime_type: String,
    /// Exact resource byte length.
    /// 精确资源字节长度。
    pub byte_len: u64,
    /// Remote URL when the primitive is a URL passthrough.
    /// 原语为 URL 直通时的远端 URL。
    pub remote_url: Option<String>,
    /// Item count for count-bounded channels.
    /// 数量受限通道的条目数量。
    pub item_count: u64,
}

/// One materialized provider object reference consumed by an operation request.
/// 一个被操作请求消费的已物化供应商对象引用。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UploadedResourceReference {
    /// Exact provider reference consumed by the operation request.
    /// 被操作请求消费的精确供应商引用。
    pub reference: String,
    /// Encoding used to render the reference.
    /// 渲染引用使用的编码。
    pub encoding: UploadReferenceEncoding,
    /// Optional raw response identifier retained for diagnostics.
    /// 可选原始响应标识，保留用于诊断。
    pub response_id: Option<String>,
    /// Optional raw response URL retained for diagnostics.
    /// 可选原始响应 URL，保留用于诊断。
    pub response_url: Option<String>,
}

/// One completely preflighted resource upload before any network side effect.
/// 任何网络副作用发生前完成全部预检的一项资源上传。
struct PreparedOperationUpload<'a> {
    /// Index of the owning operation binding.
    /// 所属操作绑定的索引。
    binding_index: usize,
    /// Optional owning array-item index for one item-field binding.
    /// 数组项字段绑定对应的可选所属数组项索引。
    item_index: Option<usize>,
    /// Exact selected upload channel.
    /// 精确选中的上传通道。
    channel: crate::UploadChannelDeclaration,
    /// Canonical resource with stable bytes/metadata.
    /// 携带稳定字节/元数据的规范资源。
    resource: UploadResource,
    /// Borrowed canonical Base64 payload decoded only after the complete operation passes preflight.
    /// 仅在完整操作通过预检后才解码的借用规范 Base64 负载。
    inline_base64: Option<&'a str>,
}

/// Source metadata with an optional borrowed Data URL payload; no decoded bytes are allocated yet.
/// 来源元数据及可选借用 Data URL 负载；此时尚未分配解码字节。
struct OperationUploadSource<'a> {
    /// Exact source metadata for policy and upload-contract checks.
    /// 用于策略与上传契约检查的精确来源元数据。
    resource: UploadResource,
    /// Canonical encoded input borrowed from immutable operation arguments.
    /// 从不可变操作参数借用的规范编码输入。
    inline_base64: Option<&'a str>,
}

/// Carries the exact local operation upload resource and selected channel after bounded preflight.
/// 携带完成有界预检后的精确本地操作上传资源与选中通道。
#[derive(Debug, Clone)]
pub(crate) struct OperationUploadRouteInspection {
    /// The immutable upload channel selected by MIME, limits, references, and credential role.
    /// 由 MIME、限制、引用与凭据角色共同选出的不可变上传通道。
    pub channel: crate::UploadChannelDeclaration,
    /// The validated source resource whose local path and byte length are stable for this phase.
    /// 当前阶段本地路径与字节长度均已校验稳定的源资源。
    pub resource: UploadResource,
}

/// Frozen host dependencies used to materialize one operation's uploads.
/// 用于物化一次操作上传的冻结宿主依赖。
pub(crate) struct OperationUploadExecutionContext<'a> {
    /// Explicit host-resolved sources indexed by their exact model-visible locator, outside the JSON contract.
    /// 按精确模型可见定位符匹配、位于JSON契约之外的显式宿主解析来源。
    pub media_sources: &'a [crate::ProviderToolMediaSource],
    /// Host-frozen independent-extension policy; never derived from AI arguments.
    /// 宿主冻结的独立扩展策略，绝不从 AI 参数派生。
    pub media_delivery_policy: crate::MediaDeliveryPolicy,
    /// HTTP client whose redirect and timeout policy applies to every upload.
    /// 对每次上传应用重定向与超时策略的 HTTP 客户端。
    pub http: &'a reqwest::Client,
    /// Exact service entry selected from the same manifest generation.
    /// 从同一 Manifest 代际选出的精确服务入口。
    pub entry: &'a ServiceEntry,
    /// Frozen credential projection for the selected entry.
    /// 为所选入口冻结的凭据投影。
    pub secret: &'a CredentialExecutionSecret,
    /// Exact credential profile selected by the owning connection.
    /// 所属连接选择的精确凭据配置档。
    pub credential_profile: &'a CredentialProfile,
    /// Upload channel declarations from the selected manifest generation.
    /// 来自所选 Manifest 代际的上传通道声明。
    pub channels: &'a [crate::UploadChannelDeclaration],
    /// Optional cooperative progress callback for the complete operation.
    /// 整次操作使用的可选协作式进度回调。
    pub progress: Option<MediaUploadProgressCallback>,
    /// Cooperative cancellation signal frozen for the complete operation.
    /// 为整次操作冻结的协作式取消信号。
    pub cancellation: &'a MediaUploadCancellation,
    /// Optional exact model identifier required by model-scoped signing handshakes.
    /// 模型作用域签名握手所需的可选精确模型标识。
    pub model_id: Option<&'a str>,
    /// Optional canonical workspace base for relative local resource paths.
    /// 相对本地资源路径使用的可选规范工作区基准。
    pub workspace_root: Option<&'a Path>,
    /// Fixture-only HTTP loopback origin used by the closed local conformance harness.
    /// 仅由封闭本地一致性夹具使用的 HTTP 回环源地址。
    pub fixture_loopback_origin: Option<&'a str>,
}

/// Frozen dependencies for materializing one resource through one upload channel.
/// 通过一个上传通道物化单个资源所需的冻结依赖。
pub(crate) struct UploadExecutionContext<'a> {
    /// HTTP client whose redirect and timeout policy applies to the upload.
    /// 对上传应用重定向与超时策略的 HTTP 客户端。
    pub http: &'a reqwest::Client,
    /// Exact service entry selected from the manifest generation.
    /// 从 Manifest 代际选出的精确服务入口。
    pub entry: &'a ServiceEntry,
    /// Frozen credential projection for the selected entry.
    /// 为所选入口冻结的凭据投影。
    pub secret: &'a CredentialExecutionSecret,
    /// Exact credential profile selected by the owning connection.
    /// 所属连接选择的精确凭据配置档。
    pub credential_profile: &'a CredentialProfile,
    /// Optional cooperative progress callback.
    /// 可选的协作式进度回调。
    pub progress: Option<MediaUploadProgressCallback>,
    /// Cooperative cancellation signal.
    /// 协作式取消信号。
    pub cancellation: &'a MediaUploadCancellation,
    /// Optional exact model identifier required by signing handshakes.
    /// 签名握手所需的可选精确模型标识。
    pub model_id: Option<&'a str>,
    /// Fixture-only HTTP loopback origin used by the closed local conformance harness.
    /// 仅由封闭本地一致性夹具使用的 HTTP 回环源地址。
    pub fixture_loopback_origin: Option<&'a str>,
}

/// Materializes every declared operation upload binding before provider-tool dispatch.
/// 在供应商工具分派前物化每个声明的操作上传绑定。
///
/// All bindings are parsed, MIME-resolved, channel-selected, and limit-checked before the first
/// network primitive executes. The returned JSON replaces only exact declared target pointers;
/// missing optional fields remain untouched and no provider-named fallback is attempted.
/// 所有绑定都会在第一个网络原语执行前完成解析、MIME 解析、通道选择与限制校验。返回 JSON
/// 仅替换精确声明的目标 Pointer；缺失的可选字段保持不变，且不会尝试供应商具名回退。
///
/// # Parameters
/// 参数
/// `arguments` is the canonical tool input; `operation` and `context.channels` belong to one
/// immutable manifest generation; `context` freezes host, credential, and transfer dependencies.
/// `arguments` 是规范工具输入；`operation` 与 `context.channels` 属于同一不可变 Manifest
/// 代际；`context` 冻结宿主、凭据与传输依赖。
///
/// # Returns
/// 返回值
/// Materialized wire arguments, or a deterministic preflight/transport failure.
/// 返回已物化 Wire 参数，或确定性的预检/传输失败。
pub(crate) async fn materialize_operation_uploads(
    context: OperationUploadExecutionContext<'_>,
    operation: &crate::OperationDeclaration,
    mut arguments: Value,
) -> Result<Value, String> {
    if operation.upload_bindings.is_empty() {
        return Ok(arguments);
    }
    // One planner accounts for all media bindings in canonical input order before any materialization.
    // 同一规划器在任何物化之前按规范输入顺序统计全部媒体绑定。
    let mut planner = crate::MediaDeliveryPlanner::new(
        context.media_delivery_policy,
        crate::MediaDeliveryPurpose::IndependentExtension,
    )?;
    let mut prepared = Vec::new();
    // Track exact host locators consumed by declared bindings; unused proofs must never permit URI leakage.
    // 追踪声明绑定消费的精确宿主定位符；未消费证据绝不能允许URI泄露。
    let mut consumed_sources = std::collections::BTreeSet::new();
    let mut source_was_array = BTreeMap::new();
    for input in operation_upload_inputs(operation, &arguments)? {
        // Resolution and materialization consume the same exact declared occurrences.
        // 解析和物化消费相同精确声明出现。
        let binding = &operation.upload_bindings[input.binding_index];
        source_was_array.insert(input.binding_index, input.source_was_array);
        // Host metadata precedes admission; no source bytes are read here.
        // 宿主元数据先于准入；此处不读取来源字节。
        let source = prepare_operation_upload_resource(
            binding,
            input.reference,
            context.workspace_root,
            context.media_sources,
        )
        .await?;
        consumed_sources.insert(input.reference);
        // One frozen policy accounts for every declared occurrence, including repeated sources.
        // 同一冻结策略统计每个声明出现，包括重复来源。
        let channel = resolve_operation_delivery_channel(
            context.channels,
            &binding.upload_channel_refs,
            &source.resource,
            context.credential_profile.credential_role.as_deref(),
            &mut planner,
            &operation.input_resources,
        )?
        .clone();
        prepared.push(PreparedOperationUpload {
            binding_index: input.binding_index,
            item_index: input.item_index,
            channel,
            resource: source.resource,
            inline_base64: source.inline_base64,
        });
    }
    if media_sources_are_unconsumed(context.media_sources, &consumed_sources) {
        return Err(String::from(
            "Host media source is not consumed by an exact operation upload binding.",
        ));
    }
    // Validate deferred payload shape and model-scoped signing before the first network side effect.
    // 在首个网络副作用之前校验延迟负载形态与模型作用域签名。
    for upload in &prepared {
        // Model-scoped signing must be executable even when a small resource prefers inline delivery.
        // 即使小资源偏好内联投递，模型作用域签名仍必须具备可执行条件。
        let real_channel = if upload.resource.remote_url.is_some() {
            None
        } else {
            resolve_real_upload_channel(
                context.channels,
                &operation.upload_bindings[upload.binding_index].upload_channel_refs,
                &upload.resource,
                context.credential_profile.credential_role.as_deref(),
            )?
        };
        if real_channel.is_some_and(|channel| {
            channel.primitive == Some(UploadPrimitiveKind::SignedObjectUpload)
        }) && context.model_id.is_none()
        {
            return Err(String::from(
                "The selected upload signing contract requires an exact host-owned model identifier.",
            ));
        }
        if let Some(encoded) = upload.inline_base64 {
            validate_operation_base64_payload(encoded, upload.resource.byte_len)?;
        }
    }
    // Verify all admitted host sources before the first upload, so a corrupt later item cannot cause partial dispatch.
    // 在首次上传前验证全部已准入宿主来源，防止后项损坏导致部分分派。
    for upload in &prepared {
        verify_host_upload_source(&upload.resource, context.cancellation).await?;
    }
    let mut references = BTreeMap::<usize, Vec<(Option<usize>, Value)>>::new();
    for mut upload in prepared {
        // Decode only an admitted resource after every binding has passed limits and shape validation.
        // 仅在全部绑定通过限制与形态校验后，解码一个已准入资源。
        if let Some(encoded) = upload.inline_base64 {
            upload.resource.inline_bytes = Some(
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|error| {
                        format!("Operation upload Data URL base64 is invalid: {error}")
                    })?,
            );
        }
        let materialized = materialize_upload(
            UploadExecutionContext {
                http: context.http,
                entry: context.entry,
                secret: context.secret,
                credential_profile: context.credential_profile,
                progress: if upload_channel_delivery_kind(&upload.channel)?
                    == UploadChannelDeliveryKind::ProviderUpload
                {
                    context.progress.clone()
                } else {
                    None
                },
                cancellation: context.cancellation,
                model_id: context.model_id,
                fixture_loopback_origin: context.fixture_loopback_origin,
            },
            &upload.channel,
            &upload.resource,
        )
        .await?;
        let value = operation_upload_target_value(
            &operation.upload_bindings[upload.binding_index],
            &upload.resource,
            &materialized,
        )?;
        references
            .entry(upload.binding_index)
            .or_default()
            .push((upload.item_index, value));
    }
    for (binding_index, values) in references {
        let binding = &operation.upload_bindings[binding_index];
        if let (Some(item_source_path), Some(item_target_path)) = (
            binding.item_source_path.as_deref(),
            binding.item_target_path.as_deref(),
        ) {
            for (item_index, value) in values {
                let item_index = item_index.ok_or_else(|| {
                    format!(
                        "Operation `{}` lost an item upload destination for `{}`.",
                        operation.operation_id, binding.target_path
                    )
                })?;
                let target_path = format!("{}/{item_index}{item_target_path}", binding.target_path);
                write_operation_argument_pointer(&mut arguments, target_path.as_str(), value)?;
                if binding.remove_source && item_source_path != item_target_path {
                    let source_path =
                        format!("{}/{item_index}{item_source_path}", binding.source_path);
                    remove_operation_argument_pointer(&mut arguments, source_path.as_str())?;
                }
            }
            continue;
        }
        let values = values
            .into_iter()
            .map(|(item_index, value)| {
                if item_index.is_some() {
                    return Err(format!(
                        "Operation `{}` produced an unexpected item upload for `{}`.",
                        operation.operation_id, binding.target_path
                    ));
                }
                Ok(value)
            })
            .collect::<Result<Vec<_>, String>>()?;
        let value = if source_was_array.get(&binding_index) == Some(&true) {
            Value::Array(values)
        } else {
            values.into_iter().next().ok_or_else(|| {
                format!(
                    "Operation `{}` produced no reference for `{}`.",
                    operation.operation_id, binding.target_path
                )
            })?
        };
        write_operation_argument_pointer(&mut arguments, &binding.target_path, value)?;
        if binding.remove_source && binding.source_path != binding.target_path {
            remove_operation_argument_pointer(&mut arguments, &binding.source_path)?;
        }
    }
    Ok(arguments)
}

/// Renders one materialized upload into the exact closed binding target shape.
/// 把一项已物化上传渲染为精确的封闭绑定目标结构。
///
/// # Parameters
/// 参数
/// `binding` selects the shape, `resource` owns trusted MIME metadata, and `uploaded` owns the reference.
/// `binding` 选择结构，`resource` 持有可信 MIME 元数据，`uploaded` 持有引用。
///
/// # Returns
/// 返回值
/// One scalar reference or provider-neutral resource descriptor.
/// 返回一个标量引用或供应商中性资源描述符。
fn operation_upload_target_value(
    binding: &crate::OperationUploadBinding,
    resource: &UploadResource,
    uploaded: &UploadedResourceReference,
) -> Result<Value, String> {
    match binding.target_encoding {
        crate::provider_manifest::OperationUploadTargetEncoding::Reference => {
            Ok(Value::String(uploaded.reference.clone()))
        }
        crate::provider_manifest::OperationUploadTargetEncoding::ResourceDescriptor => {
            let kind = resource
                .mime_type
                .split_once('/')
                .map(|(kind, _)| kind)
                .filter(|kind| !kind.is_empty())
                .ok_or_else(|| {
                    format!(
                        "Upload MIME type `{}` cannot produce a resource descriptor kind.",
                        resource.mime_type
                    )
                })?;
            let mut descriptor = Map::from_iter([
                (String::from("kind"), Value::String(kind.to_string())),
                (
                    String::from("mime_type"),
                    Value::String(resource.mime_type.clone()),
                ),
                (
                    String::from("reference"),
                    Value::String(uploaded.reference.clone()),
                ),
            ]);
            if let Some((_, payload)) = uploaded.reference.split_once(";base64,")
                && uploaded.reference.starts_with("data:")
            {
                descriptor.insert(
                    String::from("base64_data"),
                    Value::String(payload.to_string()),
                );
            }
            Ok(Value::Object(descriptor))
        }
    }
}

/// Applies one already-uploaded reference to a scalar media-understanding binding.
/// 将一条已经上传的引用应用到一个标量媒体理解绑定。
///
/// # Parameters
/// 参数
/// `binding` defines the exact source/target pointers; `resource` carries the verified MIME;
/// `arguments` is the canonical fixed-tool input; `provider_reference` and `reference_encoding`
/// are already validated by the selected upload channel.
/// `binding` 定义精确源/目标指针；`resource` 携带已校验 MIME；`arguments` 是规范固定工具输入；
/// `provider_reference` 与 `reference_encoding` 已由选中上传通道校验。
///
/// # Returns
/// 返回值
/// The canonical arguments with the provider reference written and the source removed when the
/// binding requires it, or an explicit shape error for non-scalar bindings.
/// 写入供应商引用并按绑定要求移除规范源字段后的参数；非标量绑定返回明确结构错误。
pub(crate) fn materialize_cached_scalar_operation_upload(
    binding: &crate::OperationUploadBinding,
    resource: &UploadResource,
    mut arguments: Value,
    provider_reference: String,
    reference_encoding: UploadReferenceEncoding,
) -> Result<Value, String> {
    if binding.item_source_path.is_some() || binding.item_target_path.is_some() {
        return Err(String::from(
            "Cached media-understanding upload does not support item-field bindings.",
        ));
    }
    let source = arguments.pointer(&binding.source_path).ok_or_else(|| {
        format!(
            "Cached operation upload source `{}` is missing.",
            binding.source_path
        )
    })?;
    if !source.is_string() {
        return Err(format!(
            "Cached operation upload source `{}` must be a scalar string.",
            binding.source_path
        ));
    }
    let uploaded = UploadedResourceReference {
        reference: provider_reference,
        encoding: reference_encoding,
        response_id: None,
        response_url: None,
    };
    let value = operation_upload_target_value(binding, resource, &uploaded)?;
    write_operation_argument_pointer(&mut arguments, &binding.target_path, value)?;
    if binding.remove_source && binding.source_path != binding.target_path {
        remove_operation_argument_pointer(&mut arguments, &binding.source_path)?;
    }
    Ok(arguments)
}

/// Prepares one canonical upload resource without issuing any network request.
/// 在不发出任何网络请求的情况下准备一项规范上传资源。
///
/// # Parameters
/// 参数
/// `binding` owns MIME and channel references; `reference` is an exact local path/Data URL/HTTPS
/// URL; `workspace_root` resolves ordinary relative local paths and `media_sources` supplies explicit host-owned identities.
/// `binding` 持有 MIME 与通道引用；`reference` 是精确本地路径/Data URL/HTTPS URL；
/// `workspace_root`解析普通相对本地路径，`media_sources`提供明确宿主持有身份。
/// Each result is one file in one materialization, not the cardinality of its argument array.
/// 每个结果均是一次物化中的一个文件，而非其参数数组的基数。
///
/// # Returns
/// 返回值
/// A stable resource descriptor ready for channel selection and limit validation.
/// 返回可用于通道选择与限制校验的稳定资源描述符。
async fn prepare_operation_upload_resource<'a>(
    binding: &crate::OperationUploadBinding,
    reference: &'a str,
    workspace_root: Option<&Path>,
    media_sources: &[crate::ProviderToolMediaSource],
) -> Result<OperationUploadSource<'a>, String> {
    // Only an exact host-proven reference bypasses workspace-path inference; no alternate names or fields are searched.
    // 仅精确宿主已证明引用可跳过工作区路径推断，不搜索其他名称或字段。
    let mut matching = media_sources
        .iter()
        .filter(|source| source.reference == reference);
    if let Some(source) = matching.next() {
        if matching.next().is_some() || !source.local_path.is_absolute() {
            return Err(String::from(
                "Host media source identity is ambiguous or invalid.",
            ));
        }
        validate_operation_source_mime(binding, &source.mime_type)?;
        // Catalog metadata remains authoritative even when the physical entity has no extension.
        // 即使物理实体没有扩展名，目录元数据仍是权威。
        return Ok(OperationUploadSource {
            resource: UploadResource {
                source_identity: Some(source.source_identity.clone()),
                file_name: Some(source.file_name.clone()),
                local_path: Some(source.local_path.clone()),
                inline_bytes: None,
                mime_type: source.mime_type.clone(),
                byte_len: source.source_identity.byte_len,
                remote_url: None,
                item_count: 1,
            },
            inline_base64: None,
        });
    }
    if reference.starts_with("project-resource:") || reference.starts_with("attachment:") {
        return Err(String::from(
            "Media resource locator has no host-owned source resolution.",
        ));
    }
    if let Some(payload) = reference.strip_prefix("data:") {
        // The exact Data URL shape is validated without allocating its decoded payload.
        // 校验精确 Data URL 形态，不分配其解码负载。
        let (metadata, encoded) = payload
            .split_once(',')
            .ok_or_else(|| String::from("Operation upload source carries a malformed Data URL."))?;
        let mime_type = metadata
            .strip_suffix(";base64")
            .filter(|mime| !mime.is_empty() && !mime.contains(';'))
            .ok_or_else(|| {
                String::from(
                    "Operation upload Data URL must declare one MIME type and base64 encoding.",
                )
            })?;
        validate_operation_source_mime(binding, mime_type)?;
        // Shape-only length calculation precedes policy checks; no raw payload is retained.
        // 仅依据形态计算长度发生在策略检查之前，不保留原始负载。
        let decoded_byte_len = standard_base64_decoded_len(encoded)?;
        if decoded_byte_len > crate::MEDIA_INLINE_RAW_HARD_LIMIT_BYTES {
            return Err(String::from(
                "Media extension cannot process this file: the inline source is too large for the host memory limit.",
            ));
        }
        return Ok(OperationUploadSource {
            resource: UploadResource {
                source_identity: None,
                file_name: None,
                local_path: None,
                inline_bytes: None,
                mime_type: mime_type.to_string(),
                byte_len: decoded_byte_len,
                remote_url: None,
                item_count: 1,
            },
            inline_base64: Some(encoded),
        });
    }
    // is_remote_https recognizes the case-insensitive URL scheme before local-path resolution.
    // is_remote_https 在本地路径解析前识别不区分大小写的 URL scheme。
    let is_remote_https = reference
        .get(.."https://".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"));
    if is_remote_https {
        // Remote passthrough has unknown source size and is never downloaded as an inline fallback.
        // 远程直通的来源大小未知，绝不下载为内联回退。
        let url = reqwest::Url::parse(reference)
            .map_err(|_| String::from("Operation upload source is not a valid HTTPS URL."))?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(String::from(
                "Operation upload source must be a credential-free absolute HTTPS URL.",
            ));
        }
        let mime_type = operation_upload_mime(binding, url.path())?;
        return Ok(OperationUploadSource {
            resource: UploadResource {
                source_identity: None,
                file_name: None,
                local_path: None,
                inline_bytes: None,
                mime_type,
                byte_len: 0,
                remote_url: Some(reference.to_string()),
                item_count: 1,
            },
            inline_base64: None,
        });
    }
    // Workspace resolution and file metadata establish local source size before any file read.
    // 工作区解析与文件元数据在任何文件读取之前确定本地来源大小。
    let path = resolve_provider_tool_local_path(workspace_root, reference).await?;
    let metadata = tokio::fs::metadata(&path).await.map_err(|error| {
        format!("Operation upload source metadata could not be inspected: {error}")
    })?;
    if !metadata.is_file() {
        return Err(String::from(
            "Operation upload source must be a regular file.",
        ));
    }
    let mime_type = operation_upload_mime(binding, reference)?;
    Ok(OperationUploadSource {
        resource: UploadResource {
            source_identity: None,
            file_name: None,
            local_path: Some(path),
            inline_bytes: None,
            mime_type,
            byte_len: metadata.len(),
            remote_url: None,
            item_count: 1,
        },
        inline_base64: None,
    })
}

/// Preflights one operation upload source and returns its exact channel/resource pair.
/// 预检一个操作上传源并返回精确的通道/资源组合。
///
/// # Parameters
/// 参数
/// The arguments are identical to `prepare_operation_upload_resource`; no network request is
/// issued and the returned channel is cloned from the frozen manifest snapshot.
/// 参数与 `prepare_operation_upload_resource` 相同；不会发出网络请求，返回通道来自冻结
/// Manifest 快照的克隆。
///
/// # Returns
/// 返回值
/// A fully validated operation upload route inspection.
/// 完成全部校验的操作上传路由预检结果。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn inspect_operation_upload_route(
    binding: &crate::OperationUploadBinding,
    reference: &str,
    workspace_root: Option<&Path>,
    channels: &[crate::UploadChannelDeclaration],
    credential_role: Option<&str>,
    planner: &mut crate::MediaDeliveryPlanner,
    input_resources: &[vulcan_ai_contract::ResourceKind],
    media_sources: &[crate::ProviderToolMediaSource],
) -> Result<OperationUploadRouteInspection, String> {
    let source =
        prepare_operation_upload_resource(binding, reference, workspace_root, media_sources)
            .await?;
    let channel = resolve_operation_delivery_channel(
        channels,
        &binding.upload_channel_refs,
        &source.resource,
        credential_role,
        planner,
        input_resources,
    )?
    .clone();
    if let Some(encoded) = source.inline_base64 {
        validate_operation_base64_payload(encoded, source.resource.byte_len)?;
    }
    Ok(OperationUploadRouteInspection {
        channel,
        resource: source.resource,
    })
}

/// Computes the exact decoded byte length of one canonical standard-Base64 value.
/// 计算一段规范标准 Base64 值的精确解码字节长度。
///
/// # Parameters
/// 参数
/// `encoded` is the unmodified payload after the Data URL comma.
/// `encoded` 是 Data URL 逗号之后未经修改的负载。
///
/// # Returns
/// 返回值
/// The exact allocation length, or a controlled shape error before decoding allocates memory.
/// 返回精确分配长度，或在解码分配内存前返回受控形态错误。
fn standard_base64_decoded_len(encoded: &str) -> Result<u64, String> {
    let encoded_len = u64::try_from(encoded.len()).map_err(|_| {
        String::from("Operation upload Data URL length exceeds the platform limit.")
    })?;
    if encoded_len == 0 || encoded_len % 4 != 0 {
        return Err(String::from(
            "Operation upload Data URL base64 must have a non-empty length divisible by four.",
        ));
    }
    let padding = if encoded.ends_with("==") {
        2_u64
    } else if encoded.ends_with('=') {
        1_u64
    } else {
        0_u64
    };
    encoded_len
        .checked_div(4)
        .and_then(|groups| groups.checked_mul(3))
        .and_then(|bytes| bytes.checked_sub(padding))
        .ok_or_else(|| String::from("Operation upload Data URL decoded length overflows."))
}

/// Validates canonical `encoded` content against `expected_bytes` using one bounded stack buffer.
/// 使用一个有界栈缓冲区，按照 `expected_bytes` 校验规范 `encoded` 内容。
/// Returns no retained payload; invalid alphabet, padding or byte counts fail before any upload.
/// 不返回保留负载；无效字符、填充或字节数在任何上传之前失败。
fn validate_operation_base64_payload(encoded: &str, expected_bytes: u64) -> Result<(), String> {
    // Each full chunk is a multiple of four and needs exactly three quarters as many output bytes.
    // 每个完整分块是四的倍数，输出字节数精确为其四分之三。
    let mut scratch = [0_u8; 12 * 1024];
    // Summed decoded length also rejects premature padding before the last chunk.
    // 解码长度总和也能拒绝最后一个分块之前的过早填充。
    let mut decoded = 0_u64;
    for chunk in encoded.as_bytes().chunks(16 * 1024) {
        // Validation shares the exact strict decoder used by materialization without heap allocation.
        // 校验复用物化所用的精确严格解码器，不进行堆分配。
        let count = base64::engine::general_purpose::STANDARD
            .decode_slice(chunk, &mut scratch)
            .map_err(|error| format!("Operation upload Data URL base64 is invalid: {error}"))?;
        decoded = decoded
            .checked_add(count as u64)
            .ok_or_else(|| String::from("Operation Base64 decoded size overflowed."))?;
    }
    if decoded != expected_bytes {
        return Err(String::from(
            "Operation upload Data URL decoded length disagrees with its canonical Base64 shape.",
        ));
    }
    Ok(())
}

/// Resolves one ordinary provider-tool file path against the host filesystem.
/// 基于宿主文件系统解析一条普通 Provider 工具文件路径。
///
/// # Parameters
/// 参数
/// `workspace_root` is the base directory for relative references and `reference` is the exact
/// AI-visible local path value. Absolute references do not require a workspace.
/// `workspace_root` 是相对引用的基准目录，`reference` 是 AI 可见的精确本地路径值；绝对引用
/// 不要求存在工作区。
///
/// # Returns
/// 返回值
/// The canonical existing path. The current product has no filesystem permission boundary, so
/// project-external and symlink-resolved targets remain available; a future permission gate must
/// authorize the canonical target before this resolver is called.
/// 返回规范的已存在路径。当前产品尚无文件系统权限边界，因此允许项目外路径及符号链接解析后的
/// 目标；未来权限门必须在调用本解析器前对规范目标完成授权。
pub async fn resolve_provider_tool_local_path(
    workspace_root: Option<&Path>,
    reference: &str,
) -> Result<PathBuf, String> {
    let source = PathBuf::from(reference);
    if source.as_os_str().is_empty() {
        return Err(String::from(
            "Local provider-tool resource path must not be empty.",
        ));
    }
    let resolved = if source.is_absolute() {
        source
    } else {
        let workspace_root = workspace_root.ok_or_else(|| {
            String::from("Relative local provider-tool resources require an active workspace root.")
        })?;
        let canonical_root = tokio::fs::canonicalize(workspace_root)
            .await
            .map_err(|_| String::from("Provider-tool workspace root cannot be resolved."))?;
        canonical_root.join(source)
    };
    tokio::fs::canonicalize(&resolved)
        .await
        .map_err(|_| String::from("Provider tool resource cannot be resolved."))
}

/// Resolves one exact MIME type from binding-owned literal or extension rules.
/// 从绑定持有的字面值或扩展名规则解析一个精确 MIME 类型。
///
/// # Parameters
/// 参数
/// `binding` owns all allowed mappings and `source` supplies only the final extension.
/// `binding` 持有全部允许映射，`source` 仅提供最终扩展名。
///
/// # Returns
/// 返回值
/// The exact configured MIME type; absent mappings fail closed.
/// 返回精确配置的 MIME 类型；映射缺失时关闭失败。
fn operation_upload_mime(
    binding: &crate::OperationUploadBinding,
    source: &str,
) -> Result<String, String> {
    if let Some(mime_type) = binding.mime_type.as_ref() {
        return Ok(mime_type.clone());
    }
    let extension = std::path::Path::new(source)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| {
            format!(
                "Operation upload source `{source}` has no extension and the binding declares no exact MIME type."
            )
        })?;
    binding
        .mime_types_by_extension
        .get(&extension)
        .cloned()
        .ok_or_else(|| {
            format!("Operation upload extension `{extension}` is not mapped by the binding.")
        })
}

/// Validates authoritative `mime_type` against the exact fixed MIME or declared multi-format mapping in `binding`.
/// 对照`binding`中的精确固定MIME或声明多格式映射验证权威`mime_type`。
/// Returns success without inferring from a digest path; known MIME cannot widen the binding's supported formats.
/// 不从摘要路径推断即返回成功；已知MIME不能扩大绑定支持的格式。
fn validate_operation_source_mime(
    binding: &crate::OperationUploadBinding,
    mime_type: &str,
) -> Result<(), String> {
    // Fixed MIME takes precedence, matching the existing local-path format resolver.
    // 固定MIME具有优先权，与既有本地路径格式解析器一致。
    let accepted = if let Some(declared) = binding.mime_type.as_deref() {
        declared.eq_ignore_ascii_case(mime_type)
    } else {
        binding.mime_types_by_extension.is_empty()
            || binding
                .mime_types_by_extension
                .values()
                .any(|declared| declared.eq_ignore_ascii_case(mime_type))
    };
    if !accepted {
        return Err(String::from(
            "Media source MIME does not match the declared operation binding.",
        ));
    }
    Ok(())
}

/// Writes one absolute JSON Pointer while rejecting scalar ancestors and array expansion.
/// 写入一个绝对 JSON Pointer，同时拒绝标量祖先与数组扩展。
fn write_operation_argument_pointer(
    root: &mut Value,
    pointer: &str,
    value: Value,
) -> Result<(), String> {
    let tokens = operation_pointer_tokens(pointer)?;
    let mut current = root;
    for token in &tokens[..tokens.len() - 1] {
        match current {
            Value::Object(object) => {
                current = object
                    .entry(token.clone())
                    .or_insert_with(|| Value::Object(Default::default()));
            }
            Value::Array(array) => {
                let index = token.parse::<usize>().map_err(|_| {
                    format!("Operation upload pointer `{pointer}` has a non-numeric array index.")
                })?;
                current = array.get_mut(index).ok_or_else(|| {
                    format!("Operation upload pointer `{pointer}` expands an array.")
                })?;
            }
            _ => {
                return Err(format!(
                    "Operation upload pointer `{pointer}` crosses a scalar ancestor."
                ));
            }
        }
    }
    let leaf = tokens.last().expect("validated pointer has one segment");
    match current {
        Value::Object(object) => {
            object.insert(leaf.clone(), value);
            Ok(())
        }
        Value::Array(array) => {
            let index = leaf.parse::<usize>().map_err(|_| {
                format!("Operation upload pointer `{pointer}` has a non-numeric array index.")
            })?;
            let slot = array
                .get_mut(index)
                .ok_or_else(|| format!("Operation upload pointer `{pointer}` expands an array."))?;
            *slot = value;
            Ok(())
        }
        _ => Err(format!(
            "Operation upload pointer `{pointer}` writes through a scalar parent."
        )),
    }
}

/// Removes one existing absolute JSON Pointer after a distinct target has been written.
/// 在写入不同目标后移除一个现有绝对 JSON Pointer。
fn remove_operation_argument_pointer(root: &mut Value, pointer: &str) -> Result<(), String> {
    let tokens = operation_pointer_tokens(pointer)?;
    let mut current = root;
    for token in &tokens[..tokens.len() - 1] {
        current = match current {
            Value::Object(object) => object.get_mut(token).ok_or_else(|| {
                format!("Operation upload removal pointer `{pointer}` disappeared.")
            })?,
            Value::Array(array) => {
                let index = token.parse::<usize>().map_err(|_| {
                    format!(
                        "Operation upload removal pointer `{pointer}` has a non-numeric array index."
                    )
                })?;
                array.get_mut(index).ok_or_else(|| {
                    format!("Operation upload removal pointer `{pointer}` expands an array.")
                })?
            }
            _ => {
                return Err(format!(
                    "Operation upload removal pointer `{pointer}` crosses a scalar ancestor."
                ));
            }
        };
    }
    let leaf = tokens.last().expect("validated pointer has one segment");
    match current {
        Value::Object(object) => {
            object.remove(leaf);
            Ok(())
        }
        _ => Err(format!(
            "Operation upload removal pointer `{pointer}` requires an object parent."
        )),
    }
}

/// Decodes one absolute JSON Pointer into exact unescaped path tokens.
/// 将一个绝对 JSON Pointer 解码为精确反转义路径 Token。
fn operation_pointer_tokens(pointer: &str) -> Result<Vec<String>, String> {
    if pointer.is_empty() || !pointer.starts_with('/') || pointer.contains('\0') {
        return Err(format!(
            "Operation upload path `{pointer}` is not an absolute JSON pointer."
        ));
    }
    pointer[1..]
        .split('/')
        .map(|segment| {
            let bytes = segment.as_bytes();
            let mut decoded = String::new();
            let mut index = 0;
            while index < bytes.len() {
                if bytes[index] == b'~' {
                    let escaped = bytes.get(index + 1).copied().ok_or_else(|| {
                        format!("Operation upload pointer `{pointer}` ends with `~`.")
                    })?;
                    match escaped {
                        b'0' => decoded.push('~'),
                        b'1' => decoded.push('/'),
                        _ => {
                            return Err(format!(
                                "Operation upload pointer `{pointer}` contains an invalid escape."
                            ));
                        }
                    }
                    index += 2;
                } else {
                    let remaining = &segment[index..];
                    let character = remaining.chars().next().ok_or_else(|| {
                        format!("Operation upload pointer `{pointer}` contains invalid UTF-8.")
                    })?;
                    decoded.push(character);
                    index += character.len_utf8();
                }
            }
            Ok(decoded)
        })
        .collect()
}

/// Enforces every bounded upload limit before any network call (plan §R9.5).
/// 在任何网络调用前强制全部有界上传限制（方案 §R9.5）。
///
/// # Parameters
/// 参数
/// `channel` owns the declared limits; `resource` is the exact input candidate.
/// `channel` 持有声明限制，`resource` 是精确输入候选。
///
/// # Returns
/// 返回值
/// `Ok` when every limit is satisfied, otherwise the first failing limit diagnostic.
/// 全部限制满足时返回 `Ok`，否则返回首个失败限制诊断。
pub(crate) fn validate_upload_resource(
    channel: &crate::UploadChannelDeclaration,
    resource: &UploadResource,
) -> Result<(), MediaUploadFailure> {
    if !channel.accepted_mime_types.is_empty()
        && !channel
            .accepted_mime_types
            .iter()
            .any(|accepted| accepted == &resource.mime_type)
    {
        return Err(MediaUploadFailure::UnsupportedMime);
    }
    if let Some(max_bytes) = channel.max_bytes
        && resource.byte_len > max_bytes
    {
        return Err(MediaUploadFailure::SourceTooLarge {
            actual_bytes: resource.byte_len,
            max_bytes,
        });
    }
    if let Some(max_count) = channel.max_count
        && resource.item_count > max_count
    {
        return Err(MediaUploadFailure::TooManyResources {
            actual_count: resource.item_count,
            max_count,
        });
    }
    // Network-bound primitives always carry an explicit HTTPS host and absolute path; the
    // validator guarantees this at compile time, but the engine re-checks so a malformed
    // declaration can never construct a free-form URL (SSRF boundary).
    // 网络绑定原语始终携带显式 HTTPS 宿主与绝对路径；验证器在编译期保证这一点，但引擎
    // 再次检查，使畸形声明绝不可能构造自由格式 URL（SSRF 边界）。
    if matches!(
        channel.primitive,
        Some(
            UploadPrimitiveKind::MultipartFile
                | UploadPrimitiveKind::ProviderFileCreate
                | UploadPrimitiveKind::PresignedFormUpload
                | UploadPrimitiveKind::SignedObjectUpload
        )
    ) {
        let host = channel
            .endpoint_host
            .as_deref()
            .ok_or(MediaUploadFailure::InvalidContract)?;
        if host.is_empty()
            || host.contains(['/', '\\', '@', ':', ' '])
            || host.starts_with('.')
            || !host
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '.')
        {
            return Err(MediaUploadFailure::InvalidContract);
        }
        let path = channel
            .endpoint_path
            .as_deref()
            .ok_or(MediaUploadFailure::InvalidContract)?;
        if !path.starts_with('/') || path.contains(['?', '#']) {
            return Err(MediaUploadFailure::InvalidContract);
        }
    }
    Ok(())
}

/// Materializes one input resource through the declared upload channel.
/// 通过声明的上传通道物化一个输入资源。
///
/// # Parameters
/// 参数
/// `http`, `entry`, `credential_profile`, and `secret` freeze the exact account projection for
/// network primitives; `progress` and `cancellation` drive the cooperative transfer lifecycle.
/// `http`、`entry`、`credential_profile` 与 `secret` 为网络原语冻结精确账号投影；
/// `progress` 与 `cancellation` 驱动协作式传输生命周期。
///
/// # Returns
/// 返回值
/// The materialized provider reference, or a controlled limit/transport failure.
/// 返回已物化的供应商引用，或受控的限制/传输失败。
pub(crate) async fn materialize_upload(
    context: UploadExecutionContext<'_>,
    channel: &crate::UploadChannelDeclaration,
    resource: &UploadResource,
) -> Result<UploadedResourceReference, MediaUploadFailure> {
    validate_upload_resource(channel, resource)?;
    if context.cancellation.is_cancelled() {
        return Err(MediaUploadFailure::Cancelled);
    }
    verify_host_upload_source(resource, context.cancellation)
        .await
        .map_err(|_| {
            if context.cancellation.is_cancelled() {
                MediaUploadFailure::Cancelled
            } else {
                MediaUploadFailure::SourceUnavailable
            }
        })?;
    let network_bound = matches!(
        channel.primitive,
        Some(
            UploadPrimitiveKind::MultipartFile
                | UploadPrimitiveKind::ProviderFileCreate
                | UploadPrimitiveKind::PresignedFormUpload
                | UploadPrimitiveKind::SignedObjectUpload
        )
    );
    if network_bound {
        let timeout_ms = channel
            .timeout_ms
            .ok_or(MediaUploadFailure::InvalidContract)?;
        return await_network_upload_with_timeout(
            channel.upload_channel_id.as_str(),
            timeout_ms,
            materialize_upload_inner(context, channel, resource),
        )
        .await;
    }
    materialize_upload_inner(context, channel, resource).await
}

/// Awaits one complete network upload inside its validated manifest-owned total deadline.
/// 在经校验且由 Manifest 持有的总期限内等待一次完整网络上传。
///
/// # Parameters
/// 参数
/// `channel_id` identifies the exact diagnostic owner, `timeout_ms` is the declared bounded
/// lifecycle, and `upload` contains every signing and transfer side effect.
/// `channel_id` 标识精确诊断所有者，`timeout_ms` 是声明的有界生命周期，`upload` 包含全部
/// 签名与传输副作用。
///
/// # Returns
/// 返回值
/// The upload result, or a controlled range/deadline failure that drops and cancels the future.
/// 返回上传结果，或会丢弃并取消 Future 的受控范围/期限失败。
async fn await_network_upload_with_timeout<F>(
    channel_id: &str,
    timeout_ms: u64,
    upload: F,
) -> Result<UploadedResourceReference, MediaUploadFailure>
where
    F: Future<Output = Result<UploadedResourceReference, MediaUploadFailure>>,
{
    if !(1_000..=crate::provider_manifest::MAX_ASYNC_TIMEOUT_MS).contains(&timeout_ms) {
        return Err(MediaUploadFailure::InvalidContract);
    }
    tokio::time::timeout(Duration::from_millis(timeout_ms), upload)
        .await
        .map_err(|_| MediaUploadFailure::Timeout {
            channel_id: channel_id.into(),
            timeout_ms,
        })?
}

/// Executes one already validated upload primitive inside its owning lifecycle boundary.
/// 在所属生命周期边界内执行一个已经校验的上传原语。
///
/// # Parameters
/// 参数
/// `context`, `channel`, and `resource` are the exact immutable facts validated by the caller.
/// `context`、`channel` 与 `resource` 是调用方已校验的精确不可变事实。
///
/// # Returns
/// 返回值
/// The materialized provider reference or the primitive's controlled execution failure.
/// 返回已物化供应商引用，或原语的受控执行失败。
async fn materialize_upload_inner(
    context: UploadExecutionContext<'_>,
    channel: &crate::UploadChannelDeclaration,
    resource: &UploadResource,
) -> Result<UploadedResourceReference, MediaUploadFailure> {
    match channel.primitive {
        Some(UploadPrimitiveKind::InlineDataUrl) => {
            let bytes = read_inline_bytes(resource)
                .await
                .map_err(|_| MediaUploadFailure::SourceUnavailable)?;
            let encoded = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                bytes.as_slice(),
            );
            Ok(UploadedResourceReference {
                reference: format!("data:{};base64,{}", resource.mime_type, encoded),
                encoding: channel
                    .reference_encoding
                    .unwrap_or(UploadReferenceEncoding::ResponseUrl),
                response_id: None,
                response_url: None,
            })
        }
        Some(UploadPrimitiveKind::InlineBase64) => {
            let bytes = read_inline_bytes(resource)
                .await
                .map_err(|_| MediaUploadFailure::SourceUnavailable)?;
            let encoded = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                bytes.as_slice(),
            );
            Ok(UploadedResourceReference {
                reference: encoded,
                encoding: channel
                    .reference_encoding
                    .unwrap_or(UploadReferenceEncoding::ResponseUrl),
                response_id: None,
                response_url: None,
            })
        }
        Some(UploadPrimitiveKind::RemoteUrlPassthrough) => {
            let url = resource
                .remote_url
                .clone()
                .ok_or(MediaUploadFailure::SourceUnavailable)?;
            validate_remote_url_passthrough(channel, url.as_str())
                .map_err(|_| MediaUploadFailure::InvalidReference)?;
            Ok(UploadedResourceReference {
                reference: url.clone(),
                encoding: channel
                    .reference_encoding
                    .unwrap_or(UploadReferenceEncoding::ResponseUrl),
                response_id: None,
                response_url: Some(url),
            })
        }
        Some(UploadPrimitiveKind::MultipartFile) => {
            upload_multipart_file(&context, channel, resource).await
        }
        Some(UploadPrimitiveKind::ProviderFileCreate) => {
            upload_provider_file_create(&context, channel, resource).await
        }
        Some(UploadPrimitiveKind::SignedObjectUpload)
            if channel.signing_primitive
                == Some(crate::UploadSigningPrimitiveKind::TemporaryOssPolicy) =>
        {
            let model_id = context
                .model_id
                .ok_or(MediaUploadFailure::InvalidContract)?;
            let reference =
                upload_temporary_oss_object(&context, model_id, channel, resource).await?;
            Ok(UploadedResourceReference {
                reference,
                encoding: channel
                    .reference_encoding
                    .unwrap_or(UploadReferenceEncoding::ResponseUrl),
                response_id: None,
                response_url: None,
            })
        }
        Some(UploadPrimitiveKind::SignedObjectUpload)
            if channel.signing_primitive
                == Some(crate::UploadSigningPrimitiveKind::OpenAiCodexFiles) =>
        {
            openai_codex::upload_openai_codex_file(&context, channel, resource).await
        }
        Some(UploadPrimitiveKind::PresignedFormUpload)
        | Some(UploadPrimitiveKind::SignedObjectUpload) => {
            Err(MediaUploadFailure::MissingSigningPrimitive)
        }
        Some(UploadPrimitiveKind::ManagedAssetReference) | None => {
            Err(MediaUploadFailure::InvalidContract)
        }
    }
}

/// Validates one provider-visible remote source URL against the channel's closed HTTPS policy.
/// 依据通道的封闭 HTTPS 策略校验一个供应商可见的远端来源 URL。
///
/// # Parameters
/// 参数
/// `channel` names the upload channel and `candidate` is the unmodified source URL.
/// `channel` 指明上传通道，`candidate` 是未经修改的来源 URL。
///
/// # Returns
/// 返回值
/// `Ok` for any credential-free HTTPS URL with a host and no fragment; otherwise a controlled
/// pre-dispatch error.
/// 对任何带宿主、不含凭据与片段的 HTTPS URL 返回 `Ok`；否则返回受控的分派前错误。
fn validate_remote_url_passthrough(
    channel: &crate::UploadChannelDeclaration,
    candidate: &str,
) -> Result<(), String> {
    let parsed = reqwest::Url::parse(candidate).map_err(|_| {
        format!(
            "Upload channel `{}` received an invalid remote URL.",
            channel.upload_channel_id
        )
    })?;
    if parsed.host_str().is_none() {
        return Err(format!(
            "Upload channel `{}` remote URL has no host.",
            channel.upload_channel_id
        ));
    }
    if parsed.scheme() != "https"
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
    {
        return Err(format!(
            "Upload channel `{}` remote URL must be a credential-free HTTPS URL.",
            channel.upload_channel_id
        ));
    }
    Ok(())
}

/// Bounded temporary object policy response envelope for the registered OSS signing primitive.
/// 已注册 OSS 签名原语使用的有界临时对象策略响应信封。
#[derive(Debug, Deserialize)]
struct TemporaryOssPolicyEnvelope {
    /// Exact signed multipart policy fields.
    /// 精确签名 Multipart 策略字段。
    data: TemporaryOssPolicy,
}

/// Signed multipart fields returned by a temporary OSS policy endpoint.
/// 临时 OSS 策略端点返回的签名 Multipart 字段。
#[derive(Debug, Deserialize)]
struct TemporaryOssPolicy {
    /// Provider-returned upload destination constrained by manifest suffixes.
    /// 受 Manifest 后缀约束的供应商返回上传目标。
    upload_host: String,
    /// Provider-owned object prefix.
    /// 供应商持有的对象前缀。
    upload_dir: String,
    /// Temporary OSS access-key identifier.
    /// 临时 OSS Access Key 标识。
    oss_access_key_id: String,
    /// Signed multipart signature.
    /// 签名 Multipart 签名。
    signature: String,
    /// Signed multipart policy.
    /// 签名 Multipart 策略。
    policy: String,
    /// Required object ACL.
    /// 必需对象 ACL。
    x_oss_object_acl: String,
    /// Required overwrite protection.
    /// 必需覆盖保护。
    x_oss_forbid_overwrite: String,
}

/// Uploads one verified local resource through the closed temporary OSS policy handshake.
/// 通过封闭临时 OSS 策略握手上传一个已验证本地资源。
///
/// # Parameters
/// 参数
/// `context` freezes host, credential, progress, and cancellation dependencies; `model_id`
/// selects the provider policy, while `channel` and `resource` own the exact transfer contract.
/// `context` 冻结宿主、凭据、进度与取消依赖；`model_id` 选择供应商策略，`channel` 与
/// `resource` 持有精确传输契约。
///
/// # Returns
/// 返回值
/// A temporary `oss://` object reference after both policy and multipart transfers succeed.
/// 在策略请求与 Multipart 传输均成功后返回临时 `oss://` 对象引用。
async fn upload_temporary_oss_object(
    context: &UploadExecutionContext<'_>,
    model_id: &str,
    channel: &crate::UploadChannelDeclaration,
    resource: &UploadResource,
) -> Result<String, MediaUploadFailure> {
    if !channel
        .method
        .as_deref()
        .is_some_and(|method| method.eq_ignore_ascii_case("GET"))
    {
        return Err(MediaUploadFailure::InvalidContract);
    }
    let canonical = validate_signed_upload_source(channel, resource)
        .await
        .map_err(|_| MediaUploadFailure::SourceUnavailable)?;
    let mut policy_url = channel_origin_url(channel, context.fixture_loopback_origin)
        .map_err(|_| MediaUploadFailure::InvalidContract)?;
    policy_url
        .query_pairs_mut()
        .append_pair("action", "getPolicy")
        .append_pair("model", model_id);
    let policy_headers = upload_headers(context.entry, context.credential_profile, context.secret)
        .map_err(|_| MediaUploadFailure::InvalidContract)?;
    let policy_response = tokio::select! {
        response = context.http
            .get(policy_url)
            .headers(policy_headers)
            .send() => response
                .map_err(|error| MediaUploadFailure::transport(MediaUploadStage::Signing, &error))?,
        _ = context.cancellation.wait() => return Err(MediaUploadFailure::Cancelled),
    };
    let policy_text =
        read_upload_response(context, policy_response, MediaUploadStage::Signing).await?;
    let policy = serde_json::from_value::<TemporaryOssPolicyEnvelope>(
        crate::provider_tool_execution::parse_bounded_provider_json(
            policy_text.as_str(),
            "Temporary OSS policy",
        )
        .map_err(|_| MediaUploadFailure::InvalidResponse {
            stage: MediaUploadStage::Signing,
        })?,
    )
    .map_err(|_| MediaUploadFailure::InvalidResponse {
        stage: MediaUploadStage::Signing,
    })?;
    let upload_url = validate_temporary_oss_policy(&policy.data).map_err(|_| {
        MediaUploadFailure::InvalidResponse {
            stage: MediaUploadStage::Signing,
        }
    })?;
    let digest = hash_signed_upload_source(canonical.as_path(), context.cancellation)
        .await
        .map_err(|_| {
            if context.cancellation.is_cancelled() {
                MediaUploadFailure::Cancelled
            } else {
                MediaUploadFailure::SourceUnavailable
            }
        })?;
    let file_name = format!(
        "resource-{}{}",
        &digest[..16],
        temporary_upload_extension(resource.mime_type.as_str())
    );
    let object_key = format!(
        "{}/{}",
        policy.data.upload_dir.trim_end_matches('/'),
        file_name
    );
    if object_key.starts_with('/') || object_key.contains("../") || object_key.contains("\\..") {
        return Err(MediaUploadFailure::InvalidResponse {
            stage: MediaUploadStage::Signing,
        });
    }
    // file freezes the exact source object that will be streamed after the policy handshake.
    // file 在策略握手后冻结将被流式传输的精确源对象。
    let file = tokio::fs::File::open(canonical)
        .await
        .map_err(|_| MediaUploadFailure::SourceUnavailable)?;
    // opened_metadata prevents a path replacement from changing the validated transfer length.
    // opened_metadata 防止路径替换改变已经校验的传输长度。
    let opened_metadata = file
        .metadata()
        .await
        .map_err(|_| MediaUploadFailure::SourceUnavailable)?;
    if !opened_metadata.is_file() || opened_metadata.len() != resource.byte_len {
        return Err(MediaUploadFailure::SourceChanged);
    }
    // expected_size is the only byte budget exposed to the network stream.
    // expected_size 是向网络流暴露的唯一字节预算。
    let expected_size = resource.byte_len;
    let upload_cancellation = context.cancellation.clone();
    let upload_progress = context.progress.clone();
    let stream = stream! {
        // source is hard-capped even if another writer grows the opened file during transfer.
        // source 被硬性限制，即使其他写入者在传输期间扩大已打开文件也不会越界。
        let mut source = tokio_util::io::ReaderStream::new(file.take(expected_size));
        let mut sent = 0_u64;
        while let Some(chunk) = source.next().await {
            if upload_cancellation.is_cancelled() {
                yield Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "media upload cancelled",
                ));
                return;
            }
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(error) => {
                    yield Err(error);
                    return;
                }
            };
            sent = sent.saturating_add(chunk.len() as u64);
            if let Some(callback) = upload_progress.as_ref() {
                callback(sent, Some(expected_size));
            }
            yield Ok(chunk);
        }
        if sent != expected_size {
            yield Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "signed upload source length changed during transfer",
            ));
        }
    };
    let part = reqwest::multipart::Part::stream_with_length(
        reqwest::Body::wrap_stream(stream),
        expected_size,
    )
    .file_name(file_name)
    .mime_str(resource.mime_type.as_str())
    .map_err(|_| MediaUploadFailure::InvalidContract)?;
    let form = reqwest::multipart::Form::new()
        .text("OSSAccessKeyId", policy.data.oss_access_key_id)
        .text("Signature", policy.data.signature)
        .text("policy", policy.data.policy)
        .text("x-oss-object-acl", policy.data.x_oss_object_acl)
        .text("x-oss-forbid-overwrite", policy.data.x_oss_forbid_overwrite)
        .text("key", object_key.clone())
        .text("success_action_status", "200")
        .part("file", part);
    let upload_response = tokio::select! {
        response = context.http.post(upload_url).multipart(form).send() => response.map_err(|error| {
            if context.cancellation.is_cancelled() {
                MediaUploadFailure::Cancelled
            } else {
                MediaUploadFailure::transport(MediaUploadStage::Transfer, &error)
            }
        })?,
        _ = context.cancellation.wait() => return Err(MediaUploadFailure::Cancelled),
    };
    read_upload_response(context, upload_response, MediaUploadStage::Transfer).await?;
    Ok(format!("oss://{object_key}"))
}

/// Revalidates one canonical local resource immediately before a signed upload.
/// 在签名上传前立即重新校验一个规范本地资源。
///
/// # Parameters
/// 参数
/// `channel` owns the frozen byte bound and `resource` owns the previously observed file identity.
/// `channel` 持有冻结字节上限，`resource` 持有此前观测的文件身份。
///
/// # Returns
/// 返回值
/// The unchanged canonical path when type and byte length still match.
/// 当类型与字节长度仍匹配时返回未变化的规范路径。
async fn validate_signed_upload_source(
    channel: &crate::UploadChannelDeclaration,
    resource: &UploadResource,
) -> Result<PathBuf, String> {
    let path = resource.local_path.as_ref().ok_or_else(|| {
        format!(
            "Upload channel `{}` signed-object upload requires a local file source.",
            channel.upload_channel_id
        )
    })?;
    if resource.byte_len == 0
        || channel
            .max_bytes
            .is_some_and(|maximum| resource.byte_len > maximum)
    {
        return Err(format!(
            "Upload channel `{}` signed-object source exceeds its byte bound.",
            channel.upload_channel_id
        ));
    }
    let canonical = tokio::fs::canonicalize(path)
        .await
        .map_err(|error| format!("Signed upload source cannot be resolved: {error}"))?;
    if &canonical != path {
        return Err(String::from(
            "Signed upload source path changed after materialization.",
        ));
    }
    let metadata = tokio::fs::metadata(&canonical)
        .await
        .map_err(|error| format!("Signed upload source metadata is unavailable: {error}"))?;
    if !metadata.is_file() || metadata.len() != resource.byte_len {
        return Err(String::from(
            "Signed upload source changed after materialization.",
        ));
    }
    Ok(canonical)
}

/// Validates signed policy fields and constrains the returned upload origin to manifest suffixes.
/// 校验签名策略字段，并把返回的上传源限制在 Manifest 后缀内。
///
/// # Parameters
/// 参数
/// `policy` is the decoded provider response.
/// `policy` 是解码后的供应商响应。
///
/// # Returns
/// 返回值
/// The parsed credential-free HTTPS upload URL when every field and authority check succeeds.
/// 当全部字段与权威检查成功时返回解析后的无凭据 HTTPS 上传 URL。
fn validate_temporary_oss_policy(policy: &TemporaryOssPolicy) -> Result<reqwest::Url, String> {
    if [
        policy.upload_dir.as_str(),
        policy.oss_access_key_id.as_str(),
        policy.signature.as_str(),
        policy.policy.as_str(),
        policy.x_oss_object_acl.as_str(),
        policy.x_oss_forbid_overwrite.as_str(),
    ]
    .iter()
    .any(|value| value.trim().is_empty())
    {
        return Err(String::from("Temporary OSS policy fields are incomplete."));
    }
    let upload_url = reqwest::Url::parse(policy.upload_host.as_str())
        .map_err(|error| format!("Temporary OSS upload URL is invalid: {error}"))?;
    if upload_url.host_str().is_none() {
        return Err(String::from(
            "Temporary OSS upload URL must declare a host.",
        ));
    }
    if upload_url.scheme() != "https"
        || !upload_url.username().is_empty()
        || upload_url.password().is_some()
        || upload_url.query().is_some()
        || upload_url.fragment().is_some()
    {
        return Err(String::from(
            "Temporary OSS upload URL must be a credential-free HTTPS URL without query or fragment.",
        ));
    }
    Ok(upload_url)
}

/// Hashes one revalidated local upload source without buffering it in memory.
/// 在不把文件缓冲进内存的情况下哈希一个已重新校验的本地上传来源。
///
/// # Parameters
/// 参数
/// `path` is the canonical source and `cancellation` is the host-owned stop signal.
/// `path` 是规范来源，`cancellation` 是宿主持有的停止信号。
///
/// # Returns
/// 返回值
/// Lowercase SHA-256 hexadecimal text, or a controlled I/O/cancellation error.
/// 返回小写 SHA-256 十六进制文本，或受控 I/O 与取消错误。
async fn hash_signed_upload_source(
    path: &std::path::Path,
    cancellation: &MediaUploadCancellation,
) -> Result<String, String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|error| format!("Signed upload source cannot be opened: {error}"))?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        if cancellation.is_cancelled() {
            return Err(String::from("media upload cancelled"));
        }
        let read = file
            .read(buffer.as_mut_slice())
            .await
            .map_err(|error| format!("Signed upload source cannot be hashed: {error}"))?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(hex::encode(hash.finalize()))
}

/// Maps one trusted MIME type to a safe temporary object extension.
/// 将一个受信任 MIME 类型映射为安全临时对象扩展名。
///
/// # Parameters
/// 参数
/// `mime_type` is resolved from a manifest-owned allowlist.
/// `mime_type` 由 Manifest 持有的允许列表解析。
///
/// # Returns
/// 返回值
/// A fixed non-user-controlled extension.
/// 返回固定且不受用户控制的扩展名。
fn temporary_upload_extension(mime_type: &str) -> &'static str {
    match mime_type {
        "image/jpeg" => ".jpg",
        "image/png" => ".png",
        "image/webp" => ".webp",
        "audio/mpeg" => ".mp3",
        "audio/wav" | "audio/x-wav" => ".wav",
        "audio/aac" => ".aac",
        "audio/flac" => ".flac",
        "video/mp4" => ".mp4",
        "video/quicktime" => ".mov",
        _ => ".bin",
    }
}

/// Checks whether any host source is absent from `consumed` exact binding references.
/// 检查是否存在未被`consumed`精确绑定引用消费的宿主来源。
fn media_sources_are_unconsumed(
    sources: &[crate::ProviderToolMediaSource],
    consumed: &std::collections::BTreeSet<&str>,
) -> bool {
    sources
        .iter()
        .any(|source| !consumed.contains(source.reference.as_str()))
}

/// Verifies `resource` against its optional host identity with bounded reads and `cancellation`, returning no media allocation.
/// 使用有界读取及`cancellation`按可选宿主身份验证`resource`，不返回媒体内存分配。
pub(crate) async fn verify_host_upload_source(
    resource: &UploadResource,
    cancellation: &MediaUploadCancellation,
) -> Result<(), String> {
    // Ordinary workspace sources have no earlier catalog proof; explicitly resolved sources always retain one.
    // 普通工作区来源没有更早的目录证据；显式解析来源始终保留一份证据。
    let Some(expected) = resource.source_identity.as_ref() else {
        return Ok(());
    };
    if expected.byte_len != resource.byte_len {
        return Err(String::from(
            "Host media source size differs from its registered identity.",
        ));
    }
    // Open only the exact path supplied by the resource owner, never any AI-visible locator.
    // 只打开资源所有者提供的精确路径，绝不打开AI可见定位符。
    let path = resource
        .local_path
        .as_ref()
        .ok_or_else(|| String::from("Host media source lost its local entity."))?;
    // All filesystem failures use a safe diagnostic without private entity paths.
    // 全部文件系统失败使用不含私有实体路径的安全诊断。
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|_| String::from("Host media source is unavailable."))?;
    // Metadata from the opened handle prevents size changes before any source read.
    // 已打开句柄的元数据阻止读取来源前的大小变化。
    let metadata = file
        .metadata()
        .await
        .map_err(|_| String::from("Host media source metadata is unavailable."))?;
    if !metadata.is_file() || metadata.len() != expected.byte_len {
        return Err(String::from(
            "Host media source changed after registration.",
        ));
    }
    // One extra byte detects growth while bounding all IO by the admitted size.
    // 一个额外字节检测增长，同时按准入大小约束全部IO。
    let mut reader = file.take(expected.byte_len.saturating_add(1));
    // A fixed buffer keeps hashing memory independent of upload size.
    // 固定缓冲区使哈希内存独立于上传大小。
    let mut buffer = [0_u8; 64 * 1024];
    // Both byte count and digest come from the same opened source.
    // 字节数量及摘要均来自同一已打开来源。
    let mut digest = Sha256::new();
    // The actual byte count is checked separately from the declared metadata.
    // 实际字节数量独立于声明元数据检查。
    let mut count = 0_u64;
    loop {
        // Cancellation interrupts the bounded read and never emits an upload progress event.
        // 取消中断有界读取，且绝不产生上传进度事件。
        let read = tokio::select! { result = reader.read(&mut buffer) => result.map_err(|_| String::from("Host media source could not be verified."))?, _ = cancellation.wait() => return Err(String::from("media upload cancelled")) };
        if read == 0 {
            break;
        }
        count += read as u64;
        digest.update(&buffer[..read]);
    }
    if count != expected.byte_len || hex::encode(digest.finalize()) != expected.content_sha256 {
        return Err(String::from(
            "Host media source changed after registration.",
        ));
    }
    Ok(())
}

/// Reads the exact inline resource bytes without touching the network.
/// 在不触网的情况下读取精确内联资源字节。
async fn read_inline_bytes(resource: &UploadResource) -> Result<Vec<u8>, String> {
    if let Some(bytes) = resource.inline_bytes.as_ref() {
        if bytes.len() as u64 != resource.byte_len {
            return Err(format!(
                "Inline upload source length changed during preparation ({} != {}).",
                bytes.len(),
                resource.byte_len
            ));
        }
        return Ok(bytes.clone());
    }
    let path = resource.local_path.as_ref().ok_or_else(|| {
        format!(
            "Upload resource has neither inline bytes nor a local path for MIME `{}`.",
            resource.mime_type
        )
    })?;
    // file freezes the opened source object before size validation and bounded reading.
    // file 在大小校验与有界读取前冻结已打开的源对象。
    let file = tokio::fs::File::open(path).await.map_err(|error| {
        format!(
            "Upload source `{}` could not be opened: {error}",
            path.display()
        )
    })?;
    // metadata comes from the opened handle so a post-open path replacement cannot change it.
    // metadata 来自已打开句柄，因此打开后的路径替换无法改变它。
    let metadata = file.metadata().await.map_err(|error| {
        format!(
            "Upload source `{}` metadata could not be read after opening: {error}",
            path.display()
        )
    })?;
    if !metadata.is_file() || metadata.len() != resource.byte_len {
        return Err(format!(
            "Upload source `{}` changed during preparation ({} != {}).",
            path.display(),
            metadata.len(),
            resource.byte_len
        ));
    }
    // capacity is derived only from the frozen, channel-validated byte length.
    // capacity 仅由冻结且已通过通道校验的字节长度派生。
    let capacity = usize::try_from(resource.byte_len).map_err(|_| {
        format!(
            "Upload source `{}` length exceeds the platform allocation limit.",
            path.display()
        )
    })?;
    // bytes cannot grow beyond the frozen length plus one sentinel byte during the read.
    // bytes 在读取期间无法增长到冻结长度加一个哨兵字节之外。
    let mut bytes = Vec::with_capacity(capacity);
    file.take(resource.byte_len.saturating_add(1))
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| {
            format!(
                "Upload source `{}` could not be read: {error}",
                path.display()
            )
        })?;
    if bytes.len() as u64 != resource.byte_len {
        return Err(format!(
            "Upload source `{}` length changed while reading ({} != {}).",
            path.display(),
            bytes.len(),
            resource.byte_len
        ));
    }
    if resource
        .source_identity
        .as_ref()
        .is_some_and(|expected| hex::encode(Sha256::digest(&bytes)) != expected.content_sha256)
    {
        return Err(String::from(
            "Host media source changed while reading inline bytes.",
        ));
    }
    Ok(bytes)
}

/// Builds the strict production HTTPS URL or a validated fixture-only loopback URL.
/// 构建严格的生产 HTTPS 地址或经校验且仅供夹具使用的回环地址。
///
/// # Parameters
/// 参数
/// `channel` owns the exact endpoint path and production host; `fixture_loopback_origin` is
/// accepted only when it is a credential-free HTTP loopback origin with an explicit port.
/// `channel` 持有精确端点路径与生产宿主；`fixture_loopback_origin` 仅接受带显式端口且
/// 不含凭据的 HTTP 回环源地址。
///
/// # Returns
/// 返回值
/// The exact request URL, or a controlled policy error before any network side effect.
/// 返回精确请求地址，或在任何网络副作用前返回受控策略错误。
fn channel_origin_url(
    channel: &crate::UploadChannelDeclaration,
    fixture_loopback_origin: Option<&str>,
) -> Result<reqwest::Url, String> {
    let host = channel.endpoint_host.as_deref().ok_or_else(|| {
        format!(
            "Upload channel `{}` has no endpoint host.",
            channel.upload_channel_id
        )
    })?;
    let path = channel.endpoint_path.as_deref().ok_or_else(|| {
        format!(
            "Upload channel `{}` has no endpoint path.",
            channel.upload_channel_id
        )
    })?;
    if let Some(origin) = fixture_loopback_origin {
        let mut url = reqwest::Url::parse(origin).map_err(|_| {
            format!(
                "Upload channel `{}` fixture origin is invalid.",
                channel.upload_channel_id
            )
        })?;
        // loopbackHost is parsed as an IP so DNS names cannot redirect fixture credentials.
        // loopbackHost 按 IP 解析，因此 DNS 名称无法重定向夹具凭据。
        let loopback_host = url.host_str().and_then(|value| {
            value
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .ok()
        });
        if url.scheme() != "http"
            || !loopback_host.is_some_and(|address| address.is_loopback())
            || url.port().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || !matches!(url.path(), "" | "/")
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(format!(
                "Upload channel `{}` fixture origin must be a credential-free HTTP loopback origin with an explicit port.",
                channel.upload_channel_id
            ));
        }
        url.set_path(path);
        return Ok(url);
    }
    reqwest::Url::parse(format!("https://{host}{path}").as_str()).map_err(|error| {
        format!(
            "Upload channel `{}` endpoint URL is invalid: {error}",
            channel.upload_channel_id
        )
    })
}

/// Reads the response identifier and URL through their declared JSON Pointers.
/// 通过声明的 JSON Pointer 读取响应标识与 URL。
fn read_response_reference(
    channel: &crate::UploadChannelDeclaration,
    body: &Value,
) -> Result<(Option<String>, Option<String>), MediaUploadFailure> {
    // responseId follows the exact declared scalar encoding instead of coercing arbitrary JSON.
    // responseId 严格遵循声明的标量编码，不对任意 JSON 执行隐式转换。
    let response_id = match channel.response_id_pointer.as_deref() {
        None => None,
        Some(pointer) => {
            let value = body
                .pointer(pointer)
                .ok_or(MediaUploadFailure::MissingReference)?;
            let text = match channel
                .response_id_encoding
                .unwrap_or(UploadResponseIdEncoding::String)
            {
                UploadResponseIdEncoding::String => value
                    .as_str()
                    .filter(|text| !text.trim().is_empty())
                    .map(str::to_owned)
                    .ok_or(MediaUploadFailure::InvalidReference)?,
                UploadResponseIdEncoding::UnsignedInteger => value
                    .as_u64()
                    .map(|value| value.to_string())
                    .ok_or(MediaUploadFailure::InvalidReference)?,
            };
            Some(text)
        }
    };
    // responseUrl remains a non-empty string because URL validation consumes its exact text.
    // responseUrl 保持为非空字符串，因为 URL 校验需要消费其精确文本。
    let response_url = match channel.response_url_pointer.as_deref() {
        None => None,
        Some(pointer) => Some(
            body.pointer(pointer)
                .ok_or(MediaUploadFailure::MissingReference)?
                .as_str()
                .filter(|text| !text.trim().is_empty())
                .map(str::to_owned)
                .ok_or(MediaUploadFailure::InvalidReference)?,
        ),
    };
    Ok((response_id, response_url))
}

/// Validates a provider reference against the upload channel that produced it.
/// 按生成该引用的上传通道校验一个供应商引用。
///
/// # Parameters
/// 参数
/// `channel` owns the declared reference encoding and host policy; `reference` is the exact
/// value persisted or placed into a provider request.
/// `channel` 持有声明的引用编码与宿主策略；`reference` 是要持久化或放入供应商请求的精确值。
///
/// # Returns
/// 返回值
/// `Ok` only when the value has the declared shape and cannot cross the channel's URI boundary.
/// 仅当该值符合声明形状且不能越过通道 URI 边界时返回 `Ok`。
pub(crate) fn validate_prepared_reference(
    channel: &crate::UploadChannelDeclaration,
    reference: &str,
) -> Result<(), String> {
    if reference.trim().is_empty()
        || reference != reference.trim()
        || reference.chars().any(char::is_control)
    {
        return Err(format!(
            "Upload channel `{}` produced an empty or malformed provider reference.",
            channel.upload_channel_id
        ));
    }
    let encoding = channel
        .reference_encoding
        .unwrap_or(UploadReferenceEncoding::ProviderFileId);
    match encoding {
        UploadReferenceEncoding::ProviderFileId => {
            if reference.contains("://") {
                return Err(format!(
                    "Upload channel `{}` provider file id must not carry a URI scheme.",
                    channel.upload_channel_id
                ));
            }
            Ok(())
        }
        UploadReferenceEncoding::ResponseUrl => validate_provider_response_url(channel, reference),
        UploadReferenceEncoding::Template => {
            let template = channel.reference_template.as_deref().ok_or_else(|| {
                format!(
                    "Upload channel `{}` uses template encoding without a template.",
                    channel.upload_channel_id
                )
            })?;
            let values = template_reference_values(template, reference).ok_or_else(|| {
                format!(
                    "Upload channel `{}` provider reference does not match its declared template.",
                    channel.upload_channel_id
                )
            })?;
            for (placeholder, value) in values {
                if placeholder == "url" {
                    validate_provider_response_url(channel, value.as_str())?;
                }
            }
            Ok(())
        }
    }
}

/// Validates a response URL as a credential-free HTTPS URL; any provider host is accepted.
/// 将响应 URL 校验为不含凭据的 HTTPS URL；接受供应商返回的任何宿主。
fn validate_provider_response_url(
    channel: &crate::UploadChannelDeclaration,
    candidate: &str,
) -> Result<(), String> {
    let parsed = reqwest::Url::parse(candidate).map_err(|_| {
        format!(
            "Upload channel `{}` produced an invalid provider reference URL.",
            channel.upload_channel_id
        )
    })?;
    if parsed.host_str().is_none() {
        return Err(format!(
            "Upload channel `{}` provider reference URL has no host.",
            channel.upload_channel_id
        ));
    }
    if parsed.scheme() != "https"
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
    {
        return Err(format!(
            "Upload channel `{}` provider reference URL must be a credential-free HTTPS URL.",
            channel.upload_channel_id
        ));
    }
    Ok(())
}

/// Represents one bounded literal or placeholder in a provider reference template.
/// 表示供应商引用模板中的一个有界字面量或占位符。
#[derive(Debug, Clone, Copy)]
enum PreparedReferenceTemplatePart<'a> {
    /// Literal bytes that must appear exactly in the rendered reference.
    /// 必须在渲染引用中精确出现的字面量字节。
    Literal(&'a str),
    /// One supported placeholder name, either `id` or `url`.
    /// 一个受支持的占位符名称，可以是 `id` 或 `url`。
    Placeholder(&'a str),
}

/// Parses a validated `{id}`/`{url}` template into ordered literal and placeholder parts.
/// 将已校验的 `{id}`/`{url}` 模板解析为有序字面量与占位符部分。
fn parse_prepared_reference_template(
    template: &str,
) -> Option<Vec<PreparedReferenceTemplatePart<'_>>> {
    let mut parts = Vec::new();
    let mut cursor = 0;
    while cursor < template.len() {
        let next_id = template[cursor..].find("{id}");
        let next_url = template[cursor..].find("{url}");
        let Some((offset, placeholder, placeholder_len)) = (match (next_id, next_url) {
            (None, None) => {
                parts.push(PreparedReferenceTemplatePart::Literal(&template[cursor..]));
                cursor = template.len();
                None
            }
            (Some(id), None) => Some((id, "id", 4)),
            (None, Some(url)) => Some((url, "url", 5)),
            (Some(id), Some(url)) if id <= url => Some((id, "id", 4)),
            (Some(_), Some(url)) => Some((url, "url", 5)),
        }) else {
            break;
        };
        if offset > 0 {
            parts.push(PreparedReferenceTemplatePart::Literal(
                &template[cursor..cursor + offset],
            ));
        }
        parts.push(PreparedReferenceTemplatePart::Placeholder(placeholder));
        cursor += offset + placeholder_len;
    }
    if parts.is_empty()
        || !parts
            .iter()
            .any(|part| matches!(part, PreparedReferenceTemplatePart::Placeholder(_)))
    {
        return None;
    }
    Some(parts)
}

/// Extracts non-empty values from a bounded provider reference template.
/// 从有界供应商引用模板中提取非空占位符值。
fn template_reference_values<'a>(
    template: &'a str,
    reference: &str,
) -> Option<Vec<(&'a str, String)>> {
    let parts = parse_prepared_reference_template(template)?;
    let mut values = Vec::new();
    let mut position = 0_usize;
    for (index, part) in parts.iter().enumerate() {
        match part {
            PreparedReferenceTemplatePart::Literal(literal) => {
                let remaining = reference.get(position..)?;
                if !remaining.starts_with(literal) {
                    return None;
                }
                position += literal.len();
            }
            PreparedReferenceTemplatePart::Placeholder(name) => {
                let next_literal = parts[index + 1..].iter().find_map(|part| match part {
                    PreparedReferenceTemplatePart::Literal(literal) if !literal.is_empty() => {
                        Some(*literal)
                    }
                    _ => None,
                });
                let end = if let Some(next_literal) = next_literal {
                    let remaining = reference.get(position..)?;
                    let offset = remaining.find(next_literal)?;
                    if offset == 0 {
                        return None;
                    }
                    position + offset
                } else {
                    reference.len()
                };
                if end <= position {
                    return None;
                }
                values.push((*name, reference.get(position..end)?.to_string()));
                position = end;
            }
        }
    }
    (position == reference.len()).then_some(values)
}

/// Renders the final provider reference from the declared encoding.
/// 按声明编码渲染最终供应商引用。
fn render_reference(
    channel: &crate::UploadChannelDeclaration,
    response_id: Option<String>,
    response_url: Option<String>,
) -> Result<String, MediaUploadFailure> {
    let encoding = channel
        .reference_encoding
        .unwrap_or(UploadReferenceEncoding::ProviderFileId);
    let reference = match encoding {
        UploadReferenceEncoding::ProviderFileId => {
            response_id.ok_or(MediaUploadFailure::MissingReference)
        }
        UploadReferenceEncoding::ResponseUrl => {
            response_url.ok_or(MediaUploadFailure::MissingReference)
        }
        UploadReferenceEncoding::Template => {
            let template = channel
                .reference_template
                .as_deref()
                .ok_or(MediaUploadFailure::InvalidContract)?;
            let id = response_id.as_deref().unwrap_or_default();
            let url = response_url.as_deref().unwrap_or_default();
            Ok(template.replace("{id}", id).replace("{url}", url))
        }
    }?;
    validate_prepared_reference(channel, reference.as_str())
        .map_err(|_| MediaUploadFailure::InvalidReference)?;
    Ok(reference)
}

/// Performs one bounded multipart file upload through a declared channel.
/// 通过声明的通道执行一次有界 Multipart 文件上传。
/// Reads the successful body of `response` at exact `stage` under the owning `context` cancellation.
/// 在所属`context`取消边界下读取精确`stage`的`response`成功正文。
/// Returns a bounded body, or an HTTP status captured before any untrusted error-body read or decoding.
/// 返回有界正文，或在读取/解码任何不可信错误正文之前捕获的HTTP状态。
async fn read_upload_response(
    context: &UploadExecutionContext<'_>,
    response: reqwest::Response,
    stage: MediaUploadStage,
) -> Result<String, MediaUploadFailure> {
    if context.cancellation.is_cancelled() {
        return Err(MediaUploadFailure::Cancelled);
    }
    // Error status is authoritative even if its response body is malformed, oversized or never finishes.
    // 即使错误响应正文畸形、超大或永不结束，错误状态仍为权威。
    let status = response.status();
    if !status.is_success() {
        return Err(MediaUploadFailure::Http {
            stage,
            status: status.as_u16(),
        });
    }
    tokio::select! {
        biased;
        _ = context.cancellation.wait() => Err(MediaUploadFailure::Cancelled),
        body = crate::provider_tool_execution::read_provider_response_text(response, "Media upload") => {
            body.map_err(|_| MediaUploadFailure::InvalidResponse { stage })
        }
    }
}

/// Streams exact `resource` bytes via the declared multipart `channel` under frozen `context`.
/// 在冻结`context`下通过声明的multipart `channel`流式传输精确`resource`字节。
/// Returns a validated provider reference or typed source, transport, HTTP or response failure.
/// 返回已验证供应商引用或强类型来源、传输、HTTP及响应失败。
async fn upload_multipart_file(
    context: &UploadExecutionContext<'_>,
    channel: &crate::UploadChannelDeclaration,
    resource: &UploadResource,
) -> Result<UploadedResourceReference, MediaUploadFailure> {
    let local_path = resource
        .local_path
        .as_ref()
        .ok_or(MediaUploadFailure::SourceUnavailable)?;
    // file freezes the exact source object before its metadata and byte stream are validated.
    // file 在校验元数据与字节流前冻结精确源对象。
    let file = tokio::fs::File::open(local_path)
        .await
        .map_err(|_| MediaUploadFailure::SourceUnavailable)?;
    // metadata is read from the opened handle so path replacement cannot bypass the length gate.
    // metadata 从已打开句柄读取，因此路径替换无法绕过长度门。
    let metadata = file
        .metadata()
        .await
        .map_err(|_| MediaUploadFailure::SourceUnavailable)?;
    if !metadata.is_file() || metadata.len() != resource.byte_len {
        return Err(MediaUploadFailure::SourceChanged);
    }
    let file_name = resource
        .file_name
        .as_deref()
        .or_else(|| {
            local_path
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
        })
        .unwrap_or("resource.bin")
        .to_string();
    // total_bytes is the only byte budget exposed to the multipart network stream.
    // total_bytes 是向 Multipart 网络流暴露的唯一字节预算。
    let total_bytes = resource.byte_len;
    let upload_cancellation = context.cancellation.clone();
    let upload_progress = context.progress.clone();
    // Hash the actual network body when an earlier host identity exists.
    // 存在较早宿主身份时，对实际网络正文计算哈希。
    let expected_identity = resource.source_identity.clone();
    let byte_stream = stream! {
        // source is hard-capped even if another writer grows the opened file during transfer.
        // source 被硬性限制，即使其他写入者在传输期间扩大已打开文件也不会越界。
        let mut source = tokio_util::io::ReaderStream::new(file.take(total_bytes));
        let mut sent = 0_u64;
        // The transfer digest covers exactly the chunks exposed to the HTTP body.
        // 传输摘要覆盖向HTTP正文暴露的精确数据块。
        let mut digest = Sha256::new();
        while let Some(chunk) = source.next().await {
            if upload_cancellation.is_cancelled() {
                yield Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "media upload cancelled",
                ));
                return;
            }
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(error) => {
                    yield Err(error);
                    return;
                }
            };
            sent = sent.saturating_add(chunk.len() as u64);
            digest.update(&chunk);
            if let Some(callback) = upload_progress.as_ref() {
                callback(sent, Some(total_bytes));
            }
            yield Ok(chunk);
        }
        if sent != total_bytes {
            yield Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "upload source length changed during transfer",
            ));
        }
        if expected_identity.is_some_and(|expected| hex::encode(digest.finalize()) != expected.content_sha256) {
            yield Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "upload source digest changed during transfer"));
        }
    };
    let part = reqwest::multipart::Part::stream_with_length(
        reqwest::Body::wrap_stream(byte_stream),
        total_bytes,
    )
    .file_name(file_name)
    .mime_str(resource.mime_type.as_str())
    .map_err(|_| MediaUploadFailure::InvalidContract)?;
    let field_name = channel.file_field.as_deref().unwrap_or("file").to_string();
    let mut form = reqwest::multipart::Form::new().part(field_name, part);
    for (name, value) in &channel.fixed_fields {
        form = form.text(name.clone(), value.clone());
    }
    let url = channel_origin_url(channel, context.fixture_loopback_origin)
        .map_err(|_| MediaUploadFailure::InvalidContract)?;
    let method = channel.method.as_deref().unwrap_or("POST");
    if method != "POST" {
        return Err(MediaUploadFailure::InvalidContract);
    }
    let mut headers = upload_headers(context.entry, context.credential_profile, context.secret)
        .map_err(|_| MediaUploadFailure::InvalidContract)?;
    headers.remove(reqwest::header::CONTENT_TYPE);
    let response = tokio::select! {
        response = context.http.post(url.as_str()).headers(headers).multipart(form).send() => response
            .map_err(|error| {
                if context.cancellation.is_cancelled() {
                    MediaUploadFailure::Cancelled
                } else {
                    MediaUploadFailure::transport(MediaUploadStage::Transfer, &error)
                }
            })?,
        _ = context.cancellation.wait() => return Err(MediaUploadFailure::Cancelled),
    };
    let body = read_upload_response(context, response, MediaUploadStage::Transfer).await?;
    let parsed = crate::provider_tool_execution::parse_bounded_provider_json(
        body.as_str(),
        "Upload response",
    )
    .map_err(|_| MediaUploadFailure::InvalidResponse {
        stage: MediaUploadStage::Transfer,
    })?;
    let (response_id, response_url) = read_response_reference(channel, &parsed)?;
    let reference = render_reference(channel, response_id.clone(), response_url.clone())?;
    Ok(UploadedResourceReference {
        reference,
        encoding: channel
            .reference_encoding
            .unwrap_or(UploadReferenceEncoding::ProviderFileId),
        response_id,
        response_url,
    })
}

/// Performs one bounded provider file-create JSON request through a declared channel.
/// 通过声明的通道执行一次有界供应商文件创建 JSON 请求。
/// `context` owns HTTP/progress/cancellation, `channel` freezes wire fields and reference rules, and `resource` supplies the original source proof.
/// `context`持有HTTP/进度/取消，`channel`冻结线上字段及引用规则，`resource`提供原来源证明。
/// Returns the validated native reference only after source-body completion, or a typed source/transport/response failure.
/// 仅在来源正文完成后返回已验证原生引用，否则返回强类型来源/传输/响应失败。
async fn upload_provider_file_create(
    context: &UploadExecutionContext<'_>,
    channel: &crate::UploadChannelDeclaration,
    resource: &UploadResource,
) -> Result<UploadedResourceReference, MediaUploadFailure> {
    // Stream original bytes inside one unchanged JSON request instead of allocating the whole file and Base64 value.
    // 在一个不变JSON请求内流式传输原始字节，不再分配整文件及Base64值。
    let file_create_body::PreparedFileCreateBody {
        body,
        content_length,
        mut completion,
    } = file_create_body::prepare_file_create_body(
        resource,
        &channel.fixed_fields,
        context.progress.clone(),
        context.cancellation.clone(),
    )
    .await?;
    let url = channel_origin_url(channel, context.fixture_loopback_origin)
        .map_err(|_| MediaUploadFailure::InvalidContract)?;
    let method = channel.method.as_deref().unwrap_or("POST");
    if method != "POST" {
        return Err(MediaUploadFailure::InvalidContract);
    }
    let headers = upload_headers(context.entry, context.credential_profile, context.secret)
        .map_err(|_| MediaUploadFailure::InvalidContract)?;
    let response = tokio::select! {
        response = context.http.post(url.as_str()).headers(headers).header(reqwest::header::CONTENT_LENGTH, content_length).body(reqwest::Body::wrap_stream(body)).send() => response
            .map_err(|error| {
                if context.cancellation.is_cancelled() {
                    MediaUploadFailure::Cancelled
                } else if let Ok(Err(failure)) = completion.try_recv() {
                    failure
                } else {
                    MediaUploadFailure::transport(MediaUploadStage::Transfer, &error)
                }
            })?,
        _ = context.cancellation.wait() => return Err(MediaUploadFailure::Cancelled),
    };
    let body = read_upload_response(context, response, MediaUploadStage::Transfer).await?;
    // Even a syntactically valid early success cannot publish a reference before streamed source validation completes.
    // 即使提前成功的语法有效，也不能在流式来源校验完成前发布引用。
    tokio::select! {
        biased;
        _ = context.cancellation.wait() => return Err(MediaUploadFailure::Cancelled),
        result = completion => result.map_err(|_| MediaUploadFailure::SourceUnavailable)??,
    }
    let parsed = crate::provider_tool_execution::parse_bounded_provider_json(
        body.as_str(),
        "Upload response",
    )
    .map_err(|_| MediaUploadFailure::InvalidResponse {
        stage: MediaUploadStage::Transfer,
    })?;
    let (response_id, response_url) = read_response_reference(channel, &parsed)?;
    let reference = render_reference(channel, response_id.clone(), response_url.clone())?;
    Ok(UploadedResourceReference {
        reference,
        encoding: channel
            .reference_encoding
            .unwrap_or(UploadReferenceEncoding::ProviderFileId),
        response_id,
        response_url,
    })
}

/// Builds the projection headers for one upload channel without a conversation protocol route.
/// 为无会话协议路由的上传通道构建投影请求头。
fn upload_headers(
    entry: &ServiceEntry,
    credential_profile: &CredentialProfile,
    secret: &CredentialExecutionSecret,
) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    let authentication = credential_profile
        .authentication
        .unwrap_or(entry.authentication);
    match authentication {
        crate::AuthenticationProjection::BearerAuthorization
        | crate::AuthenticationProjection::CodingDeviceBearer => {
            let token = secret.expose_value();
            let value = HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| String::from("Upload bearer token is not a valid header value."))?;
            headers.insert(reqwest::header::AUTHORIZATION, value);
            if authentication == crate::AuthenticationProjection::CodingDeviceBearer {
                crate::execution::insert_coding_device_identity_headers(&mut headers, secret)?;
            }
        }
        crate::AuthenticationProjection::XApiKey => {
            let token = secret.expose_value();
            let value = HeaderValue::from_str(token)
                .map_err(|_| String::from("Upload API key is not a valid header value."))?;
            headers.insert(
                "x-api-key".parse::<HeaderName>().expect("static header"),
                value,
            );
        }
        crate::AuthenticationProjection::OpenAiCodexBearer => {
            // Codex credentials project the account identity exactly as conversation execution does.
            let token = secret.expose_value();
            let value = HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| String::from("Upload bearer token is not a valid header value."))?;
            headers.insert(reqwest::header::AUTHORIZATION, value);
            let account_id = secret.account_id().ok_or_else(|| {
                String::from("Codex credential does not contain an account identity.")
            })?;
            let account_id = HeaderValue::from_str(account_id).map_err(|_| {
                String::from("Codex account identity contains invalid header characters.")
            })?;
            headers.insert(HeaderName::from_static("chatgpt-account-id"), account_id);
        }
        crate::AuthenticationProjection::OpenVulcanAuth
        | crate::AuthenticationProjection::GoogleCodeAssistBearer
        | crate::AuthenticationProjection::GoogleGeminiBearer
        | crate::AuthenticationProjection::GoogleApiKey => {
            // Project-scoped projections are resolved by the conversation route; a generic upload
            // channel must declare an explicit bearer or API-key credential role.
            // 项目作用域投影由会话路由解析；通用上传通道必须声明显式 Bearer 或
            // API Key 凭据角色。
            return Err(String::from(
                "Upload channel requires an explicit bearer or API-key authentication projection.",
            ));
        }
    }
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(headers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UploadChannelDeclaration;
    use std::collections::BTreeMap;

    mod http_failures;
    mod openai_codex;
    mod policy;

    /// Verifies direct local paths remain globally readable before filesystem permissions exist.
    /// 验证在文件系统权限功能存在前，直接本地路径仍可进行全局读取。
    #[tokio::test]
    async fn local_path_resolution_accepts_project_external_absolute_and_relative_targets()
    -> Result<(), Box<dyn std::error::Error>> {
        // The isolated fixture models one project directory and one sibling file outside it.
        // 隔离夹具模拟一个项目目录及位于其外部的同级文件。
        let directory = tempfile::tempdir()?;
        let workspace = directory.path().join("project");
        let external = directory.path().join("outside.png");
        tokio::fs::create_dir(&workspace).await?;
        tokio::fs::write(&external, b"abc").await?;
        let canonical_external = tokio::fs::canonicalize(&external).await?;

        // An absolute path never depends on a workspace base.
        // 绝对路径从不依赖工作区基准。
        assert_eq!(
            resolve_provider_tool_local_path(None, external.to_string_lossy().as_ref()).await?,
            canonical_external
        );

        // Parent traversal is ordinary relative addressing until a future permission gate decides otherwise.
        // 在未来权限门作出决定前，父目录跳转属于普通相对寻址。
        let relative = Path::new("..").join("outside.png");
        assert_eq!(
            resolve_provider_tool_local_path(Some(&workspace), relative.to_string_lossy().as_ref())
                .await?,
            canonical_external
        );
        assert!(
            resolve_provider_tool_local_path(None, "outside.png")
                .await
                .expect_err("a relative path without a workspace base must fail")
                .contains("workspace root")
        );
        Ok(())
    }

    /// Selects test `resource` through the production planner with default independent policy.
    /// 使用默认独立策略，通过生产规划器选择测试 `resource` 的通道。
    /// `channels`, `references` and `credential_role` retain exact manifest authorization; returns its result.
    /// `channels`、`references` 与 `credential_role` 保留精确清单授权，返回生产规划结果。
    fn select_test_channel<'a>(
        channels: &'a [UploadChannelDeclaration],
        references: &[String],
        resource: &UploadResource,
        credential_role: Option<&str>,
    ) -> Result<&'a UploadChannelDeclaration, String> {
        // Every test invocation receives a fresh request budget, never a duplicate test-only resolver.
        // 每次测试调用接收新的请求预算，不使用重复的测试专用解析器。
        let mut planner = crate::MediaDeliveryPlanner::new(
            crate::MediaDeliveryPolicy::default(),
            crate::MediaDeliveryPurpose::IndependentExtension,
        )?;
        resolve_operation_delivery_channel(
            channels,
            references,
            resource,
            credential_role,
            &mut planner,
            &[
                vulcan_ai_contract::ResourceKind::Image,
                vulcan_ai_contract::ResourceKind::Audio,
                vulcan_ai_contract::ResourceKind::Video,
                vulcan_ai_contract::ResourceKind::File,
            ],
        )
    }

    /// Verifies Coding uploads carry the same Kimi device identity as conversation execution.
    /// 验证 Coding 上传携带与会话执行相同的 Kimi 设备身份。
    #[test]
    fn coding_device_upload_headers_include_identity() {
        // entry and profile select the exact Coding-device authentication projection.
        // entry 与 profile 选择精确的 Coding 设备鉴权投影。
        let mut entry = dummy_entry();
        entry.authentication = crate::AuthenticationProjection::CodingDeviceBearer;
        let profile = dummy_credential_profile();
        let secret = crate::CredentialSecretEnvelope::new(
            crate::CredentialSecretMaterial::KimiCodingPlanOAuth {
                tokens: crate::OAuthTokenSet {
                    access_token: String::from("test-access-token"),
                    refresh_token: String::from("test-refresh-token"),
                    issued_at_unix_ms: 1,
                    expires_at_unix_ms: 2,
                    scopes: Default::default(),
                    account_subject: None,
                    email: None,
                },
                device_id: String::from("12345678-1234-1234-1234-123456789abc"),
            },
        )
        .expect("Kimi OAuth credential should validate")
        .execution_secret()
        .expect("Kimi OAuth execution secret should resolve");

        let headers = upload_headers(&entry, &profile, &secret)
            .expect("Coding-device upload headers should project");

        assert_eq!(
            headers
                .get(reqwest::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer test-access-token")
        );
        assert_eq!(
            headers
                .get("x-msh-device-id")
                .and_then(|value| value.to_str().ok()),
            Some("12345678-1234-1234-1234-123456789abc")
        );
        assert_eq!(
            headers
                .get("x-msh-platform")
                .and_then(|value| value.to_str().ok()),
            Some(vulcan_llm_core::VULCAN_CODE_CLIENT_NAME)
        );
    }

    fn channel(
        primitive: UploadPrimitiveKind,
        accepted_mime_types: Vec<String>,
        max_bytes: Option<u64>,
        max_count: Option<u64>,
    ) -> UploadChannelDeclaration {
        UploadChannelDeclaration {
            upload_channel_id: String::from("test.channel"),
            description: String::from("test channel"),
            primitive: Some(primitive),
            signing_primitive: None,
            endpoint_host: Some(String::from("upload.example.invalid")),
            endpoint_path: Some(String::from("/v1/files")),
            method: Some(String::from("POST")),
            credential_role: None,
            file_field: Some(String::from("file")),
            accepted_mime_types,
            max_bytes,
            max_count,
            timeout_ms: matches!(
                primitive,
                UploadPrimitiveKind::MultipartFile
                    | UploadPrimitiveKind::ProviderFileCreate
                    | UploadPrimitiveKind::PresignedFormUpload
                    | UploadPrimitiveKind::SignedObjectUpload
            )
            .then_some(60_000),
            response_id_pointer: Some(String::from("/id")),
            response_id_encoding: None,
            response_url_pointer: Some(String::from("/url")),
            reference_template: None,
            progress_supported: false,
            cancellation_supported: false,
            fixed_fields: BTreeMap::new(),
            cache_duration_ms: None,
            reference_encoding: None,
            reference_scope: Default::default(),
        }
    }

    fn resource(mime_type: &str, byte_len: u64) -> UploadResource {
        UploadResource {
            source_identity: None,
            file_name: None,
            local_path: None,
            inline_bytes: Some(vec![0_u8; byte_len as usize]),
            mime_type: mime_type.to_string(),
            byte_len,
            remote_url: None,
            item_count: 1,
        }
    }

    /// Builds one complete temporary OSS policy with an exact caller-selected upload host.
    /// 构建一份完整临时 OSS 策略，并使用调用方选择的精确上传宿主。
    ///
    /// # Parameters
    /// 参数
    /// `upload_host` is the provider-returned destination under validation.
    /// `upload_host` 是待校验的供应商返回目标。
    ///
    /// # Returns
    /// 返回值
    /// A structurally complete policy whose only variable authority is the upload URL.
    /// 返回结构完整且仅上传 URL 权限可变的策略。
    fn temporary_policy(upload_host: &str) -> TemporaryOssPolicy {
        TemporaryOssPolicy {
            upload_host: upload_host.to_string(),
            upload_dir: String::from("temporary/session"),
            oss_access_key_id: String::from("temporary-access-key"),
            signature: String::from("signed-value"),
            policy: String::from("encoded-policy"),
            x_oss_object_acl: String::from("private"),
            x_oss_forbid_overwrite: String::from("true"),
        }
    }

    /// Builds one exact operation binding canonical source paths to materialized image inputs.
    /// 构建一个把规范源路径绑定到已物化图片输入的精确操作。
    fn image_edit_operation() -> crate::OperationDeclaration {
        crate::OperationDeclaration {
            operation_id: String::from("fixture.image-edit"),
            operation: String::from("image_edit"),
            carrier: crate::OperationCarrier::ProviderTool,
            input_resources: vec![vulcan_ai_contract::ResourceKind::Image],
            output_resources: vec![vulcan_ai_contract::ResourceKind::Image],
            tool_target_refs: vec![String::from("fixture.image-edit")],
            tool_target_template_refs: Vec::new(),
            request_profile_refs: Vec::new(),
            response_profile_refs: Vec::new(),
            upload_channel_refs: vec![String::from("test.channel")],
            upload_bindings: vec![crate::OperationUploadBinding {
                source_path: String::from("/source_paths"),
                target_path: String::from("/source_images"),
                item_source_path: None,
                item_target_path: None,
                item_match_path: None,
                item_match_value: None,
                upload_channel_refs: vec![String::from("test.channel")],
                mime_type: None,
                mime_types_by_extension: BTreeMap::from([
                    (String::from("jpg"), String::from("image/jpeg")),
                    (String::from("png"), String::from("image/png")),
                ]),
                required: true,
                remove_source: true,
                target_encoding: crate::provider_manifest::OperationUploadTargetEncoding::Reference,
            }],
            async_execution: None,
            timeout_ms: Some(60_000),
            idempotent: false,
        }
    }

    /// Verifies a MIME type outside the channel allowlist fails before any network call.
    /// 验证通道允许列表之外的 MIME 类型在任何网络调用前失败。
    #[test]
    fn rejects_mime_outside_channel_allowlist() {
        let channel = channel(
            UploadPrimitiveKind::InlineBase64,
            vec![String::from("image/png")],
            None,
            None,
        );
        let error = validate_upload_resource(&channel, &resource("video/mp4", 16))
            .expect_err("rejected MIME must fail");
        assert_eq!(error, MediaUploadFailure::UnsupportedMime);
    }

    /// Verifies a byte length beyond the channel limit fails before any network call.
    /// 验证超过通道限制的字节长度在任何网络调用前失败。
    #[test]
    fn rejects_bytes_beyond_channel_limit() {
        let channel = channel(
            UploadPrimitiveKind::InlineBase64,
            vec![String::from("image/png")],
            Some(1_024),
            None,
        );
        let error = validate_upload_resource(&channel, &resource("image/png", 2_048))
            .expect_err("oversized upload must fail");
        assert_eq!(
            error,
            MediaUploadFailure::SourceTooLarge {
                actual_bytes: 2048,
                max_bytes: 1024
            }
        );
    }

    /// Verifies an item count beyond the channel limit fails before any network call.
    /// 验证超过通道限制的条目数量在任何网络调用前失败。
    #[test]
    fn rejects_count_beyond_channel_limit() {
        let channel = channel(
            UploadPrimitiveKind::InlineBase64,
            vec![String::from("image/png")],
            None,
            Some(1),
        );
        let mut resource = resource("image/png", 16);
        resource.item_count = 2;
        let error =
            validate_upload_resource(&channel, &resource).expect_err("excess count must fail");
        assert_eq!(
            error,
            MediaUploadFailure::TooManyResources {
                actual_count: 2,
                max_count: 1
            }
        );
    }

    /// Verifies production operation materialization rewrites only declared argument pointers.
    /// 验证生产操作物化仅重写声明的参数 Pointer。
    #[tokio::test]
    async fn operation_upload_materialization_rewrites_declared_arguments() {
        let channel = channel(
            UploadPrimitiveKind::InlineDataUrl,
            vec![String::from("image/png")],
            Some(1024),
            Some(2),
        );
        let arguments = serde_json::json!({
            "instruction": "Retain the composition.",
            "source_paths": [
                "data:image/png;base64,aGVsbG8=",
                "data:image/png;base64,d29ybGQ="
            ]
        });
        let materialized = materialize_operation_uploads(
            OperationUploadExecutionContext {
                media_sources: &[],
                media_delivery_policy: crate::MediaDeliveryPolicy::default(),
                http: &reqwest::Client::new(),
                entry: &dummy_entry(),
                secret: &dummy_secret(),
                credential_profile: &dummy_credential_profile(),
                channels: &[channel],
                progress: None,
                cancellation: &MediaUploadCancellation::new(),
                model_id: None,
                workspace_root: None,
                fixture_loopback_origin: None,
            },
            &image_edit_operation(),
            arguments,
        )
        .await
        .expect("operation uploads should materialize");
        assert!(materialized.get("source_paths").is_none());
        assert_eq!(
            materialized["source_images"].as_array().map(Vec::len),
            Some(2)
        );
        assert_eq!(
            materialized["source_images"][0],
            "data:image/png;base64,aGVsbG8="
        );
        assert_eq!(materialized["instruction"], "Retain the composition.");
    }

    /// Verifies every binding is limit-checked before the first upload primitive executes.
    /// 验证每个绑定都在第一个上传原语执行前完成限制校验。
    #[tokio::test]
    async fn operation_upload_materialization_preflights_all_items() {
        let channel = channel(
            UploadPrimitiveKind::MultipartFile,
            vec![String::from("image/png")],
            Some(4),
            Some(2),
        );
        let arguments = serde_json::json!({
            "source_paths": [
                "data:image/png;base64,YWJj",
                "data:image/png;base64,dG9vLWxvbmc="
            ]
        });
        let error = materialize_operation_uploads(
            OperationUploadExecutionContext {
                media_sources: &[],
                media_delivery_policy: crate::MediaDeliveryPolicy::default(),
                http: &reqwest::Client::new(),
                entry: &dummy_entry(),
                secret: &dummy_secret(),
                credential_profile: &dummy_credential_profile(),
                channels: &[channel],
                progress: None,
                cancellation: &MediaUploadCancellation::new(),
                model_id: None,
                workspace_root: None,
                fixture_loopback_origin: None,
            },
            &image_edit_operation(),
            arguments,
        )
        .await
        .expect_err("second oversized item must fail before upload");
        assert!(
            error.contains("too large for the selected upload interface"),
            "error: {error}"
        );
    }

    /// Verifies inline data-url materialization produces the canonical data URL reference.
    /// 验证内联 Data URL 物化产生规范 Data URL 引用。
    #[tokio::test]
    async fn inline_data_url_materializes_canonical_reference() {
        let channel = channel(
            UploadPrimitiveKind::InlineDataUrl,
            vec![String::from("image/png")],
            None,
            None,
        );
        let mut resource = resource("image/png", 3);
        resource.inline_bytes = Some(b"abc".to_vec());
        let cancellation = MediaUploadCancellation::new();
        let materialized = materialize_upload(
            UploadExecutionContext {
                http: &reqwest::Client::new(),
                entry: &dummy_entry(),
                secret: &dummy_secret(),
                credential_profile: &dummy_credential_profile(),
                progress: None,
                cancellation: &cancellation,
                model_id: None,
                fixture_loopback_origin: None,
            },
            &channel,
            &resource,
        )
        .await
        .expect("inline materialization must succeed");
        assert_eq!(materialized.reference, "data:image/png;base64,YWJj");
    }

    /// Verifies inline base64 materialization produces the canonical base64 reference.
    /// 验证内联 Base64 物化产生规范 Base64 引用。
    #[tokio::test]
    async fn inline_base64_materializes_canonical_reference() {
        let channel = channel(
            UploadPrimitiveKind::InlineBase64,
            vec![String::from("image/png")],
            None,
            None,
        );
        let mut resource = resource("image/png", 3);
        resource.inline_bytes = Some(b"abc".to_vec());
        let cancellation = MediaUploadCancellation::new();
        let materialized = materialize_upload(
            UploadExecutionContext {
                http: &reqwest::Client::new(),
                entry: &dummy_entry(),
                secret: &dummy_secret(),
                credential_profile: &dummy_credential_profile(),
                progress: None,
                cancellation: &cancellation,
                model_id: None,
                fixture_loopback_origin: None,
            },
            &channel,
            &resource,
        )
        .await
        .expect("inline materialization must succeed");
        assert_eq!(materialized.reference, "YWJj");
    }

    /// Verifies a replaced local source is rejected through a frozen, bounded file handle.
    /// 验证被替换的本地来源会通过冻结且有界的文件句柄被拒绝。
    #[tokio::test]
    async fn local_inline_source_growth_is_rejected_without_unbounded_reading() {
        // path is unique per test process and never overlaps a user-owned upload source.
        // path 在每个测试进程中唯一，绝不会与用户持有的上传来源重叠。
        let path = std::env::temp_dir().join(format!(
            "vulcan-upload-source-growth-{}.png",
            uuid::Uuid::new_v4()
        ));
        tokio::fs::write(path.as_path(), b"expanded")
            .await
            .expect("expanded source fixture should write");
        // resource freezes the earlier one-byte observation while the on-disk source is now larger.
        // resource 冻结此前的一字节观测，而磁盘来源现在已经更大。
        let resource = UploadResource {
            source_identity: None,
            file_name: None,
            local_path: Some(path),
            inline_bytes: None,
            mime_type: String::from("image/png"),
            byte_len: 1,
            remote_url: None,
            item_count: 1,
        };

        // error proves the open-handle metadata gate rejects the drift before body allocation.
        // error 证明已打开句柄的元数据门在正文分配前拒绝漂移。
        let error = read_inline_bytes(&resource)
            .await
            .expect_err("grown local source must fail closed");
        assert!(
            error.contains("changed during preparation"),
            "error: {error}"
        );
        tokio::fs::remove_file(
            resource
                .local_path
                .as_ref()
                .expect("local path should exist"),
        )
        .await
        .expect("expanded source fixture should be removed");
    }

    /// Verifies an inconsistent inline descriptor cannot bypass the channel byte ceiling.
    /// 验证不一致的内联描述符无法绕过通道字节上限。
    #[tokio::test]
    async fn inconsistent_inline_source_length_is_rejected_before_cloning() {
        // resource intentionally understates its caller-owned inline payload length.
        // resource 故意低报调用方持有的内联负载长度。
        let resource = UploadResource {
            source_identity: None,
            file_name: None,
            local_path: None,
            inline_bytes: Some(b"expanded".to_vec()),
            mime_type: String::from("image/png"),
            byte_len: 1,
            remote_url: None,
            item_count: 1,
        };

        // error proves the mismatch is rejected before duplicating the inline allocation.
        // error 证明不一致会在复制内联内存分配前被拒绝。
        let error = read_inline_bytes(&resource)
            .await
            .expect_err("understated inline source must fail closed");
        assert!(
            error.contains("length changed during preparation"),
            "error: {error}"
        );
    }

    /// Verifies a template-encoded channel renders `{id}` into the final reference.
    /// 验证模板编码通道把 `{id}` 渲染进最终引用。
    #[tokio::test]
    async fn template_reference_renders_id_placeholder() {
        let mut channel = channel(
            UploadPrimitiveKind::ProviderFileCreate,
            vec![String::from("image/png")],
            None,
            None,
        );
        channel.reference_encoding = Some(UploadReferenceEncoding::Template);
        channel.reference_template = Some(String::from("file://{id}"));
        let mut resource = resource("image/png", 3);
        resource.inline_bytes = Some(b"abc".to_vec());
        let reference = render_reference(
            &channel,
            Some(String::from("file-123")),
            Some(String::from("https://upload.example.invalid/file-123")),
        )
        .expect("template must render");
        assert_eq!(reference, "file://file-123");
    }

    /// Verifies persisted references cannot cross the declared encoding or URI boundary.
    /// 验证持久化引用不能越过声明的编码或 URI 边界。
    #[test]
    fn prepared_reference_validation_is_closed_by_channel_contract() {
        let channel = channel(
            UploadPrimitiveKind::ProviderFileCreate,
            vec![String::from("video/mp4")],
            None,
            None,
        );
        assert!(validate_prepared_reference(&channel, "file-123").is_ok());
        assert!(validate_prepared_reference(&channel, "https://other.invalid/file-123").is_err());

        let mut template_channel = channel.clone();
        template_channel.reference_encoding = Some(UploadReferenceEncoding::Template);
        template_channel.reference_template = Some(String::from("mm_file://{id}"));
        assert!(validate_prepared_reference(&template_channel, "mm_file://file-123").is_ok());
        assert!(validate_prepared_reference(&template_channel, "ms://file-123").is_err());

        let mut url_channel = channel;
        url_channel.reference_encoding = Some(UploadReferenceEncoding::ResponseUrl);
        assert!(
            validate_prepared_reference(&url_channel, "https://cdn.example.invalid/file").is_ok()
        );
        assert!(
            validate_prepared_reference(&url_channel, "http://cdn.example.invalid/file").is_err()
        );

        let mut template_url_channel = url_channel;
        template_url_channel.reference_encoding = Some(UploadReferenceEncoding::Template);
        template_url_channel.reference_template = Some(String::from("provider://{url}"));
        assert!(
            validate_prepared_reference(
                &template_url_channel,
                "provider://https://cdn.example.invalid/file"
            )
            .is_ok()
        );
        assert!(
            validate_prepared_reference(
                &template_url_channel,
                "provider://http://cdn.example.invalid/file"
            )
            .is_err()
        );
    }

    /// Verifies declared inline video is admitted by policy instead of an unconditional modality ban.
    /// 验证已声明内联视频由策略准入，而非被无条件模态禁令拒绝。
    #[test]
    fn video_upload_uses_configured_inline_policy() {
        let channel = channel(
            UploadPrimitiveKind::InlineDataUrl,
            vec![String::from("video/mp4")],
            None,
            None,
        );
        // Primitive validation remains independent of the resource delivery policy.
        // 原语校验保持独立于资源投递策略。
        validate_upload_resource(&channel, &resource("video/mp4", 4))
            .expect("declared inline video primitive");
        // The default independent threshold admits small video and rejects oversized video explicitly.
        // 默认独立阈值允许小视频，并明确拒绝超大视频。
        let channels = [channel];
        let mut planner = crate::MediaDeliveryPlanner::new(
            crate::MediaDeliveryPolicy::default(),
            crate::MediaDeliveryPurpose::IndependentExtension,
        )
        .expect("valid policy");
        assert!(
            resolve_operation_delivery_channel(
                &channels,
                &[String::from("test.channel")],
                &resource("video/mp4", 4),
                None,
                &mut planner,
                &[vulcan_ai_contract::ResourceKind::Video]
            )
            .is_ok()
        );
        assert!(
            resolve_operation_delivery_channel(
                &channels,
                &[String::from("test.channel")],
                &resource("video/mp4", 10 * 1024 * 1024 + 1),
                None,
                &mut planner,
                &[vulcan_ai_contract::ResourceKind::Video]
            )
            .expect_err("oversized video")
            .contains("inline size limit")
        );
    }

    /// Verifies the signed primitives refuse to guess a signature contract.
    /// 验证签名原语拒绝猜测签名契约。
    #[tokio::test]
    async fn signed_primitives_require_registered_signing() {
        let channel = channel(
            UploadPrimitiveKind::PresignedFormUpload,
            vec![String::from("image/png")],
            None,
            None,
        );
        let cancellation = MediaUploadCancellation::new();
        let error = materialize_upload(
            UploadExecutionContext {
                http: &reqwest::Client::new(),
                entry: &dummy_entry(),
                secret: &dummy_secret(),
                credential_profile: &dummy_credential_profile(),
                progress: None,
                cancellation: &cancellation,
                model_id: None,
                fixture_loopback_origin: None,
            },
            &channel,
            &resource("image/png", 3),
        )
        .await
        .expect_err("signed primitive without signing contract must fail");
        assert_eq!(error, MediaUploadFailure::MissingSigningPrimitive);
    }

    /// Verifies the runtime refuses a network timeout outside the validated manifest range.
    /// 验证运行时拒绝超出已校验 Manifest 范围的网络超时。
    #[tokio::test]
    async fn network_upload_timeout_range_is_rechecked_at_runtime() {
        let error = await_network_upload_with_timeout(
            "test.channel",
            999,
            std::future::pending::<Result<UploadedResourceReference, MediaUploadFailure>>(),
        )
        .await
        .expect_err("an out-of-range timeout must fail before awaiting the upload");
        assert_eq!(error, MediaUploadFailure::InvalidContract);
    }

    /// Verifies expiration drops a still-pending upload and reports the owning channel.
    /// 验证期限届满会丢弃仍在等待的上传并报告所属通道。
    #[tokio::test]
    async fn network_upload_total_timeout_cancels_pending_future() {
        let error = await_network_upload_with_timeout(
            "test.channel",
            1_000,
            std::future::pending::<Result<UploadedResourceReference, MediaUploadFailure>>(),
        )
        .await
        .expect_err("a pending upload must terminate at its total timeout");
        assert_eq!(
            error,
            MediaUploadFailure::Timeout {
                channel_id: "test.channel".into(),
                timeout_ms: 1000
            }
        );
    }

    /// Verifies dynamic signed-upload destinations accept any HTTPS host but keep the URL shape.
    /// 验证动态签名上传目标接受任何 HTTPS 宿主，但保持 URL 形状约束。
    #[test]
    fn temporary_oss_policy_accepts_any_https_host() {
        for host in [
            "https://bucket.aliyuncs.com",
            "https://bucket.oss.example.com:8443",
        ] {
            // accepted proves the provider-selected host is used as returned.
            // accepted 证明供应商选择的宿主按原样使用。
            let accepted = validate_temporary_oss_policy(&temporary_policy(host))
                .expect("a credential-free HTTPS destination should be accepted");
            assert_eq!(accepted.as_str().trim_end_matches('/'), host);
        }
        for rejected in [
            "http://bucket.aliyuncs.com",
            "https://user@bucket.aliyuncs.com",
            "https://bucket.aliyuncs.com/?x=1",
        ] {
            assert!(
                validate_temporary_oss_policy(&temporary_policy(rejected)).is_err(),
                "temporary OSS destination `{rejected}` must be rejected"
            );
        }
    }

    /// Verifies channel selection matches by reference and MIME allowlist.
    /// 验证通道选择按引用与 MIME 允许列表匹配。
    #[test]
    fn channel_selection_matches_reference_and_mime() {
        let mut png = channel(
            UploadPrimitiveKind::InlineBase64,
            vec![String::from("image/png")],
            None,
            None,
        );
        png.upload_channel_id = String::from("inline.png");
        let mut video = channel(
            UploadPrimitiveKind::MultipartFile,
            vec![String::from("video/mp4")],
            Some(8 * 1_024 * 1_024),
            Some(1),
        );
        video.upload_channel_id = String::from("multipart.video");
        let channels = vec![png, video];
        // The image channel cannot accept the video resource, so the engine moves to the next
        // declared reference and selects the multipart channel by reference + MIME allowlist.
        // 图片通道无法接受视频资源，引擎继续检查下一声明引用并按引用 + MIME 允许列表
        // 选择 Multipart 通道。
        let selected = select_test_channel(
            &channels,
            &[String::from("inline.png"), String::from("multipart.video")],
            &resource("video/mp4", 16),
            None,
        )
        .expect("video channel must accept the video resource");
        assert_eq!(
            selected.upload_channel_id, "multipart.video",
            "the second declared reference must win for the video MIME"
        );

        // The first declared reference wins when its MIME allowlist covers the resource.
        // 首个声明引用在其 MIME 允许列表覆盖资源时胜出。
        let selected = select_test_channel(
            &channels,
            &[String::from("inline.png"), String::from("multipart.video")],
            &resource("image/png", 16),
            None,
        )
        .expect("image channel must accept the image resource");
        assert_eq!(selected.upload_channel_id, "inline.png");

        let error = select_test_channel(
            &channels,
            &[String::from("inline.png")],
            &resource("audio/wav", 16),
            None,
        )
        .expect_err("no channel accepts wav");
        assert!(error.contains("accepts MIME type"), "error: {error}");

        let error = select_test_channel(
            &channels,
            &[String::from("missing.channel")],
            &resource("image/png", 16),
            None,
        )
        .expect_err("unknown channel must fail");
        assert!(error.contains("unknown upload channel"), "error: {error}");
    }

    /// Verifies channel selection fails closed when the selected credential role differs.
    /// 验证所选凭据角色不一致时通道选择会失败关闭。
    #[test]
    fn channel_selection_requires_exact_credential_role() {
        let mut channel = channel(
            UploadPrimitiveKind::InlineBase64,
            vec![String::from("image/png")],
            None,
            None,
        );
        channel.credential_role = Some(String::from("media-upload"));
        let channels = [channel];
        let references = [String::from("test.channel")];

        let error = select_test_channel(
            &channels,
            &references,
            &resource("image/png", 16),
            Some("inference"),
        )
        .expect_err("a different credential role must not select the channel");
        assert!(
            error.contains("credential role `inference`"),
            "error: {error}"
        );

        let selected = select_test_channel(
            &channels,
            &references,
            &resource("image/png", 16),
            Some("media-upload"),
        )
        .expect("the exact credential role must select the channel");
        assert_eq!(selected.upload_channel_id, "test.channel");
    }

    /// Verifies upload authentication uses the exact credential-profile override.
    /// 验证上传鉴权使用精确凭据配置档覆盖。
    #[test]
    fn upload_headers_use_exact_credential_profile_authentication() {
        let entry = dummy_entry();
        let mut credential_profile = dummy_credential_profile();
        credential_profile.authentication = Some(crate::AuthenticationProjection::XApiKey);
        let secret = dummy_secret();

        let headers = upload_headers(&entry, &credential_profile, &secret)
            .expect("API-key projection must build upload headers");

        assert!(headers.get(reqwest::header::AUTHORIZATION).is_none());
        assert_eq!(
            headers
                .get("x-api-key")
                .and_then(|value| value.to_str().ok()),
            Some("test-token")
        );
    }

    /// Verifies a bearer credential profile overrides an entry-level API-key default for Files API uploads.
    /// 验证 Bearer 凭据配置档会为 Files API 上传覆盖入口级 API Key 默认值。
    #[test]
    fn upload_headers_allow_bearer_profile_to_override_entry_api_key_default() {
        // entry simulates MiniMax's legacy service-level API-key default.
        // entry 模拟 MiniMax 旧有的服务级 API Key 默认值。
        let mut entry = dummy_entry();
        entry.authentication = crate::AuthenticationProjection::XApiKey;
        // credential_profile freezes the Bearer projection required by the Files API.
        // credential_profile 冻结 Files API 要求的 Bearer 投影。
        let mut credential_profile = dummy_credential_profile();
        credential_profile.authentication =
            Some(crate::AuthenticationProjection::BearerAuthorization);
        // secret is the opaque credential material projected by the selected profile.
        // secret 是由所选配置档投影的不透明凭据材料。
        let secret = dummy_secret();

        // headers are the final upload-only request headers sent before multipart streaming.
        // headers 是 Multipart 流式发送前的最终上传专用请求头。
        let headers = upload_headers(&entry, &credential_profile, &secret)
            .expect("bearer override must build upload headers");

        assert_eq!(
            headers
                .get(reqwest::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer test-token")
        );
        assert!(!headers.contains_key("x-api-key"));
    }

    /// Verifies a resource with an unknown MIME is rejected even when a channel declares no
    /// allowlist; the first declared reference then wins by reference only.
    /// 验证未知 MIME 资源在通道未声明允许列表时由首个声明引用胜出。
    #[test]
    fn first_declared_reference_wins_without_allowlist() {
        let mut open = channel(UploadPrimitiveKind::InlineBase64, Vec::new(), None, None);
        open.upload_channel_id = String::from("open.channel");
        let channels = [open];
        let selected = select_test_channel(
            &channels,
            &[String::from("open.channel")],
            &resource("application/octet-stream", 16),
            None,
        )
        .expect("open channel accepts any MIME");
        assert_eq!(selected.upload_channel_id, "open.channel");
    }

    /// Verifies remote URL passthrough accepts any HTTPS host and rejects unsafe URL forms.
    /// 验证远端 URL 直通接受任何 HTTPS 宿主并拒绝不安全的 URL 形式。
    #[test]
    fn remote_url_passthrough_accepts_any_https_host() {
        let mut channel = channel(
            UploadPrimitiveKind::RemoteUrlPassthrough,
            vec![String::from("image/png")],
            None,
            Some(1),
        );
        channel.endpoint_host = None;
        channel.endpoint_path = None;
        channel.method = None;
        channel.file_field = None;
        channel.response_id_pointer = None;
        channel.response_url_pointer = None;
        for accepted in [
            "https://cdn.example.com/media.png?signature=fixture",
            "https://media.other.test:8443/media.png",
        ] {
            assert!(
                validate_remote_url_passthrough(&channel, accepted).is_ok(),
                "remote URL `{accepted}` must be accepted"
            );
        }
        for rejected in [
            "http://cdn.example.com/media.png",
            "https://user@cdn.example.com/media.png",
            "https://cdn.example.com/media.png#fragment",
        ] {
            assert!(
                validate_remote_url_passthrough(&channel, rejected).is_err(),
                "remote URL `{rejected}` must be rejected"
            );
        }
    }

    fn dummy_entry() -> ServiceEntry {
        ServiceEntry {
            id: String::from("tools-api"),
            display_name: String::from("Tools API"),
            kind: crate::ServiceEntryKind::ProviderTools,
            base_url: String::from("https://api.example.invalid"),
            credential_profiles: Vec::new(),
            authentication: crate::AuthenticationProjection::BearerAuthorization,
            request_headers: BTreeMap::new(),
            session_header: None,
            service_origin_override_policy: None,
            native_inference_bindings: Vec::new(),
            request_projector: crate::RequestProjectorKind::Direct,
            model_discovery: crate::ModelDiscoveryKind::ProviderToolsOnly,
            model_discovery_path: None,
            protocol_routes: Vec::new(),
            default_usage_scopes: Default::default(),
        }
    }

    /// Builds one exact credential profile used by upload projection tests.
    /// 构建一份供上传投影测试使用的精确凭据配置档。
    fn dummy_credential_profile() -> CredentialProfile {
        CredentialProfile {
            id: String::from("api-key"),
            display_name: String::from("API Key"),
            credential_mode: crate::CredentialMode::Token,
            authorization: None,
            billing_kind: crate::CredentialBillingKind::ProviderManaged,
            account_usage_query: crate::AccountUsageQueryKind::Unsupported,
            account_model_access: None,
            authentication: None,
            key_prefix: None,
            credential_role: None,
            default_usage_scopes: Default::default(),
        }
    }

    fn dummy_secret() -> CredentialExecutionSecret {
        crate::CredentialSecretEnvelope::new_api_key(String::from("test-token"))
            .expect("api key envelope")
            .execution_secret()
            .expect("execution secret")
    }
}
