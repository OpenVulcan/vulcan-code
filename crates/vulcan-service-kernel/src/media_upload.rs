//! Pre-dispatch preparation plus post-dispatch media-understanding upload progress.
//! 分派前媒体准备与分派后媒体理解上传进度。

use super::*;
use crate::conversation_submission::SubmissionModelSelection;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use vulcan_core::types::{
    ConversationAttachment, ConversationAttachmentDeliveryMode, ConversationAttachmentKind,
};
use vulcan_provider_adapters::{
    CONVERSATION_MEDIA_INPUT_OPERATION_ID, MediaUploadCancellation, MediaUploadProgressCallback,
    ProviderOperationalProfileRevision, ProviderOperationalTelemetryEvent,
    ProviderOperationalTelemetryEventName, ResourceReferencePolicy,
    record_provider_operational_telemetry,
};
use vulcan_service_api::{
    ComposerAttachmentDeliveryPlan, ComposerAttachmentPreparationPhase,
    ComposerAttachmentPreparationSummary, ComposerAttachmentSummary, MediaUploadPhase,
    MediaUploadProgressChangedDelta, ServiceEvent, WorkbenchDelta, WorkbenchDeltaEnvelope,
};
use vulcan_workbench_model::{SessionMediaReferenceEntry, SessionMediaReferenceScope};

/// Process-local pending media submissions keyed by the client submission id.
/// 按客户端提交标识索引的进程内待处理媒体提交。
static PENDING_MEDIA_UPLOADS: OnceLock<StdMutex<BTreeMap<String, MediaUploadCancellation>>> =
    OnceLock::new();

/// Process-local frozen draft plans keyed by their service-issued transfer identity.
/// 按服务签发传输身份索引的进程内冻结草稿计划。
static DRAFT_MEDIA_UPLOAD_PLANS: OnceLock<StdMutex<BTreeMap<String, DraftMediaUploadPlan>>> =
    OnceLock::new();

/// Maximum lifetime for an unconsumed process-local draft upload plan.
/// 未消费进程内草稿上传计划的最长存活时间。
const DRAFT_MEDIA_UPLOAD_PLAN_TTL: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// Host hard limit for original inline attachment bytes on routes without usable upload.
/// 没有可用上传路线时原始附件内联字节的宿主硬上限。
const COMPOSER_INLINE_ORIGINAL_MAX_BYTES: u64 = 2 * 1024 * 1024;

/// Host request-wide Base64 budget shared by all draft attachments planned for inline delivery.
/// 所有计划内联传输的草稿附件共享的宿主请求级 Base64 预算。
const COMPOSER_INLINE_AGGREGATE_BASE64_MAX_BYTES: u64 = 2 * 1024 * 1024;

/// Immutable non-secret route identity copied into every progress and telemetry event for one file.
/// 为单个文件复制到每条进度与遥测事件的不可变非秘密路由身份。
#[derive(Clone)]
struct MediaUploadTelemetryIdentity {
    /// Exact provider identifier.
    /// 精确供应商标识。
    provider_id: String,
    /// Exact service-entry identifier.
    /// 精确服务入口标识。
    service_entry_id: String,
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    connection_id: String,
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    model_id: String,
    /// Exact provider-neutral protocol profile.
    /// 精确供应商无关协议 Profile。
    protocol_profile_id: String,
    /// Frozen manifest revision.
    /// 冻结 Manifest 修订号。
    manifest_revision: u64,
    /// Frozen runtime manifest generation.
    /// 冻结运行时 Manifest 代际。
    manifest_generation: u64,
    /// Content-addressed complete route revision.
    /// 内容寻址的完整路由修订号。
    route_revision: String,
    /// Exact manifest or host-inline upload channel.
    /// 精确 Manifest 或宿主内联上传通道。
    upload_channel_id: String,
    /// Frozen request projection profile identities.
    /// 冻结请求投影 Profile 身份。
    request_profiles: Vec<ProviderOperationalProfileRevision>,
    /// Frozen response projection profile identities.
    /// 冻结响应投影 Profile 身份。
    response_profiles: Vec<ProviderOperationalProfileRevision>,
}

/// Retains one non-persisted upload plan and its frozen route contract until preparation starts.
/// 在准备开始前保留一份不持久化的上传计划及其冻结路由契约。
struct DraftMediaUploadPlan {
    /// Non-secret caller and route identity authorized by this plan.
    /// 本计划授权的非秘密调用方与路线身份。
    identity: DraftMediaUploadPlanIdentity,
    /// Frozen route and live credential lease used by the physical upload.
    /// 真实上传使用的冻结路由与实时凭据租约。
    execution: Arc<vulcan_provider_adapters::ResolvedPublicAiExecution>,
    /// Process-local creation instant used only for orphan cleanup.
    /// 仅用于孤儿清理的进程内创建时刻。
    created_at: std::time::Instant,
}

/// Carries the exact non-secret identity that authorizes one draft upload execution.
/// 携带授权单次草稿上传执行的精确非秘密身份。
#[derive(Debug, Clone, PartialEq, Eq)]
struct DraftMediaUploadPlanIdentity {
    /// Exact workspace that owns the managed attachment.
    /// 持有托管附件的精确工作区。
    workspace_path: String,
    /// Optional persisted conversation owner used by session-scoped upload channels.
    /// 会话作用域上传通道使用的可选持久化会话所有者。
    conversation_id: Option<String>,
    /// Exact provider, model, and credential tuple selected during planning.
    /// 规划期间选择的精确供应商、模型与凭据元组。
    model_selection: SubmissionModelSelection,
    /// Stable attachment identity authorized by this plan.
    /// 本计划授权的稳定附件身份。
    attachment_id: String,
    /// Caller generation that owns this plan.
    /// 持有本计划的调用方代际。
    generation: u64,
    /// Frozen content-addressed route revision.
    /// 冻结的内容寻址路由修订。
    route_revision: String,
}

/// Carries provider-native video references prepared for one accepted prompt.
/// 携带一条已接纳提示词所准备的供应商原生视频引用。
#[derive(Debug, Clone, Default)]
pub(crate) struct PreparedSubmissionMedia {
    /// Provider-native references that must be attached to the canonical user-message owner.
    /// 必须附加到规范用户消息所有者上的供应商原生引用。
    pub(crate) references: Vec<PreparedSubmissionMediaReference>,
}

/// Carries the authoritative request-local attachment projection and its prepared references.
/// 携带权威的单次请求局部附件投影及其已准备引用。
#[derive(Debug, Clone, Default)]
pub(crate) struct PreparedSubmissionMediaProjection {
    /// Provider-facing attachment copies projected from the exact frozen execution route.
    /// 从精确冻结执行路线投影得到的 Provider 侧附件副本。
    pub(crate) provider_attachments: Vec<ConversationAttachment>,
    /// Provider-native references prepared against the same frozen execution route.
    /// 针对同一冻结执行路线准备的 Provider 原生引用。
    pub(crate) prepared_media: PreparedSubmissionMedia,
}

/// Carries one durable media-ledger candidate without exposing provider secrets.
/// 携带一个不暴露供应商秘密的可持久化媒体账本候选项。
#[derive(Debug, Clone)]
pub(crate) struct PreparedSubmissionMediaReference {
    /// SHA-256 digest of the exact source bytes.
    /// 精确源字节的 SHA-256 摘要。
    pub(crate) source_sha256: String,
    /// Byte length covered by the source digest.
    /// 源摘要覆盖的字节长度。
    pub(crate) source_byte_len: u64,
    /// Provider identity owning the uploaded object.
    /// 拥有已上传对象的供应商标识。
    pub(crate) provider_id: String,
    /// Service/API entry owning the upload channel.
    /// 拥有上传通道的服务/API 入口。
    pub(crate) service_entry_id: String,
    /// Manifest upload channel that created the object.
    /// 创建该对象的 Manifest 上传通道。
    pub(crate) upload_channel_id: String,
    /// Content fingerprint of the upload contract semantics.
    /// 上传契约语义的内容指纹。
    pub(crate) upload_contract_fingerprint: String,
    /// Explicit reuse boundary declared by the upload channel.
    /// 上传通道声明的显式复用边界。
    pub(crate) reference_scope: SessionMediaReferenceScope,
    /// Credential identity required only by credential-scoped references.
    /// 仅凭据作用域引用需要的凭据身份。
    pub(crate) connection_id: Option<String>,
    /// Session identity required only by session-scoped references.
    /// 仅会话作用域引用需要的会话身份。
    pub(crate) session_id: Option<String>,
    /// Canonical resource kind retained by the ledger.
    /// 账本保留的规范资源类型。
    pub(crate) resource_kind: String,
    /// Exact normalized source MIME type.
    /// 精确归一化源 MIME 类型。
    pub(crate) mime_type: String,
    /// Provider-native URI or URL used by request projection.
    /// 请求投影使用的供应商原生 URI 或 URL。
    pub(crate) provider_reference: String,
    /// Provider reference creation time in Unix milliseconds.
    /// 供应商引用创建时间的 Unix 毫秒值。
    pub(crate) created_at_unix_millis: u64,
    /// Provider reference expiry time in Unix milliseconds.
    /// 供应商引用过期时间的 Unix 毫秒值。
    pub(crate) expires_at_unix_millis: u64,
    /// Historical canonical user-message owners that still retain this source.
    /// 仍保留该源文件的历史规范用户消息所有者。
    pub(crate) owner_message_ids: BTreeSet<String>,
    /// Whether the current submission owner should be attached after turn admission.
    /// 是否应在本轮回合准入后附加当前提交所有者。
    pub(crate) attach_current_owner: bool,
}

impl PreparedSubmissionMediaReference {
    /// Converts the preparation result into a durable session-ledger entry.
    /// 将准备结果转换为可持久化的会话账本条目。
    pub(crate) fn into_ledger_entry(
        self,
        owner_message_id: Option<&str>,
    ) -> SessionMediaReferenceEntry {
        let owner_message_ids = self.owner_message_ids;
        let attach_current_owner = self.attach_current_owner;
        let mut entry = SessionMediaReferenceEntry {
            source_sha256: self.source_sha256,
            source_byte_len: self.source_byte_len,
            provider_id: self.provider_id,
            service_entry_id: self.service_entry_id,
            upload_channel_id: self.upload_channel_id,
            upload_contract_fingerprint: self.upload_contract_fingerprint,
            reference_scope: self.reference_scope,
            connection_id: self.connection_id,
            session_id: self.session_id,
            resource_kind: self.resource_kind,
            mime_type: self.mime_type,
            provider_reference: self.provider_reference,
            created_at_unix_millis: self.created_at_unix_millis,
            expires_at_unix_millis: self.expires_at_unix_millis,
            owner_message_ids,
        };
        if attach_current_owner {
            if let Some(owner_message_id) = owner_message_id {
                entry.owner_message_ids.insert(owner_message_id.to_string());
            }
        }
        entry
    }
}

impl PreparedSubmissionMedia {
    /// Retains one durable provider reference for any uploaded canonical media resource.
    /// 为任意已上传规范媒体资源保留一条可持久化供应商引用。
    fn push_reference(&mut self, reference: PreparedSubmissionMediaReference) {
        self.references.push(reference);
    }
}

/// Returns the process-local pending media cancellation registry.
/// 返回进程内待处理媒体取消注册表。
fn pending_media_uploads() -> &'static StdMutex<BTreeMap<String, MediaUploadCancellation>> {
    PENDING_MEDIA_UPLOADS.get_or_init(|| StdMutex::new(BTreeMap::new()))
}

/// Returns the process-local registry of unconsumed frozen draft upload plans.
/// 返回尚未消费的进程内冻结草稿上传计划注册表。
fn draft_media_upload_plans() -> &'static StdMutex<BTreeMap<String, DraftMediaUploadPlan>> {
    DRAFT_MEDIA_UPLOAD_PLANS.get_or_init(|| StdMutex::new(BTreeMap::new()))
}

/// Removes expired draft plans while the plan registry is already locked.
/// 在计划注册表已锁定时移除过期草稿计划。
///
/// # Parameters
/// # 参数
/// `plans` is the authoritative process-local plan map.
/// `plans` 是权威进程内计划映射。
fn retain_live_draft_media_upload_plans(plans: &mut BTreeMap<String, DraftMediaUploadPlan>) {
    plans.retain(|_, plan| plan.created_at.elapsed() < DRAFT_MEDIA_UPLOAD_PLAN_TTL);
}

/// Atomically publishes a complete batch of service-issued draft upload plans.
/// 原子发布一整批服务签发的草稿上传计划。
///
/// # Parameters
/// # 参数
/// `plans` contains unique transfer identities produced by one planning transaction.
/// `plans` 包含一次规划事务生成的唯一传输身份。
///
/// # Returns
/// # 返回值
/// Success after every plan is visible, or an explicit collision without partial insertion.
/// 全部计划可见后返回成功；发生冲突时不进行部分插入并返回显式错误。
fn register_draft_media_upload_plans(
    plans: Vec<(String, DraftMediaUploadPlan)>,
) -> Result<(), String> {
    if plans.is_empty() {
        return Ok(());
    }
    let mut registry = draft_media_upload_plans()
        .lock()
        .map_err(|error| format!("failed to lock draft media upload plans: {error}"))?;
    retain_live_draft_media_upload_plans(&mut registry);
    // incomingTransferIds proves the planning transaction is internally unique before mutation.
    // incomingTransferIds 在修改注册表前证明本次规划事务内部身份唯一。
    let mut incoming_transfer_ids = BTreeSet::new();
    if let Some((transfer_id, _)) = plans
        .iter()
        .find(|(transfer_id, _)| !incoming_transfer_ids.insert(transfer_id.as_str()))
    {
        return Err(format!(
            "Draft media upload plan `{transfer_id}` is duplicated within one planning transaction."
        ));
    }
    if let Some((transfer_id, _)) = plans
        .iter()
        .find(|(transfer_id, _)| registry.contains_key(transfer_id))
    {
        return Err(format!(
            "Draft media upload plan `{transfer_id}` already exists."
        ));
    }
    registry.extend(plans);
    Ok(())
}

