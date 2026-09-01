//! Chronological V1 prompt-owned media planning and request-local materialization.
//! V1提示词所有的时序媒体规划与请求局部物化。

use super::*;
use crate::session_runtime_registry_v1::SessionPhysicalPreparation;
use vulcan_agent_runtime::agent_runtime::events::ProviderRequestMediaContext;
use vulcan_core::types::ConversationAttachmentDeliveryMode;
use vulcan_provider_adapters::{
    ConversationMediaUploadContract, ConversationPreparedMediaRetention, MediaDeliveryDecision,
    MediaDeliveryOrigin, MediaDeliveryPlanner, MediaDeliveryPurpose, MediaDeliveryRejection,
    MediaDeliveryTarget, MediaUploadCancellation, MediaUploadFailure,
    PreparedMediaReferenceSnapshot, UploadReferenceScope, media_upload_source_kind,
};
use vulcan_session_core::v1::{
    UserPromptMediaBinding, UserPromptMediaOrigin, UserPromptMediaRoute, UserPromptResourceBinding,
    UserPromptResourceDelivery, UserPromptResourceDeliveryKind,
};
use vulcan_workspace_store::workspace_store::{
    ProjectResourceSource, ProjectResourceUploadReceipt, ProviderBinding, ResourceCatalogEntry,
    resolve_project_resource_source,
};

/// Prepared prompt content and controller budget facts keyed by exact projection position.
/// 按精确投影位置索引的已准备提示词内容及控制器预算事实。
#[derive(Default)]
pub(super) struct PreparedV1UserMedia {
    /// Request-local results; their order is consumed by the original semantic projection.
    /// 请求局部结果；顺序由原语义投影消费。
    pub prompts: BTreeMap<usize, PreparedV1PromptMedia>,
    /// Write-once origins for eligible new prompts only, in chronological sequence order.
    /// 仅用于合格新提示词的只写一次原身份，按事件时序排列。
    pub bindings: Vec<UserPromptMediaBinding>,
}

/// One exact prompt's content and host-only controller attachment metadata.
/// 一个精确提示词的内容及仅宿主可见的控制器附件元数据。
pub(super) struct PreparedV1PromptMedia {
    /// Exact text and native-resource sequence sent to the target encoder.
    /// 发送至目标编码器的精确文本及原生资源序列。
    pub content: Vec<ContentItem>,
    /// Budget-visible delivery facts without private provider URLs.
    /// 预算可见投递事实，不含私有供应商URL。
    pub attachments: Vec<ConversationAttachment>,
}

/// Captured input for one chronological resource occurrence before content reads or uploads.
/// 在内容读取或上传前为一次时序资源出现捕获的输入。
struct PlannedResource {
    /// Proven eligibility for native planning; absent only when original ownership requires an immutable address-only result.
    /// 已证明的原生规划资格；仅在原归属要求不可变仅地址结果时缺失。
    admission_origin: Option<MediaDeliveryOrigin>,
    /// Immutable earlier preparation supplied with this exact prompt occurrence, independent of current projection.
    /// 随本精确提示词出现提供的不可变更早准备，独立于当前投影。
    original: Option<UserPromptResourceDelivery>,
    /// Project-scoped identity preserved even when the entity is missing.
    /// 即使实体缺失也保留的项目级身份。
    id: ResourceId,
    /// Catalog display and source facts, absent only for an unknown resource id.
    /// 目录显示及源事实，仅当资源ID未知时缺失。
    entry: Option<ResourceCatalogEntry>,
    /// Metadata-verified private source; media bytes are not retained during planning.
    /// 元数据已验证的私有来源；规划期间不保留媒体字节。
    source: Option<ProjectResourceSource>,
    /// Canonical media kind used without relabeling.
    /// 使用的规范媒体类型，不重新标记。
    kind: ResourceKind,
    /// Exact selected contract, never a larger alternative chosen after size rejection.
    /// 精确选中契约，绝不是大小拒绝后选择的更大替代接口。
    upload: Option<ConversationMediaUploadContract>,
    /// Pure shared-policy result.
    /// 共用纯策略结果。
    decision: MediaDeliveryDecision,
    /// Exact original private receipt for restoration, not the latest cache value.
    /// 用于恢复的精确原私有回执，而非最新缓存值。
    original_receipt: Option<ProviderBinding>,
}

/// One prompt plan whose original parameter ownership remains independent of later request projections.
/// 一条提示词规划，其原参数归属独立于后续请求投影。
struct PlannedPrompt {
    /// Exact position in the immutable provider projection.
    /// 不可变供应商投影中的精确位置。
    index: usize,
    /// Exact user event sequence, never a guessed last-message id.
    /// 精确用户事件序号，绝不是猜测的最近消息ID。
    seq: Seq,
    /// User text kept before its ordered resources.
    /// 保持在有序资源之前的用户文本。
    text: String,
    /// Whether Core permits this prompt to acquire its first original identity.
    /// Core是否允许此提示词取得首次原身份。
    freeze: bool,
    /// Ordered occurrence plans, retaining duplicates across prompts.
    /// 有序出现规划，保留跨提示词重复资源。
    resources: Vec<PlannedResource>,
}

/// Cancels cooperative source and upload work if the preparation future is dropped.
/// 准备Future被释放时取消协作式源读取和上传工作。
struct PreparationCancellation(MediaUploadCancellation);

impl Drop for PreparationCancellation {
    /// Revokes this preparation's cancellation token without changing durable prompt history.
    /// 撤销本次准备取消令牌，不改变可靠提示词历史。
    fn drop(&mut self) {
        self.0.cancel();
    }
}

/// Checks host-owned `cancellation` before a source or publication boundary; returns an explicit preparation cancellation error.
/// 在来源或发布边界前检查宿主持有的`cancellation`；返回明确准备取消错误。
fn ensure_preparation_active(
    cancellation: &MediaUploadCancellation,
) -> Result<(), SessionRuntimeError> {
    if cancellation.is_cancelled() {
        return Err(provider_projection_runtime_error(
            "V1 media preparation cancelled",
        ));
    }
    Ok(())
}

/// Prepares `snapshot` resources against the exact optional `context`, persisting private receipts through `registry`.
/// 根据精确可选`context`准备`snapshot`资源，并通过`registry`保存私有回执。
/// Returns request-local content plus ordered prompt-owned origin bindings; all media I/O runs outside session locks.
/// 返回请求局部内容及有序提示词原身份绑定；全部媒体IO位于会话锁之外。
/// This standalone test adapter does not publish session parameters; production dispatch uses the durable owner entrypoint below.
/// 本独立测试适配器不发布会话参数；生产分派使用下方可靠所有者入口。
#[cfg(test)]
pub(super) async fn prepare_v1_user_media(
    snapshot: &SessionProviderProjectionSnapshot,
    context: Option<&ProviderRequestMediaContext>,
    registry: Arc<ProjectResourceCatalogRegistry>,
) -> Result<PreparedV1UserMedia, SessionRuntimeError> {
    prepare_user_media_with_mode(
        snapshot,
        context,
        registry,
        PromptMediaPreparationMode::Dispatch,
    )
    .await
}

