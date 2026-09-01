//! One physical execution per original draft upload, with independently disposable view observers.
//! 每个原草稿上传仅有一次实体执行，视图观察者可独立丢弃。

use super::*;

/// Exact original intent owner; neither a file-cache key nor a rendered generation may substitute for it.
/// 精确原意图所有者；文件缓存键与渲染代际均不能替代它。
#[derive(Clone, PartialEq, Eq)]
pub(super) struct DraftUploadIntentIdentity {
    /// Canonical workspace owning the asset index.
    /// 持有资产索引的规范工作区。
    pub(super) workspace_path: String,
    /// Exact persistent draft lease scope.
    /// 精确持久草稿租约作用域。
    pub(super) draft_scope: String,
    /// Immutable UUID assigned by the original draft intention.
    /// 原草稿意图分配的不可变UUID。
    pub(super) intent_id: String,
    /// Canonical asset identifier validated by the asset index.
    /// 经资产索引校验的规范资产标识。
    pub(super) attachment_id: String,
}

/// A shared start permission remains single-use even after its original operation has settled.
/// 共享启动许可即使在原操作结算后也保持只能消费一次。
pub(super) struct DraftUploadReservation {
    /// Exact original owner, independent of the current model view.
    /// 精确原所有者，独立于当前模型视图。
    pub(super) identity: DraftUploadIntentIdentity,
    /// Original frozen execution selected before any physical work.
    /// 任何实体工作开始前选定的原冻结执行。
    pub(super) execution: Arc<vulcan_provider_adapters::ResolvedPublicAiExecution>,
    /// Set once under the existing plan-then-pending registry locks.
    /// 在既有计划锁先于待处理锁的顺序下仅设置一次。
    pub(super) started: OnceLock<DraftUploadObservation>,
}

/// Read-only physical operation facts do not own or cancel the actual upload when dropped.
/// 只读实体操作事实不持有实际上传，丢弃时也不取消它。
#[derive(Clone)]
pub(super) struct DraftUploadObservation {
    /// Original progress/cancellation identity, not a later view-plan id.
    /// 原进度与取消身份，而非后续视图计划ID。
    pub(super) transfer_id: String,
    /// Exact cancellation token shared with the physical owner.
    /// 与实体所有者共享的精确取消令牌。
    pub(super) cancellation: MediaUploadCancellation,
    /// Retained terminal acknowledgement, including completion before a new observer starts.
    /// 保留的终态确认，包含新观察者开始之前已经发生的完成。
    pub(super) settled: tokio::sync::watch::Receiver<bool>,
}

impl DraftUploadObservation {
    /// Waits for this exact operation without cancelling it; returns an error for an unacknowledged close.
    /// 等待本精确操作但不取消它；未经确认便关闭时返回错误。
    pub(super) async fn wait(mut self) -> Result<(), String> {
        loop {
            if *self.settled.borrow_and_update() {
                return Ok(());
            }
            self.settled.changed().await.map_err(|_| {
                String::from("Original draft upload ended without a settlement acknowledgement.")
            })?;
        }
    }
}

/// An alias authorizes explicit cancellation, but dropping its observer only removes that alias.
/// 别名授权显式取消，但丢弃其观察者只移除此别名。
pub(super) struct DraftUploadObserver {
    /// Unique later view-plan id registered as a cancellation alias.
    /// 登记为取消别名的唯一后续视图计划ID。
    pub(super) alias_id: String,
    /// Exact original operation observed by this alias.
    /// 此别名观察的精确原操作。
    pub(super) operation: DraftUploadObservation,
}

impl Drop for DraftUploadObserver {
    /// Removes this exact alias without signalling the physical owner or deleting a replacement.
    /// 移除此精确别名，不通知实体所有者，也不删除替换条目。
    fn drop(&mut self) {
        if let Ok(mut pending) = pending_media_uploads().lock()
            && pending.get(&self.alias_id).is_some_and(|entry| {
                entry
                    .cancellation
                    .same_operation(&self.operation.cancellation)
            })
        {
            pending.remove(&self.alias_id);
        }
    }
}

/// A consumed view either owns the one physical start or observes a prior start.
/// 已消费视图要么持有唯一实体启动，要么观察此前启动。
pub(super) enum DraftUploadPreparation {
    /// Unique physical owner, retained through known-result publication.
    /// 唯一实体所有者，在已知结果发布期间持续持有。
    Start(PendingMediaUploadRegistration),
    /// Disposable observer; its drop never cancels the original operation.
    /// 可丢弃观察者；其丢弃绝不取消原操作。
    Observe(DraftUploadObserver),
}

