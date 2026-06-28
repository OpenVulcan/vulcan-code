use super::*;

/// Replaces canonical context fields while preserving locally projected visible records.
/// 替换规范上下文字段，同时保留本地投影出的可见记录。
pub fn apply_checkpoint_conversation_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    checkpoint_state: SessionConversationState,
    now: &str,
) {
    let mut preserved_records = runtime_state.conversation_state.records.clone();
    merge_checkpoint_user_records(&mut preserved_records, &checkpoint_state.records);
    let preserved_messages = runtime_state.conversation_state.messages.clone();
    runtime_state.conversation_state = checkpoint_state;
    if runtime_state.conversation_state.messages.is_empty() && !preserved_messages.is_empty() {
        runtime_state.conversation_state.messages = preserved_messages;
    }
    runtime_state.conversation_state.records = preserved_records;
    runtime_state.updated_at = now.to_string();
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
fn merge_checkpoint_user_records(
    preserved_records: &mut Vec<SessionConversationRecord>,
    checkpoint_records: &[SessionConversationRecord],
) {
    let mut preserved_ids = preserved_records
        .iter()
        .map(|record| record.id.clone())
        .collect::<BTreeSet<_>>();
    for (index, record) in checkpoint_records.iter().enumerate() {
        if !record.visible
            || record.kind != SessionMessageKind::User
            || record.record_kind != SessionMessageRecordKind::Transcript
            || preserved_ids.contains(&record.id)
            || is_internal_session_mailbox_checkpoint_record(record)
        {
            continue;
        }
        preserved_ids.insert(record.id.clone());
        let insert_index =
            checkpoint_user_record_insert_index(preserved_records, checkpoint_records, index);
        preserved_records.insert(insert_index, record.clone());
    }
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
