//! Selects due analysis work without overtaking an earlier pending turn in the same session.
//! 选择到期分析作业，禁止越过同一会话中较早的待处理回合。

use std::collections::{BTreeMap, BTreeSet};
use vulcan_memorymesh_core::{
    ScopeId, SessionSourceId, StorageRetentionTrashBatch, SubjectId, Turn, TurnStatus,
};
use vulcan_workspace_store::vmm_data_plane::{
    NativeVmmPendingAnalysisJob, VMM_PENDING_ANALYSIS_READY,
};

use super::client::parse_core_numeric_id;

/// Chooses at most `limit` due jobs using persisted turn ownership and numeric intake order.
/// 根据已保存回合的归属及数字登记顺序，选择最多 `limit` 条到期作业。
/// `jobs` contains all active jobs in database enqueue order; `turns` is the active Core snapshot.
/// `jobs` 包含按数据库入队顺序排列的全部活动作业；`turns` 是 Core 活跃快照。
/// `recycled_batches` suspends jobs whose exact turn is recoverable, without consuming attempts.
/// `recycled_batches` 暂停准确回合仍可恢复的作业，不消耗尝试次数。
/// Returns one pending head per subject/project/session, or an explicit invalid-native-ID error.
/// 每个主体/项目/会话仅返回一个待处理队首；原生 ID 无效时返回明确错误。
pub(super) fn select_ordered_due_analysis_jobs(
    jobs: Vec<NativeVmmPendingAnalysisJob>,
    turns: &[Turn],
    recycled_batches: &[StorageRetentionTrashBatch],
    now_unix_ms: i64,
    limit: usize,
) -> Result<Vec<NativeVmmPendingAnalysisJob>, String> {
    // heads includes not-yet-due turns, so a retry delay never grants its successor permission.
    // heads 包含尚未到期的回合，因此重试延迟不会给后继回合放行。
    let mut heads = BTreeMap::<(&SubjectId, &ScopeId, &SessionSourceId), (u64, &str)>::new();
    // pending_ids distinguishes ordered pending work from acknowledgement or corruption handling.
    // pending_ids 区分需要顺序处理的待提取作业与确认或损坏处理。
    let mut pending_ids = BTreeSet::new();
    for turn in turns
        .iter()
        .filter(|turn| turn.status == TurnStatus::Pending)
    {
        // sequence comes from the native allocator, never timestamps or the job's display title.
        // sequence 来自原生分配器，绝不采用时间戳或作业显示标题。
        let sequence =
            parse_core_numeric_id(&turn.id, "vmm-turn-").map_err(|error| error.to_string())?;
        pending_ids.insert(turn.id.as_str());
        // owner is the complete memory-session identity, including project and fixed subject.
        // owner 是包含项目及固定主体的完整记忆会话身份。
        let owner = (&turn.subject_id, &turn.scope_id, &turn.session_source_id);
        heads
            .entry(owner)
            .and_modify(|head| {
                if sequence < head.0 {
                    *head = (sequence, turn.id.as_str());
                }
            })
            .or_insert((sequence, turn.id.as_str()));
    }
    // eligible_ids is independent of queue-page size, preventing blocked sessions hiding others.
    // eligible_ids 不依赖队列分页大小，避免被阻塞会话遮挡其他会话。
    let eligible_ids = heads.values().map(|(_, id)| *id).collect::<BTreeSet<_>>();
    // Recycled identities remain reserved by storage and must not enter missing-turn diagnosis.
    // 回收身份仍由存储保留，不能进入缺失回合诊断。
    let recycled_ids = recycled_batches
        .iter()
        .flat_map(|batch| &batch.dataset.turns)
        .map(|turn| turn.id.as_str())
        .collect::<BTreeSet<_>>();
    Ok(jobs
        .into_iter()
        .filter(|job| {
            if job.state != VMM_PENDING_ANALYSIS_READY || job.next_attempt_at_unix_ms > now_unix_ms
            {
                return false;
            }
            match job.turn_id.as_deref() {
                Some(id) if recycled_ids.contains(id) => false,
                Some(id) if pending_ids.contains(id) => eligible_ids.contains(id),
                // Non-pending or missing turns must reach the existing explicit validation path.
                // 非待处理或缺失回合必须进入现有显式校验路径，不能静默丢弃作业。
                _ => true,
            }
        })
        .take(limit)
        .collect())
}