/// Atomically consumes one draft upload plan and publishes its cancellation token.
/// 原子消费一份草稿上传计划并发布其取消令牌。
///
/// # Parameters
/// # 参数
/// `transfer_id` is the opaque identity returned by the planning RPC; the remaining arguments are
/// the exact caller-visible facts that must match before registry mutation.
/// `transfer_id` 是规划 RPC 返回的不透明身份；其余参数是修改注册表前必须精确匹配的调用方可见事实。
///
/// # Returns
/// # 返回值
/// The unique frozen plan and registered cancellation token, or an explicit controlled error.
/// 返回唯一冻结计划与已注册取消令牌，或返回显式受控错误。
fn consume_draft_media_upload_plan(
    transfer_id: &str,
    workspace_path: &str,
    conversation_id: &Option<String>,
    model_selection: &SubmissionModelSelection,
    preparation: &ComposerAttachmentPreparationSummary,
) -> Result<(DraftMediaUploadPlan, MediaUploadCancellation), String> {
    let mut registry = draft_media_upload_plans()
        .lock()
        .map_err(|error| format!("failed to lock draft media upload plans: {error}"))?;
    retain_live_draft_media_upload_plans(&mut registry);
    let frozen_plan = registry.get(transfer_id).ok_or_else(|| {
        format!("Draft media upload plan `{transfer_id}` is missing, expired, or already consumed.")
    })?;
    if !draft_media_upload_plan_matches(
        &frozen_plan.identity,
        workspace_path,
        conversation_id,
        model_selection,
        preparation,
    ) {
        return Err(String::from(
            "Composer attachment preparation does not match its service-issued frozen upload plan.",
        ));
    }
    // pending is locked after the plan registry; cancellation uses the same order so no unobservable gap exists.
    // pending 在计划注册表之后加锁；取消路径使用相同顺序，因此不存在不可观测间隙。
    let mut pending = pending_media_uploads()
        .lock()
        .map_err(|error| format!("failed to lock pending media uploads: {error}"))?;
    if pending.contains_key(transfer_id) {
        return Err(format!(
            "client submission `{transfer_id}` is already preparing media"
        ));
    }
    let frozen_plan = registry.remove(transfer_id).ok_or_else(|| {
        format!("Draft media upload plan `{transfer_id}` is missing, expired, or already consumed.")
    })?;
    let cancellation = MediaUploadCancellation::new();
    pending.insert(transfer_id.to_string(), cancellation.clone());
    Ok((frozen_plan, cancellation))
}

/// Validates one caller preparation against the exact non-secret frozen plan identity.
/// 根据精确的非秘密冻结计划身份校验一条调用方准备请求。
///
/// # Parameters
/// # 参数
/// `identity` is service-owned; the remaining values are caller-visible request facts.
/// `identity` 由服务持有；其余值是调用方可见的请求事实。
///
/// # Returns
/// # 返回值
/// `true` only when every owner, route, generation, phase, and blocking fact matches exactly.
/// 仅当全部所有者、路线、代际、阶段与阻塞事实精确匹配时返回 `true`。
fn draft_media_upload_plan_matches(
    identity: &DraftMediaUploadPlanIdentity,
    workspace_path: &str,
    conversation_id: &Option<String>,
    model_selection: &SubmissionModelSelection,
    preparation: &ComposerAttachmentPreparationSummary,
) -> bool {
    identity.workspace_path == workspace_path
        && identity.conversation_id.as_deref() == conversation_id.as_deref()
        && &identity.model_selection == model_selection
        && identity.attachment_id.as_str() == preparation.attachment_id.as_str()
        && identity.generation == preparation.generation
        && preparation.route_revision.as_deref() == Some(identity.route_revision.as_str())
        && preparation.phase == ComposerAttachmentPreparationPhase::UploadQueued
        && preparation.blocks_submission
}

/// Registers one submission before any workspace or transcript mutation occurs.
/// 在任何工作区或转录变更前注册一条提交。
pub(crate) fn register_media_upload(
    client_submission_id: &str,
) -> Result<MediaUploadCancellation, String> {
    if client_submission_id.trim().is_empty() {
        return Err(String::from("client submission id must not be empty"));
    }
    let cancellation = MediaUploadCancellation::new();
    let mut pending = pending_media_uploads()
        .lock()
        .map_err(|error| format!("failed to lock pending media uploads: {error}"))?;
    if pending.contains_key(client_submission_id) {
        return Err(format!(
            "client submission `{client_submission_id}` is already preparing media"
        ));
    }
    pending.insert(client_submission_id.to_string(), cancellation.clone());
    Ok(cancellation)
}

/// Cancels one pending submission without touching persisted conversation state.
/// 取消一条待处理提交，但不触碰持久化会话状态。
pub(crate) fn cancel_media_upload(client_submission_id: &str) -> Result<bool, String> {
    // plans and pending use the same lock order as the consume transition.
    // plans 与 pending 使用和消费迁移相同的加锁顺序。
    let mut plans = draft_media_upload_plans()
        .lock()
        .map_err(|error| format!("failed to lock draft media upload plans: {error}"))?;
    retain_live_draft_media_upload_plans(&mut plans);
    let removed_draft_plan = plans.remove(client_submission_id).is_some();
    let pending = pending_media_uploads()
        .lock()
        .map_err(|error| format!("failed to lock pending media uploads: {error}"))?;
    let cancellation = pending.get(client_submission_id).cloned();
    if let Some(cancellation) = cancellation {
        cancellation.cancel();
        return Ok(true);
    }
    Ok(removed_draft_plan)
}

/// Removes the exact submission registration after upload preparation reaches a terminal state.
/// 媒体准备到达终态后移除精确提交注册。
fn remove_media_upload(client_submission_id: &str) {
    if let Ok(mut pending) = pending_media_uploads().lock() {
        pending.remove(client_submission_id);
    }
}

/// Resolves one provider-neutral direct-delivery decision from the exact frozen execution route.
/// 从精确冻结的执行路线解析单个供应商中立直接传输决策。
fn composer_attachment_delivery_plan(
    execution: &vulcan_provider_adapters::ResolvedPublicAiExecution,
    attachment: &ConversationAttachment,
) -> Result<(ComposerAttachmentDeliveryPlan, &'static str), String> {
    let resource_kind = attachment_kind(attachment.kind.clone());
    let constraint = execution
        .route
        .protocol_route
        .input_resource_constraints
        .iter()
        .find(|constraint| constraint.resource_kind == resource_kind);
    // originalByteLen is authoritative staged metadata; an absent value cannot be treated as an empty file.
    // originalByteLen 是权威暂存元数据；缺失值不能被当作空文件处理。
    let Some(original_byte_len) = attachment.byte_len else {
        return Ok((
            ComposerAttachmentDeliveryPlan::MetadataOnly,
            "attachment_size_is_unknown",
        ));
    };
    let original_is_native = submission_attachment_is_native(
        execution.route.protocol_route.protocol_profile_id.as_str(),
        &execution.model.capabilities,
        attachment,
    );
    let original_fits_constraint = constraint.is_none_or(|constraint| {
        (constraint.accepted_mime_types.is_empty()
            || constraint
                .accepted_mime_types
                .iter()
                .any(|mime_type| mime_type.eq_ignore_ascii_case(&attachment.mime_type)))
            && constraint
                .max_bytes
                .is_none_or(|max_bytes| original_byte_len <= max_bytes)
    });
    if original_is_native && original_fits_constraint {
        let upload_contract = execution.client.conversation_media_upload_contract(
            &execution.route,
            resource_kind,
            attachment.mime_type.as_str(),
        )?;
        let remote_only = constraint.is_some_and(|constraint| {
            constraint.reference_policy == ResourceReferencePolicy::RemoteOnly
        });
        let upload_required = resource_kind == vulcan_ai_contract::ResourceKind::Video
            || original_byte_len > vulcan_provider_adapters::PREFERRED_INLINE_MEDIA_MAX_BYTES
            || remote_only;
        if upload_required && upload_contract.is_some() {
            return Ok((
                ComposerAttachmentDeliveryPlan::UploadReference,
                "provider_upload_reference_required",
            ));
        }
        if !remote_only && original_byte_len <= COMPOSER_INLINE_ORIGINAL_MAX_BYTES {
            return Ok((
                ComposerAttachmentDeliveryPlan::InlineOriginal,
                "original_bytes_fit_host_inline_budget",
            ));
        }
    }
    if attachment.kind == ConversationAttachmentKind::Image {
        if let Some(preview_path) = attachment
            .preview_local_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if let Ok(preview_metadata) = std::fs::metadata(preview_path) {
                let mut preview = attachment.clone();
                preview.local_path = preview_path.to_string();
                preview.mime_type = String::from("image/jpeg");
                preview.byte_len = Some(preview_metadata.len());
                let preview_is_native = submission_attachment_is_native(
                    execution.route.protocol_route.protocol_profile_id.as_str(),
                    &execution.model.capabilities,
                    &preview,
                );
                let preview_fits_constraint = constraint.is_none_or(|constraint| {
                    constraint.reference_policy != ResourceReferencePolicy::RemoteOnly
                        && (constraint.accepted_mime_types.is_empty()
                            || constraint
                                .accepted_mime_types
                                .iter()
                                .any(|mime_type| mime_type.eq_ignore_ascii_case("image/jpeg")))
                        && constraint
                            .max_bytes
                            .is_none_or(|max_bytes| preview_metadata.len() <= max_bytes)
                });
                if preview_is_native
                    && preview_fits_constraint
                    && preview_metadata.len() <= COMPOSER_INLINE_ORIGINAL_MAX_BYTES
                {
                    return Ok((
                        ComposerAttachmentDeliveryPlan::InlineDerivedPreview,
                        "derived_image_preview_fits_inline_budget",
                    ));
                }
            }
        }
    }
    Ok((
        ComposerAttachmentDeliveryPlan::MetadataOnly,
        "selected_route_cannot_consume_attachment_bytes",
    ))
}

/// Applies frozen route count limits and the host-wide inline Base64 budget to one direct resource.
/// 对单个直接资源应用冻结路由数量限制与宿主级内联 Base64 总预算。
///
/// Parameters: `resource_kind` and `max_count` describe the frozen route constraint, while
/// `inline_raw_bytes` is present only for an inline projection; both accumulators are mutated only
/// when the resource remains directly deliverable.
/// 参数：`resource_kind` 与 `max_count` 描述冻结路由约束，`inline_raw_bytes` 仅在内联投影时存在；
/// 两个累计器只会在资源仍可直接传递时保留变更。
///
/// Returns: `None` when admitted, otherwise the stable metadata-only degradation reason.
/// 返回值：准入时返回 `None`，否则返回稳定的仅元数据降级原因。
fn apply_composer_direct_resource_limits(
    resource_kind: vulcan_ai_contract::ResourceKind,
    max_count: Option<u64>,
    inline_raw_bytes: Option<u64>,
    direct_resource_counts: &mut BTreeMap<vulcan_ai_contract::ResourceKind, u64>,
    inline_base64_bytes: &mut u64,
) -> Option<&'static str> {
    let direct_count = direct_resource_counts.entry(resource_kind).or_default();
    *direct_count = direct_count.saturating_add(1);
    if max_count.is_some_and(|limit| *direct_count > limit) {
        *direct_count = direct_count.saturating_sub(1);
        return Some("frozen_route_resource_count_exceeded");
    }
    let Some(inline_raw_bytes) = inline_raw_bytes else {
        return None;
    };
    // encodedBytes computes exact padded Base64 length without allocating the payload.
    // encodedBytes 在不分配负载的情况下计算精确的带填充 Base64 长度。
    let encoded_bytes = inline_raw_bytes
        .saturating_add(2)
        .checked_div(3)
        .unwrap_or_default()
        .saturating_mul(4);
    let next_inline_bytes = inline_base64_bytes.checked_add(encoded_bytes);
    if next_inline_bytes.is_none_or(|value| value > COMPOSER_INLINE_AGGREGATE_BASE64_MAX_BYTES) {
        *direct_count = direct_count.saturating_sub(1);
        return Some("host_inline_aggregate_budget_exceeded");
    }
    *inline_base64_bytes = next_inline_bytes.unwrap_or(*inline_base64_bytes);
    None
}

