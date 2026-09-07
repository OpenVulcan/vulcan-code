//! Explicit reactivation of forgotten sessions without restoring content or resetting watermarks.
//! 显式重新启用已遗忘会话，不恢复内容或重置水位。

use super::*;
use vulcan_service_api::{
    VmmNativeSessionRestartPreview, VmmNativeSessionRestartResult, VmmNativeSessionRestartSelection,
};

/// Keeps an exact server-owned restart selection until its monotonic deadline.
/// 在单调期限内保存准确服务端重新启用选择。
pub(super) struct PreparedSessionRestart {
    /// Immutable owner, identities and storage version.
    /// 不可变归属、身份及存储版本。
    summary: VmmNativeSessionRestartPreview,
    /// Monotonic creation time used for the five-minute confirmation limit.
    /// 用于五分钟确认期限的单调创建时间。
    created_at: std::time::Instant,
}

impl NativeVmmMaintenance {
    /// Previews a bounded forgotten-session selection without changing persisted state.
    /// 预览有界已遗忘会话选择，不修改持久化状态。
    /// Takes exact project/session identities and returns the normalized version-bound preview.
    /// 接收准确项目及会话身份，返回已归一化且绑定版本的预览。
    pub(crate) async fn preview_session_restart(
        &self,
        mut selection: VmmNativeSessionRestartSelection,
    ) -> Result<VmmNativeSessionRestartPreview, String> {
        // Follow the same lock order as archive, project migration and foreground intake.
        // 遵守归档、项目迁移及前台入库的相同锁顺序。
        let _project_guard = self.project_mutation_gate.lock().await;
        let _operation_guard = self.operation_gate.read().await;
        *self.session_restart_preview.lock().await = None;
        if !self.native_backend_enabled.load(Ordering::Acquire) {
            return Err("请先启用原生记忆。".to_owned());
        }
        self.ensure_project(selection.vmm_project_id).await?;
        normalize_ids(&mut selection.session_source_ids)?;
        // Validate from one authoritative snapshot, never a frontend status label.
        // 从一份权威快照校验，绝不采用前端状态标签。
        let snapshot = self.runtime.storage_snapshot()?;
        validate_selection(&snapshot, &selection)?;
        let summary = VmmNativeSessionRestartPreview {
            preview_id: uuid::Uuid::new_v4().to_string(),
            storage_version: snapshot.storage_version,
            selection,
        };
        *self.session_restart_preview.lock().await = Some(PreparedSessionRestart {
            summary: summary.clone(),
            created_at: std::time::Instant::now(),
        });
        Ok(summary)
    }

    /// Reactivates only the prepared forgotten sessions after mode, lifetime and version checks.
    /// 校验模式、期限及版本后，只重新启用已准备的遗忘会话。
    /// Takes a stable operation and opaque preview ID; returns the exact committed selection.
    /// 接收稳定操作及不透明预览 ID，返回准确已提交选择。
    pub(crate) async fn execute_session_restart(
        &self,
        operation_id: &str,
        preview_id: &str,
    ) -> Result<VmmNativeSessionRestartResult, String> {
        let _project_guard = self.project_mutation_gate.lock().await;
        let _operation_guard = self.operation_gate.write().await;
        if !self.native_backend_enabled.load(Ordering::Acquire) {
            return Err("原生记忆已关闭，请重新启用并预览。".to_owned());
        }
        // A retry replays only the same committed preview, never an unrelated operation.
        // 重试只重放同一个已提交预览，不能重放无关操作。
        let kind = format!("native-vmm-session-restart-v1:{preview_id}");
        if let Some(result) = self
            .runtime
            .replay_storage_snapshot_transform(operation_id, &kind)
            .await?
        {
            return Ok(result);
        }
        let prepared = {
            let mut slot = self.session_restart_preview.lock().await;
            match slot.as_ref() {
                Some(prepared) if prepared.summary.preview_id == preview_id => {}
                _ => return Err("重新启用预览不存在或已被替换，请重新预览。".to_owned()),
            }
            slot.take()
                .ok_or_else(|| "重新启用预览已消费，请重新预览。".to_owned())?
        };
        if prepared.created_at.elapsed() > Duration::from_secs(300) {
            return Err("重新启用预览已过期，请重新预览。".to_owned());
        }
        self.ensure_project(prepared.summary.selection.vmm_project_id)
            .await?;
        self.runtime
            .transform_storage_snapshot(
                operation_id,
                &kind,
                prepared.summary.storage_version,
                |snapshot| apply_restart(snapshot, prepared.summary.selection),
            )
            .await
    }
}

/// Validates and sorts one bounded exact ID set, rejecting malformed identities as a whole.
/// 校验并排序一组有界准确 ID，整体拒绝格式错误身份。
/// Mutates only order and duplicates; returns an error without inventing replacement identities.
/// 只修改顺序及重复项；出错时不编造替代身份。
fn normalize_ids(ids: &mut Vec<String>) -> Result<(), String> {
    if ids.is_empty() || ids.len() > 100 {
        return Err("请选择 1 至 100 个记忆会话。".to_owned());
    }
    for id in ids.iter() {
        if id.is_empty() || id.trim() != id {
            return Err("记忆会话 ID 不能为空或包含首尾空白。".to_owned());
        }
        SessionSourceId::new(id.clone()).map_err(|error| error.to_string())?;
    }
    ids.sort();
    ids.dedup();
    Ok(())
}