/// Prepares `snapshot` with frozen `context` and existing `registry`, retaining `preparation` for every completed original occurrence.
/// 使用冻结`context`及既有`registry`准备`snapshot`，为每个已完成原出现保留`preparation`。
/// Returns request-local content only after completed results are durable; cancellation never revokes already known completion facts.
/// 仅在已完成结果可靠落盘后返回请求局部内容；取消绝不撤销已知完成事实。
pub(super) async fn prepare_v1_user_media_durable(
    snapshot: &SessionProviderProjectionSnapshot,
    context: Option<&ProviderRequestMediaContext>,
    registry: Arc<ProjectResourceCatalogRegistry>,
    preparation: Arc<SessionPhysicalPreparation>,
) -> Result<PreparedV1UserMedia, SessionRuntimeError> {
    // Bound the enclosing dispatch frame while preserving ordinary Future-drop cancellation of the whole media operation.
    // 约束外围分派帧，同时保留整个媒体操作的普通Future丢弃取消语义。
    Box::pin(prepare_user_media_with_completion(
        snapshot,
        context,
        registry,
        PromptMediaPreparationMode::Dispatch,
        Some(preparation),
    ))
    .await
}

/// Exact completion destination scoped by the existing single-flight preparation, prompt sequence and resource position.
/// 由既有单飞准备、提示词序号及资源位置限定的精确完成目标。
#[derive(Clone)]
struct PromptResourceCompletion {
    /// Existing session preparation authority retained by completion workers.
    /// 完成工作线程保留的既有会话准备权限。
    preparation: Arc<SessionPhysicalPreparation>,
    /// Exact original user event sequence.
    /// 精确原用户事件序号。
    prompt_seq: Seq,
    /// Original zero-based resource position, including repeated equal file ids.
    /// 原零基资源位置，包括重复相同文件ID。
    resource_position: usize,
}

impl PromptResourceCompletion {
    /// Durably records original `resource` choice or completion through the same owner; false means its source generation was replaced.
    /// 通过同一所有者可靠记录原`resource`选择或完成；否表示其来源代际被替换。
    fn record(&self, resource: UserPromptResourceDelivery) -> Result<bool, SessionRuntimeError> {
        // Completion uses actual host time; original event sequence still owns chronology.
        // 完成使用实际宿主时间；原事件序号仍持有时序。
        let now_ms = i64::try_from(crate::session_runtime_kernel::current_unix_millis_u64())
            .map_err(|_| {
                provider_projection_runtime_error(
                    "prompt resource completion timestamp exceeds supported range",
                )
            })?;
        self.preparation.record_resource(
            UserPromptResourceBinding {
                prompt_seq: self.prompt_seq,
                resource_position: self.resource_position,
                resource,
            },
            now_ms,
        )
    }

    /// Persists the exact upload `selection` before credentials or network work, returning false when its captured preparation is stale.
    /// 在凭据或网络工作前持久化精确上传`selection`，捕获准备已陈旧时返回否。
    fn select_upload(
        &self,
        selection: UserPromptResourceDelivery,
    ) -> Result<bool, SessionRuntimeError> {
        Ok(self.record(selection)? && !self.preparation.invalidated()?)
    }
}

/// Closed preparation authority: physical dispatch may freeze new prompts; historical projection is read-only.
/// 封闭准备权限：物理分派可以冻结新提示词；历史投影是只读的。
#[derive(Clone, Copy, PartialEq, Eq)]
enum PromptMediaPreparationMode {
    /// The existing physical preparation owns new uploads and partial results; its final API transaction freezes the first request route.
    /// 既有物理准备持有新上传及部分结果；其最终API事务冻结首次请求路线。
    Dispatch,
    /// Consume original proof without acquiring upload or first-delivery ownership.
    /// 消费原证明，不取得上传或首次投递所有权。
    ReadOnly,
    /// Simulate new inline delivery but refuse upload side effects and origin publication.
    /// 模拟新内联投递，但拒绝上传副作用及原身份发布。
    Capture,
}

/// Prepares exact `snapshot` with frozen `context`, `registry` and explicit `mode` authority.
/// 使用冻结`context`、`registry`及显式`mode`权限准备精确`snapshot`。
/// Returns shared policy/materialization results; read-only consumers can never produce new uploads or origin bindings.
/// 返回共享策略/物化结果；只读消费者绝不能产生新上传或原身份绑定。
async fn prepare_user_media_with_mode(
    snapshot: &SessionProviderProjectionSnapshot,
    context: Option<&ProviderRequestMediaContext>,
    registry: Arc<ProjectResourceCatalogRegistry>,
    mode: PromptMediaPreparationMode,
) -> Result<PreparedV1UserMedia, SessionRuntimeError> {
    // Read-only and capture paths share the same bounded enclosing frame without acquiring upload completion authority.
    // 只读及捕获路径共享相同有界外围帧，不取得上传完成权限。
    Box::pin(prepare_user_media_with_completion(
        snapshot, context, registry, mode, None,
    ))
    .await
}

