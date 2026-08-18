use super::*;

/// Routes one visible panel entry into the internal pending prompt queue.
/// 将单条可见面板条目路由进内部待发送提示词队列。
pub fn route_pending_prompt_panel_entry_into_internal_queue(
    ui_state: &mut SessionTranscriptUiState,
    panel_entry_id: &str,
    internal_entry_id: String,
    created_at: String,
    source: PendingSessionPromptSource,
) -> Result<PendingPromptQueueRouteOutcome, PendingPromptQueueRouteError> {
    let Some(panel_index) = ui_state
        .pending_prompt_panel_queue
        .iter()
        .position(|entry| entry.id == panel_entry_id)
    else {
        return Err(PendingPromptQueueRouteError::MissingPanelEntry);
    };
    if ui_state.pending_prompt_panel_queue[panel_index].state
        == PendingPromptPanelEntryState::Inserted
    {
        return Ok(PendingPromptQueueRouteOutcome::AlreadyInserted);
    }
    if ui_state.pending_prompt_queue.len() >= PENDING_PROMPT_QUEUE_LIMIT {
        return Err(PendingPromptQueueRouteError::InternalQueueFull);
    }
    let panel_entry = ui_state.pending_prompt_panel_queue[panel_index].clone();
    let internal_entry = pending_session_prompt_entry_from_panel_entry(
        &panel_entry,
        internal_entry_id,
        created_at,
        source,
    );
    ui_state.pending_prompt_queue.push(internal_entry.clone());
    if let Some(panel_entry) = ui_state.pending_prompt_panel_queue.get_mut(panel_index) {
        panel_entry.state = PendingPromptPanelEntryState::Inserted;
    }
    Ok(PendingPromptQueueRouteOutcome::Inserted(Box::new(
        internal_entry,
    )))
}

/// Takes the next dispatchable prompt from the internal queue or clones the first ready visible row.
/// 从内部队列取出下一条可分发提示词，或克隆第一条待命可见行。
///
/// # Parameters
/// 参数
/// `ui_state` is the session-scoped UI state containing both prompt queues.
/// `ui_state` 是同时包含两个提示词队列的会话级 UI 状态。
/// `_priority_override` is ignored because queued prompts use FIFO dispatch.
/// `_priority_override` 会被忽略，因为排队提示词使用 FIFO 分发。
/// `fallback_internal_entry_id` is used only when a visible ready row becomes the dispatch entry.
/// `fallback_internal_entry_id` 仅在可见待命行转为分发项时使用。
/// `fallback_created_at` is used only when a visible ready row becomes the dispatch entry.
/// `fallback_created_at` 仅在可见待命行转为分发项时使用。
///
/// # Returns
/// 返回值
/// The dispatch prompt entry when one is available.
/// 存在可分发条目时返回待分发提示词项。
pub fn take_next_pending_prompt_entry(
    ui_state: &mut SessionTranscriptUiState,
    _priority_override: bool,
    fallback_internal_entry_id: String,
    fallback_created_at: String,
) -> Option<PendingSessionPromptEntry> {
    if let Some(entry) = take_next_internal_pending_prompt_entry(ui_state, false) {
        return Some(entry);
    }
    let panel_index = ui_state
        .pending_prompt_panel_queue
        .iter()
        .position(|entry| entry.state == PendingPromptPanelEntryState::Ready)?;
    let panel_entry = ui_state.pending_prompt_panel_queue[panel_index].clone();
    Some(pending_session_prompt_entry_from_panel_entry(
        &panel_entry,
        fallback_internal_entry_id,
        fallback_created_at,
        PendingSessionPromptSource::UserFollowup,
    ))
}

/// Takes the next already-internalized pending prompt without promoting a visible ready row.
/// 只取出下一条已经进入内部队列的待发送提示词，不提升可见待命行。
///
/// # Parameters
/// 参数
/// `ui_state` is the session-scoped UI state containing both prompt queues.
/// `ui_state` 是同时包含两个提示词队列的会话级 UI 状态。
/// `_priority_override` is ignored because queued prompts use FIFO dispatch.
/// `_priority_override` 会被忽略，因为排队提示词使用 FIFO 分发。
///
/// # Returns
/// 返回值
/// The internal dispatch prompt entry when one is available.
/// 存在内部可分发条目时返回待分发提示词项。
pub fn take_next_internal_pending_prompt_entry(
    ui_state: &mut SessionTranscriptUiState,
    _priority_override: bool,
) -> Option<PendingSessionPromptEntry> {
    if ui_state.pending_prompt_queue.is_empty() {
        return None;
    }
    Some(ui_state.pending_prompt_queue.remove(0))
}