/// Applies one service-owned delivery plan to an isolated Provider attachment copy.
/// 将一项服务端持有的传输计划应用到隔离的 Provider 附件副本。
///
/// # Parameters
/// 参数
/// `attachment` is the authoritative managed source and `delivery_plan` was computed from the
/// exact frozen execution route.
/// `attachment` 是权威托管源，`delivery_plan` 由精确冻结执行路线计算得到。
///
/// # Returns
/// 返回值
/// A request-local copy whose direct source and delivery mode agree, or a metadata-only copy when
/// a derived preview disappeared after planning.
/// 返回直接源与传输模式一致的单次请求局部副本；若派生预览在规划后消失，则返回仅元数据副本。
fn apply_composer_delivery_plan_to_attachment(
    attachment: &ConversationAttachment,
    delivery_plan: ComposerAttachmentDeliveryPlan,
) -> ConversationAttachment {
    let mut projected = attachment.clone();
    let delivery_mode = match delivery_plan {
        ComposerAttachmentDeliveryPlan::InlineOriginal => {
            ConversationAttachmentDeliveryMode::InlineOriginal
        }
        ComposerAttachmentDeliveryPlan::UploadReference => {
            ConversationAttachmentDeliveryMode::UploadReference
        }
        ComposerAttachmentDeliveryPlan::MetadataOnly => {
            ConversationAttachmentDeliveryMode::MetadataOnly
        }
        ComposerAttachmentDeliveryPlan::InlineDerivedPreview => {
            let Some(preview_path) = attachment
                .preview_local_path
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                projected.delivery_mode = Some(ConversationAttachmentDeliveryMode::MetadataOnly);
                return projected;
            };
            let Ok(preview_metadata) = std::fs::metadata(preview_path) else {
                projected.delivery_mode = Some(ConversationAttachmentDeliveryMode::MetadataOnly);
                return projected;
            };
            projected.local_path = preview_path.to_string();
            projected.file_name = format!("{}.vulcan-preview.jpg", attachment.file_name);
            projected.mime_type = String::from("image/jpeg");
            projected.byte_len = Some(preview_metadata.len());
            projected.source_sha256 = None;
            ConversationAttachmentDeliveryMode::InlineDerivedPreview
        }
    };
    projected.delivery_mode = Some(delivery_mode);
    projected
}

/// Recomputes every direct attachment projection from one exact frozen Provider execution.
/// 从一项精确冻结的 Provider 执行重新计算每个直接附件投影。
///
/// # Parameters
/// 参数
/// `execution` owns the exact route, model, account, MIME constraints, and upload contract;
/// `attachments` contain authoritative source facts plus an optional metadata-only reduction.
/// `execution` 持有精确路线、模型、账号、MIME 约束与上传契约；`attachments` 包含权威源事实以及
/// 可选的仅元数据降级请求。
///
/// # Returns
/// 返回值
/// Provider-facing copies that obey route count limits and the request-wide inline Base64 budget.
/// 返回遵守路线数量限制与请求级内联 Base64 预算的 Provider 侧副本。
fn project_composer_attachments_for_execution(
    execution: &vulcan_provider_adapters::ResolvedPublicAiExecution,
    attachments: &[ConversationAttachment],
) -> Result<Vec<ConversationAttachment>, String> {
    let mut direct_resource_counts = BTreeMap::new();
    let mut inline_base64_bytes = 0_u64;
    attachments
        .iter()
        .map(|attachment| {
            // metadataOnly is the sole browser-originated reduction accepted by this boundary;
            // every mode that exposes bytes is recomputed below.
            // metadataOnly 是此边界唯一接受的浏览器来源降级；每种暴露字节的模式均在下方重算。
            if attachment.delivery_mode == Some(ConversationAttachmentDeliveryMode::MetadataOnly) {
                return Ok(apply_composer_delivery_plan_to_attachment(
                    attachment,
                    ComposerAttachmentDeliveryPlan::MetadataOnly,
                ));
            }
            let (mut delivery_plan, _) = composer_attachment_delivery_plan(execution, attachment)?;
            if delivery_plan != ComposerAttachmentDeliveryPlan::MetadataOnly {
                let resource_kind = attachment_kind(attachment.kind.clone());
                let inline_raw_bytes = match delivery_plan {
                    ComposerAttachmentDeliveryPlan::InlineOriginal => attachment.byte_len,
                    ComposerAttachmentDeliveryPlan::InlineDerivedPreview => attachment
                        .preview_local_path
                        .as_deref()
                        .and_then(|path| std::fs::metadata(path).ok())
                        .map(|metadata| metadata.len()),
                    ComposerAttachmentDeliveryPlan::UploadReference
                    | ComposerAttachmentDeliveryPlan::MetadataOnly => None,
                };
                let max_count = execution
                    .route
                    .protocol_route
                    .input_resource_constraints
                    .iter()
                    .find(|constraint| constraint.resource_kind == resource_kind)
                    .and_then(|constraint| constraint.max_count);
                if apply_composer_direct_resource_limits(
                    resource_kind,
                    max_count,
                    inline_raw_bytes,
                    &mut direct_resource_counts,
                    &mut inline_base64_bytes,
                )
                .is_some()
                {
                    delivery_plan = ComposerAttachmentDeliveryPlan::MetadataOnly;
                }
            }
            Ok(apply_composer_delivery_plan_to_attachment(
                attachment,
                delivery_plan,
            ))
        })
        .collect()
}

/// Parses an optional composer conversation identity without silently changing its route scope.
/// 解析可选输入区会话身份，且不会静默改变其路由作用域。
///
/// # Parameters
/// # 参数
/// `conversation_id` is the exact owner supplied to draft planning or preparation.
/// `conversation_id` 是草稿规划或准备收到的精确所有者。
///
/// # Returns
/// # 返回值
/// `None` for a true local draft, one session id for a persisted conversation, or an explicit error.
/// 对真正本地草稿返回 `None`，对持久化会话返回会话标识，否则返回显式错误。
fn composer_media_session_id(conversation_id: Option<&str>) -> Result<Option<SessionId>, String> {
    conversation_id
        .map(|conversation_id| {
            parse_session_id(conversation_id).ok_or_else(|| {
                String::from("Composer media preparation received an invalid conversation id.")
            })
        })
        .transpose()
}

/// Projects the persisted session route fields used to freeze one draft media execution.
/// 投影用于冻结一条草稿媒体执行的持久化会话路由字段。
///
/// # Parameters
/// # 参数
/// `state` owns the current workspace snapshot and `session_id` selects the exact conversation.
/// `state` 持有当前工作区快照，`session_id` 选择精确会话。
///
/// # Returns
/// # 返回值
/// `None` for a local draft or a bounded route-only runtime projection for an existing conversation.
/// 对本地草稿返回 `None`，对既有会话返回有界的仅路由运行态投影。
fn composer_media_session_route_state(
    state: &SharedServiceBridgeState,
    session_id: Option<SessionId>,
) -> Result<Option<vulcan_workbench_model::SessionRuntimeStateEntry>, String> {
    let Some(session_id) = session_id else {
        return Ok(None);
    };
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let workspace_snapshot = guard.workspace_snapshot.as_ref().ok_or_else(|| {
        String::from("Composer media preparation requires a loaded workspace snapshot.")
    })?;
    // runtime_state is borrowed only for the four route fields consumed by resolution below.
    // runtime_state 仅为下方解析消费的四个路由字段提供借用。
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .ok_or_else(|| {
            String::from("Composer media preparation session runtime state is missing.")
        })?;
    Ok(Some(vulcan_workbench_model::SessionRuntimeStateEntry {
        active_provider_id: runtime_state.active_provider_id.clone(),
        active_connection_id: runtime_state.active_connection_id.clone(),
        active_model_id: runtime_state.active_model_id.clone(),
        provider_route_lease: runtime_state.provider_route_lease.clone(),
        ..Default::default()
    }))
}

/// Plans every staged attachment without performing network I/O or mutating conversation state.
/// 在不执行网络 I/O 或修改会话状态的情况下规划全部已暂存附件。
pub(crate) async fn service_plan_composer_attachments(
    state: SharedServiceBridgeState,
    project_id: Option<String>,
    conversation_id: Option<String>,
    generation: u64,
    model_selection: SubmissionModelSelection,
    attachments: Vec<ComposerAttachmentSummary>,
) -> Result<Vec<ComposerAttachmentPreparationSummary>, String> {
    let workspace_path = composer_attachment_workspace_path(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?;
    let (_, agent_runtime, _, _, _) = runtime_parts_from_state(&state)?;
    let session_id = composer_media_session_id(conversation_id.as_deref())?;
    let session_runtime_state = composer_media_session_route_state(&state, session_id)?;
    let execution = Arc::new(
        resolve_submission_media_execution(
            &agent_runtime,
            session_id,
            session_runtime_state.as_ref(),
            Some(&model_selection),
        )
        .await?,
    );
    let mut summaries = Vec::with_capacity(attachments.len());
    // uploadPlans are published only after every attachment has been validated and planned.
    // uploadPlans 仅在全部附件完成校验与规划后统一发布。
    let mut upload_plans = Vec::new();
    // directResourceCounts applies each route's request-wide limit before any draft upload starts.
    // directResourceCounts 在任何草稿上传开始前应用每条路线的请求级数量上限。
    let mut direct_resource_counts = BTreeMap::<vulcan_ai_contract::ResourceKind, u64>::new();
    // inlineBase64Bytes applies one request-wide host budget after exact Base64 expansion.
    // inlineBase64Bytes 在精确 Base64 膨胀后应用请求级宿主预算。
    let mut inline_base64_bytes = 0u64;
    for summary in attachments {
        let attachment = composer_attachment_summary_to_core(
            resolve_composer_attachment_summary_for_workspace(&workspace_path, summary)?,
        )?;
        let attachment_id = attachment
            .asset_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| String::from("Composer attachment is missing its stable asset id."))?
            .to_string();
        let (mut delivery_plan, mut reason) =
            composer_attachment_delivery_plan(execution.as_ref(), &attachment)?;
        let resource_kind = attachment_kind(attachment.kind.clone());
        let inline_raw_bytes = match delivery_plan {
            ComposerAttachmentDeliveryPlan::InlineOriginal => attachment.byte_len,
            ComposerAttachmentDeliveryPlan::InlineDerivedPreview => attachment
                .preview_local_path
                .as_deref()
                .and_then(|path| std::fs::metadata(path).ok())
                .map(|metadata| metadata.len()),
            ComposerAttachmentDeliveryPlan::UploadReference
            | ComposerAttachmentDeliveryPlan::MetadataOnly => None,
        };
        if delivery_plan != ComposerAttachmentDeliveryPlan::MetadataOnly {
            let max_count = execution
                .route
                .protocol_route
                .input_resource_constraints
                .iter()
                .find(|constraint| constraint.resource_kind == resource_kind)
                .and_then(|constraint| constraint.max_count);
            if let Some(limit_reason) = apply_composer_direct_resource_limits(
                resource_kind,
                max_count,
                inline_raw_bytes,
                &mut direct_resource_counts,
                &mut inline_base64_bytes,
            ) {
                delivery_plan = ComposerAttachmentDeliveryPlan::MetadataOnly;
                reason = limit_reason;
            }
        }
        let phase = match delivery_plan {
            ComposerAttachmentDeliveryPlan::UploadReference => {
                ComposerAttachmentPreparationPhase::UploadQueued
            }
            ComposerAttachmentDeliveryPlan::MetadataOnly => {
                ComposerAttachmentPreparationPhase::MetadataOnly
            }
            ComposerAttachmentDeliveryPlan::InlineOriginal
            | ComposerAttachmentDeliveryPlan::InlineDerivedPreview => {
                ComposerAttachmentPreparationPhase::LocalReady
            }
        };
        let transfer_id = format!("draft-media:{}", uuid::Uuid::new_v4());
        if delivery_plan == ComposerAttachmentDeliveryPlan::UploadReference {
            upload_plans.push((
                transfer_id.clone(),
                DraftMediaUploadPlan {
                    identity: DraftMediaUploadPlanIdentity {
                        workspace_path: workspace_path.clone(),
                        conversation_id: conversation_id.clone(),
                        model_selection: model_selection.clone(),
                        attachment_id: attachment_id.clone(),
                        generation,
                        route_revision: execution.route.route_revision.clone(),
                    },
                    execution: Arc::clone(&execution),
                    created_at: std::time::Instant::now(),
                },
            ));
        }
        summaries.push(ComposerAttachmentPreparationSummary {
            attachment_id,
            generation,
            transfer_id,
            delivery_plan,
            phase,
            blocks_submission: delivery_plan == ComposerAttachmentDeliveryPlan::UploadReference,
            route_revision: Some(execution.route.route_revision.clone()),
            reason: reason.to_string(),
            error: None,
        });
    }
    register_draft_media_upload_plans(upload_plans)?;
    Ok(summaries)
}

