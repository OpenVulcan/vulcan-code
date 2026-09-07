//! Bounded inspection of the exact retained dataset in one native recycle batch.
//! 对一个原生回收批次准确保留数据集进行有界查看。
use super::*;
use vulcan_service_api::{
    VmmNativeRecycleRecord, VmmNativeRecycleRecordKind, VmmNativeRecycleRecordPage,
    VmmNativeRecycleRecordQuery, VmmNativeTimelineItemSummary,
};

impl NativeVmmMaintenance {
    /// Reads a version-bound page without restoring data, invoking AI or exposing embedding arrays.
    /// 读取绑定版本的一页，不恢复数据、不调用 AI 或暴露嵌入数组。
    /// Takes exact project/batch and paging coordinates; returns only records retained in that batch.
    /// 接收准确项目、批次及分页位置，只返回该批次内保留的记录。
    pub(crate) async fn recycle_records(
        &self,
        query: VmmNativeRecycleRecordQuery,
    ) -> Result<VmmNativeRecycleRecordPage, String> {
        let _project_guard = self.project_mutation_gate.lock().await;
        let _operation_guard = self.operation_gate.read().await;
        self.ensure_project(query.vmm_project_id).await?;
        // The snapshot is the sole authority; active records and other batches never supply missing data.
        // 快照是唯一权威，活动记录及其他批次绝不补入缺失数据。
        let snapshot = self.runtime.storage_snapshot()?;
        build_page(&snapshot, query)
    }
}

/// Projects one exact batch page after validating ownership, bounds and the observed version.
/// 校验归属、边界及观察版本后投影一个准确批次页。
/// Takes a coherent snapshot and closed query; missing or moved batches produce an explicit error.
/// 接收一致快照及封闭查询，缺失或已移动批次产生明确错误。
pub(super) fn build_page(
    snapshot: &StorageSnapshot,
    query: VmmNativeRecycleRecordQuery,
) -> Result<VmmNativeRecycleRecordPage, String> {
    if query.limit == 0
        || query.limit > 100
        || query.batch_id.is_empty()
        || query.batch_id.trim() != query.batch_id
    {
        return Err("回收明细需要准确批次 ID，页大小应为 1 至 100。".to_owned());
    }
    if query
        .expected_storage_version
        .is_some_and(|version| version != snapshot.storage_version)
    {
        return Err("回收站内容已变化，请刷新明细后重新翻页。".to_owned());
    }
    let scope = project_scope(query.vmm_project_id)?;
    let subject = native_vmm_subject_id()?;
    let subject_scope = native_vmm_subject_scope()?;
    let batch = snapshot
        .retention_trash_batches
        .iter()
        .find(|batch| {
            batch.batch_id == query.batch_id
                && batch.scope_id == scope
                && batch.subject_id == subject
        })
        .ok_or_else(|| "该项目中已不存在此回收批次，请刷新回收站。".to_owned())?;
    let offset =
        usize::try_from(query.offset).map_err(|_| "回收明细偏移超过平台范围。".to_owned())?;
    let limit = query.limit as usize;
    let (total, records) = match query.record_kind {
        VmmNativeRecycleRecordKind::Turn => select_page(
            &batch.dataset.turns,
            offset,
            limit,
            |left, right| left.id.cmp(&right.id),
            |turn| {
                Ok(VmmNativeRecycleRecord::Turn {
                    turn_id: turn.id.to_string(),
                    session_source_id: turn.session_source_id.to_string(),
                    timeline: turn
                        .timeline
                        .iter()
                        .map(|item| VmmNativeTimelineItemSummary {
                            role: timeline_role_token(item.role).to_owned(),
                            content: item.content.clone(),
                            occurred_at_unix_ms: unix_ns_to_ms(item.occurred_at_ns),
                        })
                        .collect(),
                    status: turn_status_token(turn.status).to_owned(),
                    analysis_details: turn.analysis_details.clone(),
                })
            },
        )?,
        VmmNativeRecycleRecordKind::Memory => select_page(
            &batch.dataset.memories,
            offset,
            limit,
            |left, right| left.id.cmp(&right.id),
            |memory| {
                Ok(VmmNativeRecycleRecord::Memory {
                    memory_id: memory.id.to_string(),
                    session_source_id: memory
                        .origin_session_source_id
                        .as_ref()
                        .map(ToString::to_string),
                    source_turn_id: memory.source_turn_id.as_ref().map(ToString::to_string),
                    summary: memory.summary.clone(),
                    content: memory.content.clone(),
                    level: memory_level_token(memory.memory_level).to_owned(),
                    state: memory_state_token(memory.state).to_owned(),
                    source_kind: memory_source_kind_token(memory.source_kind).to_owned(),
                })
            },
        )?,
        VmmNativeRecycleRecordKind::Profile => select_page(
            &batch.dataset.profile_nodes,
            offset,
            limit,
            |left, right| left.id.cmp(&right.id),
            |profile| {
                // Personal and project profiles are separate declared scopes; no other scope is relabeled.
                // 个人及项目画像是分别声明的作用域，其他作用域不能被改标签。
                let target = if profile.scope_id == scope {
                    "project"
                } else if profile.scope_id == subject_scope {
                    "user"
                } else {
                    return Err("回收画像归属与批次不一致。".to_owned());
                };
                Ok(VmmNativeRecycleRecord::Profile {
                    profile_node_id: profile.id.to_string(),
                    target: target.to_owned(),
                    source_turn_ids: profile
                        .source_turn_ids
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                    content: profile.content.clone(),
                })
            },
        )?,
        VmmNativeRecycleRecordKind::Evidence => select_page(
            &batch.dataset.context_evidence,
            offset,
            limit,
            |left, right| {
                (&left.memory_id, &left.context_key, &left.context_value).cmp(&(
                    &right.memory_id,
                    &right.context_key,
                    &right.context_value,
                ))
            },
            |evidence| {
                Ok(VmmNativeRecycleRecord::Evidence {
                    memory_id: evidence.memory_id.to_string(),
                    context_key: evidence.context_key.clone(),
                    context_value: evidence.context_value.clone(),
                    support_count: evidence.support_count,
                    rebuttal_count: evidence.rebuttal_count,
                })
            },
        )?,
    };
    // Offset overflow cannot wrap into a different page or silently skip records.
    // 偏移溢出不能回绕到另一页或静默跳过记录。
    let end = offset
        .checked_add(records.len())
        .ok_or_else(|| "回收明细分页范围溢出。".to_owned())?;
    let next_offset = if end < total {
        Some(u32::try_from(end).map_err(|_| "回收明细超过可分页范围。".to_owned())?)
    } else {
        None
    };
    Ok(VmmNativeRecycleRecordPage {
        storage_version: snapshot.storage_version,
        total: usize_to_u64(total),
        next_offset,
        query,
        records,
    })
}

/// Orders lightweight references and materializes only the selected page's business fields.
/// 排序轻量引用，仅构造所选页的业务字段。
/// Takes typed retained rows, bounds, stable comparator and projector; returns total and bounded records.
/// 接收类型化保留行、边界、稳定比较器及投影器，返回总数和有界记录。
fn select_page<T>(
    rows: &[T],
    offset: usize,
    limit: usize,
    compare: impl Fn(&T, &T) -> std::cmp::Ordering,
    project: impl Fn(&T) -> Result<VmmNativeRecycleRecord, String>,
) -> Result<(usize, Vec<VmmNativeRecycleRecord>), String> {
    let mut ordered = rows.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| compare(left, right));
    let records = ordered
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(project)
        .collect::<Result<Vec<_>, _>>()?;
    Ok((rows.len(), records))
}
