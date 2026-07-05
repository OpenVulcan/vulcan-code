use super::*;

/// Error returned when one internal pending prompt cannot be added to the queue.
/// 当内部待发送提示词无法加入队列时返回的错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingPromptInternalQueueEnqueueError {
    /// The internal queue has no replaceable slot for the incoming prompt.
    /// 内部队列没有可替换的空位供传入提示词使用。
    QueueFull,
}

/// Shared mutation plan for adding one internal pending prompt entry.
/// 添加单条内部待发送提示词条目的共享变更计划。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingPromptInternalQueueEnqueuePlan {
    /// The same internal prompt is already queued.
    /// 相同的内部提示词已经在队列中。
    AlreadyQueued,
    /// The prompt should be inserted, optionally removing one older internal entry first.
    /// 应插入该提示词，并可选地先移除一条更旧的内部条目。
    Insert {
        /// Existing queue index that should be removed before insertion.
        /// 插入前应移除的现有队列索引。
        remove_index: Option<usize>,
    },
}

/// Result returned after applying one internal pending-prompt enqueue plan.
/// 应用一条内部待发送提示词入队计划后返回的结果。
#[derive(Debug, Clone)]
pub enum PendingPromptInternalQueueEnqueueOutcome {
    /// The incoming prompt was already represented in the queue.
    /// 传入的提示词已经由队列中的现有条目表示。
    AlreadyQueued,
    /// The entry was inserted and may have evicted one older internal entry.
    /// 条目已插入，并且可能挤出了一条更旧的内部条目。
    Inserted {
        /// Internal entry removed to keep the queue within its capacity.
        /// 为保持队列容量而移除的内部条目。
        removed_for_capacity: Box<Option<PendingSessionPromptEntry>>,
    },
}

/// Builds the service-compatible internal queue mutation plan without changing the queue.
/// 在不修改队列的情况下构建与服务兼容的内部队列变更计划。
///
/// # Parameters
/// 参数
/// `queue` is the current internal prompt queue.
/// `queue` 是当前内部提示词队列。
/// `entry` is the prompt entry that the caller wants to enqueue.
/// `entry` 是调用方希望加入队列的提示词条目。
///
/// # Returns
/// 返回值
/// A reusable mutation plan, or a queue-full error when no safe slot exists.
/// 返回可复用的变更计划；当没有安全空位时返回队列已满错误。
pub fn plan_pending_prompt_internal_queue_enqueue(
    queue: &[PendingSessionPromptEntry],
    entry: &PendingSessionPromptEntry,
) -> Result<PendingPromptInternalQueueEnqueuePlan, PendingPromptInternalQueueEnqueueError> {
    if queue.len() >= PENDING_PROMPT_QUEUE_LIMIT {
        let _ = entry;
        return Err(PendingPromptInternalQueueEnqueueError::QueueFull);
    }

    Ok(PendingPromptInternalQueueEnqueuePlan::Insert { remove_index: None })
}

/// Applies one shared internal queue enqueue plan to the provided UI state.
/// 将一条共享内部队列入队计划应用到给定 UI 状态。
///
/// # Parameters
/// 参数
/// `ui_state` owns the internal queue to mutate.
/// `ui_state` 拥有需要变更的内部队列。
/// `entry` is the prompt entry to insert when the plan allows insertion.
/// `entry` 是计划允许插入时要写入的提示词条目。
/// `insert_front` requests front insertion for interrupt-style prompts.
/// `insert_front` 要求以前插方式插入插队式提示词。
/// `plan` is the mutation plan previously returned by `plan_pending_prompt_internal_queue_enqueue`.
/// `plan` 是此前由 `plan_pending_prompt_internal_queue_enqueue` 返回的变更计划。
///
/// # Returns
/// 返回值
/// The applied mutation outcome, including any evicted internal queue entry.
/// 返回已应用的变更结果，包含任何被挤出的内部队列条目。
pub fn apply_pending_prompt_internal_queue_enqueue_plan(
    ui_state: &mut SessionTranscriptUiState,
    entry: PendingSessionPromptEntry,
    insert_front: bool,
    plan: PendingPromptInternalQueueEnqueuePlan,
) -> PendingPromptInternalQueueEnqueueOutcome {
    let PendingPromptInternalQueueEnqueuePlan::Insert { remove_index } = plan else {
        return PendingPromptInternalQueueEnqueueOutcome::AlreadyQueued;
    };
    let removed_for_capacity = remove_index.and_then(|index| {
        (index < ui_state.pending_prompt_queue.len())
            .then(|| ui_state.pending_prompt_queue.remove(index))
    });
    if insert_front {
        ui_state.pending_prompt_queue.insert(0, entry);
    } else {
        ui_state.pending_prompt_queue.push(entry);
    }
    PendingPromptInternalQueueEnqueueOutcome::Inserted {
        removed_for_capacity: Box::new(removed_for_capacity),
    }
}