/// Materializes exact `snapshot` under frozen `context`/`registry` and `mode`, optionally publishing through the real `preparation` owner.
/// 在冻结`context`、`registry`及`mode`下物化精确`snapshot`，可选通过真实`preparation`所有者发布。
/// Read-only/capture callers supply no completion authority; the dispatch entrypoint retains its lease across result workers.
/// 只读或捕获调用方不提供完成权限；分派入口跨结果工作线程保留其租约。
async fn prepare_user_media_with_completion(
    snapshot: &SessionProviderProjectionSnapshot,
    context: Option<&ProviderRequestMediaContext>,
    registry: Arc<ProjectResourceCatalogRegistry>,
    mode: PromptMediaPreparationMode,
    preparation: Option<Arc<SessionPhysicalPreparation>>,
) -> Result<PreparedV1UserMedia, SessionRuntimeError> {
    if mode != PromptMediaPreparationMode::Dispatch && preparation.is_some() {
        return Err(provider_projection_runtime_error(
            "read-only media projection cannot own resource completion",
        ));
    }
    if !snapshot.items.iter().any(|item| matches!(item, ProjectedItem::UserInput { resource_ids, .. } if !resource_ids.is_empty())) {
        return Ok(PreparedV1UserMedia::default());
    }
    validate_prompt_media_preparation_frontier(snapshot)?;
    // Real resource-bearing requests require their frozen execution, never guessed policy defaults.
    // 真正带资源请求必须具有冻结执行，绝不能猜测默认策略。
    let context = context
        .ok_or_else(|| {
            provider_projection_runtime_error("V1 media request has no frozen execution context")
        })?
        .clone();
    // Arm cancellation before any blocking worker starts; aborting its parent must revoke queued planning as well.
    // 在任何阻塞工作线程启动前建立取消；父任务中止也必须撤销排队规划。
    let cancellation = PreparationCancellation(MediaUploadCancellation::new());
    // The worker receives one immutable projection and does metadata-only planning before any upload begins.
    // 工作线程接收单份不可变投影，并在任何上传开始前执行仅元数据规划。
    let planning_snapshot = snapshot.clone();
    // The worker shares the exact frozen route and session scope.
    // 工作线程共享精确冻结路线及会话作用域。
    let planning_context = context.clone();
    // The worker checks the same preparation token between exact prompts and resource occurrences.
    // 工作线程在精确提示词及资源出现之间检查同一准备令牌。
    let planning_cancellation = cancellation.0.clone();
    // Complete metadata planning before any media materialization.
    // 在任何媒体物化前完成元数据规划。
    let plans = tokio::task::spawn_blocking(move || {
        plan_prompts(
            &planning_snapshot,
            &planning_context,
            mode,
            &planning_cancellation,
        )
    })
    .await
    .map_err(|error| SessionRuntimeError::BlockingTaskJoin(error.to_string()))??;
    // Historical replay never acquires first-delivery authority; capture may preview earlier inline sources before its final upload check.
    // 历史重放绝不取得首次投递权限；捕获可以在最终上传检查前预览更早内联来源。
    if mode == PromptMediaPreparationMode::ReadOnly
        && plans.iter().any(|plan| {
            plan.freeze
                || plan
                    .resources
                    .iter()
                    .any(|resource| resource.decision == MediaDeliveryDecision::Upload)
        })
    {
        return Err(provider_projection_runtime_error(
            "Exact read-only media projection requires a new upload; prepare the attachment before capture. No upload or prompt-origin mutation was performed.",
        ));
    }
    // Retain the exact prepared result without changing prior prompt ownership.
    // 保留精确准备结果，不改变既有提示词归属。
    let mut prepared = PreparedV1UserMedia::default();
    // Final admission advances only with actually materialized bytes, while the full metadata preflight above remains mandatory.
    // 最终准入仅随实际物化字节推进，上面的完整元数据预检仍为必需。
    let mut final_planner = MediaDeliveryPlanner::new(
        context.execution.media_delivery_policy,
        MediaDeliveryPurpose::Conversation,
    )
    .map_err(provider_projection_runtime_error)?;
    for plan in plans {
        // Native counts apply to this exact prompt; encoded reservations remain chronological across the whole request.
        // 原生数量适用于此精确提示词；编码预留按时序覆盖整个请求。
        let mut native_counts = BTreeMap::<ResourceKind, u64>::new();
        // Each prompt keeps text followed by its original resource occurrence order.
        // 每条提示词保持文本在前、原资源出现顺序在后。
        let mut content = Vec::new();
        // Collect actual attachment facts for final controller budgeting.
        // 为最终控制器预算收集实际附件事实。
        let mut attachments = Vec::new();
        // Retain one original identity for each resource occurrence in order.
        // 按顺序为每次资源出现保留一个原身份。
        let mut origins = Vec::new();
        if !plan.text.is_empty() {
            content.push(ContentItem::Text { text: plan.text });
        }
        for (resource_position, mut resource) in plan.resources.into_iter().enumerate() {
            ensure_preparation_active(&cancellation.0)?;
            // Roll back only this occurrence's tentative reservation if its actual content is not emitted.
            // 若未产出实际内容，仅回滚此次出现的暂定预留。
            let budget_before = final_planner.clone();
            // The category remains the original catalog kind even when materialization fails.
            // 即使物化失败，分类仍保持原目录类型。
            let kind = resource.kind;
            replan_resource_for_materialization(
                &mut resource,
                &context,
                &mut final_planner,
                native_counts.get(&kind).copied().unwrap_or(0) + 1,
            )?;
            if mode != PromptMediaPreparationMode::Dispatch
                && resource.decision == MediaDeliveryDecision::Upload
            {
                return Err(provider_projection_runtime_error(
                    "Exact read-only media projection requires a new upload at final chronological admission; no upload or prompt-origin mutation was performed.",
                ));
            }
            // Materialization can degrade an unavailable source but cannot change its original historical identity.
            // 物化可降级不可用来源，但不能改变其原历史身份。
            let completion = if plan.freeze {
                preparation
                    .as_ref()
                    .map(|preparation| PromptResourceCompletion {
                        preparation: Arc::clone(preparation),
                        prompt_seq: plan.seq,
                        resource_position,
                    })
            } else {
                None
            };
            // Upload completion persists before its next await; other completed representations persist immediately afterwards.
            // 上传完成在其下一次等待前持久化；其他已完成表示在紧随其后持久化。
            let (item, attachment, origin) = materialize_resource(
                resource,
                &context,
                &registry,
                &snapshot.project_directory,
                &cancellation.0,
                completion.clone(),
            )
            .await?;
            if let Some(completion) = completion {
                // The owned worker survives parent cancellation and retains the same preparation gate until persistence settles.
                // 自有工作线程在父取消后继续，并保留同一准备门直到持久化结算。
                let completed = origin.clone();
                tokio::task::spawn_blocking(move || completion.record(completed))
                    .await
                    .map_err(|error| SessionRuntimeError::BlockingTaskJoin(error.to_string()))??;
            }
            if matches!(item, ContentItem::Resource { .. }) {
                *native_counts.entry(kind).or_default() += 1;
            } else {
                final_planner = budget_before;
            }
            content.push(item);
            if let Some(attachment) = attachment {
                attachments.push(attachment);
            }
            origins.push(origin);
        }
        if plan.freeze && mode == PromptMediaPreparationMode::Dispatch {
            prepared.bindings.push(UserPromptMediaBinding {
                prompt_seq: plan.seq,
                origin: UserPromptMediaOrigin {
                    route: original_route(&context),
                    resources: origins,
                },
            });
        }
        prepared.prompts.insert(
            plan.index,
            PreparedV1PromptMedia {
                content,
                attachments,
            },
        );
    }
    Ok(prepared)
}

