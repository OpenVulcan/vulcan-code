//! Moves exact user-selected sources into recoverable storage while preserving profiles.
//! 将用户准确选择的来源移动到可恢复存储，并保留画像。

use super::*;
use vulcan_memorymesh_core::{StorageManualRecycleSelection, StorageSessionStatus};
use vulcan_service_api::{
    VmmNativeRemovalPreview, VmmNativeRemovalQuery, VmmNativeRemovalResult, VmmNativeRemovalTarget,
};

/// Owns one bounded confirmation independently of client-supplied record lists.
/// 独立于客户端提供的记录列表持有一份有界确认。
pub(super) struct PreparedManualRemoval {
    /// Immutable observed selection and effective rules.
    /// 不可变观察选择及有效规则。
    summary: VmmNativeRemovalPreview,
    /// Monotonic start of the five-minute confirmation lifetime.
    /// 五分钟确认有效期的单调起点。
    created_at: std::time::Instant,
}

impl NativeVmmMaintenance {
    /// Prepares exact source impact without moving any content or calling AI.
    /// 准备准确来源影响，不移动内容或调用 AI。
    /// Takes one bounded project selection and returns its opaque version-bound confirmation.
    /// 接收一份有界项目选择，返回绑定版本的不透明确认。
    pub(crate) async fn preview_manual_removal(
        &self,
        query: VmmNativeRemovalQuery,
    ) -> Result<VmmNativeRemovalPreview, String> {
        // Preserve the established project-before-operation maintenance lock order.
        // 保持既有项目锁先于操作锁的维护锁顺序。
        let _project_guard = self.project_mutation_gate.lock().await;
        // Exclude destructive maintenance while the runtime supplies a coherent snapshot.
        // 运行时提供一致快照期间排除破坏性维护。
        let _operation_guard = self.operation_gate.read().await;
        *self.removal_preview.lock().await = None;
        if !self.native_backend_enabled.load(Ordering::Acquire) {
            return Err("Enable native memory first.".to_owned());
        }
        self.ensure_project(query.vmm_project_id).await?;
        // Read the same owner-specific policy used by scheduled retention.
        // 读取周期保留使用的相同归属专属策略。
        let rules = self
            .runtime
            .project_retention_summary(query.vmm_project_id)
            .await?;
        // All source identities come from one authoritative snapshot.
        // 全部来源身份来自一份权威快照。
        let impact = removal_impact::build_impact(
            &self.runtime.storage_snapshot()?,
            query,
            rules.effective.turn_hot_window_size,
        )?;
        // The token does not encode client-editable target data.
        // 令牌不编码可由客户端修改的目标数据。
        let summary = VmmNativeRemovalPreview {
            preview_id: uuid::Uuid::new_v4().to_string(),
            impact,
            trash_retention_ms: rules.effective.trash_retention_ms,
        };
        *self.removal_preview.lock().await = Some(PreparedManualRemoval {
            summary: summary.clone(),
            created_at: std::time::Instant::now(),
        });
        Ok(summary)
    }

