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
    ProviderOperationalTelemetryEventName, record_provider_operational_telemetry,
};
use vulcan_service_api::{
    ComposerAttachmentDeliveryPlan, ComposerAttachmentPreparationPhase,
    ComposerAttachmentPreparationSummary, ComposerAttachmentSummary, MediaUploadPhase,
    MediaUploadProgressChangedDelta, ServiceEvent, WorkbenchDelta, WorkbenchDeltaEnvelope,
};
use vulcan_workbench_model::SessionMediaReferenceScope;

mod composer_policy;
mod draft_execution;
mod dropped_files;
pub(crate) use dropped_files::{classify_dropped_composer_files, stage_dropped_composer_file};
mod image_conversion;
#[cfg(test)]
pub(crate) use image_conversion::exercise_composer_image_conversion;
mod registration;
use composer_policy::ComposerMediaDeliveryPlanner;
pub(crate) use composer_policy::media_receipt_is_usable;
pub(crate) use draft_execution::discard_composer_attachment_plans;
#[cfg(test)]
pub(crate) use draft_execution::tests::exercise_draft_upload_reservation_lifecycle;
use draft_execution::{
    DraftUploadIntentIdentity, DraftUploadObservation, DraftUploadObserver, DraftUploadPreparation,
    DraftUploadReservation,
};
use registration::{PendingMediaUpload, PendingMediaUploadRegistration};

/// Process-local pending media submissions keyed by the client submission id.
/// 按客户端提交标识索引的进程内待处理媒体提交。
static PENDING_MEDIA_UPLOADS: OnceLock<StdMutex<BTreeMap<String, PendingMediaUpload>>> =
    OnceLock::new();

/// Process-local frozen draft plans keyed by their service-issued transfer identity.
/// 按服务签发传输身份索引的进程内冻结草稿计划。
static DRAFT_MEDIA_UPLOAD_PLANS: OnceLock<StdMutex<BTreeMap<String, DraftMediaUploadPlan>>> =
    OnceLock::new();

/// Maximum lifetime for an unconsumed process-local draft upload plan.
/// 未消费进程内草稿上传计划的最长存活时间。
const DRAFT_MEDIA_UPLOAD_PLAN_TTL: std::time::Duration = std::time::Duration::from_secs(10 * 60);

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

/// Retains one view plan; its reservation alone owns original intent and physical execution authority.
/// 保留一份视图计划；其许可独占原意图与实体执行权威。
struct DraftMediaUploadPlan {
    /// Shared single-use physical start, independent of the view generation.
    /// 共享且只能消费一次的实体启动，独立于视图代际。
    reservation: Arc<DraftUploadReservation>,
    /// Non-secret caller and route identity authorized by this plan.
    /// 本计划授权的非秘密调用方与路线身份。
    identity: DraftMediaUploadPlanIdentity,
    /// Current view's frozen route for receipt projection; physical work uses the reservation's original execution.
    /// 当前视图用于回执投影的冻结路线；实体工作使用许可中的原执行。
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
fn pending_media_uploads() -> &'static StdMutex<BTreeMap<String, PendingMediaUpload>> {
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
    mut plans: Vec<(String, DraftMediaUploadPlan)>,
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
    // Merge concurrent view publications under the existing locks before exposing any of this batch.
    // 在暴露本批次任何计划之前，在既有锁内合并并发视图发布。
    let pending = pending_media_uploads()
        .lock()
        .map_err(|error| error.to_string())?;
    let mut incoming_reservations = BTreeMap::<String, Arc<DraftUploadReservation>>::new();
    for (_, plan) in &mut plans {
        // A delayed view already tied to a started operation must never be rebound to a newer explicit retry.
        // 已绑定启动操作的延迟视图绝不能重新绑定到更新的显式重试。
        if plan.reservation.started.get().is_some() {
            continue;
        }
        if let Some(retained) =
            draft_execution::find_reservation(&plan.reservation.identity, &registry, &pending)?
        {
            plan.reservation = retained;
        } else if let Some(retained) =
            incoming_reservations.get(&plan.reservation.identity.intent_id)
        {
            if retained.identity != plan.reservation.identity {
                return Err(String::from(
                    "Draft upload batch contains conflicting intent owners.",
                ));
            }
            plan.reservation = Arc::clone(retained);
        }
        incoming_reservations.insert(
            plan.reservation.identity.intent_id.clone(),
            Arc::clone(&plan.reservation),
        );
    }
    registry.extend(plans);
    Ok(())
}