/// Enqueues one internal prompt entry using the shared service queue semantics.
/// 使用共享服务队列语义加入一条内部提示词条目。
///
/// # Parameters
/// 参数
/// `ui_state` owns the internal queue to mutate.
/// `ui_state` 拥有需要变更的内部队列。
/// `entry` is the prompt entry to insert.
/// `entry` 是需要插入的提示词条目。
/// `insert_front` requests front insertion for interrupt-style prompts.
/// `insert_front` 要求以前插方式插入插队式提示词。
///
/// # Returns
/// 返回值
/// The applied mutation outcome, or a queue-full error.
/// 返回已应用的变更结果，或队列已满错误。
pub fn enqueue_pending_prompt_internal_queue_entry(
    ui_state: &mut SessionTranscriptUiState,
    entry: PendingSessionPromptEntry,
    insert_front: bool,
) -> Result<PendingPromptInternalQueueEnqueueOutcome, PendingPromptInternalQueueEnqueueError> {
    let plan = plan_pending_prompt_internal_queue_enqueue(
        ui_state.pending_prompt_queue.as_slice(),
        &entry,
    )?;
    Ok(apply_pending_prompt_internal_queue_enqueue_plan(
        ui_state,
        entry,
        insert_front,
        plan,
    ))
}

/// Error returned when a visible pending prompt cannot be routed into the internal queue.
/// 当可见待发送提示词无法路由进内部队列时返回的错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingPromptQueueRouteError {
    /// The requested visible panel entry no longer exists.
    /// 请求的可见面板条目已不存在。
    MissingPanelEntry,
    /// The internal queue is already at capacity.
    /// 内部队列已经达到容量上限。
    InternalQueueFull,
}

/// Outcome returned after routing a visible pending prompt into the internal queue.
/// 将可见待发送提示词路由进内部队列后的结果。
#[derive(Debug, Clone)]
pub enum PendingPromptQueueRouteOutcome {
    /// A fresh internal queue entry was created and inserted.
    /// 已创建并插入新的内部队列项。
    Inserted(Box<PendingSessionPromptEntry>),
    /// The visible row had already been inserted earlier.
    /// 该可见行此前已经插入过。
    AlreadyInserted,
}

/// Builds one internal pending prompt entry from a visible panel entry.
/// 从可见面板条目构建一条内部待发送提示词项。
///
/// # Parameters
/// 参数
/// `panel_entry` is the visible queue row selected by the user or passive dispatcher.
/// `panel_entry` 是用户或被动分发器选中的可见队列行。
/// `internal_entry_id` is the stable id assigned by the caller.
/// `internal_entry_id` 是调用方分配的稳定内部条目标识。
/// `created_at` is the Unix-millis text captured for the internal entry.
/// `created_at` 是为内部条目捕获的 Unix 毫秒文本。
///
/// # Returns
/// 返回值
/// The internal prompt entry that can be appended to a session execution queue.
/// 可追加到会话执行队列中的内部提示词条目。
pub fn pending_session_prompt_entry_from_panel_entry(
    panel_entry: &PendingPromptPanelEntry,
    internal_entry_id: String,
    created_at: String,
    source: PendingSessionPromptSource,
) -> PendingSessionPromptEntry {
    PendingSessionPromptEntry {
        id: internal_entry_id,
        prompt: panel_entry.prompt.clone(),
        hidden_model_guidance: panel_entry.hidden_model_guidance.clone(),
        preview: panel_entry.preview.clone(),
        attachments: panel_entry.attachments.clone(),
        model_reference: panel_entry.model_reference.clone(),
        runtime_options: panel_entry.runtime_options.clone(),
        source,
        task_id: None,
        panel_entry_id: Some(panel_entry.id.clone()),
        created_at,
    }
}

/// Takes consecutive manually guided internal prompts using the shared dispatch priority.
/// 按共享分发优先级取出连续的手动引导内部提示词。
pub fn take_guided_pending_prompt_entries(
    ui_state: &mut SessionTranscriptUiState,
    _priority_override: bool,
) -> Vec<PendingSessionPromptEntry> {
    let mut guided_entries = Vec::new();
    let mut removed_panel_entry_ids = Vec::new();

    while !ui_state.pending_prompt_queue.is_empty() {
        let dispatch_index =
            pending_prompt_dispatch_index(ui_state.pending_prompt_queue.as_slice(), false);
        let Some(panel_entry_id) = ui_state.pending_prompt_queue[dispatch_index]
            .panel_entry_id
            .clone()
        else {
            break;
        };
        let entry = ui_state.pending_prompt_queue.remove(dispatch_index);
        removed_panel_entry_ids.push(panel_entry_id);
        guided_entries.push(entry);
    }
    for panel_entry_id in removed_panel_entry_ids {
        let _ = remove_pending_prompt_panel_entry(ui_state, &panel_entry_id);
    }
    guided_entries
}

/// Takes guided prompt entries that may be safely injected at a provider runtime boundary.
/// 取出可在 Provider 运行时边界安全注入的引导提示词条目。
///
/// # Parameters
/// 参数
/// `ui_state` is the session-scoped queue state mutated by the shared router.
/// `ui_state` 是共享路由器会修改的会话级队列状态。
/// `context` contains the same routing facts used by idle queued-prompt dispatch.
/// `context` 包含与空闲队列分发相同的路由事实。
///
/// # Returns
/// 返回值
/// Guided prompt entries in intervention order.
/// 返回按干预顺序排列的引导提示词。
pub fn take_runtime_intervention_guided_pending_prompt_entries(
    ui_state: &mut SessionTranscriptUiState,
    context: PendingPromptDispatchContext,
) -> Vec<PendingSessionPromptEntry> {
    let _ = context;
    take_guided_pending_prompt_entries(ui_state, false)
}