/// Checks every selected exact owner before any lifecycle field can change.
/// 在任一生命周期字段改变前检查每个选中项的准确归属。
/// Takes a coherent snapshot and normalized selection; rejects missing or non-forgotten sessions.
/// 接收一致快照及归一化选择，拒绝缺失或非遗忘会话。
fn validate_selection(
    snapshot: &StorageSnapshot,
    selection: &VmmNativeSessionRestartSelection,
) -> Result<(), String> {
    let scope = project_scope(selection.vmm_project_id)?;
    let subject = native_vmm_subject_id()?;
    for id in &selection.session_source_ids {
        let session = snapshot
            .memory_sessions
            .iter()
            .find(|session| {
                session.scope_id == scope
                    && session.subject_id == subject
                    && session.session_source_id.as_str() == id
            })
            .ok_or_else(|| format!("项目中不存在记忆会话 `{id}`，请刷新列表。"))?;
        session
            .status
            .restart_transition()
            .map_err(|_| format!("会话 `{id}` 不是已遗忘状态，请刷新列表。"))?;
    }
    Ok(())
}

/// Changes only lifecycle state and leaves all content, source identities and observation watermarks untouched.
/// 只改变生命周期状态，保留所有内容、来源身份及观察水位不变。
/// Takes the host transaction snapshot and selection; returns its next committed version and exact targets.
/// 接收宿主事务快照及选择，返回下一个提交版本及准确目标。
fn apply_restart(
    snapshot: &mut StorageSnapshot,
    selection: VmmNativeSessionRestartSelection,
) -> Result<VmmNativeSessionRestartResult, String> {
    validate_selection(snapshot, &selection)?;
    let version = snapshot
        .storage_version
        .checked_add(1)
        .ok_or_else(|| "原生记忆存储版本已耗尽。".to_owned())?;
    let scope = project_scope(selection.vmm_project_id)?;
    let subject = native_vmm_subject_id()?;
    for session in &mut snapshot.memory_sessions {
        if session.scope_id == scope
            && session.subject_id == subject
            && selection
                .session_source_ids
                .iter()
                .any(|id| id == session.session_source_id.as_str())
        {
            session.status = session.status.restart_transition()?;
        }
    }
    Ok(VmmNativeSessionRestartResult {
        storage_version: version,
        selection,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rejects invalid identity sets and preserves exact normalized IDs.
    /// 拒绝非法身份集合，并保留准确归一化 ID。
    #[test]
    fn restart_ids_are_bounded_and_exact() {
        for mut ids in [
            Vec::new(),
            vec![String::new()],
            vec![" x".to_owned()],
            vec!["x".to_owned(); 101],
        ] {
            assert!(normalize_ids(&mut ids).is_err());
        }
        let mut ids = vec!["z".to_owned(), "a".to_owned(), "z".to_owned()];
        normalize_ids(&mut ids).expect("normalize");
        assert_eq!(ids, vec!["a", "z"]);
    }

    /// Reactivates only the exact forgotten owner and refuses invalid batches without partial changes.
    /// 只重新启用准确已遗忘归属，拒绝非法批次且不产生部分修改。
    #[test]
    fn restart_changes_only_selected_lifecycle() {
        let mut snapshot = vulcan_memorymesh_core::InMemoryStorage::default().snapshot();
        for (project, id, status) in [
            (
                2,
                "same",
                vulcan_memorymesh_core::StorageSessionStatus::Forgotten,
            ),
            (
                2,
                "other",
                vulcan_memorymesh_core::StorageSessionStatus::Recycled,
            ),
            (
                3,
                "same",
                vulcan_memorymesh_core::StorageSessionStatus::Forgotten,
            ),
        ] {
            snapshot
                .memory_sessions
                .push(vulcan_memorymesh_core::StorageMemorySession {
                    status,
                    subject_id: native_vmm_subject_id().expect("subject"),
                    scope_id: project_scope(project).expect("scope"),
                    session_source_id: SessionSourceId::new(id).expect("session"),
                    last_extraction_observed_at_ns: 7,
                    last_extraction_completed_at_ns: Some(9),
                });
        }
        let before = snapshot.clone();
        let selection = VmmNativeSessionRestartSelection {
            vmm_project_id: 2,
            session_source_ids: vec!["same".to_owned()],
        };
        for invalid in ["other", "missing"] {
            assert!(
                apply_restart(
                    &mut snapshot,
                    VmmNativeSessionRestartSelection {
                        session_source_ids: vec!["same".to_owned(), invalid.to_owned()],
                        ..selection.clone()
                    }
                )
                .is_err()
            );
            assert_eq!(snapshot, before);
        }
        let result = apply_restart(&mut snapshot, selection.clone()).expect("restart");
        let mut expected = before;
        expected.memory_sessions[0].status = vulcan_memorymesh_core::StorageSessionStatus::Active;
        assert_eq!(snapshot, expected);
        assert_eq!(result.selection, selection);
        assert_eq!(result.storage_version, snapshot.storage_version + 1);
        assert!(apply_restart(&mut snapshot, selection).is_err());
        assert_eq!(snapshot, expected);
    }
}