/// Executes one previously planned provider upload and returns a closed draft state.
/// 执行一个先前规划的供应商上传并返回封闭草稿状态。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn service_prepare_composer_attachment(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    project_id: Option<String>,
    conversation_id: Option<String>,
    model_selection: SubmissionModelSelection,
    attachment: ComposerAttachmentSummary,
    preparation: ComposerAttachmentPreparationSummary,
) -> Result<ComposerAttachmentPreparationSummary, String> {
    if preparation.delivery_plan != ComposerAttachmentDeliveryPlan::UploadReference {
        return Err(String::from(
            "Composer attachment preparation accepts only upload_reference plans.",
        ));
    }
    let workspace_path = composer_attachment_workspace_path(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?;
    let attachment = composer_attachment_summary_to_core(
        resolve_composer_attachment_summary_for_workspace(&workspace_path, attachment)?,
    )?;
    if attachment.asset_id.as_deref() != Some(preparation.attachment_id.as_str()) {
        return Err(String::from(
            "Composer attachment preparation identity does not match the staged asset.",
        ));
    }
    // Runtime and session validation precede plan consumption so a rejected request cannot strand a pending upload registration.
    // 运行时与会话校验先于计划消费，避免被拒绝的请求遗留无法回收的待上传登记。
    let (_, agent_runtime, _, _, _) = runtime_parts_from_state(&state)?;
    let session_id = composer_media_session_id(conversation_id.as_deref())?;
    let (frozen_plan, cancellation) = consume_draft_media_upload_plan(
        preparation.transfer_id.as_str(),
        workspace_path.as_str(),
        &conversation_id,
        &model_selection,
        &preparation,
    )?;
    let result = prepare_submission_media_with_frozen_execution(
        &app,
        &state,
        &agent_runtime,
        preparation.transfer_id.as_str(),
        session_id,
        None,
        Some(&model_selection),
        std::slice::from_ref(&attachment),
        frozen_plan.execution,
        cancellation,
    )
    .await;
    match result {
        Ok(prepared) => {
            for reference in &prepared.references {
                vulcan_agent_runtime::attachment_asset_index::record_attachment_media_reference(
                    &attachment,
                    attachment_media_reference_from_prepared(reference),
                )?;
            }
            Ok(ComposerAttachmentPreparationSummary {
                phase: ComposerAttachmentPreparationPhase::DirectReady,
                blocks_submission: false,
                error: None,
                ..preparation
            })
        }
        Err(error) if error.contains("cancelled") => Ok(ComposerAttachmentPreparationSummary {
            phase: ComposerAttachmentPreparationPhase::Cancelled,
            blocks_submission: false,
            error: None,
            ..preparation
        }),
        Err(error) => Ok(ComposerAttachmentPreparationSummary {
            phase: ComposerAttachmentPreparationPhase::Failed,
            blocks_submission: false,
            reason: String::from("provider_upload_failed_metadata_only"),
            error: Some(error),
            ..preparation
        }),
    }
}

/// Builds one complete progress payload from immutable route identity and current byte facts.
/// 从不可变路由身份与当前字节事实构建一条完整进度载荷。
///
/// # Parameters
/// 参数
/// `identity` is the frozen route; the remaining arguments are exact submission and file state.
/// `identity` 是冻结路由；其余参数是精确提交与文件状态。
///
/// # Returns
/// 返回值
/// One browser-safe progress delta with the same trace identity as operational telemetry.
/// 返回一条浏览器安全进度增量，其追踪身份与运行遥测完全一致。
#[allow(clippy::too_many_arguments)]
fn media_upload_progress_delta(
    identity: &MediaUploadTelemetryIdentity,
    client_submission_id: &str,
    conversation_id: Option<&str>,
    attachment_id: Option<&str>,
    file_name: &str,
    media_kind: &str,
    file_index: usize,
    file_count: usize,
    phase: MediaUploadPhase,
    bytes_sent: u64,
    total_bytes: Option<u64>,
    error: Option<String>,
) -> MediaUploadProgressChangedDelta {
    MediaUploadProgressChangedDelta {
        client_submission_id: client_submission_id.to_string(),
        conversation_id: conversation_id.map(str::to_string),
        attachment_id: attachment_id.map(str::to_string),
        provider_id: identity.provider_id.clone(),
        service_entry_id: identity.service_entry_id.clone(),
        connection_id: identity.connection_id.clone(),
        model_id: identity.model_id.clone(),
        protocol_profile_id: identity.protocol_profile_id.clone(),
        manifest_revision: identity.manifest_revision,
        manifest_generation: identity.manifest_generation,
        route_revision: identity.route_revision.clone(),
        operation_id: CONVERSATION_MEDIA_INPUT_OPERATION_ID.to_string(),
        upload_channel_id: identity.upload_channel_id.clone(),
        file_name: file_name.to_string(),
        media_kind: media_kind.to_string(),
        file_index,
        file_count,
        phase,
        bytes_sent,
        total_bytes,
        error,
        cancellable: true,
    }
}

/// Installs the service bridge for media uploads started by post-dispatch understanding tools.
/// 为分派后理解工具启动的媒体上传安装服务桥接。
///
/// # Parameters
/// 参数
/// `runtime` owns the process-local observer slot; `app` and `state` emit ordered workbench deltas.
/// `runtime` 持有进程内观察器槽位；`app` 与 `state` 发出有序工作台增量。
///
/// # Returns
/// 返回值
/// Success after installing the observer, or a controlled runtime observer error.
/// 安装观察器后返回成功，或返回受控运行时观察器错误。
pub(crate) fn install_runtime_media_upload_progress_bridge(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
) -> Result<(), String> {
    runtime.set_runtime_media_upload_progress_observer(Some(Arc::new(move |progress| {
        // phase maps the provider-neutral runtime lifecycle into the public service contract.
        // phase 将供应商中立运行时生命周期映射到公开服务契约。
        let phase = match progress.phase {
            vulcan_provider_adapters::RuntimeMediaUploadPhase::Preparing => {
                MediaUploadPhase::Preparing
            }
            vulcan_provider_adapters::RuntimeMediaUploadPhase::Uploading => {
                MediaUploadPhase::Uploading
            }
            vulcan_provider_adapters::RuntimeMediaUploadPhase::Completed => {
                MediaUploadPhase::Completed
            }
            vulcan_provider_adapters::RuntimeMediaUploadPhase::Failed => MediaUploadPhase::Failed,
            vulcan_provider_adapters::RuntimeMediaUploadPhase::Cancelled => {
                MediaUploadPhase::Cancelled
            }
        };
        emit_media_upload_progress(
            &app,
            &state,
            MediaUploadProgressChangedDelta {
                client_submission_id: progress.transfer_id,
                conversation_id: Some(progress.owner_session_id),
                attachment_id: None,
                provider_id: progress.provider_id,
                service_entry_id: progress.service_entry_id,
                connection_id: progress.connection_id,
                model_id: progress.model_id,
                protocol_profile_id: progress.protocol_profile_id,
                manifest_revision: progress.manifest_revision,
                manifest_generation: progress.manifest_generation,
                route_revision: progress.route_revision,
                operation_id: progress.operation_id,
                upload_channel_id: progress.upload_channel_id,
                file_name: progress.file_name,
                media_kind: progress.media_kind,
                file_index: 0,
                file_count: 1,
                phase,
                bytes_sent: progress.bytes_sent,
                total_bytes: progress.total_bytes,
                error: progress.error,
                cancellable: false,
            },
        );
    })))
}

/// Emits one low-frequency upload lifecycle event without file names, paths, URLs, or bodies.
/// 发出一条不含文件名、路径、URL 或正文的低频上传生命周期事件。
///
/// # Parameters
/// 参数
/// `identity` is the exact frozen route; `event_name` is a closed lifecycle point and
/// `failure_kind` is a stable host category for terminal failures.
/// `identity` 是精确冻结路由；`event_name` 是封闭生命周期点，`failure_kind` 是终态失败的
/// 稳定宿主分类。
///
/// # Returns
/// 返回值
/// Returns no value because telemetry diagnostics cannot change provider execution outcome.
/// 不返回值，因为遥测诊断不能改变供应商执行结果。
fn emit_media_operational_telemetry(
    identity: &MediaUploadTelemetryIdentity,
    event_name: ProviderOperationalTelemetryEventName,
    failure_kind: Option<&str>,
) {
    let mut event = ProviderOperationalTelemetryEvent::new(event_name);
    event.provider_id = Some(identity.provider_id.clone());
    event.service_entry_id = Some(identity.service_entry_id.clone());
    event.connection_id = Some(identity.connection_id.clone());
    event.model_id = Some(identity.model_id.clone());
    event.protocol_profile_id = Some(identity.protocol_profile_id.clone());
    event.route_revision = Some(identity.route_revision.clone());
    event.manifest_revision = Some(identity.manifest_revision);
    event.manifest_generation = Some(identity.manifest_generation);
    event.request_profiles = identity.request_profiles.clone();
    event.response_profiles = identity.response_profiles.clone();
    event.operation_id = Some(CONVERSATION_MEDIA_INPUT_OPERATION_ID.to_string());
    event.upload_channel_id = Some(identity.upload_channel_id.clone());
    event.failure_kind = failure_kind.map(str::to_string);
    if let Err(error) = record_provider_operational_telemetry(event) {
        eprintln!("[media-upload] structured telemetry was rejected: {error}");
    }
}

/// Prepares every local attachment before session materialization and provider dispatch.
/// 在会话物化与供应商分派前准备所有本地附件。
///
/// # Parameters
/// 参数
/// `app` and `state` carry ordered progress events without persisting ephemeral byte counters.
/// `app` 与 `state` 发送有序进度事件，但不持久化临时字节计数。
/// `agent_runtime` resolves the exact selected account and provider route; `client_submission_id`
/// owns cancellation and progress; `session_id` scopes the immutable route lease;
/// `session_runtime_state` supplies the persisted lease identity; `model_selection` is the exact
/// composer snapshot; and `attachments` are the already validated core attachments.
/// `agent_runtime` 解析精确选定的账号与供应商路由；`client_submission_id` 持有取消与进度；
/// `session_id` 约束不可变路由租约；`session_runtime_state` 提供持久化租约身份；
/// `model_selection` 是精确输入区快照；`attachments` 是已经校验的核心附件。
///
/// # Returns
/// 返回值
/// One authoritative Provider attachment projection and every prepared reference produced from the
/// same frozen execution, or an explicit lease, capability, credential, upload, cancellation, or
/// projection error.
/// 返回从同一冻结执行生成的一份权威 Provider 附件投影与全部已准备引用；否则返回明确的租约、能力、
/// 凭据、上传、取消或投影错误。
pub(crate) async fn prepare_submission_media(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    client_submission_id: &str,
    session_id: Option<SessionId>,
    session_runtime_state: Option<&vulcan_workbench_model::SessionRuntimeStateEntry>,
    model_selection: Option<&SubmissionModelSelection>,
    attachments: &[ConversationAttachment],
) -> Result<PreparedSubmissionMediaProjection, String> {
    if attachments.is_empty()
        && !session_runtime_state.is_some_and(session_runtime_state_has_attachment)
    {
        return Ok(PreparedSubmissionMediaProjection::default());
    }
    let execution = Arc::new(
        resolve_submission_media_execution(
            agent_runtime,
            session_id,
            session_runtime_state,
            model_selection,
        )
        .await?,
    );
    let provider_attachments =
        project_composer_attachments_for_execution(execution.as_ref(), attachments)?;
    let cancellation = register_media_upload(client_submission_id)?;
    let prepared_media = prepare_submission_media_with_frozen_execution(
        app,
        state,
        agent_runtime,
        client_submission_id,
        session_id,
        session_runtime_state,
        model_selection,
        provider_attachments.as_slice(),
        execution,
        cancellation,
    )
    .await?;
    Ok(PreparedSubmissionMediaProjection {
        provider_attachments,
        prepared_media,
    })
}

/// Executes media preparation with a service-issued frozen route and live credential lease.
/// 使用服务签发的冻结路由与实时凭据租约执行媒体准备。
///
/// # Parameters
/// # 参数
/// The common preparation arguments retain ownership and progress scope; `execution` is the exact
/// contract captured by the immediately preceding draft-planning transaction.
/// 通用准备参数保留所有权与进度作用域；`execution` 是紧邻的草稿规划事务捕获的精确契约。
///
/// # Returns
/// # 返回值
/// Prepared internal references, or one explicit cancellation, upload, or contract error.
/// 返回已准备的内部引用，或一项显式取消、上传或契约错误。
#[allow(clippy::too_many_arguments)]
async fn prepare_submission_media_with_frozen_execution(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    client_submission_id: &str,
    session_id: Option<SessionId>,
    session_runtime_state: Option<&vulcan_workbench_model::SessionRuntimeStateEntry>,
    model_selection: Option<&SubmissionModelSelection>,
    attachments: &[ConversationAttachment],
    execution: Arc<vulcan_provider_adapters::ResolvedPublicAiExecution>,
    cancellation: MediaUploadCancellation,
) -> Result<PreparedSubmissionMedia, String> {
    let result = prepare_submission_media_inner(
        app,
        state,
        agent_runtime,
        client_submission_id,
        session_id,
        session_runtime_state,
        model_selection,
        attachments,
        &cancellation,
        Some(execution),
    )
    .await;
    remove_media_upload(client_submission_id);
    result
}

/// Commits prepared video references together with their canonical user-message owner.
/// 将已准备视频引用与其规范用户消息所有者一并提交。
pub(crate) fn commit_prepared_submission_media_ledger(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    owner_message_id: Option<&str>,
    prepared: PreparedSubmissionMedia,
) -> Result<(), String> {
    if prepared.references.is_empty() {
        return Ok(());
    }
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .ok_or_else(|| String::from("Media reference ledger session runtime state is missing."))?;
    for reference in prepared.references {
        // A reference without a historical owner and without a newly admitted canonical owner
        // is request-scoped only and must not become an orphaned durable ledger entry.
        // 没有历史所有者且没有新准入规范所有者的引用仅属于请求作用域，不得成为孤立持久账本条目。
        if reference.owner_message_ids.is_empty()
            && (!reference.attach_current_owner || owner_message_id.is_none())
        {
            continue;
        }
        runtime_state
            .media_reference_ledger
            .upsert(reference.into_ledger_entry(owner_message_id));
    }
    runtime_state.updated_at = current_unix_millis_text();
    Ok(())
}

/// Resolves media preparation through the same immutable session route as provider dispatch.
/// 通过与供应商分派相同的不可变会话路由解析媒体准备执行。
///
/// # Parameters
/// 参数
/// `agent_runtime` owns current catalogs and secrets; session inputs identify the lease owner and
/// persisted identity; `model_selection` is the exact per-submit UI snapshot.
/// `agent_runtime` 持有当前目录与秘密；会话输入标识租约所有者与持久化身份；
/// `model_selection` 是精确的逐次提交界面快照。
///
/// # Returns
/// 返回值
/// A credential-bearing execution built from the frozen session selection, or from current
/// configuration only for a draft without a session owner.
/// 返回从冻结会话选择构建的携带凭据执行；仅无会话所有者的草稿从当前配置构建。
async fn resolve_submission_media_execution(
    agent_runtime: &AgentRuntimeBridge,
    session_id: Option<SessionId>,
    session_runtime_state: Option<&vulcan_workbench_model::SessionRuntimeStateEntry>,
    model_selection: Option<&SubmissionModelSelection>,
) -> Result<vulcan_provider_adapters::ResolvedPublicAiExecution, String> {
    // requestTarget follows the explicit submission snapshot, then queued-runtime overrides,
    // then the persisted session target. Partial targets remain explicit errors.
    // requestTarget 依次采用显式提交快照、排队运行时覆盖和持久化会话目标；不完整目标保持显式错误。
    let request_target = if let Some(selection) = model_selection {
        (
            Some(selection.provider_id.as_str()),
            Some(selection.model_id.as_str()),
            Some(selection.connection_id.as_str()),
        )
    } else if agent_runtime.provider_id_override.is_some()
        || agent_runtime.model_id_override.is_some()
        || agent_runtime.connection_id_override.is_some()
    {
        (
            agent_runtime.provider_id_override.as_deref(),
            agent_runtime.model_id_override.as_deref(),
            agent_runtime.connection_id_override.as_deref(),
        )
    } else {
        (
            session_runtime_state.and_then(|state| state.active_provider_id.as_deref()),
            session_runtime_state.and_then(|state| state.active_model_id.as_deref()),
            session_runtime_state.and_then(|state| state.active_connection_id.as_deref()),
        )
    };
    if let Some(session_id) = session_id {
        // blueprint installs or restores exactly one session lease and validates persisted identity
        // before any source file is read or uploaded.
        // blueprint 在读取或上传任何源文件前安装或恢复唯一会话租约，并校验持久化身份。
        let blueprint =
            vulcan_agent_runtime::agent_runtime::provider::resolve_public_ai_provider_blueprint(
                agent_runtime.public_ai_runtime.as_ref(),
                session_id.to_string().as_str(),
                session_runtime_state.and_then(|state| state.provider_route_lease.as_ref()),
                ReasoningEffortLevel::Off,
                request_target.0,
                request_target.1,
                request_target.2,
            )?;
        let selection = blueprint.frozen_selection.ok_or_else(|| {
            String::from("Media preparation did not freeze a session provider route.")
        })?;
        return agent_runtime
            .public_ai_runtime
            .resolve_execution_from_frozen_selection(selection)
            .await;
    }

    let configuration = agent_runtime.public_ai_runtime.configuration_snapshot()?;
    match request_target {
        (Some(provider_id), Some(model_id), Some(connection_id)) => {
            agent_runtime
                .public_ai_runtime
                .resolve_connection_model_execution(
                    &configuration,
                    connection_id,
                    provider_id,
                    model_id,
                )
                .await
        }
        (Some(provider_id), Some(model_id), None) => {
            agent_runtime
                .public_ai_runtime
                .resolve_model_execution(&configuration, provider_id, model_id)
                .await
        }
        (None, None, None) => {
            agent_runtime
                .public_ai_runtime
                .resolve_active_execution(&configuration)
                .await
        }
        _ => Err(String::from(
            "Media preparation requires provider_id and model_id together; connection_id is optional only with both.",
        )),
    }
}

/// Executes the cancellable attachment loop while the outer registration remains visible to RPC callers.
/// 在外层注册仍对 RPC 调用方可见期间执行可取消的附件循环。
///
/// # Parameters
/// 参数
/// `app`, `state`, and `client_submission_id` publish ordered progress; `agent_runtime`,
/// `session_id`, `session_runtime_state`, and `model_selection` resolve the frozen execution;
/// `attachments` are exact source resources; and `cancellation` is the registered submission token.
/// `app`、`state` 与 `client_submission_id` 发布有序进度；`agent_runtime`、`session_id`、
/// `session_runtime_state` 与 `model_selection` 解析冻结执行；`attachments` 是精确源资源；
/// `cancellation` 是已注册的提交取消令牌。
///
/// # Returns
/// 返回值
/// Success after every attachment crosses its required preparation path, or the first explicit error.
/// 全部附件越过其必需准备路径后返回成功；否则返回首个明确错误。
async fn prepare_submission_media_inner(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    client_submission_id: &str,
    session_id: Option<SessionId>,
    session_runtime_state: Option<&vulcan_workbench_model::SessionRuntimeStateEntry>,
    model_selection: Option<&SubmissionModelSelection>,
    attachments: &[ConversationAttachment],
    cancellation: &MediaUploadCancellation,
    frozen_execution: Option<Arc<vulcan_provider_adapters::ResolvedPublicAiExecution>>,
) -> Result<PreparedSubmissionMedia, String> {
    let execution = match frozen_execution {
        Some(execution) => execution,
        None => Arc::new(
            resolve_submission_media_execution(
                agent_runtime,
                session_id,
                session_runtime_state,
                model_selection,
            )
            .await?,
        ),
    };
    // conversationId is derived from the parsed session owner and cannot diverge from lease scope.
    // conversationId 从已解析会话所有者派生，无法与租约作用域产生偏离。
    let conversation_id = session_id.map(|session_id| session_id.to_string());
    let provider_id = execution.route.provider.id.clone();
    let model_id = execution.model.id.clone();
    let preparation_attachments =
        collect_media_preparation_attachments(session_runtime_state, attachments);
    let file_count = preparation_attachments.len();
    let mut prepared_submission_media = PreparedSubmissionMedia::default();
    for (file_index, preparation_attachment) in preparation_attachments.iter().enumerate() {
        let attachment = &preparation_attachment.attachment;
        let media_kind = attachment_kind_text(attachment.kind.clone());
        let total_bytes = attachment.byte_len;
        let resource_kind = attachment_kind(attachment.kind.clone());
        if attachment.delivery_mode == Some(ConversationAttachmentDeliveryMode::MetadataOnly) {
            continue;
        }
        // Bridged attachments, including MIME formats unsupported by the selected wire encoder,
        // remain local until the fixed understanding tool is actually invoked.
        // 桥接附件（包括所选 Wire 编码器不支持的 MIME 格式）保持本地，直到固定理解工具被实际调用。
        let native_media = submission_attachment_is_native(
            execution.route.protocol_route.protocol_profile_id.as_str(),
            &execution.model.capabilities,
            attachment,
        );
        if !native_media {
            continue;
        }
        let upload_contract = execution.client.conversation_media_upload_contract(
            &execution.route,
            resource_kind,
            attachment.mime_type.as_str(),
        )?;
        let Some(upload_contract) = upload_contract else {
            continue;
        };
        // hostUploadRequired obeys an authoritative request-local projection when present; canonical
        // history without a transient mode is recomputed through the same exact route planner.
        // hostUploadRequired 在存在权威单次请求投影时严格服从该投影；缺少临时模式的规范历史通过
        // 同一个精确路线规划器重新计算。
        let host_upload_required = match attachment.delivery_mode {
            Some(ConversationAttachmentDeliveryMode::UploadReference) => true,
            Some(ConversationAttachmentDeliveryMode::InlineOriginal)
            | Some(ConversationAttachmentDeliveryMode::InlineDerivedPreview) => false,
            Some(ConversationAttachmentDeliveryMode::MetadataOnly) => false,
            None => {
                composer_attachment_delivery_plan(execution.as_ref(), attachment)?.0
                    == ComposerAttachmentDeliveryPlan::UploadReference
            }
        };
        if !host_upload_required {
            continue;
        }
        let session_scope_id = session_id.map(|session_id| session_id.to_string());
        if upload_contract.reference_scope
            == vulcan_provider_adapters::UploadReferenceScope::Session
            && session_scope_id.is_none()
        {
            return Err(format!(
                "Upload channel `{}` requires a session identity before media preparation.",
                upload_contract.upload_channel_id
            ));
        }
        let Some(media_source_path) =
            media_preparation_replay_path(attachment, preparation_attachment.attach_current_owner)?
        else {
            // Historical unavailable media remains canonical and is downgraded to metadata by the
            // final provider-input projection; it must not abort a new user turn.
            // 历史不可用媒体仍保留在规范历史中，并由最终 Provider 输入投影降级为元数据；它不得中止
            // 新的用户轮次。
            continue;
        };
        let source_identity = execution
            .client
            .read_media_source_identity(media_source_path.as_path(), Some(cancellation))
            .await?;
        // The managed source is immutable. A changed length or digest invalidates both reuse and a
        // fresh upload, while final request normalization degrades the canonical attachment to metadata.
        // 托管源是不可变的。长度或摘要变化会同时使复用与新上传失效，最终请求归一化会把规范附件
        // 降级为仅元数据。
        if attachment.byte_len != Some(source_identity.byte_len)
            || attachment.source_sha256.as_deref() != Some(source_identity.content_sha256.as_str())
        {
            if preparation_attachment.attach_current_owner {
                return Err(format!(
                    "Attachment `{}` changed after it was staged and cannot be prepared.",
                    attachment.file_name
                ));
            }
            continue;
        }
        let now_unix_millis = current_unix_millis_u64();
        let reference_scope = session_media_reference_scope(upload_contract.reference_scope);
        if attachment.source_sha256.as_deref() == Some(source_identity.content_sha256.as_str())
            && attachment.byte_len == Some(source_identity.byte_len)
        {
            // indexedQuery contains the exact route key but no Provider reference value.
            // indexedQuery 包含精确路线键，但不包含供应商引用值。
            let indexed_query =
                vulcan_agent_runtime::attachment_asset_index::AttachmentMediaReferenceRecord {
                    source_sha256: source_identity.content_sha256.clone(),
                    source_byte_len: source_identity.byte_len,
                    provider_id: provider_id.clone(),
                    service_entry_id: execution.route.entry.id.clone(),
                    upload_channel_id: upload_contract.upload_channel_id.clone(),
                    upload_contract_fingerprint: upload_contract.contract_fingerprint.clone(),
                    reference_scope,
                    connection_id: match reference_scope {
                        SessionMediaReferenceScope::Credential => {
                            Some(execution.route.connection.id.clone())
                        }
                        SessionMediaReferenceScope::ProviderProtocol
                        | SessionMediaReferenceScope::Session => None,
                    },
                    session_id: match reference_scope {
                        SessionMediaReferenceScope::Session => session_scope_id.clone(),
                        SessionMediaReferenceScope::ProviderProtocol
                        | SessionMediaReferenceScope::Credential => None,
                    },
                    resource_kind: resource_kind_text(resource_kind).to_string(),
                    mime_type: attachment.mime_type.clone(),
                    provider_reference: String::new(),
                    created_at_unix_millis: 0,
                    expires_at_unix_millis: u64::MAX,
                };
            if let Some(indexed_reference) =
                vulcan_agent_runtime::attachment_asset_index::find_attachment_media_reference(
                    attachment,
                    &indexed_query,
                    now_unix_millis,
                )?
            {
                execution
                    .client
                    .remember_persisted_media_reference(
                        &execution.route,
                        media_source_path.as_path(),
                        resource_kind,
                        attachment.mime_type.as_str(),
                        indexed_reference.provider_reference.clone(),
                        indexed_reference.source_sha256.as_str(),
                        indexed_reference.source_byte_len,
                        session_scope_id.as_deref(),
                        indexed_reference.created_at_unix_millis,
                        indexed_reference.expires_at_unix_millis,
                    )
                    .await?;
                prepared_submission_media.push_reference(
                    prepared_media_reference_from_attachment_index(
                        &indexed_reference,
                        preparation_attachment.attach_current_owner,
                    ),
                );
                continue;
            }
        }
        let source_ledger_reference = session_runtime_state
            .and_then(|runtime_state| {
                runtime_state.media_reference_ledger.find_usable(
                    source_identity.content_sha256.as_str(),
                    source_identity.byte_len,
                    provider_id.as_str(),
                    execution.route.entry.id.as_str(),
                    upload_contract.upload_channel_id.as_str(),
                    upload_contract.contract_fingerprint.as_str(),
                    reference_scope,
                    execution.route.connection.id.as_str(),
                    session_scope_id.as_deref(),
                    resource_kind_text(resource_kind),
                    attachment.mime_type.as_str(),
                    current_unix_millis_u64(),
                )
            })
            .filter(|reference| {
                // Every provider reference is reusable only while its exact declared TTL remains valid.
                // 每条供应商引用都只能在其精确声明的 TTL 仍有效时复用。
                submission_media_reference_is_usable_for_current_attachment(
                    preparation_attachment.attach_current_owner,
                    reference.created_at_unix_millis,
                    reference.expires_at_unix_millis,
                    now_unix_millis,
                )
            })
            .cloned();
        if let Some(source_ledger_reference) = source_ledger_reference {
            execution
                .client
                .remember_persisted_media_reference(
                    &execution.route,
                    media_source_path.as_path(),
                    resource_kind,
                    attachment.mime_type.as_str(),
                    source_ledger_reference.provider_reference.clone(),
                    source_ledger_reference.source_sha256.as_str(),
                    source_ledger_reference.source_byte_len,
                    session_scope_id.as_deref(),
                    source_ledger_reference.created_at_unix_millis,
                    source_ledger_reference.expires_at_unix_millis,
                )
                .await?;
            prepared_submission_media.push_reference(prepared_media_reference_from_ledger(
                &source_ledger_reference,
                preparation_attachment.attach_current_owner,
            ));
            continue;
        }
        if let Some(prepared_reference) = execution
            .client
            .prepared_media_reference(
                &execution.route,
                media_source_path.as_path(),
                resource_kind,
                attachment.mime_type.as_str(),
                session_scope_id.as_deref(),
                Some(cancellation),
            )
            .await
            .filter(|reference| {
                // Re-attaching an expired local video is a new upload intent; an old history replay is not.
                // 重新附加已过期本地视频代表新的上传意图；历史回放不代表新的上传意图。
                submission_media_reference_is_usable_for_current_attachment(
                    preparation_attachment.attach_current_owner,
                    reference.created_at_unix_millis,
                    reference.expires_at_unix_millis,
                    now_unix_millis,
                )
            })
        {
            prepared_submission_media.push_reference(prepared_media_reference_from_source(
                &execution,
                &upload_contract,
                &source_identity,
                resource_kind,
                attachment.mime_type.as_str(),
                prepared_reference.provider_reference,
                prepared_reference.created_at_unix_millis,
                prepared_reference.expires_at_unix_millis,
                session_scope_id.as_deref(),
                preparation_attachment.owner_message_ids.clone(),
                preparation_attachment.attach_current_owner,
            ));
            continue;
        }
        let upload_channel_id = upload_contract.upload_channel_id.clone();
        // identity is captured once before any source read so progress, telemetry, cache, and
        // dispatch cannot disagree after a manifest hot replacement.
        // identity 在任何源读取前只捕获一次，确保 Manifest 热替换后进度、遥测、缓存与分派
        // 不会产生分歧。
        let identity = MediaUploadTelemetryIdentity {
            provider_id: provider_id.clone(),
            service_entry_id: execution.route.entry.id.clone(),
            connection_id: execution.route.connection.id.clone(),
            model_id: model_id.clone(),
            protocol_profile_id: execution.route.protocol_route.protocol_profile_id.clone(),
            manifest_revision: execution.route.manifest_revision,
            manifest_generation: execution.route.manifest_generation,
            route_revision: execution.route.route_revision.clone(),
            upload_channel_id,
            request_profiles: execution
                .route
                .request_projection_profiles
                .iter()
                .map(|profile| ProviderOperationalProfileRevision {
                    profile_id: profile.profile_id.clone(),
                    revision: profile.revision,
                })
                .collect(),
            response_profiles: execution
                .route
                .response_projection_profiles
                .iter()
                .map(|profile| ProviderOperationalProfileRevision {
                    profile_id: profile.profile_id.clone(),
                    revision: profile.revision,
                })
                .collect(),
        };
        emit_media_operational_telemetry(
            &identity,
            ProviderOperationalTelemetryEventName::ProviderUploadStarted,
            None,
        );
        emit_media_upload_progress(
            app,
            state,
            media_upload_progress_delta(
                &identity,
                client_submission_id,
                conversation_id.as_deref(),
                attachment.asset_id.as_deref(),
                attachment.file_name.as_str(),
                media_kind,
                file_index,
                file_count,
                MediaUploadPhase::Preparing,
                0,
                total_bytes,
                None,
            ),
        );
        let progress_attachment = attachment.clone();
        let progress_identity = identity.clone();
        let progress_conversation_id = conversation_id.clone();
        let progress_client_submission_id = client_submission_id.to_string();
        let progress_media_kind = media_kind.to_string();
        let progress_state = state.clone();
        let progress_app = app.clone();
        let progress: MediaUploadProgressCallback = Arc::new(move |bytes_sent, total_bytes| {
            emit_media_upload_progress(
                &progress_app,
                &progress_state,
                media_upload_progress_delta(
                    &progress_identity,
                    progress_client_submission_id.as_str(),
                    progress_conversation_id.as_deref(),
                    progress_attachment.asset_id.as_deref(),
                    progress_attachment.file_name.as_str(),
                    progress_media_kind.as_str(),
                    file_index,
                    file_count,
                    if total_bytes.is_some_and(|total_bytes| bytes_sent >= total_bytes) {
                        MediaUploadPhase::AwaitingProviderConfirmation
                    } else {
                        MediaUploadPhase::Uploading
                    },
                    bytes_sent,
                    total_bytes,
                    None,
                ),
            );
        });
        // firstUsedAt anchors both the process cache and durable ledger to the beginning of this upload attempt.
        // firstUsedAt 将进程缓存与持久账本同时锚定到本次上传尝试开始时。
        let first_used_at_unix_millis = current_unix_millis_u64();
        // Physical media upload reads the same latest credential revision as conversation requests.
        // 媒体物理上传与会话请求一样读取同一份最新凭据修订版。
        let prepared = async {
            let secret = execution.credential.resolve_for_request().await?;
            execution
                .client
                .prepare_local_media(
                    &execution.route,
                    model_id.as_str(),
                    &secret,
                    media_source_path.as_path(),
                    resource_kind,
                    attachment.mime_type.as_str(),
                    false,
                    true,
                    Some(progress),
                    cancellation,
                )
                .await
        }
        .await;
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(error) if cancellation.is_cancelled() || error == "media upload cancelled" => {
                emit_media_operational_telemetry(
                    &identity,
                    ProviderOperationalTelemetryEventName::ProviderUploadCancelled,
                    Some("host_cancelled"),
                );
                emit_media_upload_progress(
                    app,
                    state,
                    media_upload_progress_delta(
                        &identity,
                        client_submission_id,
                        conversation_id.as_deref(),
                        attachment.asset_id.as_deref(),
                        attachment.file_name.as_str(),
                        media_kind,
                        file_index,
                        file_count,
                        MediaUploadPhase::Cancelled,
                        0,
                        total_bytes,
                        None,
                    ),
                );
                return Err(String::from("submission_cancelled: media upload cancelled"));
            }
            Err(error) => {
                emit_media_operational_telemetry(
                    &identity,
                    ProviderOperationalTelemetryEventName::ProviderUploadFailed,
                    Some("materialization_failed"),
                );
                emit_media_upload_progress(
                    app,
                    state,
                    media_upload_progress_delta(
                        &identity,
                        client_submission_id,
                        conversation_id.as_deref(),
                        attachment.asset_id.as_deref(),
                        attachment.file_name.as_str(),
                        media_kind,
                        file_index,
                        file_count,
                        MediaUploadPhase::Failed,
                        0,
                        total_bytes,
                        Some(error.clone()),
                    ),
                );
                return Err(error);
            }
        };
        if cancellation.is_cancelled() {
            emit_media_operational_telemetry(
                &identity,
                ProviderOperationalTelemetryEventName::ProviderUploadCancelled,
                Some("host_cancelled"),
            );
            emit_media_upload_progress(
                app,
                state,
                media_upload_progress_delta(
                    &identity,
                    client_submission_id,
                    conversation_id.as_deref(),
                    attachment.asset_id.as_deref(),
                    attachment.file_name.as_str(),
                    media_kind,
                    file_index,
                    file_count,
                    MediaUploadPhase::Cancelled,
                    total_bytes.unwrap_or_default(),
                    total_bytes,
                    None,
                ),
            );
            return Err(String::from("submission_cancelled: media upload cancelled"));
        }
        let prepared_reference = prepared
            .prepared_reference()
            .ok_or_else(|| String::from("Provider media upload did not return a reference."))?
            .to_string();
        let prepared_source_identity = prepared.source_identity();
        let prepared_created_at_unix_millis = first_used_at_unix_millis;
        let prepared_expires_at_unix_millis = upload_contract
            .cache_duration_ms
            .map(|cache_duration_ms| {
                prepared_created_at_unix_millis.saturating_add(cache_duration_ms)
            })
            .unwrap_or(u64::MAX);
        execution.client.remember_conversation_prepared_media(
            &execution.route,
            media_source_path.as_path(),
            attachment_kind(attachment.kind.clone()),
            attachment.mime_type.as_str(),
            prepared,
            session_scope_id.as_deref(),
            Some((
                prepared_created_at_unix_millis,
                prepared_expires_at_unix_millis,
            )),
        );
        prepared_submission_media.push_reference(prepared_media_reference_from_source(
            &execution,
            &upload_contract,
            &prepared_source_identity,
            resource_kind,
            attachment.mime_type.as_str(),
            prepared_reference,
            prepared_created_at_unix_millis,
            prepared_expires_at_unix_millis,
            session_scope_id.as_deref(),
            preparation_attachment.owner_message_ids.clone(),
            preparation_attachment.attach_current_owner,
        ));
        emit_media_operational_telemetry(
            &identity,
            ProviderOperationalTelemetryEventName::ProviderUploadCompleted,
            None,
        );
        emit_media_upload_progress(
            app,
            state,
            media_upload_progress_delta(
                &identity,
                client_submission_id,
                conversation_id.as_deref(),
                attachment.asset_id.as_deref(),
                attachment.file_name.as_str(),
                media_kind,
                file_index,
                file_count,
                MediaUploadPhase::Completed,
                total_bytes.unwrap_or_default(),
                total_bytes,
                None,
            ),
        );
    }
    Ok(prepared_submission_media)
}

