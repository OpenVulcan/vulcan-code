//! Validates exact manual recycle selections against their durable session tombstones.
//! 对照耐久会话墓碑校验准确人工回收选择。

use super::*;
use vulcan_memorymesh_core_contracts::ScopeId;

/// Preserves the user's exact selection without inferring sessions from surviving records.
/// 保留用户准确选择，不从残存记录推断所选会话。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StorageManualRecycleSelection {
    /// Saves original metadata, including empty sessions, for explicit whole-session restore.
    /// 保存原始元数据，包含空会话，用于显式完整会话恢复。
    Sessions {
        /// Exact selected original sessions before transition to recycled tombstones.
        /// 转为已回收墓碑之前准确选中的原始会话。
        sessions: Vec<StorageMemorySession>,
    },
    /// Saves the original selected turns without granting ownership of their source sessions.
    /// 保存原始所选回合，不授予其来源会话的所有权。
    Turns {
        /// Canonical IDs selected before moving the corresponding original records.
        /// 移动对应原始记录之前选中的规范 ID。
        turn_ids: Vec<TurnId>,
    },
}

/// Identifies one session by subject, project and source ID, never by a display name.
/// 使用主体、项目及来源 ID 标识一个会话，绝不使用显示名称。
type SessionOwner = (SubjectId, ScopeId, SessionSourceId);