    /// Executes only the prepared source move after mode, version, time and policy checks.
    /// 校验模式、版本、时间及策略后，只执行已准备的来源移动。
    /// Takes a stable operation and exact preview ID; returns the committed batch and impact.
    /// 接收稳定操作及准确预览 ID，返回已提交批次与影响。
    pub(crate) async fn execute_manual_removal(
        &self,
        operation_id: &str,
        preview_id: &str,
    ) -> Result<VmmNativeRemovalResult, String> {
        // Use the same order as foreground intake and project migration.
        // 使用与前台入库及项目迁移相同的顺序。
        let _project_guard = self.project_mutation_gate.lock().await;
        // Moving original records excludes extraction and every other snapshot mutation.
        // 移动原记录时排除提取及全部其他快照变更。
        let _operation_guard = self.operation_gate.write().await;
        if !self.native_backend_enabled.load(Ordering::Acquire) {
            return Err("Native memory is turned off; enable it again and preview.".to_owned());
        }
        // A committed retry is bound to the same opaque preview, not a new client selection.
        // 已提交重试绑定同一不透明预览，而非新的客户端选择。
        let kind = format!("native-vmm-manual-removal-v1:{preview_id}");
        if let Some(result) = self
            .runtime
            .replay_storage_snapshot_transform(operation_id, &kind)
            .await?
        {
            return Ok(result);
        }
        // A mismatched token cannot consume another client's current preview.
        // 不匹配令牌不能消费另一客户端的当前预览。
        let prepared = {
            // Only the selected slot is consumed, once.
            // 只消费所选槽位，且仅一次。
            let mut slot = self.removal_preview.lock().await;
            match slot.as_ref() {
                Some(prepared) if prepared.summary.preview_id == preview_id => {}
                _ => {
                    return Err(
                        "The removal preview does not exist or was replaced; preview again."
                            .to_owned(),
                    );
                }
            }
            slot.take()
                .ok_or_else(|| "The removal preview was already used; preview again.".to_owned())?
        };
        if prepared.created_at.elapsed() > Duration::from_secs(300) {
            return Err("The removal preview expired; preview again.".to_owned());
        }
        // Check live policy without silently replacing the already displayed impact.
        // 检查实时策略，不静默替换已经显示的影响。
        let project_id = prepared.summary.impact.query.vmm_project_id;
        self.ensure_project(project_id).await?;
        // Only the two rules used by this action participate in confirmation equality.
        // 只有本操作使用的两项规则参与确认一致性判断。
        let rules = self.runtime.project_retention_summary(project_id).await?;
        if rules.effective.turn_hot_window_size != prepared.summary.impact.hot_window_size
            || rules.effective.trash_retention_ms != prepared.summary.trash_retention_ms
        {
            return Err("The removal retention rules changed; preview again.".to_owned());
        }
        // Retention starts when records are moved, not while the user reads the dialog.
        // 保留期限从记录移动时开始，而非用户阅读对话框时开始。
        let now_ns = native_vmm_now_ns()?;
        self.runtime
            .transform_storage_snapshot(
                operation_id,
                &kind,
                prepared.summary.impact.storage_version,
                |snapshot| apply_manual_removal(snapshot, prepared.summary, now_ns),
            )
            .await
    }
}