/// Finds the exact reservation in the existing registries; callers hold the plan lock before the pending lock.
/// 在既有注册表内查找精确许可；调用方先持有计划锁，再持有待处理锁。
/// Returns none only when no live plan/operation retains this original intent; conflicting ownership is an error.
/// 仅当无活动计划或操作保留此原意图时返回空；所有权冲突则报错。
pub(super) fn find_reservation(
    identity: &DraftUploadIntentIdentity,
    plans: &BTreeMap<String, DraftMediaUploadPlan>,
    pending: &BTreeMap<String, PendingMediaUpload>,
) -> Result<Option<Arc<DraftUploadReservation>>, String> {
    // Both routes represent the same documented lifecycle: queued view or already-started operation.
    // 两条路径代表同一已声明生命周期：排队视图或已经启动的操作。
    let retained = pending
        .values()
        .filter_map(|entry| entry.draft.as_ref())
        .chain(plans.values().map(|plan| &plan.reservation))
        .filter(|reservation| {
            reservation
                .started
                .get()
                .is_none_or(|operation| !*operation.settled.borrow())
        })
        .find(|reservation| reservation.identity.intent_id == identity.intent_id);
    match retained {
        Some(reservation) if &reservation.identity != identity => Err(String::from(
            "Draft upload intent is already retained by a different exact owner.",
        )),
        Some(reservation) => Ok(Some(Arc::clone(reservation))),
        None => Ok(None),
    }
}

/// Resolves a live original reservation, creating one only for a first intent or explicit user retry.
/// 解析活动原许可，仅为首次意图或用户显式重试创建许可。
/// Parameters identify canonical provenance and frozen execution; no source bytes or credentials are read here.
/// 参数标识规范来源与冻结执行；此处不读取来源字节或凭据。
pub(super) fn resolve_reservation(
    identity: DraftUploadIntentIdentity,
    execution: &Arc<vulcan_provider_adapters::ResolvedPublicAiExecution>,
    allow_start: bool,
) -> Result<Option<Arc<DraftUploadReservation>>, String> {
    // Preserve the registry lock order used by consumption and explicit cancellation.
    // 保持与消费及显式取消相同的注册表加锁顺序。
    let mut plans = draft_media_upload_plans()
        .lock()
        .map_err(|error| error.to_string())?;
    retain_live_draft_media_upload_plans(&mut plans);
    let pending = pending_media_uploads()
        .lock()
        .map_err(|error| error.to_string())?;
    if let Some(reservation) = find_reservation(&identity, &plans, &pending)? {
        return Ok(Some(reservation));
    }
    Ok(allow_start.then(|| {
        Arc::new(DraftUploadReservation {
            identity,
            execution: Arc::clone(execution),
            started: OnceLock::new(),
        })
    }))
}

/// Drops unused view plans without cancelling any original physical operation or changing a draft lease.
/// 丢弃未使用视图计划，不取消任何原实体操作，也不修改草稿租约。
/// Parameter: transfer_ids are opaque service-issued view ids; returns the number actually discarded.
/// 参数：transfer_ids 是服务签发的不透明视图ID；返回实际丢弃数量。
pub(crate) fn discard_composer_attachment_plans(
    transfer_ids: Vec<String>,
) -> Result<usize, String> {
    let mut plans = draft_media_upload_plans()
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(transfer_ids
        .into_iter()
        .filter(|id| plans.remove(id).is_some())
        .count())
}

/// Projects the exact original receipt after its operation has settled; never starts replacement network work.
/// 原操作结算后投影精确原回执；绝不开始替代网络工作。
/// Parameters retain the validated view plan, canonical source and conversation scope; returns readiness or an explicit failed view.
/// 参数保留已校验视图计划、规范来源与会话作用域；返回就绪或显式失败视图。
pub(super) fn project_observed_upload(
    plan: &DraftMediaUploadPlan,
    attachment: &ConversationAttachment,
    conversation_id: Option<&str>,
    preparation: ComposerAttachmentPreparationSummary,
) -> Result<ComposerAttachmentPreparationSummary, String> {
    // The original UUID must still belong to this exact lease; newer same-file results are never candidates.
    // 原UUID必须仍属于此精确租约；较新的同文件结果绝不是候选。
    let original = vulcan_agent_runtime::attachment_asset_index::load_attachment_draft_media(
        attachment,
        &plan.reservation.identity.draft_scope,
    )?
    .filter(|original| original.intent_id == plan.reservation.identity.intent_id)
    .ok_or_else(|| String::from("Original draft upload intention is no longer active."))?;
    if !matches!(
        original.delivery,
        vulcan_agent_runtime::attachment_asset_index::AttachmentDraftMediaDelivery::Upload {
            receipt: Some(_),
            ..
        }
    ) {
        return Ok(ComposerAttachmentPreparationSummary {
            phase: ComposerAttachmentPreparationPhase::Failed,
            blocks_submission: false,
            reason: String::from("original_upload_finished_without_receipt"),
            error: Some(String::from(
                "The original upload ended without a completed receipt; retry explicitly if needed.",
            )),
            ..preparation
        });
    }
    // Current hard limits still govern projection; this is not permission to reuse an expired or incompatible receipt.
    // 当前硬限制仍控制投影；这不授权复用过期或不兼容回执。
    let (delivery_plan, reason) =
        ComposerMediaDeliveryPlanner::new(plan.execution.media_delivery_policy)?.plan_draft(
            &plan.execution,
            attachment,
            Some(&original),
            conversation_id,
        )?;
    Ok(ComposerAttachmentPreparationSummary {
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
    })
}

#[cfg(test)]
pub(super) mod tests;