/// Validates selection shape, source ownership and exclusive tombstone ownership on snapshot load.
/// 加载快照时校验选择形状、来源归属及墓碑独占归属。
/// Takes one batch, active storage and prior reservations; returns an explicit invariant error.
/// 接收一个批次、活动存储及已有占用集合；违反不变量时返回明确错误。
pub(super) fn validate_selection(
    batch: &StorageRetentionTrashBatch,
    storage: &InMemoryStorage,
    reserved_sessions: &mut BTreeSet<SessionOwner>,
) -> Result<(), String> {
    // A reason and its source must agree; absent metadata cannot be used as manual compatibility.
    // 原因码必须与来源一致；缺失元数据不能作为人工回收兼容路径。
    let selection = match (&batch.manual_selection, batch.reason) {
        (
            None,
            RetentionReasonCode::ManualSessionRecycle | RetentionReasonCode::ManualTurnRecycle,
        ) => {
            return Err("manual recycle batch is missing its exact selection".to_owned());
        }
        (None, _) => return Ok(()),
        (Some(selection), _) => selection,
    };
    if !batch.dataset.profile_nodes.is_empty() {
        return Err("manual recycle must preserve independent profiles".to_owned());
    }
    match selection {
        StorageManualRecycleSelection::Turns { turn_ids } => {
            if batch.reason != RetentionReasonCode::ManualTurnRecycle
                || turn_ids.is_empty()
                || turn_ids.len() > 100
                || !batch.dataset.chat_compact_boundaries.is_empty()
            {
                return Err("manual turn recycle has invalid selection metadata".to_owned());
            }
            // Exact set equality rejects duplicates, unselected records and missing selected turns.
            // 准确集合相等检查拒绝重复项、未选中记录及缺失的所选回合。
            let selected = turn_ids.iter().collect::<BTreeSet<_>>();
            if selected.len() != turn_ids.len()
                || selected != batch.dataset.turns.iter().map(|turn| &turn.id).collect()
            {
                return Err(
                    "manual turn recycle selection differs from its original records".to_owned(),
                );
            }
        }
        StorageManualRecycleSelection::Sessions { sessions } => {
            if batch.reason != RetentionReasonCode::ManualSessionRecycle
                || sessions.is_empty()
                || sessions.len() > 100
            {
                return Err("manual session recycle has invalid selection metadata".to_owned());
            }
            // Selected source IDs are local to the already checked batch owner.
            // 所选来源 ID 属于已经校验的批次归属。
            let mut selected = BTreeSet::new();
            for original in sessions {
                if original.subject_id != batch.subject_id
                    || original.scope_id != batch.scope_id
                    || !original.status.allows_runtime_memory()
                    || !selected.insert(&original.session_source_id)
                    || original.last_extraction_observed_at_ns < 0
                    || match original.last_extraction_completed_at_ns {
                        None => original.last_extraction_observed_at_ns != 0,
                        Some(completed) => completed < original.last_extraction_observed_at_ns,
                    }
                {
                    return Err(
                        "manual session recycle contains invalid original metadata".to_owned()
                    );
                }
                // Preserve full composite identity when two projects reuse the same session string.
                // 两个项目复用同一会话字符串时，仍保留完整复合身份。
                let owner = (
                    original.subject_id.clone(),
                    original.scope_id.clone(),
                    original.session_source_id.clone(),
                );
                if !reserved_sessions.insert(owner.clone()) {
                    return Err("multiple recycle batches own one original session".to_owned());
                }
                // A restore owner must exist as a scrubbed tombstone, not a new or already active session.
                // 恢复归属必须以已清理墓碑存在，不能是新会话或已活动会话。
                let tombstone = storage
                    .memory_sessions
                    .get(&owner)
                    .ok_or_else(|| "manual session recycle has no session tombstone".to_owned())?;
                if tombstone.status != StorageSessionStatus::Recycled
                    || tombstone.last_extraction_observed_at_ns != 0
                    || tombstone.last_extraction_completed_at_ns.is_some()
                    || storage.turns.values().any(|turn| {
                        turn.subject_id == owner.0
                            && turn.scope_id == owner.1
                            && turn.session_source_id == owner.2
                    })
                {
                    return Err(
                        "manual session recycle has an inconsistent live tombstone".to_owned()
                    );
                }
            }
            if batch
                .dataset
                .turns
                .iter()
                .any(|turn| !selected.contains(&turn.session_source_id))
                || batch
                    .dataset
                    .chat_compact_boundaries
                    .iter()
                    .any(|boundary| !selected.contains(&boundary.session_source_id))
            {
                return Err("manual session recycle contains an unselected session".to_owned());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an empty archived session whose metadata survives manual recycling.
    /// 构建空归档会话，其元数据应在人工回收中保留。
    /// Returns a complete valid snapshot with one original session and its scrubbed tombstone.
    /// 返回完整有效快照，包含一个原会话及其已清理墓碑。
    fn session_snapshot() -> StorageSnapshot {
        // Original metadata remains separate from the scrubbed live tombstone.
        // 原始元数据与已清理活动墓碑分离。
        let original = StorageMemorySession {
            status: StorageSessionStatus::Archived,
            subject_id: SubjectId::new("subject").expect("subject"),
            scope_id: ScopeId::new("project").expect("project"),
            session_source_id: SessionSourceId::new("empty-session").expect("session"),
            last_extraction_observed_at_ns: 20,
            last_extraction_completed_at_ns: Some(30),
        };
        // The live record retains ownership but no extraction window while recycled.
        // 已回收活动记录保留归属，但不保留提取窗口。
        let mut tombstone = original.clone();
        tombstone.status = StorageSessionStatus::Recycled;
        tombstone.last_extraction_observed_at_ns = 0;
        tombstone.last_extraction_completed_at_ns = None;
        // Use the production snapshot constructor instead of manually duplicating its schema.
        // 使用生产快照构造器，不手写重复结构。
        let mut snapshot = InMemoryStorage::new().snapshot();
        snapshot.memory_sessions.push(tombstone);
        snapshot
            .retention_trash_batches
            .push(StorageRetentionTrashBatch {
                batch_id: "manual-empty-session".to_owned(),
                subject_id: original.subject_id.clone(),
                scope_id: original.scope_id.clone(),
                recycled_at_ns: 40,
                expires_at_ns: 100,
                reason: RetentionReasonCode::ManualSessionRecycle,
                manual_selection: Some(StorageManualRecycleSelection::Sessions {
                    sessions: vec![original],
                }),
                dataset: ExportDataset::empty(),
                analysis_attempts: Vec::new(),
            });
        snapshot
    }

    /// Verifies empty original sessions round-trip, but duplicate or changed owners cannot load.
    /// 验证空原会话可完整往返，但重复或变化的归属不能加载。
    #[test]
    fn manual_sessions_require_unique_scrubbed_tombstones() {
        // Keep one known-valid baseline so each case changes exactly one invariant.
        // 保留一个已知有效基线，每个用例只改变一个不变量。
        let valid = session_snapshot();
        assert_eq!(
            InMemoryStorage::from_snapshot(valid.clone())
                .expect("empty session loads")
                .snapshot(),
            valid
        );
        for changed in 0..6 {
            // Each malformed snapshot is independent of earlier rejected cases.
            // 每个错误快照独立于之前被拒绝的用例。
            let mut invalid = valid.clone();
            match changed {
                0 => invalid.memory_sessions.clear(),
                1 => invalid.memory_sessions[0].status = StorageSessionStatus::Active,
                2 => {
                    invalid.memory_sessions[0].last_extraction_observed_at_ns = 1;
                    invalid.memory_sessions[0].last_extraction_completed_at_ns = Some(1);
                }
                3 => invalid.retention_trash_batches[0].manual_selection = None,
                4 => {
                    invalid.retention_trash_batches[0].reason =
                        RetentionReasonCode::IdleSessionRecycle
                }
                5 => {
                    // A different batch ID cannot grant a second owner of the same original session.
                    // 不同批次 ID 不能授予同一原会话的第二个归属。
                    let mut duplicate = invalid.retention_trash_batches[0].clone();
                    duplicate.batch_id = "another-owner".to_owned();
                    invalid.retention_trash_batches.push(duplicate);
                }
                _ => unreachable!(),
            }
            assert!(
                InMemoryStorage::from_snapshot(invalid).is_err(),
                "case {changed}"
            );
        }
    }

    /// Verifies original metadata cannot smuggle foreign owners, invalid watermarks or empty selections.
    /// 验证原元数据不能夹带其他归属、无效水位或空选择。
    #[test]
    fn manual_session_originals_are_exact_and_bounded() {
        for changed in 0..7 {
            // Alter only the saved original metadata, leaving the live tombstone valid.
            // 只改变保存的原始元数据，保持活动墓碑有效。
            let mut invalid = session_snapshot();
            // The fixture declares exactly the session selection variant.
            // 夹具准确声明会话选择分支。
            let Some(StorageManualRecycleSelection::Sessions { sessions }) =
                &mut invalid.retention_trash_batches[0].manual_selection
            else {
                panic!("session fixture")
            };
            match changed {
                0 => sessions[0].scope_id = ScopeId::new("foreign").expect("foreign"),
                1 => sessions[0].status = StorageSessionStatus::Recycled,
                2 => sessions[0].last_extraction_observed_at_ns = -1,
                3 => sessions[0].last_extraction_completed_at_ns = Some(19),
                4 => sessions.clear(),
                5 => sessions.push(sessions[0].clone()),
                6 => sessions.resize(101, sessions[0].clone()),
                _ => unreachable!(),
            }
            assert!(
                InMemoryStorage::from_snapshot(invalid).is_err(),
                "case {changed}"
            );
        }
    }

    /// Verifies turn-only metadata matches its exact original records and never claims session metadata.
    /// 验证仅回合元数据与准确原记录匹配，且不声明会话元数据。
    #[test]
    fn manual_turn_selection_matches_the_original_record_set() {
        // Turn-only recycling leaves the source session active.
        // 仅回合回收使来源会话保持活动。
        let mut valid = session_snapshot();
        valid.memory_sessions[0].status = StorageSessionStatus::Active;
        // The same canonical ID must appear in the selection and the original record.
        // 选择与原始记录必须出现同一规范 ID。
        let id = TurnId::new("vmm-turn-2").expect("turn");
        // Replace the fixture's original-session ownership with exact turn ownership.
        // 将夹具的原会话归属替换为准确回合归属。
        let batch = &mut valid.retention_trash_batches[0];
        batch.reason = RetentionReasonCode::ManualTurnRecycle;
        batch.manual_selection = Some(StorageManualRecycleSelection::Turns {
            turn_ids: vec![id.clone()],
        });
        batch.dataset.turns.push(Turn {
            id: id.clone(),
            subject_id: batch.subject_id.clone(),
            scope_id: batch.scope_id.clone(),
            session_source_id: SessionSourceId::new("empty-session").expect("session"),
            timeline: Vec::new(),
            previous_turn_ids: Vec::new(),
            next_turn_ids: Vec::new(),
            created_at_ns: 1,
            status: TurnStatus::Pending,
            user_input_kind: None,
            analysis_details: None,
        });
        assert!(InMemoryStorage::from_snapshot(valid.clone()).is_ok());
        for ids in [
            Vec::new(),
            vec![id.clone(), id],
            vec![TurnId::new("vmm-turn-3").expect("other")],
        ] {
            // Duplicate, missing and foreign IDs must each reject the entire candidate snapshot.
            // 重复、缺失及其他 ID 均必须拒绝整个候选快照。
            let mut invalid = valid.clone();
            invalid.retention_trash_batches[0].manual_selection =
                Some(StorageManualRecycleSelection::Turns { turn_ids: ids });
            assert!(InMemoryStorage::from_snapshot(invalid).is_err());
        }
    }
}