/// Plans all exact user resource occurrences in `snapshot` with shared encoded budget and frozen `context`.
/// 使用共享编码预算及冻结`context`规划`snapshot`中全部精确用户资源出现。
/// `mode` freezes mutation authority and `cancellation` stops queued or in-progress metadata planning at each source boundary.
/// `mode`冻结修改权限，`cancellation`在每个来源边界停止排队或进行中的元数据规划。
/// Returns metadata-only plans; malformed projection ownership fails before any network side effect.
/// 返回仅元数据规划；投影归属异常在任何网络副作用之前失败。
fn plan_prompts(
    snapshot: &SessionProviderProjectionSnapshot,
    context: &ProviderRequestMediaContext,
    mode: PromptMediaPreparationMode,
    cancellation: &MediaUploadCancellation,
) -> Result<Vec<PlannedPrompt>, SessionRuntimeError> {
    ensure_preparation_active(cancellation)?;
    if snapshot.items.len() != snapshot.sourced_items.len()
        || snapshot
            .items
            .iter()
            .zip(&snapshot.sourced_items)
            .any(|(item, sourced)| item != &sourced.item)
    {
        return Err(provider_projection_runtime_error(
            "V1 media semantic items differ from their exact source projection",
        ));
    }
    // The catalog is loaded once; per-occurrence origins never come from a last-used-resource lookup.
    // 目录仅加载一次；逐出现原身份绝不来自资源最近使用记录查询。
    let catalog = load_project_resource_catalog(&snapshot.project_directory)
        .map_err(provider_projection_runtime_error)?;
    ensure_preparation_active(cancellation)?;
    // Only the frozen execution supplies this request's target and policy.
    // 只有冻结执行提供本请求目标及策略。
    let execution = &context.execution;
    // Use the exact frozen route rather than global or latest-cache selection.
    // 使用精确冻结路线，而非全局或最新缓存选择。
    let route = original_route(context);
    // One planner preserves chronological admission and encoded reservations.
    // 同一规划器保留时序准入及编码预留。
    let mut planner = MediaDeliveryPlanner::new(
        execution.media_delivery_policy,
        MediaDeliveryPurpose::Conversation,
    )
    .map_err(provider_projection_runtime_error)?;
    // Preserve the original source-event order when collecting prompt plans.
    // 收集提示词规划时保留原来源事件顺序。
    let mut prompts = Vec::new();
    for (index, sourced) in snapshot.sourced_items.iter().enumerate() {
        ensure_preparation_active(cancellation)?;
        // Only original user events own prompt media parameters.
        // 只有原用户事件持有提示词媒体参数。
        let ProjectedItem::UserInput { text, resource_ids } = &sourced.item else {
            continue;
        };
        if resource_ids.is_empty() {
            continue;
        }
        if sourced.source_from_seq != sourced.source_to_seq {
            return Err(provider_projection_runtime_error(
                "V1 media prompt does not have an exact source event",
            ));
        }
        if let Some(parameters) = &sourced.user_prompt_parameters {
            parameters
                .validate(resource_ids)
                .map_err(provider_projection_runtime_error)?;
        }
        // Eligibility comes from Core's first-API frontier, never from a Pending flag alone.
        // 资格来自Core首次API前沿，绝不单凭Pending标志决定。
        let freeze = mode != PromptMediaPreparationMode::ReadOnly
            && snapshot
                .preparing_user_prompt_media_seqs
                .contains(&sourced.source_from_seq);
        // Native counts reset per message while encoded budget remains request-wide.
        // 原生数量按消息重置，编码预算仍覆盖整个请求。
        let mut counts = BTreeMap::<ResourceKind, u64>::new();
        // Preserve duplicate occurrences instead of collapsing file-global identity.
        // 保留重复出现，不折叠为文件全局身份。
        let mut resources = Vec::new();
        for (position, id) in resource_ids.iter().enumerate() {
            ensure_preparation_active(cancellation)?;
            // Copied resources with explicitly unknown original delivery must never enter fresh upload admission.
            // 明确未知原投递的复制资源绝不能进入新上传准入。
            let unproven = sourced
                .user_prompt_parameters
                .as_ref()
                .is_some_and(|parameters| {
                    parameters
                        .unproven_resource_positions
                        .binary_search(&position)
                        .is_ok()
                });
            // Pending prompts may already own explicit draft preparation proof; absent proof never means a guessed history mode.
            // 待准备提示词可能已经持有明确草稿准备证明；缺失证明绝不表示猜测的历史方式。
            let original = sourced
                .user_prompt_parameters
                .as_ref()
                .and_then(|parameters| parameters.original_resource(position));
            // Catalog entry identity is exact even when its source has been deleted or corrupted.
            // 即使来源已删除或损坏，目录条目身份仍是精确的。
            let entry = catalog
                .entries
                .iter()
                .find(|entry| entry.resource_id == *id)
                .cloned();
            // Use the catalog MIME, with its declared generic-file meaning when absent.
            // 使用目录MIME，缺失时遵循其声明的通用文件含义。
            let mime = entry
                .as_ref()
                .and_then(|entry| entry.mime.as_deref())
                .unwrap_or("application/octet-stream");
            // Derive the category only from authoritative MIME, never the physical filename.
            // 仅从权威MIME推导类别，绝不使用物理文件名。
            let kind = resource_kind_from_mime(mime);
            // Original uploaded ownership wins before inspecting the new route or touching its local source.
            // 原上传归属先于新路线检查和本地来源访问生效。
            let prior_delivery = original.map(|origin| &origin.delivery);
            // Each occurrence keeps its own earlier route, even when the prompt was first sent using another model.
            // 即使提示词首次发送使用另一模型，每次资源出现仍保留各自更早路线。
            let prior_route = original.map(|origin| &origin.original_route);
            // Original uploads restore only on the exact original target.
            // 原上传只在精确原目标恢复。
            let target_changed = matches!(
                prior_delivery,
                Some(
                    UserPromptResourceDeliveryKind::Uploaded { .. }
                        | UserPromptResourceDeliveryKind::UploadPending { .. }
                )
            ) && prior_route.is_some_and(|original| *original != route);
            // Already-address-only occurrences do not need source inspection or a new target's upload contract.
            // 已为仅地址的出现不需要来源检查或新目标上传契约。
            let attachment_only = target_changed
                || unproven
                || matches!(
                    prior_delivery,
                    Some(UserPromptResourceDeliveryKind::UploadPending {
                        draft_intent_id: None,
                        ..
                    })
                )
                || prior_delivery == Some(&UserPromptResourceDeliveryKind::Attachment)
                || (original.is_none() && !freeze);
            // Native delivery requires the exact metadata-verified source; cross-target history needs no source read.
            // 原生投递要求精确元数据已验证来源；跨目标历史无需读取来源。
            let source = if attachment_only {
                None
            } else {
                resolve_project_resource_source(&snapshot.project_directory, &catalog, *id)
                    .ok()
                    .flatten()
            };
            // Only the frozen declared contract may authorize an upload.
            // 只有冻结声明契约可以授权上传。
            let upload = if attachment_only {
                None
            } else {
                execution
                    .client
                    .conversation_media_upload_contract(&execution.route, kind, mime)
                    .map_err(provider_projection_runtime_error)?
            };
            // Select the exact original receipt fingerprint or registered draft intent, never the latest binding.
            // 选择精确原回执指纹或已登记草稿意图，绝不选择最新绑定。
            let original_receipt = match (original, prior_delivery, entry.as_ref(), upload.as_ref())
            {
                (
                    Some(origin),
                    Some(
                        delivery @ (UserPromptResourceDeliveryKind::Uploaded { .. }
                        | UserPromptResourceDeliveryKind::UploadPending {
                            draft_intent_id: Some(_),
                            ..
                        }),
                    ),
                    Some(entry),
                    Some(contract),
                ) if origin.original_route == route => entry
                    .provider_bindings
                    .iter()
                    .find(|binding| {
                        binding.matches_prompt_upload(delivery, &entry.sha256, entry.size)
                            && receipt_matches(binding, entry, context, contract)
                    })
                    .cloned(),
                _ => None,
            };
            // Historical planner branches require proven per-occurrence facts.
            // 历史规划分支要求已证明逐出现事实。
            let origin = match prior_delivery {
                Some(UserPromptResourceDeliveryKind::Inline) => {
                    MediaDeliveryOrigin::HistoricalInline
                }
                Some(
                    UserPromptResourceDeliveryKind::Uploaded { .. }
                    | UserPromptResourceDeliveryKind::UploadPending { .. },
                ) => MediaDeliveryOrigin::HistoricalUpload {
                    original_target_matches: prior_route.is_some_and(|original| *original == route),
                    reference_usable: original_receipt.is_some(),
                },
                _ => MediaDeliveryOrigin::NewIntent,
            };
            // Only the native-admission branch can be reconsidered after an earlier occurrence fails.
            // 只有原生准入分支能在更早出现失败后被重新判断。
            let mut admission_origin = None;
            // Ownership and preparation eligibility precede soft delivery preferences.
            // 归属及准备资格先于软投递偏好。
            let decision = if unproven {
                MediaDeliveryDecision::Attachment {
                    reason: MediaDeliveryRejection::HistoricalOriginUnknown,
                }
            } else if target_changed {
                MediaDeliveryDecision::Attachment {
                    reason: MediaDeliveryRejection::HistoricalTargetChanged,
                }
            } else if matches!(
                prior_delivery,
                Some(UserPromptResourceDeliveryKind::UploadPending {
                    draft_intent_id: None,
                    ..
                })
            ) {
                MediaDeliveryDecision::Attachment {
                    reason: MediaDeliveryRejection::OriginalReferenceUnavailable,
                }
            } else if prior_delivery == Some(&UserPromptResourceDeliveryKind::Attachment) {
                MediaDeliveryDecision::Attachment {
                    reason: MediaDeliveryRejection::OriginalAttachmentOnly,
                }
            } else if original.is_none() && !freeze {
                MediaDeliveryDecision::Attachment {
                    reason: MediaDeliveryRejection::HistoricalOriginUnknown,
                }
            } else {
                admission_origin = Some(origin);
                // Model and protocol jointly constrain the admitted source representation.
                // 模型与协议共同约束已准入来源表示。
                let target = frozen_media_delivery_target(context, kind, upload.as_ref());
                // Keep source availability separate from size before reserving the shared budget.
                // 预留共用预算前区分来源可用性与大小。
                let mut facts = target.facts(
                    kind,
                    mime,
                    entry.as_ref().map_or(0, |entry| entry.size),
                    origin,
                    counts.get(&kind).copied().unwrap_or(0) + 1,
                );
                facts.source_available = source.is_some() && facts.raw_bytes != 0;
                planner
                    .plan(facts)
                    .map_err(provider_projection_runtime_error)?
            };
            if matches!(
                decision,
                MediaDeliveryDecision::Inline { .. }
                    | MediaDeliveryDecision::Upload
                    | MediaDeliveryDecision::RestoreOriginalUpload
            ) {
                *counts.entry(kind).or_default() += 1;
            }
            resources.push(PlannedResource {
                admission_origin,
                original: original.cloned(),
                id: *id,
                entry,
                source,
                kind,
                upload,
                decision,
                original_receipt,
            });
        }
        prompts.push(PlannedPrompt {
            index,
            seq: sourced.source_from_seq,
            text: text.clone(),
            freeze,
            resources,
        });
    }
    Ok(prompts)
}

