use super::*;
use vulcan_context::protocol::{
    validate_session_conversation_message_identities,
    validate_session_conversation_record_identities,
};

/// Replaces canonical context fields while preserving locally projected visible records.
/// 替换规范上下文字段，同时保留本地投影出的可见记录。
///
/// # Returns
/// 返回值
/// Returns whether host-visible records changed after one validated atomic replacement,
/// or an identity error with runtime state unchanged.
/// 完成一次经过校验的原子替换后返回宿主可见记录是否变化；身份错误时运行态保持不变。
pub fn apply_checkpoint_conversation_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    checkpoint_state: SessionConversationState,
    now: &str,
) -> Result<bool, String> {
    validate_session_conversation_message_identities(&checkpoint_state.messages)
        .map_err(|error| format!("Invalid checkpoint conversation message identity: {error}"))?;
    validate_session_conversation_record_identities(&checkpoint_state.records)
        .map_err(|error| format!("Invalid checkpoint conversation record identity: {error}"))?;
    // preserved_records moves out only after every fallible checkpoint identity check has completed.
    // preserved_records 仅在全部可能失败的检查点身份校验完成后移出。
    let mut preserved_records = std::mem::take(&mut runtime_state.conversation_state.records);
    let visible_records_changed =
        merge_checkpoint_user_records(&mut preserved_records, &checkpoint_state.records);
    // next_conversation_state receives ownership of the incoming canonical snapshot without cloning history.
    // next_conversation_state 直接取得传入规范快照的所有权，不克隆历史。
    let mut next_conversation_state = checkpoint_state;
    if next_conversation_state.messages.is_empty()
        && !runtime_state.conversation_state.messages.is_empty()
    {
        next_conversation_state.messages =
            std::mem::take(&mut runtime_state.conversation_state.messages);
    }
    next_conversation_state.records = preserved_records;
    // Existing state already satisfies the invariant, while checkpoint identities and cross-state ids were validated above.
    // 既有状态已经满足不变量，检查点身份与跨状态标识也已在上方完成校验。
    runtime_state.conversation_state = next_conversation_state;
    runtime_state.updated_at = now.to_string();
    Ok(visible_records_changed)
}

/// Merges user-visible checkpoint records that were created inside the host-owned controller.
/// 合并由宿主控制器内部创建的用户可见检查点记录。
///
/// # Parameters
/// 参数
/// `preserved_records` are the workbench-local transcript projections retained across checkpoints.
/// `preserved_records` 是跨检查点保留的工作台本地转录投影。
/// `checkpoint_records` are records emitted by the canonical controller snapshot.
/// `checkpoint_records` 是规范控制器快照发出的记录。
///
/// # Returns
/// 返回值
/// `true` when at least one new host-visible user record was inserted, otherwise `false`.
/// 至少插入一条新的宿主可见用户记录时返回 `true`，否则返回 `false`。
fn merge_checkpoint_user_records(
    preserved_records: &mut Vec<SessionConversationRecord>,
    checkpoint_records: &[SessionConversationRecord],
) -> bool {
    // preserved_ids borrows identifiers during admission so the common no-op path clones no history ids.
    // preserved_ids 在接纳期间借用标识，使常见空操作路径不克隆历史标识。
    let mut preserved_ids = preserved_records
        .iter()
        .map(|record| record.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut admitted_records = Vec::<(usize, SessionConversationRecord)>::new();
    for (index, record) in checkpoint_records.iter().enumerate() {
        if !record.visible
            || record.kind != SessionMessageKind::User
            || record.record_kind != SessionMessageRecordKind::Transcript
            || preserved_ids.contains(record.id.as_str())
            || is_internal_session_mailbox_checkpoint_record(record)
        {
            continue;
        }
        preserved_ids.insert(record.id.as_str());
        admitted_records.push((index, record.clone()));
    }
    drop(preserved_ids);
    if admitted_records.is_empty() {
        return false;
    }
    for (index, record) in admitted_records {
        let insert_index =
            checkpoint_user_record_insert_index(preserved_records, checkpoint_records, index);
        preserved_records.insert(insert_index, record);
    }
    true
}

/// Returns whether one checkpoint record is an internal session-mailbox prompt.
/// 判断一条检查点记录是否是内部会话 mailbox 提示词。
fn is_internal_session_mailbox_checkpoint_record(record: &SessionConversationRecord) -> bool {
    let body = record.body.trim_start();
    body.starts_with("<subagent_mailbox_event>") || body.starts_with("<subagent_mailbox_events>")
}

/// Returns where a checkpoint-created user record belongs in the preserved transcript.
/// 返回由检查点创建的用户记录在保留转录中的插入位置。
///
/// # Parameters
/// 参数
/// `preserved_records` are the existing host-side visible records.
/// `preserved_records` 是宿主侧已经存在的可见记录。
/// `checkpoint_records` are the ordered records emitted by the controller snapshot.
/// `checkpoint_records` 是控制器快照发出的有序记录。
/// `checkpoint_index` points at the missing checkpoint user record.
/// `checkpoint_index` 指向缺失的检查点用户记录。
///
/// # Returns
/// 返回值
/// The index that preserves checkpoint and timestamp order as closely as possible.
/// 返回尽可能保持检查点顺序与时间顺序的插入索引。
fn checkpoint_user_record_insert_index(
    preserved_records: &[SessionConversationRecord],
    checkpoint_records: &[SessionConversationRecord],
    checkpoint_index: usize,
) -> usize {
    let checkpoint_record = &checkpoint_records[checkpoint_index];
    if let Some(successor_index) = checkpoint_records[checkpoint_index.saturating_add(1)..]
        .iter()
        .find_map(|successor| {
            preserved_record_index_for_checkpoint_record(preserved_records, successor)
        })
    {
        return successor_index;
    }
    if !checkpoint_record.created_at.trim().is_empty()
        && let Some(time_index) = preserved_records.iter().position(|record| {
            !record.created_at.trim().is_empty() && record.created_at > checkpoint_record.created_at
        })
    {
        return time_index;
    }
    preserved_records.len()
}

/// Finds the preserved record that corresponds to one checkpoint record.
/// 查找与某条检查点记录对应的保留记录。
///
/// # Parameters
/// 参数
/// `preserved_records` are the host-side records retained across checkpoints.
/// `preserved_records` 是跨检查点保留的宿主侧记录。
/// `checkpoint_record` is the controller-side record used as an ordering anchor.
/// `checkpoint_record` 是作为排序锚点的控制器侧记录。
///
/// # Returns
/// 返回值
/// The preserved record index when an anchor can be matched.
/// 当可以匹配锚点时返回保留记录索引。
fn preserved_record_index_for_checkpoint_record(
    preserved_records: &[SessionConversationRecord],
    checkpoint_record: &SessionConversationRecord,
) -> Option<usize> {
    preserved_records
        .iter()
        .position(|record| record.id == checkpoint_record.id)
}