/// Resolves the immutable local source used by one media-preparation attempt.
/// 解析一次媒体准备尝试所使用的不可变本地来源。
///
/// # Parameters
/// 参数
/// `attachment` is the canonical resource descriptor and `attach_current_owner` reports whether
/// the same source belongs to the prompt currently being submitted.
/// `attachment` 是规范资源描述符，`attach_current_owner` 表示同一来源是否属于当前正在提交的提示词。
///
/// # Returns
/// 返回值
/// A verified managed or original-source path, `None` for unavailable historical media that may
/// degrade to metadata, or an explicit error for an unavailable current attachment.
/// 返回已验证的托管路径或原始来源路径；对于可降级为元数据的历史不可用媒体返回 `None`；对于当前
/// 不可用附件返回明确错误。
fn media_preparation_replay_path(
    attachment: &ConversationAttachment,
    attach_current_owner: bool,
) -> Result<Option<PathBuf>, String> {
    let replay_path =
        vulcan_agent_runtime::attachment_assets::resolve_attachment_replay_path(attachment);
    if replay_path.is_some() || !attach_current_owner {
        return Ok(replay_path);
    }
    Err(format!(
        "Attachment `{}` is no longer available for media preparation.",
        attachment.file_name
    ))
}

/// Reports whether the active canonical conversation still contains any attachment.
/// 判断活动规范会话中是否仍包含任意附件。
fn session_runtime_state_has_attachment(
    runtime_state: &vulcan_workbench_model::SessionRuntimeStateEntry,
) -> bool {
    runtime_state
        .conversation_state
        .messages
        .iter()
        .any(|message| {
            let vulcan_context::SessionConversationPayload::UserText { attachments, .. } =
                &message.payload
            else {
                return false;
            };
            !attachments.is_empty()
        })
}