/// Removes internal pending prompts that match one source.
/// 移除匹配指定来源的内部待发送提示词。
///
/// # Parameters
/// 参数
/// `ui_state` is the session-scoped UI state containing the internal queue.
/// `ui_state` 是包含内部队列的会话级 UI 状态。
/// `source` selects entries that must be removed.
/// `source` 用于选择必须移除的条目。
///
/// # Returns
/// 返回值
/// Number of removed internal queue entries.
/// 被移除的内部队列条目数量。
pub fn remove_internal_pending_prompt_entries_by_source(
    ui_state: &mut SessionTranscriptUiState,
    source: PendingSessionPromptSource,
) -> usize {
    let before = ui_state.pending_prompt_queue.len();
    ui_state
        .pending_prompt_queue
        .retain(|entry| entry.source != source);
    before.saturating_sub(ui_state.pending_prompt_queue.len())
}

/// Removes one visible panel entry and any linked internal queue entries.
/// 移除单条可见面板条目及其关联的内部队列项。
///
/// # Parameters
/// 参数
/// `ui_state` is the session-scoped UI state containing both prompt queues.
/// `ui_state` 是同时包含两个提示词队列的会话级 UI 状态。
/// `panel_entry_id` identifies the visible row to remove.
/// `panel_entry_id` 标识需要移除的可见行。
///
/// # Returns
/// 返回值
/// The removed visible panel entry when it existed.
/// 若可见面板条目存在，则返回被移除的条目。
pub fn remove_pending_prompt_panel_entry(
    ui_state: &mut SessionTranscriptUiState,
    panel_entry_id: &str,
) -> Option<PendingPromptPanelEntry> {
    let panel_index = ui_state
        .pending_prompt_panel_queue
        .iter()
        .position(|entry| entry.id == panel_entry_id)?;
    let removed_entry = ui_state.pending_prompt_panel_queue.remove(panel_index);
    ui_state
        .pending_prompt_queue
        .retain(|entry| entry.panel_entry_id.as_deref() != Some(panel_entry_id));
    Some(removed_entry)
}

/// Applies the service pending-prompt drop-target semantics to the shared visible panel queue.
/// 将服务待发送提示词落点语义应用到共享可见面板队列。
///
/// # Parameters
/// 参数
/// `ui_state` is the session-scoped UI state containing both prompt queues.
/// `ui_state` 是同时包含两个提示词队列的会话级 UI 状态。
/// `dragged_entry_id` identifies the visible row being moved.
/// `dragged_entry_id` 标识正在移动的可见行。
/// `drop_target` is the service-compatible insertion target.
/// `drop_target` 是与服务兼容的插入落点。
///
/// # Returns
/// 返回值
/// Whether the visible queue order changed.
/// 返回可见队列顺序是否发生变化。
pub fn apply_pending_prompt_panel_drop_target(
    ui_state: &mut SessionTranscriptUiState,
    dragged_entry_id: &str,
    drop_target: &PendingPromptPanelDropTarget,
) -> bool {
    let queue = &mut ui_state.pending_prompt_panel_queue;
    let Some(current_index) = queue.iter().position(|entry| entry.id == dragged_entry_id) else {
        return false;
    };
    let mut target_index = match drop_target {
        PendingPromptPanelDropTarget::Before(target_entry_id) => {
            let Some(index) = queue.iter().position(|entry| entry.id == *target_entry_id) else {
                return false;
            };
            index
        }
        PendingPromptPanelDropTarget::End => queue.len(),
    };
    let entry = queue.remove(current_index);
    if current_index < target_index {
        target_index = target_index.saturating_sub(1);
    }
    if target_index == current_index {
        queue.insert(current_index, entry);
        return false;
    }
    let bounded_target_index = target_index.min(queue.len());
    queue.insert(bounded_target_index, entry);
    reorder_internal_pending_prompt_queue_for_panel_order(ui_state);
    true
}

