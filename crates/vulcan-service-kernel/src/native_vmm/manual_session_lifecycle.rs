//! Applies manual whole-session restore and purge to the exact saved session owners.
//! 将人工完整会话恢复及清除应用到准确保存的会话归属。

use super::*;
use vulcan_memorymesh_core::{StorageManualRecycleSelection, StorageSessionStatus};

/// Checks all whole-session tombstones before restore or purge; turn-only batches own none.
/// 恢复或清除前检查全部完整会话墓碑；仅回合批次不拥有会话。
/// Takes the current transaction snapshot and saved selection; returns explicit conflicts.
/// 接收当前事务快照及保存的选择；冲突时返回明确错误。
pub(super) fn validate(
    snapshot: &StorageSnapshot,
    selection: Option<&StorageManualRecycleSelection>,
) -> Result<(), String> {
    // Only an explicit whole-session selection owns lifecycle transitions.
    // 只有明确完整会话选择才拥有生命周期迁移。
    let Some(StorageManualRecycleSelection::Sessions { sessions }) = selection else {
        return Ok(());
    };
    for original in sessions {
        // Composite identity remains authoritative even after a project migration renamed the source ID.
        // 即使项目迁移已将来源 ID 改名，复合身份仍是权威。
        let current = snapshot
            .memory_sessions
            .iter()
            .find(|session| {
                session.subject_id == original.subject_id
                    && session.scope_id == original.scope_id
                    && session.session_source_id == original.session_source_id
            })
            .ok_or_else(|| {
                format!(
                    "原会话 `{}` 的回收状态不存在，请刷新。",
                    original.session_source_id
                )
            })?;
        if current.status != StorageSessionStatus::Recycled
            || current.last_extraction_observed_at_ns != 0
            || current.last_extraction_completed_at_ns.is_some()
        {
            return Err(format!(
                "原会话 `{}` 的回收状态已变化，请刷新。",
                original.session_source_id
            ));
        }
    }
    Ok(())
}