/// Decides whether one prepared reference may satisfy the current attachment intent.
/// 判断一条已准备引用是否可以满足当前附件意图。
///
/// Both current and historical request projections reuse only a reference whose known lifetime is
/// still valid; `u64::MAX` means the route declared no finite expiry.
/// 当前与历史请求投影都只复用已知生命周期仍有效的引用；`u64::MAX` 表示路线未声明有限过期时间。
fn submission_media_reference_is_usable_for_current_attachment(
    _attach_current_owner: bool,
    created_at_unix_millis: u64,
    expires_at_unix_millis: u64,
    now_unix_millis: u64,
) -> bool {
    expires_at_unix_millis == u64::MAX
        || (created_at_unix_millis <= now_unix_millis && expires_at_unix_millis > now_unix_millis)
}

/// Describes one deduplicated media source and its canonical message ownership.
/// 描述一个去重后的媒体源及其规范消息所有权。
#[derive(Debug, Clone)]
struct MediaPreparationAttachment {
    /// Exact attachment source selected for provider preparation.
    /// 用于供应商准备的精确附件源。
    attachment: ConversationAttachment,
    /// Historical canonical user-message owners of this source.
    /// 该源文件对应的历史规范用户消息所有者。
    owner_message_ids: BTreeSet<String>,
    /// Whether this source is also attached to the current submission.
    /// 该源文件是否同时属于当前提交。
    attach_current_owner: bool,
}

/// Returns whether two attachment sources identify the same local media input.
/// 判断两个附件源是否标识同一个本地媒体输入。
fn same_media_preparation_attachment(
    left: &ConversationAttachment,
    right: &ConversationAttachment,
) -> bool {
    left.kind == right.kind
        && left.local_path == right.local_path
        && left.mime_type == right.mime_type
        && left.source_sha256 == right.source_sha256
        && left.byte_len == right.byte_len
}

/// Collects current attachments plus every historical resource that must be re-projected for a route.
/// 收集当前附件以及必须为路线重新投影的全部历史资源。
fn collect_media_preparation_attachments(
    session_runtime_state: Option<&vulcan_workbench_model::SessionRuntimeStateEntry>,
    attachments: &[ConversationAttachment],
) -> Vec<MediaPreparationAttachment> {
    let mut collected: Vec<MediaPreparationAttachment> = Vec::new();
    for attachment in session_runtime_state
        .into_iter()
        .flat_map(|runtime_state| runtime_state.conversation_state.messages.iter())
        .flat_map(|message| {
            let vulcan_context::SessionConversationPayload::UserText { attachments, .. } =
                &message.payload
            else {
                return Vec::new();
            };
            attachments
                .iter()
                .map(|attachment| (message.id.as_str(), attachment))
                .collect::<Vec<_>>()
        })
    {
        if let Some(existing) = collected
            .iter_mut()
            .find(|existing| same_media_preparation_attachment(&existing.attachment, attachment.1))
        {
            existing.owner_message_ids.insert(attachment.0.to_string());
        } else {
            let mut owner_message_ids = BTreeSet::new();
            owner_message_ids.insert(attachment.0.to_string());
            collected.push(MediaPreparationAttachment {
                attachment: attachment.1.clone(),
                owner_message_ids,
                attach_current_owner: false,
            });
        }
    }
    for attachment in attachments {
        if let Some(existing) = collected
            .iter_mut()
            .find(|existing| same_media_preparation_attachment(&existing.attachment, attachment))
        {
            existing.attach_current_owner = true;
        } else {
            collected.push(MediaPreparationAttachment {
                attachment: attachment.clone(),
                owner_message_ids: BTreeSet::new(),
                attach_current_owner: true,
            });
        }
    }
    collected
}

