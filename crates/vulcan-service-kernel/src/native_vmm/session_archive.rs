//! Native session archive preview and confirmation, independent of content recycling.
//! 独立于内容回收的原生会话归档预览与确认。

use super::*;
use vulcan_service_api::{
    VmmNativeSessionArchivePreview, VmmNativeSessionArchiveResult, VmmNativeSessionArchiveSelection,
};

/// Keeps the exact latest preview and a monotonic confirmation deadline.
/// 保存准确最近预览及单调确认期限。
pub(super) struct PreparedSessionArchive {
    /// Immutable selection and storage version.
    /// 不可变选择及存储版本。
    summary: VmmNativeSessionArchivePreview,
    /// Monotonic creation time, unaffected by wall-clock edits.
    /// 不受系统时钟修改影响的单调创建时间。
    created_at: std::time::Instant,
}

impl NativeVmmMaintenance {
    /// Prepares up to one hundred existing sessions for archive or unarchive without changing data.
    /// 在不修改数据的情况下，为至多一百条已有会话准备归档或取消归档。
    /// Takes the exact project and selection; invalid identities or lifecycle transitions fail together.
    /// 接收准确项目及选择；非法身份或生命周期转换整体失败。
    pub(crate) async fn preview_session_archive(
        &self,
        selection: VmmNativeSessionArchiveSelection,
    ) -> Result<VmmNativeSessionArchivePreview, String> {
        // Preserve the project-operation lock order used by all native maintenance actions.
        // 保持全部原生维护动作使用的项目及操作锁顺序。
        let _project_guard = self.project_mutation_gate.lock().await;
        let _operation_guard = self.operation_gate.read().await;
        *self.session_archive_preview.lock().await = None;
        if !self.native_backend_enabled.load(Ordering::Acquire) {
            return Err("请先启用原生记忆。".to_owned());
        }
        self.ensure_project(selection.vmm_project_id).await?;
        // Bound and normalize only declared session identities, never session titles.
        // 只限制及归一化声明的会话身份，绝不使用会话标题。
        let selection = normalize_selection(selection)?;
        let snapshot = self.runtime.storage_snapshot()?;
        validate_selection(&snapshot, &selection)?;
        let summary = VmmNativeSessionArchivePreview {
            preview_id: uuid::Uuid::new_v4().to_string(),
            storage_version: snapshot.storage_version,
            selection,
        };
        *self.session_archive_preview.lock().await = Some(PreparedSessionArchive {
            summary: summary.clone(),
            created_at: std::time::Instant::now(),
        });
        Ok(summary)
    }

    /// Commits only the server-prepared selection after version, mode and lifetime checks.
    /// 校验版本、模式及期限后只提交服务端准备的选择。
    /// Takes the stable command ID and preview ID; returns the exact committed classification or an error.
    /// 接收稳定命令 ID 及预览 ID；返回准确已提交分类或错误。
    pub(crate) async fn execute_session_archive(
        &self,
        operation_id: &str,
        preview_id: &str,
    ) -> Result<VmmNativeSessionArchiveResult, String> {
        let _project_guard = self.project_mutation_gate.lock().await;
        let _operation_guard = self.operation_gate.write().await;
        if !self.native_backend_enabled.load(Ordering::Acquire) {
            return Err("原生记忆已关闭，请重新启用并预览。".to_owned());
        }
        // The durable operation identity binds to the opaque preview, not to newly supplied targets.
        // 持久操作身份绑定不透明预览，而不是新提交的目标。
        let kind = format!("native-vmm-session-archive-v1:{preview_id}");
        if let Some(result) = self
            .runtime
            .replay_storage_snapshot_transform(operation_id, &kind)
            .await?
        {
            return Ok(result);
        }
        let prepared = {
            let mut slot = self.session_archive_preview.lock().await;
            match slot.as_ref() {
                Some(prepared) if prepared.summary.preview_id == preview_id => {}
                _ => return Err("归档预览不存在或已被替换，请重新预览。".to_owned()),
            }
            slot.take()
                .ok_or_else(|| "归档预览已消费，请重新预览。".to_owned())?
        };
        if prepared.created_at.elapsed() > Duration::from_secs(300) {
            return Err("归档预览已过期，请重新预览。".to_owned());
        }
        self.ensure_project(prepared.summary.selection.vmm_project_id)
            .await?;
        self.runtime
            .transform_storage_snapshot(
                operation_id,
                &kind,
                prepared.summary.storage_version,
                |snapshot| apply_archive(snapshot, prepared.summary.selection),
            )
            .await
    }
}

/// Normalizes a bounded exact identity set; blank or invalid IDs are never silently dropped.
/// 归一化有界准确身份集合；空或非法 ID 绝不静默丢弃。
fn normalize_selection(
    mut selection: VmmNativeSessionArchiveSelection,
) -> Result<VmmNativeSessionArchiveSelection, String> {
    if selection.session_source_ids.is_empty() || selection.session_source_ids.len() > 100 {
        return Err("请选择 1 至 100 个记忆会话。".to_owned());
    }
    for id in &selection.session_source_ids {
        if id.trim() != id || id.is_empty() {
            return Err("记忆会话 ID 不能为空或包含首尾空白。".to_owned());
        }
        SessionSourceId::new(id.clone()).map_err(|error| error.to_string())?;
    }
    selection.session_source_ids.sort();
    selection.session_source_ids.dedup();
    Ok(selection)
}