/// Atomically consumes a view plan and either claims its original start or registers an independent observer.
/// 原子消费视图计划，取得原启动权限或登记独立观察者。
///
/// # Parameters
/// # 参数
/// `transfer_id` is the opaque identity returned by the planning RPC; the remaining arguments are
/// the exact caller-visible facts that must match before registry mutation.
/// `transfer_id` 是规划 RPC 返回的不透明身份；其余参数是修改注册表前必须精确匹配的调用方可见事实。
///
/// # Returns
/// # 返回值
/// The validated view plan and a physical owner or observer, or an explicit controlled error.
/// 返回已校验视图计划及实体所有者或观察者，或返回显式受控错误。
fn consume_draft_media_upload_plan(
    transfer_id: &str,
    workspace_path: &str,
    conversation_id: &Option<String>,
    model_selection: &SubmissionModelSelection,
    preparation: &ComposerAttachmentPreparationSummary,
) -> Result<(DraftMediaUploadPlan, DraftUploadPreparation), String> {
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
    // Register under the plan owner using the same plan-then-pending lock order as cancellation.
    // 在计划所有者内登记，使用与取消相同的计划锁先于待处理锁顺序。
    let mut pending = pending_media_uploads()
        .lock()
        .map_err(|error| error.to_string())?;
    if pending.contains_key(transfer_id) {
        return Err(String::from("Draft view preparation is already active."));
    }
    // Every view shares the same once-set operation, including observers that start after it has settled.
    // 每个视图共享同一个仅设置一次的操作，包含在其结算后才开始的观察者。
    let start = if let Some(operation) = frozen_plan.reservation.started.get() {
        let observer = DraftUploadObserver {
            alias_id: transfer_id.to_owned(),
            operation: operation.clone(),
        };
        pending.insert(
            transfer_id.to_owned(),
            PendingMediaUpload {
                draft: Some(Arc::clone(&frozen_plan.reservation)),
                cancellation: operation.cancellation.clone(),
                settled: operation.settled.clone(),
            },
        );
        DraftUploadPreparation::Observe(observer)
    } else {
        let cancellation = MediaUploadCancellation::new();
        let (settled, settlement) = tokio::sync::watch::channel(false);
        let operation = DraftUploadObservation {
            transfer_id: transfer_id.to_owned(),
            cancellation: cancellation.clone(),
            settled: settlement.clone(),
        };
        frozen_plan
            .reservation
            .started
            .set(operation)
            .map_err(|_| String::from("Draft upload start was already claimed."))?;
        pending.insert(
            transfer_id.to_owned(),
            PendingMediaUpload {
                draft: Some(Arc::clone(&frozen_plan.reservation)),
                cancellation: cancellation.clone(),
                settled: settlement,
            },
        );
        DraftUploadPreparation::Start(PendingMediaUploadRegistration::new(
            transfer_id.to_owned(),
            cancellation,
            settled,
        ))
    };
    drop(pending);
    // The plan cannot change while this guard is held; explicitly undo registration on an invariant failure.
    // 持有守卫期间计划不能变化；不变量失败时显式撤销登记。
    let Some(frozen_plan) = registry.remove(transfer_id) else {
        return Err(String::from(
            "Validated draft media upload plan disappeared before consumption.",
        ));
    };
    Ok((frozen_plan, start))
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
/// Returns its unique terminal owner; dropping the owner settles cancellation waiters and removes only this operation.
/// 返回其唯一终态所有者；丢弃所有者会结算取消等待者，并仅移除本操作。
#[cfg(test)]
pub(crate) fn register_media_upload(
    client_submission_id: &str,
) -> Result<PendingMediaUploadRegistration, String> {
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
    // A cancellation acknowledgement waits for the actual owner to settle, not just for its token to be signalled.
    // 取消确认等待实际所有者结算，而非仅等待其令牌被通知。
    let (settled, settlement) = tokio::sync::watch::channel(false);
    pending.insert(
        client_submission_id.to_string(),
        PendingMediaUpload {
            draft: None,
            cancellation: cancellation.clone(),
            settled: settlement,
        },
    );
    Ok(PendingMediaUploadRegistration::new(
        client_submission_id.to_string(),
        cancellation,
        settled,
    ))
}

/// Requests cancellation for exact `client_submission_id` and acknowledges only after its active owner settles, without changing conversation history.
/// 为精确`client_submission_id`请求取消，并仅在活动所有者结算后确认，不修改会话历史。
/// Returns false for no active work, true after removing an unconsumed plan or settling the exact running operation, or an explicit settlement failure.
/// 无活动工作时返回假，移除未消费计划或结算精确运行操作后返回真，结算失败则明确报错。
pub(crate) async fn cancel_media_upload(client_submission_id: &str) -> Result<bool, String> {
    // Capture the exact operation without holding either registry mutex across cancellation or settlement waiting.
    // 捕获精确操作，不在取消或结算等待期间持有任一注册表互斥锁。
    let (removed_draft_plan, operation) = {
        // plans and pending use the same lock order as the consume transition.
        // plans 与 pending 使用和消费迁移相同的加锁顺序。
        let mut plans = draft_media_upload_plans()
            .lock()
            .map_err(|error| format!("failed to lock draft media upload plans: {error}"))?;
        retain_live_draft_media_upload_plans(&mut plans);
        let removed_draft_plan = plans.remove(client_submission_id);
        // Cancelling an unstarted shared permission also revokes every other queued view of that permission.
        // 取消未启动的共享许可时，也撤销该许可的其他全部排队视图。
        if let Some(plan) = &removed_draft_plan
            && plan.reservation.started.get().is_none()
        {
            let cancellation = MediaUploadCancellation::new();
            cancellation.cancel();
            let (_, settled) = tokio::sync::watch::channel(true);
            plan.reservation
                .started
                .set(DraftUploadObservation {
                    transfer_id: client_submission_id.to_owned(),
                    cancellation,
                    settled,
                })
                .map_err(|_| String::from("Draft start changed during cancellation."))?;
        }
        let pending = pending_media_uploads()
            .lock()
            .map_err(|error| format!("failed to lock pending media uploads: {error}"))?;
        // An explicit user cancellation of an unconsumed observing view also targets its original operation.
        // 用户显式取消未消费的观察视图时，也针对其原操作。
        let operation = pending.get(client_submission_id).cloned().or_else(|| {
            removed_draft_plan.as_ref().and_then(|plan| {
                plan.reservation
                    .started
                    .get()
                    .map(|original| PendingMediaUpload {
                        draft: Some(Arc::clone(&plan.reservation)),
                        cancellation: original.cancellation.clone(),
                        settled: original.settled.clone(),
                    })
            })
        });
        (removed_draft_plan.is_some(), operation)
    };
    if let Some(mut operation) = operation {
        operation.cancellation.cancel();
        loop {
            // Completion can precede this first poll; watch retains that exact terminal fact.
            // 完成可先于首次轮询；watch保留该精确终态事实。
            let settled = *operation.settled.borrow_and_update();
            if settled {
                return Ok(true);
            }
            operation.settled.changed().await.map_err(|_| {
                String::from("Media upload settlement ended without a terminal acknowledgement.")
            })?;
        }
    }
    Ok(removed_draft_plan)
}

/// Resolves one provider-neutral direct-delivery decision from the exact frozen execution route.
/// 从精确冻结的执行路线解析单个供应商中立直接传输决策。
fn composer_attachment_delivery_plan(
    execution: &vulcan_provider_adapters::ResolvedPublicAiExecution,
    attachment: &ConversationAttachment,
) -> Result<(ComposerAttachmentDeliveryPlan, &'static str), String> {
    ComposerMediaDeliveryPlanner::new(execution.media_delivery_policy)?.plan(execution, attachment)
}

/// Captures the exact non-secret model route from frozen `execution` for one prompt-owned media intention.
/// 从冻结`execution`捕获精确非秘密模型路线，用于一个提示词所有媒体意图。
fn composer_media_original_route(
    execution: &vulcan_provider_adapters::ResolvedPublicAiExecution,
) -> vulcan_session_core::v1::UserPromptMediaRoute {
    vulcan_session_core::v1::UserPromptMediaRoute {
        provider_id: execution.route.provider.id.clone(),
        service_entry_id: execution.route.entry.id.clone(),
        connection_id: execution.route.connection.id.clone(),
        model_id: execution.model.id.clone(),
        protocol_profile_id: execution.route.protocol_route.protocol_profile_id.clone(),
    }
}

/// Exercises the real composer decision for one fixture occurrence without exposing the private planner in production.
/// 为一次夹具出现执行真实输入区决策，不在生产暴露私有规划器。
/// `execution`, `attachment`, `original`, and `session_id` are exact host facts; returns the selected mode and safe reason.
/// `execution`、`attachment`、`original`和`session_id`是精确宿主事实；返回选中方式及安全原因。
#[cfg(test)]
pub(crate) fn test_composer_draft_decision(
    execution: &vulcan_provider_adapters::ResolvedPublicAiExecution,
    attachment: &ConversationAttachment,
    original: &vulcan_agent_runtime::attachment_asset_index::AttachmentDraftMediaOrigin,
    session_id: &str,
) -> Result<(ComposerAttachmentDeliveryPlan, &'static str), String> {
    ComposerMediaDeliveryPlanner::new(execution.media_delivery_policy)?.plan_draft(
        execution,
        attachment,
        Some(original),
        Some(session_id),
    )
}

/// Converts one exact completed draft `reference` into the existing private project receipt using its frozen `execution` and canonical `attachment`.
/// 根据冻结`execution`及规范`attachment`，将精确已完成草稿`reference`转换为既有私有项目回执。
fn draft_upload_binding(
    reference: &PreparedSubmissionMediaReference,
    execution: &vulcan_provider_adapters::ResolvedPublicAiExecution,
    attachment: &ConversationAttachment,
) -> Result<vulcan_workspace_store::workspace_store::ProviderBinding, String> {
    // Resource kind belongs to the canonical attachment, never to a guessed reference string format.
    // 资源类别属于规范附件，绝不来自猜测的引用字符串格式。
    let kind = attachment_kind(attachment.kind.clone());
    // Only the already selected upload contract defines the reference representation.
    // 只有已经选中的上传契约定义引用表示形式。
    let contract = execution
        .client
        .conversation_media_upload_contract(&execution.route, kind, &attachment.mime_type)?
        .ok_or_else(|| String::from("Draft upload receipt has no exact consuming contract."))?;
    if reference.provider_id != execution.route.provider.id
        || reference.service_entry_id != execution.route.entry.id
        || reference.upload_channel_id != contract.upload_channel_id
        || reference.upload_contract_fingerprint != contract.contract_fingerprint
        || attachment.source_sha256.as_deref() != Some(reference.source_sha256.as_str())
        || attachment.byte_len != Some(reference.source_byte_len)
        || reference.mime_type != attachment.mime_type
        || reference.resource_kind != resource_kind_text(kind)
    {
        return Err(String::from(
            "Draft upload result differs from its frozen source or route.",
        ));
    }
    execution.client.validate_conversation_media_reference(
        &execution.route,
        kind,
        &attachment.mime_type,
        &reference.provider_reference,
    )?;
    // Private provider values remain in the resource store; prompt parameters will receive only their fingerprint.
    // 私有供应商值保留在资源存储；提示词参数只接收其指纹。
    let binding = vulcan_workspace_store::workspace_store::ProviderBinding {
        provider: reference.provider_id.clone(),
        connection_id: execution.route.connection.id.clone(),
        media_id: reference.provider_reference.clone(),
        upload_receipt: vulcan_workspace_store::workspace_store::ProjectResourceUploadReceipt {
            draft_intent_id: None,
            version: 1,
            service_entry_id: reference.service_entry_id.clone(),
            upload_channel_id: reference.upload_channel_id.clone(),
            upload_contract_fingerprint: reference.upload_contract_fingerprint.clone(),
            resource_kind: kind,
            mime_type: reference.mime_type.clone(),
            source_kind: vulcan_provider_adapters::media_upload_source_kind(
                contract.reference_encoding,
            ),
            session_scope_id: reference.session_id.clone(),
            created_at_unix_millis: reference.created_at_unix_millis,
            expires_at_unix_millis: reference.expires_at_unix_millis,
        },
    };
    binding
        .upload_receipt_fingerprint(&reference.source_sha256, reference.source_byte_len)
        .map_err(|error| error.to_string())?;
    Ok(binding)
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
// Media preparation keeps source identity, frozen execution and cancellation ownership separate.
// 媒体准备分别保留来源身份、冻结执行及取消所有权。
#[expect(
    clippy::too_many_arguments,
    reason = "Media preparation keeps source identity, frozen execution and cancellation ownership separate."
)]
pub(crate) async fn service_plan_composer_attachments(
    state: SharedServiceBridgeState,
    project_id: Option<String>,
    conversation_id: Option<String>,
    draft_slot_key: String,
    generation: u64,
    model_selection: SubmissionModelSelection,
    attachments: Vec<ComposerAttachmentSummary>,
    retry_uploads: bool,
    compress_images: bool,
) -> Result<Vec<ComposerAttachmentPreparationSummary>, String> {
    // The frontend's lease barrier precedes planning; the host verifies the exact live scope again for each asset.
    // 前端租约屏障先于规划；宿主仍为每个资产重新核对精确活动作用域。
    let draft_scope = crate::attachments::composer_attachment_draft_scope_for_owner(
        &draft_slot_key,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?;
    let workspace_path = composer_attachment_workspace_path(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?;
    let (controller_runtime, agent_runtime, _, _, _) = runtime_parts_from_state(&state)?;
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
    // Finish image normalization for the whole draft before recording any upload intention.
    // 在记录任何上传意图前完成整份草稿的图片规范化。
    if let Some(converted) = image_conversion::prepare_image_conversions(
        &workspace_path,
        &draft_scope,
        generation,
        &execution,
        &attachments,
        compress_images,
    )
    .await?
    {
        return Ok(converted);
    }
    let mut summaries = Vec::with_capacity(attachments.len());
    // uploadPlans are published only after every attachment has been validated and planned.
    // uploadPlans 仅在全部附件完成校验与规划后统一发布。
    let mut upload_plans = Vec::new();
    // Freeze one shared policy budget before any draft upload plan is published.
    // 在任何草稿上传计划发布前冻结一个共享策略预算。
    let mut media_planner = ComposerMediaDeliveryPlanner::new(execution.media_delivery_policy)?;
    // Project references use their exact V1 owner and one catalog snapshot, without acquiring a staging lease.
    // 项目引用使用精确V1所有者及一份目录快照，不取得暂存租约。
    let project_context = if attachments
        .iter()
        .any(|attachment| attachment.project_resource_id.is_some())
    {
        // An unmaterialized composer cannot name another session's project resource.
        // 未物化输入区不能引用另一会话的项目资源。
        let session_id = session_id.ok_or_else(|| {
            String::from("Project composer attachments require a materialized V1 session.")
        })?;
        // Clone the existing unique registry owner before asynchronous loading.
        // 异步加载之前克隆既有唯一注册表所有者。
        let registry = Arc::clone(
            &state
                .lock()
                .map_err(|error| error.to_string())?
                .session_runtime_registry,
        );
        // Exact session loading never falls back to another project or a latest global conversation.
        // 精确会话加载绝不回退到另一项目或最近全局会话。
        let handle = crate::session_v1_lifecycle::get_or_load_session_v1(
            &controller_runtime,
            &registry,
            session_id,
        )
        .await
        .map_err(|error| error.to_string())?;
        // The locator supplies the sole project partition for all source and receipt lookups.
        // 定位器为全部来源及回执查询提供唯一项目分区。
        let locator = handle
            .storage_locator()
            .map_err(|error| error.to_string())?;
        // Only metadata is loaded during planning; source bytes remain deferred.
        // 规划期间只加载元数据；来源字节保持延迟读取。
        let catalog = vulcan_workspace_store::workspace_store::load_project_resource_catalog(
            &locator.project_directory(),
        )
        .map_err(|error| error.to_string())?;
        Some((handle, locator, catalog))
    } else {
        None
    };
    for (position, summary) in attachments.into_iter().enumerate() {
        if let Some(raw_resource_id) = summary.project_resource_id {
            crate::conversation_resources_v1::validate_project_resource_attachment_reference(
                &summary,
            )?;
            // The exact occurrence carries its original input anchor independently of this rendered position.
            // 精确出现独立于本次渲染位置携带其原输入锚点。
            let resource_id =
                crate::conversation_resources_v1::checked_resource_id(raw_resource_id)?;
            // The branch above created the project owner only for real project references.
            // 上述分支只为真实项目引用创建项目所有者。
            let (handle, locator, catalog) = project_context.as_ref().ok_or_else(|| {
                String::from("Project attachment planning has no exact source owner.")
            })?;
            // Rebuild canonical display facts from metadata rather than trusting browser fields or loading the whole file.
            // 从元数据重建规范显示事实，不信任浏览器字段，也不加载整个文件。
            let attachment = composer_attachment_summary_to_core(
                crate::conversation_resources_v1::composer_attachment_from_project_resource_v1(
                    locator,
                    raw_resource_id,
                )?,
            )?;
            // Missing source proof remains unknown; identical ResourceIds never select a replacement source occurrence.
            // 缺失来源证明保持未知；相同ResourceId绝不选择替代来源出现。
            let preparation = match &summary.prompt_source {
                Some(source) => handle
                    .read(|document| document.user_prompt_resource_preparation(source, resource_id))
                    .map_err(|error| error.to_string())?
                    .map_err(|error| error.to_string())?,
                None => vulcan_session_core::v1::UserPromptResourcePreparation::Unproven,
            };
            // The catalog row is exact and immutable, including all append-only private receipts.
            // 目录行是精确且不可变的，包含全部仅追加私有回执。
            let entry = catalog
                .entries
                .iter()
                .find(|entry| entry.resource_id == resource_id)
                .ok_or_else(|| {
                    String::from("Project attachment catalog entry disappeared during planning.")
                })?;
            // Availability checks use metadata only and reserve no budget for unavailable sources.
            // 可用性检查只使用元数据，不为不可用来源预留预算。
            let (delivery_plan, reason) =
                if vulcan_workspace_store::workspace_store::resolve_project_resource_source(
                    &locator.project_directory(),
                    catalog,
                    resource_id,
                )
                .is_ok_and(|source| source.is_some())
                {
                    media_planner.plan_project(
                        &execution,
                        &attachment,
                        entry,
                        &preparation,
                        locator.session_id(),
                    )?
                } else {
                    (
                        ComposerAttachmentDeliveryPlan::MetadataOnly,
                        "project_source_unavailable",
                    )
                };
            // Existing completed uploads are already ready; genuinely unprepared project media is prepared only by its next prompt.
            // 既有完成上传已经就绪；真正未准备的项目媒体只由其下一提示词准备。
            let restored_upload = matches!(preparation, vulcan_session_core::v1::UserPromptResourcePreparation::Prepared(ref original) if matches!(original.delivery, vulcan_session_core::v1::UserPromptResourceDeliveryKind::Uploaded { .. } | vulcan_session_core::v1::UserPromptResourceDeliveryKind::UploadPending { draft_intent_id: Some(_), .. }));
            summaries.push(ComposerAttachmentPreparationSummary {
                converted_attachment: None,
                attachment_id: format!("project-resource:{raw_resource_id}:{position}"),
                generation,
                transfer_id: format!("project-media-view:{}", uuid::Uuid::new_v4()),
                physical_transfer_id: None,
                delivery_plan,
                phase: if delivery_plan == ComposerAttachmentDeliveryPlan::MetadataOnly {
                    ComposerAttachmentPreparationPhase::MetadataOnly
                } else if delivery_plan == ComposerAttachmentDeliveryPlan::UploadReference
                    && restored_upload
                {
                    ComposerAttachmentPreparationPhase::DirectReady
                } else {
                    ComposerAttachmentPreparationPhase::LocalReady
                },
                blocks_submission: false,
                route_revision: Some(execution.route.route_revision.clone()),
                reason: if delivery_plan == ComposerAttachmentDeliveryPlan::UploadReference
                    && !restored_upload
                {
                    "project_preparation_deferred_to_prompt".into()
                } else {
                    reason.into()
                },
                error: None,
            });
            continue;
        }
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
        // Read original intent from its exact live lease, never from the most recent provider cache entry.
        // 从精确活动租约读取原意图，绝不使用最近供应商缓存条目。
        let mut original =
            vulcan_agent_runtime::attachment_asset_index::load_attachment_draft_media(
                &attachment,
                &draft_scope,
            )?;
        // Decide one chronological occurrence with the shared batch budget.
        // 使用共享批次预算决定一次时序出现。
        let (mut delivery_plan, mut reason) = media_planner.plan_draft(
            execution.as_ref(),
            &attachment,
            original.as_ref(),
            conversation_id.as_deref(),
        )?;
        // Only the actual first writer may create an automatic first physical-start permission.
        // 只有真正首次写入者可以创建自动首次实体启动许可。
        let mut created_original = false;
        if original.is_none()
            && matches!(
                delivery_plan,
                ComposerAttachmentDeliveryPlan::InlineOriginal
                    | ComposerAttachmentDeliveryPlan::UploadReference
            )
        {
            // The first admitted native plan owns the draft mode even if a later model only displays its address.
            // 首次准入原生规划持有草稿方式，即使后续模型只显示其地址。
            let delivery = if delivery_plan == ComposerAttachmentDeliveryPlan::InlineOriginal {
                vulcan_agent_runtime::attachment_asset_index::AttachmentDraftMediaDelivery::Inline
            } else {
                // Reuse the exact route resolver; no fallback interface is introduced for provenance storage.
                // 复用精确路线解析器；来源存储不引入回退接口。
                let contract = execution
                    .client
                    .conversation_media_upload_contract(
                        &execution.route,
                        attachment_kind(attachment.kind.clone()),
                        &attachment.mime_type,
                    )?
                    .ok_or_else(|| String::from("Draft upload plan has no exact contract."))?;
                vulcan_agent_runtime::attachment_asset_index::AttachmentDraftMediaDelivery::Upload {
                    upload_channel_id: contract.upload_channel_id,
                    upload_contract_fingerprint: contract.contract_fingerprint,
                    receipt: None,
                }
            };
            // Atomic first-writer selection prevents later model planning from overwriting the first intention.
            // 原子首次写入选择阻止后续模型规划覆盖首次意图。
            let proposed =
                vulcan_agent_runtime::attachment_asset_index::AttachmentDraftMediaOrigin::new(
                    &attachment,
                    composer_media_original_route(&execution),
                    delivery,
                )?;
            // A concurrent winner requires a fresh whole-batch plan rather than double-reserving this resource's budget.
            // 并发胜出者要求重新规划整批，而不是为本资源重复预留预算。
            let retained =
                vulcan_agent_runtime::attachment_asset_index::record_attachment_draft_media(
                    &attachment,
                    &draft_scope,
                    proposed.clone(),
                )?;
            if retained.original_route != proposed.original_route
                || retained.delivery != proposed.delivery
            {
                return Err(String::from(
                    "Draft media original intent changed during planning; replan the exact draft.",
                ));
            }
            created_original = retained.intent_id == proposed.intent_id;
            original = Some(retained);
        }
        // A restored receipt is already ready and must not schedule another upload or synthetic progress.
        // 已恢复回执已经就绪，不能调度另一上传或伪造进度。
        let upload_ready = original.as_ref().is_some_and(|original| matches!(original.delivery, vulcan_agent_runtime::attachment_asset_index::AttachmentDraftMediaDelivery::Upload { receipt: Some(_), .. }));
        // Metadata views may observe the original transfer for removal, but never receive new start authority.
        // 元数据视图可以为移除而观察原传输，但绝不获得新启动权限。
        let reservation = if let Some(original) = original.as_ref().filter(|original| matches!(original.delivery, vulcan_agent_runtime::attachment_asset_index::AttachmentDraftMediaDelivery::Upload { receipt: None, .. })) {
            draft_execution::resolve_reservation(DraftUploadIntentIdentity {
                workspace_path: workspace_path.clone(),
                draft_scope: draft_scope.clone(),
                intent_id: original.intent_id.clone(),
                attachment_id: attachment_id.clone(),
            }, &execution, delivery_plan == ComposerAttachmentDeliveryPlan::UploadReference && (created_original || retry_uploads))?
        } else {
            None
        };
        if delivery_plan == ComposerAttachmentDeliveryPlan::UploadReference
            && !upload_ready
            && reservation.is_none()
        {
            // An old pending intention without a live execution/plan is not proof that its upload never started.
            // 没有活动执行或计划的旧待回执意图，不能证明其上传从未开始。
            media_planner.discard_upload_count(attachment_kind(attachment.kind.clone()))?;
            delivery_plan = ComposerAttachmentDeliveryPlan::MetadataOnly;
            reason = "original_upload_requires_explicit_retry";
        }
        let phase = match delivery_plan {
            ComposerAttachmentDeliveryPlan::UploadReference => {
                if upload_ready {
                    ComposerAttachmentPreparationPhase::DirectReady
                } else {
                    ComposerAttachmentPreparationPhase::UploadQueued
                }
            }
            ComposerAttachmentDeliveryPlan::MetadataOnly => {
                if reason == "original_upload_requires_explicit_retry" {
                    ComposerAttachmentPreparationPhase::Failed
                } else {
                    ComposerAttachmentPreparationPhase::MetadataOnly
                }
            }
            ComposerAttachmentDeliveryPlan::InlineOriginal
            | ComposerAttachmentDeliveryPlan::InlineDerivedPreview => {
                ComposerAttachmentPreparationPhase::LocalReady
            }
        };
        // A foreign metadata view points cancellation at the real original operation, not a fabricated new upload.
        // 外部模型的元数据视图将取消指向真实原操作，而非虚构的新上传。
        let transfer_id = if delivery_plan == ComposerAttachmentDeliveryPlan::MetadataOnly {
            reservation
                .as_ref()
                .and_then(|reservation| reservation.started.get())
                .map(|operation| operation.transfer_id.clone())
                .unwrap_or_else(|| format!("draft-media-view:{}", uuid::Uuid::new_v4()))
        } else {
            format!("draft-media:{}", uuid::Uuid::new_v4())
        };
        // Actual progress remains attached to the original operation even for a later observing view.
        // 即使是后续观察视图，实际进度也始终附着于原操作。
        let physical_transfer_id = reservation
            .as_ref()
            .and_then(|reservation| reservation.started.get())
            .map(|operation| operation.transfer_id.clone());
        if delivery_plan == ComposerAttachmentDeliveryPlan::UploadReference && !upload_ready {
            upload_plans.push((
                transfer_id.clone(),
                DraftMediaUploadPlan {
                    reservation: reservation.ok_or_else(|| {
                        String::from("Draft upload has no start or observation reservation.")
                    })?,
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
            converted_attachment: None,
            attachment_id,
            generation,
            transfer_id,
            physical_transfer_id,
            delivery_plan,
            phase,
            blocks_submission: delivery_plan == ComposerAttachmentDeliveryPlan::UploadReference
                && !upload_ready,
            route_revision: Some(execution.route.route_revision.clone()),
            reason: reason.to_string(),
            error: None,
        });
    }
    register_draft_media_upload_plans(upload_plans)?;
    Ok(summaries)
}

/// Starts or observes the exact original upload and returns a closed state for this caller's view.
/// 启动或观察精确原上传，并返回当前调用方视图的封闭状态。
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
    let (frozen_plan, start) = consume_draft_media_upload_plan(
        preparation.transfer_id.as_str(),
        workspace_path.as_str(),
        &conversation_id,
        &model_selection,
        &preparation,
    )?;
    // Rebuild progress identity from the consumed reservation instead of echoing an untrusted browser hint.
    // 从已消费许可重建进度身份，不回显不可信浏览器提示。
    let preparation = ComposerAttachmentPreparationSummary {
        physical_transfer_id: Some(
            frozen_plan
                .reservation
                .started
                .get()
                .ok_or_else(|| String::from("Consumed draft upload has no original operation."))?
                .transfer_id
                .clone(),
        ),
        ..preparation
    };
    // A later model view waits for the exact original operation; dropping this RPC cannot cancel its owner.
    // 后续模型视图等待精确原操作；丢弃本RPC不能取消其所有者。
    let mut registration = match start {
        DraftUploadPreparation::Start(registration) => registration,
        DraftUploadPreparation::Observe(observer) => {
            observer.operation.clone().wait().await?;
            return draft_execution::project_observed_upload(
                &frozen_plan,
                &attachment,
                conversation_id.as_deref(),
                preparation,
            );
        }
    };
    // The unique registration guard spans every early return and transfers to any known-result worker before its next await.
    // 唯一登记守卫覆盖每个提前返回，并在下一次等待前交给任何已知结果工作线程。
    let cancellation = registration.cancellation.clone();
    // Validate original ownership and acquire physical source protection in one asset-index transaction.
    // 在同一资产索引事务内校验原所有权并取得实体来源保护。
    let original = registration.retain_draft_source(
        &attachment,
        &frozen_plan.reservation.identity.draft_scope,
        &frozen_plan.reservation.identity.intent_id,
    )?;
    if matches!(
        original.delivery,
        vulcan_agent_runtime::attachment_asset_index::AttachmentDraftMediaDelivery::Upload {
            receipt: Some(_),
            ..
        }
    ) {
        // A concurrent exact upload already finished; return its current view without another upload or fake progress.
        // 并发精确上传已经完成；不执行另一上传或伪造进度，直接返回其当前视图。
        let projection =
            ComposerMediaDeliveryPlanner::new(frozen_plan.execution.media_delivery_policy)
                .and_then(|mut planner| {
                    planner.plan_draft(
                        &frozen_plan.execution,
                        &attachment,
                        Some(&original),
                        conversation_id.as_deref(),
                    )
                });
        // The original reference may have expired since planning; report metadata without replacement upload.
        // 原引用可能在规划后过期；报告元数据，不进行替代上传。
        let (delivery_plan, reason) = projection?;
        return Ok(ComposerAttachmentPreparationSummary {
            delivery_plan,
            phase: if delivery_plan == ComposerAttachmentDeliveryPlan::UploadReference {
                ComposerAttachmentPreparationPhase::DirectReady
            } else {
                ComposerAttachmentPreparationPhase::MetadataOnly
            },
            blocks_submission: false,
            reason: reason.into(),
            error: None,
            ..preparation
        });
    }
    if !matches!(
        original.delivery,
        vulcan_agent_runtime::attachment_asset_index::AttachmentDraftMediaDelivery::Upload {
            receipt: None,
            ..
        }
    ) {
        return Err(String::from(
            "Draft preparation requires its original unfinished upload intention.",
        ));
    }
    // The consumed plan selected upload before any I/O; later soft inline preferences cannot reclassify this intention.
    // 已消费规划在任何I/O之前选定上传；后续软内联偏好不能重新分类此意图。
    let upload_attachment = apply_composer_delivery_plan_to_attachment(
        &attachment,
        ComposerAttachmentDeliveryPlan::UploadReference,
    );
    // Retain frozen receipt authority after the preparation helper consumes its execution clone.
    // 准备辅助函数消费执行克隆后，仍保留冻结回执权威。
    let receipt_execution = Arc::clone(&frozen_plan.reservation.execution);
    // Only the actual parent token proves user cancellation; timeout text or a channel name cannot classify it.
    // 只有实际父令牌证明用户取消；超时文本或通道名称不能决定此分类。
    let cancellation_observer = cancellation.clone();
    let result = prepare_submission_media_with_frozen_execution(
        &app,
        &state,
        &agent_runtime,
        preparation.transfer_id.as_str(),
        session_id,
        None,
        Some(&model_selection),
        std::slice::from_ref(&upload_attachment),
        Arc::clone(&frozen_plan.reservation.execution),
        cancellation,
    )
    .await;
    match result {
        Ok(prepared) => {
            // One draft preparation owns exactly one source and cannot report readiness without one proven receipt.
            // 一次草稿准备精确持有一个来源，缺少已证明回执时不能报告就绪。
            let [reference] = prepared.references.as_slice() else {
                return Err(String::from(
                    "Draft upload did not produce exactly one original source receipt.",
                ));
            };
            // Bind completion to the original lease or its exact registered prompt handoff, never the current draft.
            // 将完成绑定到原租约或其精确已登记提示词交接，绝不绑定当前草稿。
            let binding = draft_upload_binding(reference, &receipt_execution, &attachment)?;
            // Capture the existing catalog owner before the next await; known completion must survive RPC future cancellation.
            // 在下一次等待前捕获既有目录所有者；已知完成必须跨RPC Future取消保留。
            let resource_registry = state
                .lock()
                .map_err(|_| {
                    String::from("Service state is unavailable during draft upload completion.")
                })?
                .project_resource_catalog_registry
                .clone();
            // Known successful publication keeps the same pending owner alive if the requesting RPC future is dropped.
            // 即使请求RPC Future被丢弃，已知成功发布也保持同一待处理所有者存活。
            registration.spawn_known_completion(move || {
                vulcan_agent_runtime::attachment_asset_index::complete_attachment_draft_upload(
                    &attachment,
                    &frozen_plan.reservation.identity.draft_scope,
                    &original,
                    binding,
                    &resource_registry,
                )?;
                // Preserve the existing route cache only after the original intent has durably accepted this result.
                // 仅在原意图可靠接纳本结果后保留既有路线缓存。
                for reference in &prepared.references {
                    vulcan_agent_runtime::attachment_asset_index::record_attachment_media_reference(
                        &attachment, attachment_media_reference_from_prepared(reference),
                    )?;
                }
                Ok::<(), String>(())
            })
            .await
            .map_err(|_| String::from("Draft upload completion worker failed."))??;
            Ok(ComposerAttachmentPreparationSummary {
                phase: ComposerAttachmentPreparationPhase::DirectReady,
                blocks_submission: false,
                error: None,
                ..preparation
            })
        }
        Err(_) if cancellation_observer.is_cancelled() => {
            Ok(ComposerAttachmentPreparationSummary {
                phase: ComposerAttachmentPreparationPhase::Cancelled,
                blocks_submission: false,
                error: None,
                ..preparation
            })
        }
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
        tracing::warn!("[media-upload] structured telemetry was rejected: {error}");
    }
}

/// Defers every provisional `attachment` to the authoritative V1 request preparation without reading or uploading media.
/// 将每个临时`attachment`延迟到权威V1请求准备，不读取或上传媒体。
/// Returns request-local metadata-only copies; canonical prompt state and its original delivery remain unchanged.
/// 返回请求局部仅元数据副本；规范提示词状态及其原投递保持不变。
pub(crate) fn defer_attachment_delivery_to_v1(
    attachments: &[ConversationAttachment],
) -> Vec<ConversationAttachment> {
    attachments
        .iter()
        .cloned()
        .map(|mut attachment| {
            attachment.delivery_mode = Some(ConversationAttachmentDeliveryMode::MetadataOnly);
            attachment
        })
        .collect()
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
    prepare_submission_media_inner(
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
    .await
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
// Media preparation keeps source identity, frozen execution and cancellation ownership separate.
// 媒体准备分别保留来源身份、冻结执行及取消所有权。
#[expect(
    clippy::too_many_arguments,
    reason = "Media preparation keeps source identity, frozen execution and cancellation ownership separate."
)]
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
    // Only explicit current attachments own new preparation; history remains with its chronological prompt owner.
    // 只有显式当前附件持有新准备意图；历史仍归其时序提示词所有者。
    // Filter before counting progress rows so skipped history neither uploads nor inflates the current batch.
    // 在统计进度行前过滤，使跳过的历史既不上传也不扩大当前批次数量。
    let preparation_attachments =
        collect_media_preparation_attachments(session_runtime_state, attachments)
            .into_iter()
            .filter(|attachment| attachment.attach_current_owner)
            .collect::<Vec<_>>();
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
        // The shared frozen composer projection already validated both inline and uploaded reference forms.
        // 共用的冻结输入区投影已经验证内联和上传引用两种形态。
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
        // One selected-channel deadline starts before replay-path, source fingerprint, cache or credential preparation.
        // 单一选中通道截止时间在回放路径、来源指纹、缓存或凭据准备之前启动。
        let deadline =
            vulcan_provider_adapters::MediaUploadDeadline::for_contract(&upload_contract)
                .map_err(|failure| failure.to_string())?;
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
        // Offload exact local metadata lookup so the async parent can observe its original deadline while the OS call settles.
        // 转移精确本地元数据查询，使异步父任务可在操作系统调用结算期间观察原截止时间。
        let path_attachment = attachment.clone();
        // This owner bit comes from the actual preparation occurrence, not a current UI selection.
        // 本所有权位来自实际准备出现，而非当前UI选择。
        let attach_current_owner = preparation_attachment.attach_current_owner;
        let Some(media_source_path) = deadline
            .wait(
                cancellation,
                tokio::task::spawn_blocking(move || {
                    media_preparation_replay_path(&path_attachment, attach_current_owner)
                }),
            )
            .await
            .map_err(|failure| failure.to_string())?
            .map_err(|_| String::from("Media source metadata worker failed."))??
        else {
            // Historical unavailable media remains canonical and is downgraded to metadata by the
            // final provider-input projection; it must not abort a new user turn.
            // 历史不可用媒体仍保留在规范历史中，并由最终 Provider 输入投影降级为元数据；它不得中止
            // 新的用户轮次。
            continue;
        };
        // Source size was admitted by the frozen composer policy and remains the bound for every subsequent read.
        // 来源大小已由冻结输入区策略准入，并持续约束后续每次读取。
        let admitted_byte_len = attachment.byte_len.ok_or_else(|| {
            String::from("Prepared attachment requires an exact admitted source size.")
        })?;
        let source_identity = deadline
            .wait(
                cancellation,
                execution.client.read_admitted_media_source_identity(
                    media_source_path.as_path(),
                    admitted_byte_len,
                    Some(cancellation),
                ),
            )
            .await
            .map_err(|failure| failure.to_string())??;
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
            // The exact asset-index read has the same budget and cannot block the async caller past expiry.
            // 精确资产索引读取具有相同预算，不能使异步调用方阻塞超过到期时间。
            let indexed_attachment = attachment.clone();
            if let Some(indexed_reference) = deadline.wait(cancellation, tokio::task::spawn_blocking(move || vulcan_agent_runtime::attachment_asset_index::find_attachment_media_reference(
                    &indexed_attachment, &indexed_query, now_unix_millis,
                ))).await.map_err(|failure| failure.to_string())?.map_err(|_| String::from("Media receipt lookup worker failed."))??
            {
                deadline.wait(cancellation, execution
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
                    )).await.map_err(|failure| failure.to_string())??;
                prepared_submission_media.push_reference(
                    prepared_media_reference_from_attachment_index(&indexed_reference),
                );
                continue;
            }
        }
        if let Some(prepared_reference) = deadline
            .wait(
                cancellation,
                execution
                    .client
                    .prepared_media_reference_for_admitted_source(
                        &execution.route,
                        media_source_path.as_path(),
                        resource_kind,
                        attachment.mime_type.as_str(),
                        &source_identity,
                        session_scope_id.as_deref(),
                        Some(cancellation),
                    ),
            )
            .await
            .map_err(|failure| failure.to_string())?
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
        let prepared = deadline
            .wait(cancellation, async {
                let secret = execution
                    .credential
                    .resolve_for_request()
                    .await
                    .map_err(|_| {
                        vulcan_provider_adapters::MediaUploadFailure::CredentialUnavailable
                    })?;
                execution
                    .client
                    .prepare_local_media_typed(
                        &execution.route,
                        model_id.as_str(),
                        &secret,
                        vulcan_provider_adapters::AdmittedLocalMediaSource {
                            path: media_source_path.as_path(),
                            file_name: &attachment.file_name,
                            mime_type: &attachment.mime_type,
                            identity: &source_identity,
                        },
                        resource_kind,
                        false,
                        vulcan_provider_adapters::LocalMediaMaterialization::ProviderUpload {
                            source_byte_len: source_identity.byte_len,
                        },
                        Some(progress),
                        cancellation,
                    )
                    .await
            })
            .await
            .and_then(|result| result)
            .map_err(|failure| failure.to_string());
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(_) if cancellation.is_cancelled() => {
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
            vulcan_provider_adapters::ConversationPreparedMediaRetention {
                session_scope_id: session_scope_id.as_deref(),
                wall_clock_lifetime: Some((
                    prepared_created_at_unix_millis,
                    prepared_expires_at_unix_millis,
                )),
            },
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
        if let Some(existing) = collected.iter_mut().find(|existing| {
            existing.attach_current_owner
                && same_media_preparation_attachment(&existing.attachment, attachment)
        }) {
            // Deduplicate only this explicit submission; never merge its new receipt with historical prompt owners.
            // 仅对本次显式提交去重，绝不把其新回执与历史提示词所有者合并。
            existing.attachment = attachment.clone();
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
    }
}

/// Converts one freshly verified source and upload contract into a ledger candidate.
/// 将一个刚验证的源与上传契约转换为账本候选项。
// Media preparation keeps source identity, frozen execution and cancellation ownership separate.
// 媒体准备分别保留来源身份、冻结执行及取消所有权。
#[expect(
    clippy::too_many_arguments,
    reason = "Media preparation keeps source identity, frozen execution and cancellation ownership separate."
)]
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