/// Converts a persisted ledger entry into the current submission result shape.
/// 将已持久化账本条目转换为当前提交结果形状。
fn prepared_media_reference_from_ledger(
    entry: &SessionMediaReferenceEntry,
    attach_current_owner: bool,
) -> PreparedSubmissionMediaReference {
    PreparedSubmissionMediaReference {
        source_sha256: entry.source_sha256.clone(),
        source_byte_len: entry.source_byte_len,
        provider_id: entry.provider_id.clone(),
        service_entry_id: entry.service_entry_id.clone(),
        upload_channel_id: entry.upload_channel_id.clone(),
        upload_contract_fingerprint: entry.upload_contract_fingerprint.clone(),
        reference_scope: entry.reference_scope,
        connection_id: entry.connection_id.clone(),
        session_id: entry.session_id.clone(),
        resource_kind: entry.resource_kind.clone(),
        mime_type: entry.mime_type.clone(),
        provider_reference: entry.provider_reference.clone(),
        created_at_unix_millis: entry.created_at_unix_millis,
        expires_at_unix_millis: entry.expires_at_unix_millis,
        owner_message_ids: entry.owner_message_ids.clone(),
        attach_current_owner,
    }
}

/// Converts one prepared result into the attachment-index persistence shape used by drafts.
/// 将一条准备结果转换为草稿使用的附件索引持久化形状。
fn attachment_media_reference_from_prepared(
    entry: &PreparedSubmissionMediaReference,
) -> vulcan_agent_runtime::attachment_asset_index::AttachmentMediaReferenceRecord {
    vulcan_agent_runtime::attachment_asset_index::AttachmentMediaReferenceRecord {
        source_sha256: entry.source_sha256.clone(),
        source_byte_len: entry.source_byte_len,
        provider_id: entry.provider_id.clone(),
        service_entry_id: entry.service_entry_id.clone(),
        upload_channel_id: entry.upload_channel_id.clone(),
        upload_contract_fingerprint: entry.upload_contract_fingerprint.clone(),
        reference_scope: entry.reference_scope,
        connection_id: entry.connection_id.clone(),
        session_id: entry.session_id.clone(),
        resource_kind: entry.resource_kind.clone(),
        mime_type: entry.mime_type.clone(),
        provider_reference: entry.provider_reference.clone(),
        created_at_unix_millis: entry.created_at_unix_millis,
        expires_at_unix_millis: entry.expires_at_unix_millis,
    }
}

/// Converts one attachment-index reference into the current submission result shape.
/// 将一条附件索引引用转换为当前提交结果形状。
fn prepared_media_reference_from_attachment_index(
    entry: &vulcan_agent_runtime::attachment_asset_index::AttachmentMediaReferenceRecord,
    attach_current_owner: bool,
) -> PreparedSubmissionMediaReference {
    PreparedSubmissionMediaReference {
        source_sha256: entry.source_sha256.clone(),
        source_byte_len: entry.source_byte_len,
        provider_id: entry.provider_id.clone(),
        service_entry_id: entry.service_entry_id.clone(),
        upload_channel_id: entry.upload_channel_id.clone(),
        upload_contract_fingerprint: entry.upload_contract_fingerprint.clone(),
        reference_scope: entry.reference_scope,
        connection_id: entry.connection_id.clone(),
        session_id: entry.session_id.clone(),
        resource_kind: entry.resource_kind.clone(),
        mime_type: entry.mime_type.clone(),
        provider_reference: entry.provider_reference.clone(),
        created_at_unix_millis: entry.created_at_unix_millis,
        expires_at_unix_millis: entry.expires_at_unix_millis,
        owner_message_ids: BTreeSet::new(),
        attach_current_owner,
    }
}

/// Converts one freshly verified source and upload contract into a ledger candidate.
/// 将一个刚验证的源与上传契约转换为账本候选项。
fn prepared_media_reference_from_source(
    execution: &vulcan_provider_adapters::ResolvedPublicAiExecution,
    upload_contract: &vulcan_provider_adapters::ConversationMediaUploadContract,
    source_identity: &vulcan_provider_adapters::PreparedMediaSourceIdentity,
    resource_kind: vulcan_ai_contract::ResourceKind,
    mime_type: &str,
    provider_reference: String,
    created_at_unix_millis: u64,
    expires_at_unix_millis: u64,
    session_scope_id: Option<&str>,
    owner_message_ids: BTreeSet<String>,
    attach_current_owner: bool,
) -> PreparedSubmissionMediaReference {
    let reference_scope = match upload_contract.reference_scope {
        vulcan_provider_adapters::UploadReferenceScope::ProviderProtocol => {
            SessionMediaReferenceScope::ProviderProtocol
        }
        vulcan_provider_adapters::UploadReferenceScope::Credential => {
            SessionMediaReferenceScope::Credential
        }
        vulcan_provider_adapters::UploadReferenceScope::Session => {
            SessionMediaReferenceScope::Session
        }
    };
    PreparedSubmissionMediaReference {
        source_sha256: source_identity.content_sha256.clone(),
        source_byte_len: source_identity.byte_len,
        provider_id: execution.route.provider.id.clone(),
        service_entry_id: execution.route.entry.id.clone(),
        upload_channel_id: upload_contract.upload_channel_id.clone(),
        upload_contract_fingerprint: upload_contract.contract_fingerprint.clone(),
        reference_scope,
        connection_id: matches!(reference_scope, SessionMediaReferenceScope::Credential)
            .then(|| execution.route.connection.id.clone()),
        session_id: matches!(reference_scope, SessionMediaReferenceScope::Session)
            .then(|| session_scope_id.map(str::to_owned))
            .flatten(),
        resource_kind: resource_kind_text(resource_kind).to_string(),
        mime_type: mime_type.to_string(),
        provider_reference,
        created_at_unix_millis,
        expires_at_unix_millis,
        owner_message_ids,
        attach_current_owner,
    }
}

/// Converts the shared signed millisecond clock into the non-negative ledger time domain.
/// 将共享的有符号毫秒时钟转换为非负账本时间域。
fn current_unix_millis_u64() -> u64 {
    u64::try_from(current_unix_millis_value()).unwrap_or(u64::MAX)
}

/// Converts the Manifest scope enum into the durable workbench scope enum.
/// 将 Manifest 作用域枚举转换为可持久化工作台作用域枚举。
fn session_media_reference_scope(
    scope: vulcan_provider_adapters::UploadReferenceScope,
) -> SessionMediaReferenceScope {
    match scope {
        vulcan_provider_adapters::UploadReferenceScope::ProviderProtocol => {
            SessionMediaReferenceScope::ProviderProtocol
        }
        vulcan_provider_adapters::UploadReferenceScope::Credential => {
            SessionMediaReferenceScope::Credential
        }
        vulcan_provider_adapters::UploadReferenceScope::Session => {
            SessionMediaReferenceScope::Session
        }
    }
}

/// Reports whether one attachment can use the selected model's exact native request route.
/// 判断一个附件是否可使用所选模型的精确原生请求路由。
///
/// # Parameters
/// 参数
/// `protocol_profile_id` identifies the frozen encoder, `capabilities` contains the effective
/// model contract, and `attachment` contains the actual media kind and MIME type.
/// `protocol_profile_id` 标识冻结编码器，`capabilities` 包含生效模型契约，`attachment` 包含实际媒体类型与 MIME。
///
/// # Returns
/// 返回值
/// `true` only when model declaration, modality projector, and format-sensitive encoder agree.
/// 仅当模型声明、模态投影器与格式敏感编码器全部一致时返回 `true`。
fn submission_attachment_is_native(
    protocol_profile_id: &str,
    capabilities: &vulcan_ai_contract::ModelCapabilities,
    attachment: &ConversationAttachment,
) -> bool {
    // resourceKind normalizes the core attachment category for model and protocol comparison.
    // resourceKind 归一化核心附件类别，用于模型与协议比较。
    let resource_kind = attachment_kind(attachment.kind.clone());
    // protocolSupported reuses the request normalizer's exact kind-and-MIME predicate so
    // pre-dispatch preparation and final message projection cannot diverge.
    // protocolSupported 复用请求归一化器的精确类型与 MIME 判定，确保分派前准备与最终消息投影不会分叉。
    let protocol_supported =
        vulcan_agent_runtime::provider_input_normalizer::protocol_profile_supports_attachment(
            protocol_profile_id,
            attachment,
        );
    capabilities.input_resources.contains(&resource_kind) && protocol_supported
}

/// Emits one ordered ephemeral progress delta and advances the shared workbench revision.
/// 发送一条有序临时进度增量，并推进共享工作台版本。
fn emit_media_upload_progress(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    progress: MediaUploadProgressChangedDelta,
) {
    let event = state.lock().ok().map(|mut guard| {
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::MediaUploadProgressChanged(progress),
        }
    });
    if let Some(event) = event {
        emit_service_event_with_trace(
            app,
            "media_upload_progress",
            ServiceEvent::WorkbenchDelta(Box::new(event)),
        );
    }
}

/// Converts a core attachment kind into the provider-neutral media contract.
/// 将核心附件类型转换为供应商无关的媒体契约。
fn attachment_kind(kind: ConversationAttachmentKind) -> vulcan_ai_contract::ResourceKind {
    match kind {
        ConversationAttachmentKind::Image => vulcan_ai_contract::ResourceKind::Image,
        ConversationAttachmentKind::Audio => vulcan_ai_contract::ResourceKind::Audio,
        ConversationAttachmentKind::Video => vulcan_ai_contract::ResourceKind::Video,
        ConversationAttachmentKind::Pdf | ConversationAttachmentKind::File => {
            vulcan_ai_contract::ResourceKind::File
        }
    }
}

/// Converts a core attachment kind into the stable UI label.
/// 将核心附件类型转换为稳定的界面标签。
fn attachment_kind_text(kind: ConversationAttachmentKind) -> &'static str {
    match kind {
        ConversationAttachmentKind::Image => "image",
        ConversationAttachmentKind::Audio => "audio",
        ConversationAttachmentKind::Video => "video",
        ConversationAttachmentKind::Pdf => "pdf",
        ConversationAttachmentKind::File => "file",
    }
}