/// Builds the same frozen target for preflight and final admission using `context`, exact `kind` and selected `upload`.
/// 使用`context`、精确`kind`及选中`upload`为预检和最终准入构建同一冻结目标。
/// Returns borrowed declared capabilities only; no new route, channel lookup or fallback is performed.
/// 仅返回借用的声明能力；不进行新路线、通道查询或回退。
fn frozen_media_delivery_target<'a>(
    context: &'a ProviderRequestMediaContext,
    kind: ResourceKind,
    upload: Option<&'a ConversationMediaUploadContract>,
) -> MediaDeliveryTarget<'a> {
    MediaDeliveryTarget {
        profile_id: &context.execution.route.protocol_route.protocol_profile_id,
        model_inputs: &context.execution.model.capabilities.input_resources,
        constraint: context
            .execution
            .route
            .protocol_route
            .input_resource_constraints
            .iter()
            .find(|constraint| constraint.resource_kind == kind),
        upload,
    }
}

/// Replans one preflighted `resource` against the actual prefix in `planner` and the prompt's next `native_count`.
/// 根据`planner`中的实际前缀及提示词下一`native_count`，重新规划一个已预检`resource`。
/// Frozen `context` and per-occurrence origin remain authoritative; returns only a local decision, never source I/O or uploads.
/// 冻结`context`及逐出现原身份仍为权威；仅返回局部决定，绝不进行来源IO或上传。
fn replan_resource_for_materialization(
    resource: &mut PlannedResource,
    context: &ProviderRequestMediaContext,
    planner: &mut MediaDeliveryPlanner,
    native_count: u64,
) -> Result<(), SessionRuntimeError> {
    // Metadata mandated by historical ownership or missing provenance must never gain native eligibility from refunded budget.
    // 由历史归属或来源证明缺失要求的元数据绝不能因预算返还而取得原生资格。
    let Some(mut origin) = resource.admission_origin else {
        return Ok(());
    };
    if let MediaDeliveryOrigin::HistoricalUpload {
        original_target_matches,
        ..
    } = origin
    {
        // A long earlier transfer may outlive this exact original receipt; never consult a newer binding to refresh it.
        // 更早的长时间传输可能超过此精确原回执寿命；绝不查询较新绑定来刷新它。
        let reference_usable = match (
            &resource.original_receipt,
            &resource.entry,
            &resource.upload,
        ) {
            (Some(binding), Some(entry), Some(contract)) => {
                receipt_matches(binding, entry, context, contract)
            }
            _ => false,
        };
        origin = MediaDeliveryOrigin::HistoricalUpload {
            original_target_matches,
            reference_usable,
        };
    }
    // The source descriptor and metadata are from complete preflight; final budgeting does not rediscover a file or channel.
    // 来源描述及元数据来自完整预检；最终预算不重新发现文件或通道。
    let target = frozen_media_delivery_target(context, resource.kind, resource.upload.as_ref());
    // MIME absence has the catalog's existing generic-file meaning, not an alternate format guess.
    // MIME缺失遵循目录既有通用文件含义，而非猜测替代格式。
    let mime = resource
        .entry
        .as_ref()
        .and_then(|entry| entry.mime.as_deref())
        .unwrap_or("application/octet-stream");
    // The same shared target facts enforce model/protocol MIME, byte, count and upload hard limits again.
    // 相同共享目标事实再次强制模型/协议的MIME、大小、数量及上传硬限制。
    let mut facts = target.facts(
        resource.kind,
        mime,
        resource.entry.as_ref().map_or(0, |entry| entry.size),
        origin,
        native_count,
    );
    facts.source_available = resource.source.is_some() && facts.raw_bytes != 0;
    resource.decision = planner
        .plan(facts)
        .map_err(provider_projection_runtime_error)?;
    Ok(())
}

/// Checks `binding` against exact source `entry`, frozen `context` and selected `contract`, including its original lifetime.
/// 根据精确来源`entry`、冻结`context`及选中`contract`检查`binding`，包含原生命周期。
fn receipt_matches(
    binding: &ProviderBinding,
    entry: &ResourceCatalogEntry,
    context: &ProviderRequestMediaContext,
    contract: &ConversationMediaUploadContract,
) -> bool {
    crate::media_upload::media_receipt_is_usable(
        binding,
        &context.execution,
        resource_kind_from_mime(entry.mime.as_deref().unwrap_or("application/octet-stream")),
        entry.mime.as_deref().unwrap_or("application/octet-stream"),
        contract,
        Some(&context.session_scope_id),
    )
}

/// Materializes one `planned` occurrence using frozen `context`, exact `registry`/`project_directory` and `cancellation`.
/// 使用冻结`context`、精确`registry`及`project_directory`、`cancellation`物化一次`planned`出现。
/// Returns one native or safe text item, controller metadata and the actual first-delivery result.
/// 返回一个原生或安全文本项、控制器元数据及实际首次投递结果。
async fn materialize_resource(
    mut planned: PlannedResource,
    context: &ProviderRequestMediaContext,
    registry: &Arc<ProjectResourceCatalogRegistry>,
    project_directory: &std::path::Path,
    cancellation: &MediaUploadCancellation,
    completion: Option<PromptResourceCompletion>,
) -> Result<
    (
        ContentItem,
        Option<ConversationAttachment>,
        UserPromptResourceDelivery,
    ),
    SessionRuntimeError,