/// Validates every selected exact owner before any classification changes are applied.
/// 在应用任何分类修改前校验每个选中项的准确归属。
fn validate_selection(
    snapshot: &StorageSnapshot,
    selection: &VmmNativeSessionArchiveSelection,
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
            .archive_transition(selection.archived)
            .map_err(|_| format!("会话 `{id}` 的状态不允许此归档操作，请刷新列表。"))?;
    }
    Ok(())
}

/// Changes only selected lifecycle fields and reports the version committed by the host transform.
/// 只修改选中的生命周期字段，并报告宿主转换提交的版本。
fn apply_archive(
    snapshot: &mut StorageSnapshot,
    selection: VmmNativeSessionArchiveSelection,
) -> Result<VmmNativeSessionArchiveResult, String> {
    validate_selection(snapshot, &selection)?;
    let scope = project_scope(selection.vmm_project_id)?;
    let subject = native_vmm_subject_id()?;
    let ids = selection
        .session_source_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for session in &mut snapshot.memory_sessions {
        if session.scope_id == scope
            && session.subject_id == subject
            && ids.contains(session.session_source_id.as_str())
        {
            session.status = session.status.archive_transition(selection.archived)?;
        }
    }
    Ok(VmmNativeSessionArchiveResult {
        storage_version: snapshot
            .storage_version
            .checked_add(1)
            .ok_or_else(|| "原生记忆存储版本已耗尽。".to_owned())?,
        selection,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds metadata-only sessions with overlapping external names and distinct exact owners.
    /// 构造外部名称重叠但准确归属不同的纯元数据会话。
    fn fixture() -> StorageSnapshot {
        let mut snapshot = vulcan_memorymesh_core::InMemoryStorage::default().snapshot();
        for (project, name) in [(2, "same"), (2, "other"), (3, "same")] {
            snapshot
                .memory_sessions
                .push(vulcan_memorymesh_core::StorageMemorySession {
                    status: vulcan_memorymesh_core::StorageSessionStatus::Active,
                    subject_id: native_vmm_subject_id().expect("subject"),
                    scope_id: project_scope(project).expect("scope"),
                    session_source_id: SessionSourceId::new(name).expect("session"),
                    last_extraction_observed_at_ns: 0,
                    last_extraction_completed_at_ns: None,
                });
        }
        snapshot
    }

    /// Rejects invalid sets and normalizes duplicate identities without confusing them with titles.
    /// 拒绝非法集合，并归一化重复身份，不与标题混淆。
    #[test]
    fn archive_selection_is_bounded_and_exact() {
        let selection = VmmNativeSessionArchiveSelection {
            vmm_project_id: 2,
            session_source_ids: vec!["same".to_owned(), "other".to_owned(), "same".to_owned()],
            archived: true,
        };
        assert_eq!(
            normalize_selection(selection.clone())
                .expect("normalized")
                .session_source_ids,
            vec!["other", "same"]
        );
        for ids in [
            Vec::new(),
            vec![String::new()],
            vec![" same".to_owned()],
            vec!["same".to_owned(); 101],
        ] {
            assert!(
                normalize_selection(VmmNativeSessionArchiveSelection {
                    session_source_ids: ids,
                    ..selection.clone()
                })
                .is_err()
            );
        }
    }

    /// Classifies only the selected owner and keeps the whole batch unchanged on any invalid member.
    /// 只分类选中归属；任一成员非法时保持整个批次不变。
    #[test]
    fn archive_is_owner_scoped_and_invalid_batches_do_not_partially_apply() {
        let mut snapshot = fixture();
        let before = snapshot.clone();
        let selection = VmmNativeSessionArchiveSelection {
            vmm_project_id: 2,
            session_source_ids: vec!["same".to_owned()],
            archived: true,
        };
        assert!(
            apply_archive(
                &mut snapshot,
                VmmNativeSessionArchiveSelection {
                    session_source_ids: vec!["same".to_owned(), "missing".to_owned()],
                    ..selection
                }
            )
            .is_err()
        );
        assert_eq!(snapshot, before);
        apply_archive(&mut snapshot, selection.clone()).expect("archive one owner");
        let mut expected = before;
        expected.memory_sessions[0].status = vulcan_memorymesh_core::StorageSessionStatus::Archived;
        assert_eq!(snapshot, expected);
        assert!(
            apply_archive(
                &mut snapshot,
                VmmNativeSessionArchiveSelection {
                    session_source_ids: vec!["other".to_owned(), "same".to_owned()],
                    ..selection
                }
            )
            .is_err()
        );
        assert_eq!(snapshot, expected);
    }
}