/// Finds an exact reserved turn in recoverable storage for a lock-protected execution recheck.
/// 在可恢复存储中查找准确保留回合，供持锁执行复查。
/// Takes current batches and the canonical job turn ID; returns only that original turn, or none.
/// 接收当前批次及作业规范回合 ID；只返回对应原始回合，不存在时返回空。
pub(super) fn recycled_analysis_turn<'a>(
    batches: &'a [StorageRetentionTrashBatch],
    turn_id: &str,
) -> Option<&'a Turn> {
    batches
        .iter()
        .flat_map(|batch| &batch.dataset.turns)
        .find(|turn| turn.id.as_str() == turn_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_memorymesh_core::TurnId;
    use vulcan_workspace_store::vmm_data_plane::VMM_PENDING_ANALYSIS_INTAKING;

    /// Creates a pending turn whose timestamp intentionally cannot define its intake order.
    /// 创建时间戳故意无法确定登记顺序的待处理回合。
    fn turn(id: u64, subject: &str, project: &str, session: &str) -> Turn {
        Turn {
            id: TurnId::new(format!("vmm-turn-{id}")).expect("turn id"),
            subject_id: SubjectId::new(subject).expect("subject"),
            scope_id: ScopeId::new(project).expect("project"),
            session_source_id: SessionSourceId::new(session).expect("session"),
            timeline: Vec::new(),
            previous_turn_ids: Vec::new(),
            next_turn_ids: Vec::new(),
            created_at_ns: 1,
            status: TurnStatus::Pending,
            user_input_kind: None,
            analysis_details: None,
        }
    }

    /// Creates a ready job with an explicit retry deadline; semantic payload is not inspected here.
    /// 创建具有明确重试期限的就绪作业；本模块不读取语义载荷。
    fn job(id: u64, due: i64) -> NativeVmmPendingAnalysisJob {
        NativeVmmPendingAnalysisJob {
            operation_id: format!("job-{id}"),
            request_fingerprint: format!("fingerprint-{id}"),
            payload_json: "{}".to_owned(),
            intake_mutation_operation_id: format!("intake-{id}"),
            turn_id: Some(format!("vmm-turn-{id}")),
            state: VMM_PENDING_ANALYSIS_READY.to_owned(),
            attempt_count: 0,
            next_attempt_at_unix_ms: due,
            last_error: None,
        }
    }

    /// Checks that numeric turn 2 blocks turn 10 even while its retry is not due.
    /// 验证数字回合 2 即使尚未到重试时间，也会阻止回合 10 越过执行。
    #[test]
    fn retrying_head_blocks_only_its_own_session() {
        // turns includes same-name sessions under independent subjects and projects.
        // turns 包含不同主体和项目下的同名会话。
        let turns = vec![
            turn(10, "u1", "p1", "s1"),
            turn(2, "u1", "p1", "s1"),
            turn(3, "u1", "p2", "s1"),
            turn(4, "u2", "p1", "s1"),
            turn(5, "u1", "p1", "s2"),
        ];
        // jobs deliberately places the blocked successor first, as an overdue-page query can.
        // jobs 故意将被阻塞的后继放在首位，模拟到期分页查询。
        let jobs = vec![job(10, 0), job(2, 200), job(3, 0), job(4, 0), job(5, 0)];
        // selected must contain all independent heads despite the blocked first queue entry.
        // selected 必须包含全部独立队首，不受首条被阻塞作业影响。
        let selected =
            select_ordered_due_analysis_jobs(jobs, &turns, &[], 100, 3).expect("selection");
        assert_eq!(
            selected
                .iter()
                .map(|job| job.operation_id.as_str())
                .collect::<Vec<_>>(),
            vec!["job-3", "job-4", "job-5"]
        );
    }

    /// Checks that a fresh pass releases the successor only after the earlier turn leaves pending.
    /// 验证仅在较早回合离开待处理状态后，新一轮扫描才放行其后继。
    #[test]
    fn each_pass_observes_the_current_pending_head() {
        // turns models authoritative status changes between worker passes.
        // turns 模拟工作器两次扫描间的权威状态变化。
        let mut turns = vec![turn(2, "u1", "p1", "s1"), turn(10, "u1", "p1", "s1")];
        // jobs intentionally uses lexical rather than numeric order.
        // jobs 故意采用字典序而非数字顺序。
        let jobs = vec![job(10, 0), job(2, 0)];
        // selected must return the smaller committed numeric ID first.
        // selected 必须先返回已提交的较小数字 ID。
        let selected = select_ordered_due_analysis_jobs(jobs, &turns, &[], 100, 128).expect("head");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].operation_id, "job-2");
        for terminal_status in [
            TurnStatus::Completed,
            TurnStatus::AnalysisBypassed,
            TurnStatus::Corrupted,
        ] {
            turns[0].status = terminal_status;
            // successor is released for successful extraction or explicit terminal failure alike.
            // successor 在提取成功或明确终态失败后均可放行。
            let successor =
                select_ordered_due_analysis_jobs(vec![job(10, 0)], &turns, &[], 100, 128)
                    .expect("successor");
            assert_eq!(successor[0].operation_id, "job-10");
        }
    }

    /// Checks that uncommitted intake is not analyzed and missing-turn jobs still get diagnosed.
    /// 验证尚未提交的登记不会被分析，而缺失回合作业仍能进入诊断流程。
    #[test]
    fn intake_and_corrupted_job_paths_remain_explicit() {
        // intake has no committed turn and must never be analyzed prematurely.
        // intake 尚无已提交回合，绝不能提前分析。
        let mut intake = job(1, 0);
        intake.state = VMM_PENDING_ANALYSIS_INTAKING.to_owned();
        intake.turn_id = None;
        // selected retains missing-turn work for the worker's existing explicit diagnostic path.
        // selected 保留缺失回合作业，使其进入工作器已有的显式诊断路径。
        let selected =
            select_ordered_due_analysis_jobs(vec![intake, job(9, 0)], &[], &[], 100, 128)
                .expect("missing-turn diagnosis");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].operation_id, "job-9");
        assert!(
            select_ordered_due_analysis_jobs(vec![job(9, 0)], &[], &[], 100, 0)
                .expect("zero limit")
                .is_empty()
        );
    }

    /// Checks that more than one execution page of blocked successors cannot starve another session.
    /// 验证超过一页执行量的被阻塞后继不能饿死另一个会话。
    #[test]
    fn blocked_successors_do_not_consume_the_execution_limit() {
        // turns and jobs place an independent session after 129 blocked successors.
        // turns 和 jobs 将一个独立会话放在 129 条被阻塞后继之后。
        let mut turns = (1..=130)
            .map(|id| turn(id, "u1", "p1", "s1"))
            .collect::<Vec<_>>();
        let mut jobs = (1..=130)
            .map(|id| job(id, if id == 1 { 200 } else { 0 }))
            .collect::<Vec<_>>();
        turns.push(turn(131, "u1", "p1", "s2"));
        jobs.push(job(131, 0));
        // selected must be the independent ready head, not an empty first database page.
        // selected 必须是独立就绪队首，而不是空的第一数据库分页。
        let selected =
            select_ordered_due_analysis_jobs(jobs, &turns, &[], 100, 128).expect("selection");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].operation_id, "job-131");
    }

    /// Verifies recyclable work is suspended without starving live jobs and resumes by native order.
    /// 验证可恢复作业暂停时不会饿死活动作业，并在恢复后按原生顺序继续。
    /// Uses exact turn identities across active and trash storage; missing IDs remain diagnosable.
    /// 使用活动及回收存储的准确回合身份；真正缺失的 ID 仍可诊断。
    #[test]
    fn recycled_jobs_pause_and_restore_without_consuming_the_page() {
        // The oldest same-session turn is absent from active storage but remains recoverable.
        // 同会话最早回合已离开活动存储，但仍可恢复。
        let recycled = turn(2, "u1", "p1", "s1");
        // The batch preserves the original pending state, not a fabricated failure or completion.
        // 批次保留原始待处理状态，不伪造失败或完成。
        let batches = vec![StorageRetentionTrashBatch {
            batch_id: "queue-test".to_owned(),
            subject_id: recycled.subject_id.clone(),
            scope_id: recycled.scope_id.clone(),
            recycled_at_ns: 1,
            expires_at_ns: 2,
            reason: vulcan_memorymesh_core::RetentionReasonCode::ColdTurnRecycle,
            manual_selection: None,
            dataset: vulcan_memorymesh_core::ExportDataset {
                turns: vec![recycled.clone()],
                ..vulcan_memorymesh_core::ExportDataset::empty()
            },
            analysis_attempts: Vec::new(),
        }];
        // Active work has independent eligibility; suspended jobs never occupy its execution page.
        // 活动作业独立判定资格；暂停作业不占用执行分页。
        let mut turns = vec![turn(10, "u1", "p1", "s1"), turn(20, "u1", "p2", "s1")];
        // Retry metadata must survive selection untouched, including an already recorded failure.
        // 选择过程必须完整保留重试元数据，包括已经记录的失败。
        let mut suspended = job(2, 0);
        suspended.attempt_count = 3;
        suspended.last_error = Some("previous failure".to_owned());
        // One page slot goes to live work, not the first suspended database entry.
        // 单个分页名额交给活动作业，而非数据库首条暂停记录。
        let selected = select_ordered_due_analysis_jobs(
            vec![suspended.clone(), job(10, 0), job(20, 0)],
            &turns,
            &batches,
            100,
            1,
        )
        .expect("live work passes recycled job");
        assert_eq!(selected[0].operation_id, "job-10");
        assert_eq!(
            recycled_analysis_turn(&batches, "vmm-turn-2"),
            Some(&recycled)
        );
        assert!(recycled_analysis_turn(&batches, "vmm-turn-20").is_none());
        assert!(
            select_ordered_due_analysis_jobs(vec![suspended.clone()], &turns, &batches, 100, 128)
                .expect("suspended only")
                .is_empty()
        );
        turns.push(recycled);
        // After restore, numeric turn 2 wins over 10 again and retains its original retry metadata.
        // 恢复后数字回合 2 再次先于 10，并保留原重试元数据。
        let resumed =
            select_ordered_due_analysis_jobs(vec![job(10, 0), suspended], &turns, &[], 100, 128)
                .expect("restored numeric head");
        assert_eq!(resumed.len(), 1);
        assert_eq!(resumed[0].operation_id, "job-2");
        assert_eq!(resumed[0].attempt_count, 3);
        assert_eq!(resumed[0].last_error.as_deref(), Some("previous failure"));
        // Removing the batch and turn is a genuinely missing identity, not an indefinite pause.
        // 批次与回合均移除才是真正缺失身份，不得无限暂停。
        assert_eq!(
            select_ordered_due_analysis_jobs(vec![job(2, 0)], &[], &[], 100, 128)
                .expect("missing turn diagnosis")
                .len(),
            1
        );
    }
}