/// Converts a provider-neutral resource kind into its stable ledger label.
/// 将供应商无关资源类型转换为稳定的账本标签。
fn resource_kind_text(kind: vulcan_ai_contract::ResourceKind) -> &'static str {
    match kind {
        vulcan_ai_contract::ResourceKind::Image => "image",
        vulcan_ai_contract::ResourceKind::Video => "video",
        vulcan_ai_contract::ResourceKind::Audio => "audio",
        vulcan_ai_contract::ResourceKind::File => "file",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        COMPOSER_INLINE_AGGREGATE_BASE64_MAX_BYTES, DraftMediaUploadPlanIdentity,
        apply_composer_delivery_plan_to_attachment, apply_composer_direct_resource_limits,
        collect_media_preparation_attachments, draft_media_upload_plan_matches,
        media_preparation_replay_path, register_media_upload, remove_media_upload,
        submission_attachment_is_native,
        submission_media_reference_is_usable_for_current_attachment,
    };
    use crate::conversation_submission::SubmissionModelSelection;
    use std::collections::{BTreeMap, BTreeSet};
    use vulcan_ai_contract::{ModelCapabilities, ResourceKind};
    use vulcan_context::{SessionConversationMessage, SessionConversationPayload};
    use vulcan_core::types::{
        ConversationAttachment, ConversationAttachmentDeliveryMode, ConversationAttachmentKind,
    };
    use vulcan_service_api::{
        ComposerAttachmentDeliveryPlan, ComposerAttachmentPreparationPhase,
        ComposerAttachmentPreparationSummary,
    };
    use vulcan_workbench_model::SessionRuntimeStateEntry;

    /// Builds one attachment fixture for pre-upload capability validation.
    /// 构建一个用于上传前能力校验的附件夹具。
    fn attachment(kind: ConversationAttachmentKind) -> ConversationAttachment {
        // mimeType matches the exact media fixture kind so format-sensitive validation stays meaningful.
        // mimeType 与精确媒体夹具类型一致，使格式敏感校验保持有效。
        let mime_type = match &kind {
            ConversationAttachmentKind::Image => "image/png",
            ConversationAttachmentKind::Video => "video/mp4",
            ConversationAttachmentKind::Audio => "audio/wav",
            ConversationAttachmentKind::Pdf => "application/pdf",
            ConversationAttachmentKind::File => "application/octet-stream",
        };
        ConversationAttachment {
            kind,
            file_name: String::from("media.bin"),
            local_path: String::from("media.bin"),
            resource_path: None,
            source_path: None,
            asset_id: None,
            byte_len: Some(1),
            source_sha256: None,
            delivery_mode: None,
            preview_local_path: None,
            mime_type: String::from(mime_type),
        }
    }

    /// Verifies a derived preview changes only the isolated request copy and records exact bytes.
    /// 验证派生预览仅修改隔离的请求副本，并记录精确字节数。
    #[test]
    fn derived_preview_projection_uses_request_local_source() {
        let directory = tempfile::tempdir().expect("temporary directory should create");
        let preview_path = directory.path().join("preview.jpg");
        std::fs::write(&preview_path, [0xff_u8, 0xd8, 0xff, 0xd9])
            .expect("preview fixture should write");
        let mut source = attachment(ConversationAttachmentKind::Image);
        source.file_name = String::from("diagram.svg");
        source.local_path = String::from("canonical/diagram.svg");
        source.source_sha256 = Some(String::from("source-hash"));
        source.preview_local_path = Some(preview_path.to_string_lossy().to_string());

        let projected = apply_composer_delivery_plan_to_attachment(
            &source,
            ComposerAttachmentDeliveryPlan::InlineDerivedPreview,
        );

        assert_eq!(source.local_path, "canonical/diagram.svg");
        assert_eq!(projected.local_path, preview_path.to_string_lossy());
        assert_eq!(projected.file_name, "diagram.svg.vulcan-preview.jpg");
        assert_eq!(projected.mime_type, "image/jpeg");
        assert_eq!(projected.byte_len, Some(4));
        assert_eq!(projected.source_sha256, None);
        assert_eq!(
            projected.delivery_mode,
            Some(ConversationAttachmentDeliveryMode::InlineDerivedPreview)
        );
    }

    /// Verifies a vanished derived preview fails closed to metadata-only delivery.
    /// 验证已消失的派生预览会封闭降级为仅元数据传输。
    #[test]
    fn missing_derived_preview_projection_degrades_to_metadata_only() {
        let directory = tempfile::tempdir().expect("temporary directory should create");
        let mut source = attachment(ConversationAttachmentKind::Image);
        source.preview_local_path = Some(
            directory
                .path()
                .join("missing-preview.jpg")
                .to_string_lossy()
                .to_string(),
        );

        let projected = apply_composer_delivery_plan_to_attachment(
            &source,
            ComposerAttachmentDeliveryPlan::InlineDerivedPreview,
        );

        assert_eq!(projected.local_path, source.local_path);
        assert_eq!(projected.mime_type, source.mime_type);
        assert_eq!(
            projected.delivery_mode,
            Some(ConversationAttachmentDeliveryMode::MetadataOnly)
        );
    }

    /// Builds one exact service-owned draft upload identity fixture.
    /// 构建一个精确的服务端草稿上传身份夹具。
    fn draft_upload_identity() -> DraftMediaUploadPlanIdentity {
        DraftMediaUploadPlanIdentity {
            workspace_path: String::from("D:/workspace"),
            conversation_id: Some(String::from("conversation-a")),
            model_selection: SubmissionModelSelection {
                provider_id: String::from("provider-a"),
                model_id: String::from("model-a"),
                connection_id: String::from("connection-a"),
            },
            attachment_id: String::from("asset-a"),
            generation: 7,
            route_revision: String::from("sha256:route-a"),
        }
    }

    /// Builds one caller-visible upload preparation matching the frozen fixture.
    /// 构建一条与冻结夹具匹配的调用方可见上传准备记录。
    fn draft_upload_preparation() -> ComposerAttachmentPreparationSummary {
        ComposerAttachmentPreparationSummary {
            attachment_id: String::from("asset-a"),
            generation: 7,
            transfer_id: String::from("draft-media:transfer-a"),
            delivery_plan: ComposerAttachmentDeliveryPlan::UploadReference,
            phase: ComposerAttachmentPreparationPhase::UploadQueued,
            blocks_submission: true,
            route_revision: Some(String::from("sha256:route-a")),
            reason: String::from("provider_upload_required"),
            error: None,
        }
    }

    /// Verifies only an exact owner, route, generation, and queued blocking state can consume a plan.
    /// 验证仅精确所有者、路线、代际以及排队阻塞状态能够消费计划。
    #[test]
    fn draft_upload_plan_requires_exact_frozen_identity() {
        let identity = draft_upload_identity();
        let selection = identity.model_selection.clone();
        let conversation_id = identity.conversation_id.clone();
        let preparation = draft_upload_preparation();
        assert!(draft_media_upload_plan_matches(
            &identity,
            identity.workspace_path.as_str(),
            &conversation_id,
            &selection,
            &preparation,
        ));

        let mut stale_route = preparation.clone();
        stale_route.route_revision = Some(String::from("sha256:route-b"));
        assert!(!draft_media_upload_plan_matches(
            &identity,
            identity.workspace_path.as_str(),
            &conversation_id,
            &selection,
            &stale_route,
        ));

        let mut non_blocking = preparation;
        non_blocking.blocks_submission = false;
        assert!(!draft_media_upload_plan_matches(
            &identity,
            identity.workspace_path.as_str(),
            &conversation_id,
            &selection,
            &non_blocking,
        ));
    }

    /// Verifies duplicate client submission identities cannot replace an active cancellation handle.
    /// 验证重复的客户端提交标识不会替换正在生效的取消句柄。
    #[test]
    fn duplicate_submission_registration_is_rejected() {
        let client_submission_id = "media-upload-duplicate-test";
        let _cancellation = register_media_upload(client_submission_id).expect("first register");
        match register_media_upload(client_submission_id) {
            Err(error) => assert!(error.contains("already preparing media")),
            Ok(_) => panic!("duplicate registration must be rejected"),
        }
        remove_media_upload(client_submission_id);
    }

    /// Verifies every historical video keeps its real owners while current reuse adds only the new owner.
    /// 验证每个历史视频保留真实所有者，而当前复用只追加新的所有者。
    #[test]
    fn historical_video_owners_are_merged_without_rebinding_history() {
        let mut first_video = attachment(ConversationAttachmentKind::Video);
        first_video.local_path = String::from("first.mp4");
        let mut second_video = attachment(ConversationAttachmentKind::Video);
        second_video.local_path = String::from("second.mp4");
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.conversation_state.messages = vec![
            SessionConversationMessage {
                id: String::from("old-a"),
                payload: SessionConversationPayload::UserText {
                    text: String::from("old a"),
                    attachments: vec![first_video.clone(), second_video],
                },
            },
            SessionConversationMessage {
                id: String::from("old-b"),
                payload: SessionConversationPayload::UserText {
                    text: String::from("old b"),
                    attachments: vec![first_video.clone()],
                },
            },
        ];

        let collected = collect_media_preparation_attachments(
            Some(&runtime_state),
            std::slice::from_ref(&first_video),
        );
        assert_eq!(collected.len(), 2);
        let first = collected
            .iter()
            .find(|candidate| candidate.attachment.local_path == "first.mp4")
            .expect("first historical video should be collected");
        assert_eq!(
            first.owner_message_ids,
            BTreeSet::from([String::from("old-a"), String::from("old-b")])
        );
        assert!(first.attach_current_owner);
        let second = collected
            .iter()
            .find(|candidate| candidate.attachment.local_path == "second.mp4")
            .expect("second historical video should be collected");
        assert_eq!(
            second.owner_message_ids,
            BTreeSet::from([String::from("old-a")])
        );
        assert!(!second.attach_current_owner);
    }

    /// Verifies path reuse never merges different immutable media source identities.
    /// 验证路径复用绝不会合并不同的不可变媒体源身份。
    #[test]
    fn media_preparation_keeps_replaced_files_as_distinct_sources() {
        let mut historical_video = attachment(ConversationAttachmentKind::Video);
        historical_video.local_path = String::from("shared.mp4");
        historical_video.source_sha256 = Some(String::from("historical-hash"));
        historical_video.byte_len = Some(10);
        let mut current_video = historical_video.clone();
        current_video.source_sha256 = Some(String::from("current-hash"));
        current_video.byte_len = Some(20);
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.conversation_state.messages = vec![SessionConversationMessage {
            id: String::from("old-a"),
            payload: SessionConversationPayload::UserText {
                text: String::from("old a"),
                attachments: vec![historical_video],
            },
        }];

        let collected = collect_media_preparation_attachments(
            Some(&runtime_state),
            std::slice::from_ref(&current_video),
        );

        assert_eq!(collected.len(), 2);
        let historical = collected
            .iter()
            .find(|candidate| {
                candidate.attachment.source_sha256.as_deref() == Some("historical-hash")
            })
            .expect("historical source should remain distinct");
        assert_eq!(
            historical.owner_message_ids,
            BTreeSet::from([String::from("old-a")])
        );
        assert!(!historical.attach_current_owner);
        let current = collected
            .iter()
            .find(|candidate| candidate.attachment.source_sha256.as_deref() == Some("current-hash"))
            .expect("current source should remain distinct");
        assert!(current.owner_message_ids.is_empty());
        assert!(current.attach_current_owner);
    }

    /// Verifies unavailable history degrades while an unavailable current attachment fails closed.
    /// 验证不可用历史会降级，而不可用当前附件会封闭失败。
    #[test]
    fn media_preparation_missing_source_distinguishes_history_from_current_prompt() {
        let directory = tempfile::tempdir().expect("temporary directory should create");
        let mut missing = attachment(ConversationAttachmentKind::Video);
        missing.file_name = String::from("missing.mp4");
        missing.local_path = directory
            .path()
            .join("missing.mp4")
            .to_string_lossy()
            .to_string();

        assert_eq!(
            media_preparation_replay_path(&missing, false)
                .expect("historical missing media should degrade"),
            None
        );
        let error = media_preparation_replay_path(&missing, true)
            .expect_err("current missing media should fail closed");
        assert!(error.contains("missing.mp4"));
    }

    /// Verifies preparation uses the original source only when the managed replay copy is absent.
    /// 验证仅当托管回放副本缺失时，准备流程才使用原始来源。
    #[test]
    fn media_preparation_uses_verified_original_source_fallback() {
        let directory = tempfile::tempdir().expect("temporary directory should create");
        let source_path = directory.path().join("source.mp4");
        std::fs::write(&source_path, b"video").expect("source fixture should write");
        let mut fallback = attachment(ConversationAttachmentKind::Video);
        fallback.local_path = directory
            .path()
            .join("missing-managed.mp4")
            .to_string_lossy()
            .to_string();
        fallback.source_path = Some(source_path.to_string_lossy().to_string());

        assert_eq!(
            media_preparation_replay_path(&fallback, true).expect("source fallback should resolve"),
            Some(source_path)
        );
    }

    /// Verifies bridge admission never reclassifies a format-incompatible attachment as native.
    /// 验证桥接准入绝不会把格式不兼容的附件重新归类为原生附件。
    #[test]
    fn understanding_bridge_preserves_format_incompatible_audio_as_local() {
        // capabilities declares audio at model level while the frozen encoder cannot encode Opus.
        // capabilities 在模型级声明音频，但冻结编码器无法编码 Opus。
        let capabilities = ModelCapabilities {
            input_resources: vec![ResourceKind::Audio],
            ..ModelCapabilities::default()
        };
        // opusAttachment is bridge-eligible but must not start native preparation.
        // opusAttachment 可由桥接处理，但不得启动原生准备。
        let mut opus_attachment = attachment(ConversationAttachmentKind::Audio);
        opus_attachment.file_name = String::from("voice.opus");
        opus_attachment.mime_type = String::from("audio/opus");
        assert!(!submission_attachment_is_native(
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,
            &capabilities,
            &opus_attachment,
        ));
    }

    /// Verifies no current or historical projection reuses an expired provider reference.
    /// 验证当前与历史投影都不会复用已过期的供应商引用。
    #[test]
    fn media_reference_expiry_applies_to_current_and_historical_projection() {
        assert!(!submission_media_reference_is_usable_for_current_attachment(false, 1, 2, 3));
        assert!(!submission_media_reference_is_usable_for_current_attachment(true, 1, 2, 3));
        assert!(submission_media_reference_is_usable_for_current_attachment(
            true, 1, 10, 3
        ));
        assert!(submission_media_reference_is_usable_for_current_attachment(
            true,
            1,
            u64::MAX,
            u64::MAX,
        ));
    }

    /// Verifies route count rejection does not consume a direct-resource slot.
    /// 验证路由数量拒绝不会额外占用直接资源名额。
    #[test]
    fn composer_direct_resource_count_limit_preserves_admitted_total() {
        let mut counts = BTreeMap::new();
        let mut inline_base64_bytes = 0;
        assert_eq!(
            apply_composer_direct_resource_limits(
                ResourceKind::Image,
                Some(1),
                None,
                &mut counts,
                &mut inline_base64_bytes,
            ),
            None
        );
        assert_eq!(
            apply_composer_direct_resource_limits(
                ResourceKind::Image,
                Some(1),
                None,
                &mut counts,
                &mut inline_base64_bytes,
            ),
            Some("frozen_route_resource_count_exceeded")
        );
        assert_eq!(counts.get(&ResourceKind::Image), Some(&1));
        assert_eq!(inline_base64_bytes, 0);
    }

    /// Verifies exact padded Base64 accounting admits the boundary and rejects the next byte.
    /// 验证精确的带填充 Base64 计数会准入边界值并拒绝后续一个字节。
    #[test]
    fn composer_inline_aggregate_budget_uses_exact_base64_length() {
        let mut counts = BTreeMap::new();
        let mut inline_base64_bytes = 0;
        // boundaryRawBytes expands to exactly the frozen two-mebibyte Base64 budget.
        // boundaryRawBytes 恰好膨胀为冻结的二 MiB Base64 总预算。
        let boundary_raw_bytes = COMPOSER_INLINE_AGGREGATE_BASE64_MAX_BYTES / 4 * 3;
        assert_eq!(
            apply_composer_direct_resource_limits(
                ResourceKind::Image,
                None,
                Some(boundary_raw_bytes),
                &mut counts,
                &mut inline_base64_bytes,
            ),
            None
        );
        assert_eq!(
            inline_base64_bytes,
            COMPOSER_INLINE_AGGREGATE_BASE64_MAX_BYTES
        );
        assert_eq!(
            apply_composer_direct_resource_limits(
                ResourceKind::File,
                None,
                Some(1),
                &mut counts,
                &mut inline_base64_bytes,
            ),
            Some("host_inline_aggregate_budget_exceeded")
        );
        assert_eq!(counts.get(&ResourceKind::Image), Some(&1));
        assert_eq!(counts.get(&ResourceKind::File), Some(&0));
        assert_eq!(
            inline_base64_bytes,
            COMPOSER_INLINE_AGGREGATE_BASE64_MAX_BYTES
        );
    }
}