> {
    ensure_preparation_active(cancellation)?;
    // No native content exists until exact admitted materialization succeeds.
    // 精确已准入物化成功之前不存在原生内容。
    let mut native = None;
    // Unsent media starts as explicit metadata-only delivery.
    // 未发送媒体初始为明确仅元数据投递。
    let mut delivery = UserPromptResourceDeliveryKind::Attachment;
    // Retain only typed non-secret failure facts for this request's metadata projection.
    // 仅为本请求元数据投影保留强类型非秘密失败事实。
    let mut upload_failure = None;
    // A newly selected upload starts one declared deadline before its first byte/identity read, never at the later network stage.
    // 新选中上传在首次字节/身份读取之前启动单一声明截止时间，绝不在较晚网络阶段才计时。
    let upload_deadline = if planned.decision == MediaDeliveryDecision::Upload {
        Some(
            vulcan_provider_adapters::MediaUploadDeadline::for_contract(
                planned.upload.as_ref().ok_or_else(|| {
                    provider_projection_runtime_error("V1 upload plan has no executable contract")
                })?,
            )
            .map_err(provider_projection_runtime_error)?,
        )
    } else {
        None
    };
    if matches!(
        planned.decision,
        MediaDeliveryDecision::Inline { .. }
            | MediaDeliveryDecision::Upload
            | MediaDeliveryDecision::RestoreOriginalUpload
    ) {
        // Native delivery requires the exact metadata-verified source; cross-target history needs no source read.
        // 原生投递要求精确元数据已验证来源；跨目标历史无需读取来源。
        let source = planned.source.as_ref().ok_or_else(|| {
            provider_projection_runtime_error("Native V1 media plan has no verified source")
        })?;
        // Compare both the admitted byte length and immutable catalog digest.
        // 同时比较准入字节长度及不可变目录摘要。
        let identity_read = Box::pin(
            context
                .execution
                .client
                .read_admitted_media_source_identity(
                    &source.path,
                    source.entry.size,
                    Some(cancellation),
                ),
        );
        // Preserve typed expiry separately from an unavailable or changed source, without parsing an I/O error string.
        // 将强类型到期与不可用或变化来源分开保留，不解析I/O错误字符串。
        let identity = if let Some(deadline) = &upload_deadline {
            deadline.wait(cancellation, identity_read).await
        } else {
            Ok(identity_read.await)
        };
        // Only a verified matching source authorizes native materialization; timeout cannot turn into another upload attempt.
        // 只有已验证匹配来源授权原生物化；超时不能变成另一上传尝试。
        let source_matches = match identity {
            Ok(Ok(identity)) => {
                identity.byte_len == source.entry.size
                    && identity.content_sha256 == source.entry.sha256
            }
            Err(failure) => {
                upload_failure = Some(failure);
                false
            }
            Ok(Err(_)) => false,
        };
        if !source_matches {
            planned.decision = MediaDeliveryDecision::Attachment {
                reason: if upload_failure.is_some() {
                    MediaDeliveryRejection::UploadPreparationFailed
                } else {
                    MediaDeliveryRejection::SourceUnavailable
                },
            };
        } else {
            match planned.decision {
                MediaDeliveryDecision::Inline { .. } => {
                    // The blocking worker owns only the exact bounded source descriptor.
                    // 阻塞工作线程只持有精确有界来源描述符。
                    let read_source = source.clone();
                    // A dropped preparation revokes the already spawned source worker as well as its async parent.
                    // 被丢弃的准备同时撤销已启动来源工作线程及其异步父任务。
                    let read_cancellation = cancellation.clone();
                    match tokio::task::spawn_blocking(move || {
                        read_source.read_bounded_cancellable(read_source.entry.size, || {
                            read_cancellation.is_cancelled()
                        })
                    })
                    .await
                    .map_err(|error| SessionRuntimeError::BlockingTaskJoin(error.to_string()))?
                    {
                        Ok(bytes) => {
                            native = Some((
                                ResourceSourceKind::InlineData,
                                format!(
                                    "data:{};base64,{}",
                                    source
                                        .entry
                                        .mime
                                        .as_deref()
                                        .unwrap_or("application/octet-stream"),
                                    BASE64_STANDARD.encode(bytes)
                                ),
                            ));
                            delivery = UserPromptResourceDeliveryKind::Inline;
                        }
                        Err(_) => {
                            planned.decision = MediaDeliveryDecision::Attachment {
                                reason: MediaDeliveryRejection::SourceUnavailable,
                            }
                        }
                    }
                }
                MediaDeliveryDecision::RestoreOriginalUpload => {
                    // Keep the exact private receipt and its scope separate from model-visible text.
                    // 将精确私有回执及其作用域与模型可见文本分离。
                    let binding = planned.original_receipt.as_ref().ok_or_else(|| {
                        provider_projection_runtime_error(
                            "Original upload restoration has no exact receipt",
                        )
                    })?;
                    // Only typed receipt metadata proves original reference representation.
                    // 只有强类型回执元数据证明原引用表示。
                    let receipt = binding.upload_receipt.as_ref().ok_or_else(|| {
                        provider_projection_runtime_error(
                            "Original upload restoration has no typed receipt",
                        )
                    })?;
                    native = Some((receipt.source_kind, binding.media_id.clone()));
                    delivery = uploaded_origin(binding, &source.entry)?;
                }
                MediaDeliveryDecision::Upload => {
                    // This exact deadline was created before the source read and remains unchanged through selection, cache and transfer.
                    // 本精确截止时间创建于来源读取之前，在选择、缓存及传输期间保持不变。
                    let deadline = upload_deadline.as_ref().ok_or_else(|| {
                        provider_projection_runtime_error(
                            "V1 upload preparation has no original deadline",
                        )
                    })?;
                    // Materialization consumes the selected contract without a fallback channel.
                    // 物化消费已选契约，不使用回退通道。
                    let contract = planned.upload.as_ref().ok_or_else(|| {
                        provider_projection_runtime_error(
                            "V1 upload plan has no executable contract",
                        )
                    })?;
                    if planned.original.is_some() {
                        return Err(provider_projection_runtime_error(
                            "an existing original media choice cannot start a new upload",
                        ));
                    }
                    // Record choice, not success, before executing the selected upload or consulting its authenticated cache.
                    // 在执行所选上传或查询其认证缓存之前记录选择，而非成功。
                    let selection = UserPromptResourceDelivery {
                        resource_id: planned.id,
                        original_route: original_route(context),
                        delivery: UserPromptResourceDeliveryKind::UploadPending {
                            draft_intent_id: None,
                            upload_channel_id: contract.upload_channel_id.clone(),
                            upload_contract_fingerprint: contract.contract_fingerprint.clone(),
                        },
                    };
                    planned.original = Some(selection.clone());
                    // The actual production owner persists selection before the first upload await; standalone tests keep the same local result semantics.
                    // 实际生产所有者在首次上传等待前持久化选择；独立测试保持相同局部结果语义。
                    let may_upload = if let Some(completion) = completion.clone() {
                        // Selection may finish durably after expiry; the same owner settles it without authorizing late network I/O.
                        // 选择可在到期后可靠完成；同一所有者结算它，但不授权迟到网络I/O。
                        let worker = tokio::task::spawn_blocking(move || {
                            completion.select_upload(selection)
                        });
                        match deadline.wait(cancellation, worker).await {
                            Ok(result) => result.map_err(|error| {
                                SessionRuntimeError::BlockingTaskJoin(error.to_string())
                            })??,
                            Err(failure) => {
                                upload_failure = Some(failure);
                                false
                            }
                        }
                    } else {
                        true
                    };
                    ensure_preparation_active(cancellation)?;
                    if upload_failure.is_some() {
                        planned.decision = MediaDeliveryDecision::Attachment {
                            reason: MediaDeliveryRejection::UploadPreparationFailed,
                        };
                    } else if !may_upload {
                        // A stale preparation still preserves its original choice but cannot begin more external work.
                        // 陈旧准备仍保留其原选择，但不能开始更多外部工作。
                        planned.decision = MediaDeliveryDecision::Attachment {
                            reason: MediaDeliveryRejection::OriginalReferenceUnavailable,
                        };
                    } else {
                        match prepare_upload(source, context, contract, cancellation, deadline)
                            .await
                        {
                            Ok(binding) => {
                                // Persist through the same project catalog owner outside request locks.
                                // 在请求锁之外通过同一项目目录所有者持久化。
                                let owner = Arc::clone(registry);
                                // The immutable snapshot fixes the sole destination project partition.
                                // 不可变快照固定唯一目标项目分区。
                                let project = project_directory.to_path_buf();
                                // Catalog identity retains exact source and display facts for this occurrence.
                                // 目录身份为本次出现保留精确来源及显示事实。
                                let entry = source.entry.clone();
                                // Append a private receipt without replacing earlier uploads.
                                // 追加私有回执，不替换更早上传。
                                let persisted = binding.clone();
                                // A completed upload already has a validated reference; publish its exact prompt proof before the private receipt.
                                // 已完成上传已经具有验证引用；先发布其精确提示词证明，再发布私有回执。
                                let completed_origin = UserPromptResourceDelivery {
                                    resource_id: planned.id,
                                    original_route: original_route(context),
                                    delivery: uploaded_origin(&binding, &source.entry)?,
                                };
                                planned.original = Some(completed_origin.clone());
                                // Retaining this exact destination keeps the existing gate held even if the parent is dropped at the following await.
                                // 保留本精确目标使既有准备门在父任务于下一次等待被丢弃时仍保持占有。
                                let prompt_completion = completion.clone();
                                // Test-only standalone materialization has no session owner; it retains the existing cancellation guard.
                                // 仅测试独立物化没有会话所有者；它保留既有取消守卫。
                                let receipt_cancellation = cancellation.clone();
                                tokio::task::spawn_blocking(move || {
                                    if let Some(prompt_completion) = &prompt_completion {
                                        prompt_completion.record(completed_origin)?;
                                    } else {
                                        ensure_preparation_active(&receipt_cancellation)?;
                                    }
                                    // Keep the same lease through private receipt publication as well as prompt proof persistence.
                                    // 在私有回执发布及提示词证明持久化整个期间保留同一租约。
                                    let recorded = owner
                                        .record_provider_upload_receipt(
                                            &project,
                                            entry.resource_id,
                                            &entry.sha256,
                                            entry.size,
                                            persisted,
                                        )
                                        .map_err(provider_projection_runtime_error);
                                    drop(prompt_completion);
                                    recorded
                                })
                                .await
                                .map_err(|error| {
                                    SessionRuntimeError::BlockingTaskJoin(error.to_string())
                                })??;
                                native = Some((
                                    media_upload_source_kind(contract.reference_encoding),
                                    binding.media_id.clone(),
                                ));
                                delivery = uploaded_origin(&binding, &source.entry)?;
                            }
                            Err(failure) => {
                                upload_failure = Some(failure);
                                planned.decision = MediaDeliveryDecision::Attachment {
                                    reason: MediaDeliveryRejection::UploadPreparationFailed,
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    ensure_preparation_active(cancellation)?;
    // Disclose only the project-scoped logical locator to the model.
    // 只向模型披露项目级逻辑定位符。
    let locator = vulcan_workspace_store::workspace_store::project_resource_locator(planned.id);
    // Controller facts reflect the actually materialized delivery.
    // 控制器事实反映实际物化投递。
    let mode = match &delivery {
        UserPromptResourceDeliveryKind::Inline => {
            ConversationAttachmentDeliveryMode::InlineOriginal
        }
        UserPromptResourceDeliveryKind::Uploaded { .. } => {
            ConversationAttachmentDeliveryMode::UploadReference
        }
        UserPromptResourceDeliveryKind::Attachment
        | UserPromptResourceDeliveryKind::UploadPending { .. } => {
            ConversationAttachmentDeliveryMode::MetadataOnly
        }
    };
    // Retain source metadata without exposing private provider references.
    // 保留来源元数据，不暴露私有供应商引用。
    let attachment = planned.entry.as_ref().map(|entry| ConversationAttachment {
        kind: attachment_kind(planned.kind, entry.mime.as_deref()),
        file_name: entry.original_name.clone(),
        local_path: planned
            .source
            .as_ref()
            .map(|source| source.path.to_string_lossy().into_owned())
            .unwrap_or_default(),
        resource_path: Some(locator.clone()),
        source_path: None,
        asset_id: None,
        byte_len: Some(entry.size),
        source_sha256: Some(entry.sha256.clone()),
        delivery_mode: Some(mode),
        preview_local_path: None,
        mime_type: entry
            .mime
            .clone()
            .unwrap_or_else(|| "application/octet-stream".into()),
    });
    // Emit native content only after an exact representation has been produced.
    // 仅在产出精确表示后输出原生内容。
    let item = if let Some((source_kind, reference)) = native {
        // Catalog identity retains exact source and display facts for this occurrence.
        // 目录身份为本次出现保留精确来源及显示事实。
        let entry = planned.entry.as_ref().ok_or_else(|| {
            provider_projection_runtime_error("Native resource has no catalog identity")
        })?;
        ContentItem::Resource {
            resource: ResourceReference {
                id: if source_kind == ResourceSourceKind::ProviderFileId {
                    reference.clone()
                } else {
                    format!("project-resource-{}", planned.id.value())
                },
                kind: planned.kind,
                media_type: Some(
                    entry
                        .mime
                        .clone()
                        .unwrap_or_else(|| "application/octet-stream".into()),
                ),
                source_kind,
                file_name: Some(entry.original_name.clone()),
                image_detail: None,
                resource_path: Some(locator),
                local_path: None,
                url: (source_kind != ResourceSourceKind::ProviderFileId).then_some(reference),
            },
        }
    } else {
        // Every metadata-only projection explains why no media bytes were sent.
        // 每个仅元数据投影都说明未发送媒体字节的原因。
        let reason = match planned.decision {
            MediaDeliveryDecision::Attachment { reason } => reason,
            _ => {
                return Err(provider_projection_runtime_error(
                    "V1 media materialization left an unresolved native action",
                ));
            }
        };
        ContentItem::Text {
            text: attachment_text(&planned, &locator, reason, upload_failure.as_ref()),
        }
    };
    Ok((
        item,
        attachment,
        planned
            .original
            .unwrap_or_else(|| UserPromptResourceDelivery {
                resource_id: planned.id,
                original_route: original_route(context),
                delivery,
            }),
    ))
}

/// Reuses or creates a fresh source upload for `context` and `contract`; historical restoration never calls this function.
/// 为`context`及`contract`复用或创建新的来源上传；历史恢复绝不调用本函数。
async fn prepare_upload(
    source: &ProjectResourceSource,
    context: &ProviderRequestMediaContext,
    contract: &ConversationMediaUploadContract,
    cancellation: &MediaUploadCancellation,
    deadline: &vulcan_provider_adapters::MediaUploadDeadline,
) -> Result<ProviderBinding, MediaUploadFailure> {
    deadline.validate_contract(contract)?;
    deadline.check(cancellation)?;
    // Only the frozen execution supplies this request's target and policy.
    // 只有冻结执行提供本请求目标及策略。
    let execution = &context.execution;
    // Use the catalog MIME, with its declared generic-file meaning when absent.
    // 使用目录MIME，缺失时遵循其声明的通用文件含义。
    let mime = source
        .entry
        .mime
        .as_deref()
        .unwrap_or("application/octet-stream");
    // Derive the category only from authoritative MIME, never the physical filename.
    // 仅从权威MIME推导类别，绝不使用物理文件名。
    let kind = resource_kind_from_mime(mime);
    // The project catalog fixes both source length and digest before cache reuse or upload.
    // 项目目录在缓存复用或上传前固定来源长度及摘要。
    let expected_identity = vulcan_provider_adapters::PreparedMediaSourceIdentity {
        byte_len: source.entry.size,
        content_sha256: source.entry.sha256.clone(),
    };
    // A new explicit intention may reuse an exact valid private receipt.
    // 新显式意图可以复用精确有效私有回执。
    let cached = source
        .entry
        .provider_bindings
        .iter()
        .find(|binding| receipt_matches(binding, &source.entry, context, contract));
    if let Some(cached) = cached {
        deadline.check(cancellation)?;
        return Ok(cached.clone());
    }
    // Cache reuse must preserve exact registered source and upload scope.
    // 缓存复用必须保留精确注册来源及上传作用域。
    let reference = if let Some(reference) = deadline
        .wait(
            cancellation,
            execution
                .client
                .prepared_media_reference_for_admitted_source(
                    &execution.route,
                    &source.path,
                    kind,
                    mime,
                    &expected_identity,
                    Some(&context.session_scope_id),
                    Some(cancellation),
                ),
        )
        .await?
    {
        reference
    } else {
        // Anchor lifetime to the beginning of this physical upload.
        // 将生命周期锚定到本次物理上传开始。
        let created = crate::session_runtime_kernel::current_unix_millis_u64();
        // The frozen execution reuses the runtime observer and bounds credential, source and upload work together.
        // 冻结执行复用运行时观察器，并共同限制凭据、来源及上传工作。
        let prepared = execution
            .prepare_conversation_upload_until(
                vulcan_provider_adapters::AdmittedLocalMediaSource {
                    path: &source.path,
                    file_name: &source.entry.original_name,
                    mime_type: mime,
                    identity: &expected_identity,
                },
                kind,
                &context.session_scope_id,
                contract,
                cancellation,
                deadline,
            )
            .await?;
        // Compare both the admitted byte length and immutable catalog digest.
        // 同时比较准入字节长度及不可变目录摘要。
        let identity = prepared.source_identity();
        if identity.byte_len != source.entry.size || identity.content_sha256 != source.entry.sha256
        {
            return Err(MediaUploadFailure::SourceChanged);
        }
        // An upload without a provider reference cannot become successful native delivery.
        // 没有供应商引用的上传不能成为成功原生投递。
        let provider_reference = prepared
            .prepared_reference()
            .ok_or(MediaUploadFailure::MissingReference)?
            .to_string();
        // Preserve original TTL; absent TTL uses the existing integer sentinel.
        // 保留原TTL；无TTL使用既有整数哨兵。
        let expires = contract
            .cache_duration_ms
            .map(|duration| created.saturating_add(duration))
            .unwrap_or(u64::MAX);
        execution.client.remember_conversation_prepared_media(
            &execution.route,
            &source.path,
            kind,
            mime,
            prepared,
            ConversationPreparedMediaRetention {
                session_scope_id: Some(&context.session_scope_id),
                wall_clock_lifetime: Some((created, expires)),
            },
        );
        PreparedMediaReferenceSnapshot {
            provider_reference,
            reference_encoding: contract.reference_encoding,
            created_at_unix_millis: created,
            expires_at_unix_millis: expires,
        }
    };
    // Keep the exact private receipt and its scope separate from model-visible text.
    // 将精确私有回执及其作用域与模型可见文本分离。
    let binding = ProviderBinding {
        provider: execution.route.provider.id.clone(),
        connection_id: Some(execution.route.connection.id.clone()),
        media_id: reference.provider_reference,
        upload_receipt: Some(ProjectResourceUploadReceipt {
            draft_intent_id: None,
            version: 1,
            service_entry_id: execution.route.entry.id.clone(),
            upload_channel_id: contract.upload_channel_id.clone(),
            upload_contract_fingerprint: contract.contract_fingerprint.clone(),
            resource_kind: kind,
            mime_type: mime.into(),
            source_kind: media_upload_source_kind(reference.reference_encoding),
            session_scope_id: (contract.reference_scope == UploadReferenceScope::Session)
                .then(|| context.session_scope_id.clone()),
            created_at_unix_millis: reference.created_at_unix_millis,
            expires_at_unix_millis: reference.expires_at_unix_millis,
        }),
    };
    if !receipt_matches(&binding, &source.entry, context, contract) {
        return Err(MediaUploadFailure::InvalidReference);
    }
    Ok(binding)
}

/// Returns the non-secret original route from exact frozen `context`.
/// 从精确冻结`context`返回非秘密原路线。
fn original_route(context: &ProviderRequestMediaContext) -> UserPromptMediaRoute {
    // Only the frozen execution supplies this request's target and policy.
    // 只有冻结执行提供本请求目标及策略。
    let execution = &context.execution;
    UserPromptMediaRoute {
        provider_id: execution.route.provider.id.clone(),
        service_entry_id: execution.route.entry.id.clone(),
        connection_id: execution.route.connection.id.clone(),
        model_id: execution.model.id.clone(),
        protocol_profile_id: execution.route.protocol_route.protocol_profile_id.clone(),
    }
}

/// Converts exact private `binding` and immutable `entry` into prompt-safe opaque upload identity.
/// 将精确私有`binding`和不可变`entry`转换为提示词安全的不透明上传身份。
fn uploaded_origin(
    binding: &ProviderBinding,
    entry: &ResourceCatalogEntry,
) -> Result<UserPromptResourceDeliveryKind, SessionRuntimeError> {
    // Only typed receipt metadata proves original reference representation.
    // 只有强类型回执元数据证明原引用表示。
    let receipt = binding.upload_receipt.as_ref().ok_or_else(|| {
        provider_projection_runtime_error("Prompt upload origin has no typed receipt")
    })?;
    Ok(UserPromptResourceDeliveryKind::Uploaded {
        upload_channel_id: receipt.upload_channel_id.clone(),
        upload_contract_fingerprint: receipt.upload_contract_fingerprint.clone(),
        reference_fingerprint: binding
            .upload_receipt_fingerprint(&entry.sha256, entry.size)
            .map_err(provider_projection_runtime_error)?,
    })
}

/// Maps canonical `kind` and optional `mime` to the existing controller attachment category without preview relabeling.
/// 将规范`kind`及可选`mime`映射至既有控制器附件类型，不进行预览改标。
fn attachment_kind(kind: ResourceKind, mime: Option<&str>) -> ConversationAttachmentKind {
    match kind {
        ResourceKind::Image => ConversationAttachmentKind::Image,
        ResourceKind::Audio => ConversationAttachmentKind::Audio,
        ResourceKind::Video => ConversationAttachmentKind::Video,
        ResourceKind::File
            if mime.is_some_and(|mime| mime.eq_ignore_ascii_case("application/pdf")) =>
        {
            ConversationAttachmentKind::Pdf
        }
        ResourceKind::File => ConversationAttachmentKind::File,
    }
}

/// Renders safe metadata for `planned` at logical `locator`, explicitly stating why native bytes were not sent.
/// 为逻辑`locator`处的`planned`渲染安全元数据，明确说明未发送原生字节的原因。
/// `reason` is the policy outcome; optional `upload_failure` contains only this occurrence's observed typed failure.
/// `reason`是策略结果；可选`upload_failure`仅包含本次出现观测到的强类型失败。
/// Returns escaped model-visible text without modifying the original user prompt or exposing private upload responses.
/// 返回转义后的模型可见文本，不修改原用户提示词，也不暴露私有上传响应。
fn attachment_text(
    planned: &PlannedResource,
    locator: &str,
    reason: MediaDeliveryRejection,
    upload_failure: Option<&MediaUploadFailure>,
) -> String {
    // JSON escaping keeps filenames inert while disclosing only safe metadata.
    // JSON转义使文件名保持纯数据，同时只披露安全元数据。
    let metadata = serde_json::json!({"resource_id": planned.id.value(), "resource_path": locator, "file_name": planned.entry.as_ref().map(|entry| &entry.original_name), "mime_type": planned.entry.as_ref().and_then(|entry| entry.mime.as_deref()), "byte_len": planned.entry.as_ref().map(|entry| entry.size), "native_delivery": "metadata_only", "reason": reason.explanation(), "upload_failure": upload_failure.map(ToString::to_string)});
    format!(
        "ATTACHMENT: {metadata}. CONTENT_VISIBILITY: resource bytes were not provided; do not infer or guess the attachment contents."
    )
}

#[cfg(test)]
mod tests;

mod readonly;
pub(crate) use readonly::frozen_prompt_media_projection;