/// Runs the real project composer planner for one canonical fixture occurrence without exposing the planner type in production.
/// 为一个规范夹具出现运行真实项目输入区规划器，不在生产暴露规划器类型。
/// Inputs are exact frozen execution, metadata, source proof and session; returns the actual mode and safe reason.
/// 输入为精确冻结执行、元数据、来源证明及会话；返回真实方式和安全原因。
#[cfg(test)]
pub(crate) fn test_composer_project_decision(
    execution: &vulcan_provider_adapters::ResolvedPublicAiExecution,
    attachment: &ConversationAttachment,
    entry: &vulcan_workspace_store::workspace_store::ResourceCatalogEntry,
    preparation: &vulcan_session_core::v1::UserPromptResourcePreparation,
    session_id: &str,
) -> Result<(ComposerAttachmentDeliveryPlan, &'static str), String> {
    ComposerMediaDeliveryPlanner::new(execution.media_delivery_policy)?.plan_project(
        execution,
        attachment,
        entry,
        preparation,
        session_id,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        DraftMediaUploadPlanIdentity, apply_composer_delivery_plan_to_attachment,
        collect_media_preparation_attachments, draft_media_upload_plan_matches,
        media_preparation_replay_path, register_media_upload,
        submission_media_reference_is_usable_for_current_attachment,
    };
    use crate::conversation_submission::SubmissionModelSelection;
    use std::collections::BTreeSet;
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
            converted_attachment: None,
            attachment_id: String::from("asset-a"),
            generation: 7,
            transfer_id: String::from("draft-media:transfer-a"),
            physical_transfer_id: None,
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
        let registration = register_media_upload(client_submission_id).expect("first register");
        match register_media_upload(client_submission_id) {
            Err(error) => assert!(error.contains("already preparing media")),
            Ok(_) => panic!("duplicate registration must be rejected"),
        }
        drop(registration);
    }

    /// Verifies current reattachment is a distinct intent and cannot rebind any historical receipt owner.
    /// 验证当前重新附加属于独立意图，不能重新绑定任何历史回执所有者。
    #[test]
    fn historical_video_owners_stay_separate_from_current_upload_intent() {
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

        // The current request projection is distinct from the canonical attachment stored in earlier prompts.
        // 当前请求投影区别于更早提示词中保存的规范附件。
        first_video.delivery_mode = Some(ConversationAttachmentDeliveryMode::InlineOriginal);
        let collected = collect_media_preparation_attachments(
            Some(&runtime_state),
            std::slice::from_ref(&first_video),
        );
        assert_eq!(collected.len(), 3);
        let first = collected
            .iter()
            .find(|candidate| {
                !candidate.attach_current_owner && candidate.attachment.local_path == "first.mp4"
            })
            .expect("first historical video should be collected");
        assert_eq!(
            first.owner_message_ids,
            BTreeSet::from([String::from("old-a"), String::from("old-b")])
        );
        assert!(!first.attach_current_owner);
        assert_eq!(first.attachment.delivery_mode, None);
        let current = collected
            .iter()
            .find(|candidate| candidate.attach_current_owner)
            .expect("explicit current intent must remain separate");
        assert!(current.owner_message_ids.is_empty());
        assert_eq!(
            current.attachment.delivery_mode,
            Some(ConversationAttachmentDeliveryMode::InlineOriginal)
        );
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
}