/// Reorders already-inserted internal prompts to match their visible panel order.
/// 按可见面板顺序同步重排已经插入的内部提示词。
///
/// # Parameters
/// 参数
/// `ui_state` contains both the visible panel queue and the internal dispatch queue.
/// `ui_state` 同时包含可见面板队列与内部派发队列。
fn reorder_internal_pending_prompt_queue_for_panel_order(ui_state: &mut SessionTranscriptUiState) {
    let panel_order = ui_state
        .pending_prompt_panel_queue
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.id.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    if panel_order.is_empty() {
        return;
    }
    let mut linked_entries = ui_state
        .pending_prompt_queue
        .iter()
        .cloned()
        .enumerate()
        .filter(|(_, entry)| {
            entry
                .panel_entry_id
                .as_deref()
                .is_some_and(|panel_entry_id| panel_order.contains_key(panel_entry_id))
        })
        .collect::<Vec<_>>();
    if linked_entries.len() < 2 {
        return;
    }
    linked_entries.sort_by_key(|(original_index, entry)| {
        let panel_index = entry
            .panel_entry_id
            .as_deref()
            .and_then(|panel_entry_id| panel_order.get(panel_entry_id))
            .copied()
            .unwrap_or(usize::MAX);
        (panel_index, *original_index)
    });
    let mut reordered_entries = linked_entries.into_iter().map(|(_, entry)| entry);
    for entry in &mut ui_state.pending_prompt_queue {
        let is_linked_panel_entry = entry
            .panel_entry_id
            .as_deref()
            .is_some_and(|panel_entry_id| panel_order.contains_key(panel_entry_id));
        if is_linked_panel_entry && let Some(reordered_entry) = reordered_entries.next() {
            *entry = reordered_entry;
        }
    }
}

/// Clears internal queued prompts and restores linked visible entries to ready.
/// 清空内部待发送提示词，并把关联的可见条目恢复为待命状态。
///
/// # Parameters
/// 参数
/// `ui_state` is the session-scoped UI state containing both prompt queues.
/// `ui_state` 是同时包含两个提示词队列的会话级 UI 状态。
///
/// # Returns
/// 返回值
/// Whether any queue state changed.
/// 是否发生了任何队列状态变更。
pub fn clear_internal_pending_prompt_queue_and_restore_panel(
    ui_state: &mut SessionTranscriptUiState,
) -> bool {
    let had_internal_entries = !ui_state.pending_prompt_queue.is_empty();
    let linked_panel_ids = ui_state
        .pending_prompt_queue
        .iter()
        .filter_map(|entry| entry.panel_entry_id.clone())
        .collect::<BTreeSet<_>>();
    ui_state.pending_prompt_queue.clear();
    let mut changed = had_internal_entries;
    for panel_entry in &mut ui_state.pending_prompt_panel_queue {
        if (linked_panel_ids.is_empty() || linked_panel_ids.contains(&panel_entry.id))
            && panel_entry.state == PendingPromptPanelEntryState::Inserted
        {
            panel_entry.state = PendingPromptPanelEntryState::Ready;
            changed = true;
        }
    }
    changed
}

/// Restores process-scoped guided prompt rows while preserving host-owned hidden queue entries.
/// 恢复进程级引导提示词行，同时保留宿主持有的隐藏队列项。
///
/// # Parameters
/// 参数
/// `ui_state` owns the persisted visible panel and internal pending queues.
/// `ui_state` 持有持久化的可见面板队列与内部待发送队列。
///
/// # Returns
/// 返回值
/// Whether any linked internal entry or stale inserted marker was repaired.
/// 是否修复了任何关联内部项或陈旧的已插入标记。
pub fn restore_guided_pending_prompts_for_fresh_process(
    ui_state: &mut SessionTranscriptUiState,
) -> bool {
    let internal_len_before = ui_state.pending_prompt_queue.len();
    ui_state
        .pending_prompt_queue
        .retain(|entry| entry.panel_entry_id.is_none());
    let mut changed = ui_state.pending_prompt_queue.len() != internal_len_before;
    for panel_entry in &mut ui_state.pending_prompt_panel_queue {
        if panel_entry.state == PendingPromptPanelEntryState::Inserted {
            panel_entry.state = PendingPromptPanelEntryState::Ready;
            changed = true;
        }
    }
    changed
}