/// Moves original records selected by exact source relations; independent profiles never move.
/// 按准确来源关系移动原记录；独立画像绝不移动。
/// Takes the transaction snapshot, frozen preview and actual time; validates all inputs before mutation.
/// 接收事务快照、冻结预览及实际时间；在变更前校验全部输入。
pub(super) fn apply_manual_removal(
    snapshot: &mut StorageSnapshot,
    preview: VmmNativeRemovalPreview,
    now_ns: i64,
) -> Result<VmmNativeRemovalResult, String> {
    // Recompute exact membership; a matching count alone cannot authorize different records.
    // 重新计算准确成员；仅数量相同不能授权不同记录。
    let observed = removal_impact::build_impact(
        snapshot,
        preview.impact.query.clone(),
        preview.impact.hot_window_size,
    )?;
    if observed != preview.impact {
        return Err("The source of the selected memories changed; preview again.".to_owned());
    }
    // Check both time bounds and the next storage version before moving any original record.
    // 移动任一原记录之前检查时间边界及下一存储版本。
    let expires_at_ns = now_ns
        .checked_add(retention_duration_ns(preview.trash_retention_ms)?)
        .filter(|expires| now_ns > 0 && *expires > now_ns)
        .ok_or_else(|| "The recycle batch time or retention period is invalid.".to_owned())?;
    // The transaction owner performs the actual version increment.
    // 事务归属方执行实际版本递增。
    let storage_version = snapshot
        .storage_version
        .checked_add(1)
        .ok_or_else(|| "The native memory storage versions are exhausted.".to_owned())?;
    // The batch identity is derived only from the server-generated confirmation token.
    // 批次身份只从服务端生成的确认令牌派生。
    let batch_id = format!("manual:{}", preview.preview_id);
    if snapshot
        .retention_trash_batches
        .iter()
        .any(|batch| batch.batch_id == batch_id)
    {
        return Err("The recycle batch already exists; refresh.".to_owned());
    }
    // Every selected record was resolved under these exact native owner identities.
    // 每条所选记录均在这些准确原生归属身份下解析。
    let scope = project_scope(preview.impact.query.vmm_project_id)?;
    // Native personal identity remains fixed while memory projects are independent.
    // 原生个人身份保持固定，而记忆项目相互独立。
    let subject = native_vmm_subject_id()?;
    // The target variant, not inferred record counts, owns the lifecycle transition.
    // 目标变体而非推测记录数量决定生命周期迁移。
    let whole_sessions = matches!(
        &preview.impact.query.selection,
        VmmNativeRemovalTarget::Sessions { .. }
    );
    // Exact sets avoid name matching and keep source-based selection independent of retention filters.
    // 准确集合避免名称匹配，并使来源选择独立于周期保留过滤器。
    let session_ids = preview
        .impact
        .session_source_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    // Parse every canonical turn before the first mutation.
    // 首次变更前解析全部规范回合。
    let turn_ids = preview
        .impact
        .turn_ids
        .iter()
        .map(|id| TurnId::new(id.clone()).map_err(|error| error.to_string()))
        .collect::<Result<BTreeSet<_>, _>>()?;
    // Preserve ordinary memory identity, vectors and state across the move.
    // 在移动中保留普通记忆身份、向量及状态。
    let memory_ids = preview
        .impact
        .memory_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    // A whole-session batch retains the original state and extraction windows even when empty.
    // 即使为空，完整会话批次仍保留原始状态及提取水位。
    let selection = if whole_sessions {
        StorageManualRecycleSelection::Sessions {
            sessions: snapshot
                .memory_sessions
                .iter()
                .filter(|session| {
                    session.scope_id == scope
                        && session.subject_id == subject
                        && session_ids.contains(session.session_source_id.as_str())
                })
                .cloned()
                .collect(),
        }
    } else {
        StorageManualRecycleSelection::Turns {
            turn_ids: turn_ids.iter().cloned().collect(),
        }
    };
    // The manual reason must agree with the saved selection for storage validation.
    // 人工原因必须与保存选择一致，以通过存储校验。
    let reason = if whole_sessions {
        RetentionReasonCode::ManualSessionRecycle
    } else {
        RetentionReasonCode::ManualTurnRecycle
    };
    // Reuse the established move helpers; never copy and regenerate IDs or embeddings.
    // 复用既有移动辅助函数；绝不复制后重新生成 ID 或嵌入。
    let mut batch = retention_trash_batch(
        batch_id.clone(),
        subject.clone(),
        scope.clone(),
        now_ns,
        expires_at_ns,
        reason,
    );
    batch.manual_selection = Some(selection);
    move_memories_to_retention_batch(snapshot, &memory_ids, &mut batch);
    move_turns_to_retention_batch(snapshot, &turn_ids, &mut batch);
    if whole_sessions {
        for session in &mut snapshot.memory_sessions {
            if session.subject_id == subject
                && session.scope_id == scope
                && session_ids.contains(session.session_source_id.as_str())
            {
                session.status = StorageSessionStatus::Recycled;
                session.last_extraction_observed_at_ns = 0;
                session.last_extraction_completed_at_ns = None;
            }
        }
        // Original compaction belongs to the whole session; a turn-only selection cannot own it.
        // 原压缩信息属于完整会话；仅回合选择不能拥有它。
        let mut retained_boundaries = Vec::new();
        for boundary in std::mem::take(&mut snapshot.dataset.chat_compact_boundaries) {
            if boundary.subject_id == subject
                && boundary.scope_id == scope
                && session_ids.contains(boundary.session_source_id.as_str())
            {
                batch.dataset.chat_compact_boundaries.push(boundary);
            } else {
                retained_boundaries.push(boundary);
            }
        }
        snapshot.dataset.chat_compact_boundaries = retained_boundaries;
        snapshot.retention_cold_turn_jobs.retain(|job| {
            !(job.subject_id == subject
                && job.scope_id == scope
                && session_ids.contains(job.session_source_id.as_str()))
        });
    }
    push_retention_batch(snapshot, batch);
    Ok(VmmNativeRemovalResult {
        storage_version,
        batch_id,
        impact: preview.impact,
    })
}