/// Restores original windows as active, or forgets scrubbed tombstones on permanent removal.
/// 恢复原水位并设为活动状态，或在永久移除时将已清理墓碑设为遗忘。
/// Takes the transaction, saved selection and restore/purge choice; never changes unrelated sessions.
/// 接收事务、保存的选择及恢复/清除选择；不改变无关会话。
pub(super) fn apply(
    snapshot: &mut StorageSnapshot,
    selection: Option<&StorageManualRecycleSelection>,
    restore: bool,
) -> Result<(), String> {
    validate(snapshot, selection)?;
    // A turn-only or policy batch must leave every session classification untouched.
    // 仅回合或策略批次必须保持全部会话分类不变。
    let Some(StorageManualRecycleSelection::Sessions { sessions }) = selection else {
        return Ok(());
    };
    for current in &mut snapshot.memory_sessions {
        // Matching is exact; absence was rejected before any lifecycle field changed.
        // 准确匹配；缺失情况已在任一生命周期字段改变之前被拒绝。
        if let Some(original) = sessions.iter().find(|original| {
            current.subject_id == original.subject_id
                && current.scope_id == original.scope_id
                && current.session_source_id == original.session_source_id
        }) {
            if restore {
                *current = original.clone();
                current.status = StorageSessionStatus::Active;
            } else {
                current.status = StorageSessionStatus::Forgotten;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_memorymesh_core::{InMemoryStorage, StorageMemorySession};

    /// Creates a fully valid empty-session batch with a distinct unaffected session.
    /// 创建完整有效的空会话批次，并保留一个独立不受影响的会话。
    /// Returns original metadata, a coherent snapshot and its exact batch.
    /// 返回原始元数据、一致快照及其准确批次。
    fn fixture() -> (
        StorageMemorySession,
        StorageSnapshot,
        StorageRetentionTrashBatch,
    ) {
        // Original windows and archived classification belong to the saved session snapshot.
        // 原窗口与归档分类属于保存的会话快照。
        let original = StorageMemorySession {
            status: StorageSessionStatus::Archived,
            subject_id: native_vmm_subject_id().expect("subject"),
            scope_id: project_scope(2).expect("scope"),
            session_source_id: SessionSourceId::new("selected-session").expect("session"),
            last_extraction_observed_at_ns: 20,
            last_extraction_completed_at_ns: Some(30),
        };
        // The live recycled owner has no usable extraction window.
        // 活动回收归属没有可用提取窗口。
        let mut tombstone = original.clone();
        tombstone.status = StorageSessionStatus::Recycled;
        tombstone.last_extraction_observed_at_ns = 0;
        tombstone.last_extraction_completed_at_ns = None;
        // The same source name in another project is not the selected owner.
        // 另一项目内的相同来源名称不是所选归属。
        let mut unaffected = original.clone();
        unaffected.scope_id = project_scope(3).expect("other project");
        // Construct the full production snapshot before adding fixture records.
        // 先构造完整生产快照，再加入夹具记录。
        let mut snapshot = InMemoryStorage::new().snapshot();
        snapshot.memory_sessions = vec![tombstone, unaffected];
        // The manual reason is paired with original session metadata below.
        // 人工原因码与下面的原会话元数据配对。
        let mut batch = retention_trash_batch(
            "manual-session".to_owned(),
            original.subject_id.clone(),
            original.scope_id.clone(),
            40,
            100,
            RetentionReasonCode::ManualSessionRecycle,
        );
        batch.manual_selection = Some(StorageManualRecycleSelection::Sessions {
            sessions: vec![original.clone()],
        });
        snapshot.retention_trash_batches.push(batch.clone());
        InMemoryStorage::from_snapshot(snapshot.clone()).expect("valid manual session snapshot");
        (original, snapshot, batch)
    }

    /// Verifies empty sessions survive batching and restore original windows as active, not archived.
    /// 验证空会话可保留在批次中，恢复原水位并设为活动而非归档状态。
    #[test]
    fn manual_session_restore_preserves_original_windows_and_exact_owner() {
        // Restore one exact batch while retaining the foreign same-name session as a control.
        // 恢复一个准确批次，并保留其他归属同名会话作为对照。
        let (mut original, mut snapshot, batch) = fixture();
        // Capture unrelated metadata before applying any transition.
        // 在任一迁移之前捕获无关元数据。
        let unaffected = snapshot.memory_sessions[1].clone();
        ensure_recycle_restore_has_no_conflicts(&snapshot, &batch).expect("exact tombstone");
        snapshot.retention_trash_batches.clear();
        push_retention_batch(&mut snapshot, batch.clone());
        assert_eq!(
            snapshot.retention_trash_batches.len(),
            1,
            "empty sessions remain restorable"
        );
        snapshot.retention_trash_batches.clear();
        restore_retention_batch(&mut snapshot, batch).expect("restore metadata");
        original.status = StorageSessionStatus::Active;
        assert_eq!(snapshot.memory_sessions, vec![original, unaffected]);
        InMemoryStorage::from_snapshot(snapshot).expect("restored snapshot loads");
    }

    /// Verifies permanent removal forgets only the owned session, while conflicts reject before mutation.
    /// 验证永久移除仅遗忘所归属会话，冲突在变更前拒绝。
    #[test]
    fn manual_session_purge_forgets_only_the_saved_owner() {
        // Use independent snapshots for conflict rejection and successful purge.
        // 冲突拒绝与成功清除使用独立快照。
        let (_, mut snapshot, batch) = fixture();
        // A foreign session must survive byte-for-byte in both paths.
        // 两条路径均必须完整保留其他归属会话。
        let unaffected = snapshot.memory_sessions[1].clone();
        // Active classification is incompatible with the saved recycle owner.
        // 活动分类与保存的回收归属不一致。
        let mut conflicting = snapshot.clone();
        conflicting.memory_sessions[0].status = StorageSessionStatus::Active;
        // A rejected operation must leave even the conflicting snapshot unchanged.
        // 被拒绝操作也必须保持该冲突快照不变。
        let before = conflicting.clone();
        assert!(apply(&mut conflicting, batch.manual_selection.as_ref(), false).is_err());
        assert_eq!(conflicting, before);
        snapshot.retention_trash_batches.clear();
        apply(&mut snapshot, batch.manual_selection.as_ref(), false).expect("forget tombstone");
        assert_eq!(
            snapshot.memory_sessions[0].status,
            StorageSessionStatus::Forgotten
        );
        assert_eq!(
            snapshot.memory_sessions[0].last_extraction_observed_at_ns,
            0
        );
        assert_eq!(
            snapshot.memory_sessions[0].last_extraction_completed_at_ns,
            None
        );
        assert_eq!(snapshot.memory_sessions[1], unaffected);
        InMemoryStorage::from_snapshot(snapshot).expect("forgotten snapshot loads");
    }

    /// Verifies migration rewrites saved metadata and tombstones together before restoration or clear.
    /// 验证迁移在恢复或清空之前一起改写保存元数据及墓碑。
    #[test]
    fn manual_session_migration_preserves_restore_ownership() {
        // The target already contains the same source name, requiring the explicit rename below.
        // 目标已经存在相同来源名称，因此需要下面的显式改名。
        let (_, mut snapshot, _) = fixture();
        apply_migration_records(
            &mut snapshot,
            &project_scope(2).expect("source"),
            &project_scope(3).expect("target"),
            &[NativeVmmSessionRename {
                from: "selected-session".to_owned(),
                to: "selected-session-1".to_owned(),
            }],
        )
        .expect("move exact saved owner");
        InMemoryStorage::from_snapshot(snapshot.clone()).expect("migrated batch loads");
        // Restoration and project clear start from the same migrated authoritative state.
        // 恢复与项目清空从相同迁移后权威状态开始。
        let mut restore = snapshot.clone();
        // Removing the batch before restore mirrors the actual maintenance transaction.
        // 恢复前移出批次，与实际维护事务一致。
        let batch = restore.retention_trash_batches.remove(0);
        restore_retention_batch(&mut restore, batch).expect("restore migrated session");
        assert_eq!(
            restore.memory_sessions[0].session_source_id.as_str(),
            "selected-session-1"
        );
        assert_eq!(
            restore.memory_sessions[0].scope_id,
            project_scope(3).expect("target")
        );
        assert_eq!(
            restore.memory_sessions[0].last_extraction_observed_at_ns,
            20
        );
        assert_eq!(
            restore.memory_sessions[0].status,
            StorageSessionStatus::Active
        );
        assert_eq!(
            restore.memory_sessions[1].status,
            StorageSessionStatus::Archived
        );
        clear_project(&mut snapshot, 3).expect("clear destination");
        assert!(snapshot.retention_trash_batches.is_empty());
        assert!(snapshot.memory_sessions.is_empty());
    }
}
