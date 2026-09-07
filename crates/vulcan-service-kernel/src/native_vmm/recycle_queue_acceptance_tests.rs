//! Real storage acceptance for suspended analysis and exact continuation after restore.
//! 已暂停提取及恢复后准确继续的真实存储验收。

use super::*;

/// Verifies formal purge removes only jobs whose original turn leaves both active and trash storage.
/// 验证正式清除只移除原回合已离开活动与回收存储的作业。
/// Takes an isolated real client, AI capture and bound project; completes the unselected control job.
/// 接收隔离真实客户端、AI 捕获及已绑定项目；最后完成未选中的对照作业。
pub(super) async fn verify_permanent_removal(
    client: &NativeVmmClient,
    port: &CapturingAnalysisPort,
    project_id: u64,
) {
    // Two real intakes provide an exact removal target and an independently retained queue row.
    // 两次真实登记提供准确移除目标及独立保留队列行。
    let mut jobs = Vec::new();
    for operation in ["queue-purge-selected", "queue-purge-retained"] {
        client
            .post_action(PostActionRequest {
                host_project_id: "queue-host-a".to_owned(),
                project_id,
                user_id: 1,
                session_id: operation.to_owned(),
                operation_id: operation.to_owned(),
                user_content: "项目使用 PostgreSQL 保存业务数据。".to_owned(),
                assistant_content: "收到项目配置。".to_owned(),
                ..Default::default()
            })
            .await
            .expect("real pending intake");
        jobs.push(
            client
                .data_plane
                .load_pending_analysis_job(operation)
                .await
                .expect("load queue row")
                .expect("durable pending job"),
        );
    }
    // Capture the whole authoritative state before the fixture-only selection move.
    // 在仅用于夹具的选择移动之前捕获完整权威状态。
    let before = client.runtime.storage_snapshot().expect("before recycle");
    // This ID comes from the committed intake row and identifies the sole selected turn.
    // 此 ID 来自已提交登记行，标识唯一所选回合。
    let turn_id = jobs[0].turn_id.as_deref().expect("selected turn");
    // The fixture's lifetime is bounded using the same host business clock as maintenance.
    // 夹具期限使用与维护相同的宿主业务时钟限定。
    let recycled_at_ns = now_ns().expect("clock");
    client
        .runtime
        .transform_storage_snapshot(
            "queue-purge-fixture",
            "fixture-only-manual-turn-recycle",
            before.storage_version,
            |snapshot| {
                // Move the original record, never reconstruct its contents from the queue payload.
                // 移动原始记录，绝不从队列载荷重建内容。
                let index = snapshot
                    .dataset
                    .turns
                    .iter()
                    .position(|turn| turn.id.as_str() == turn_id)
                    .expect("selected original turn");
                // Preserve every field and status in the recoverable dataset.
                // 在可恢复数据集中保留全部字段及状态。
                let turn = snapshot.dataset.turns.remove(index);
                snapshot.retention_trash_batches.push(
                    vulcan_memorymesh_core::StorageRetentionTrashBatch {
                        batch_id: "queue-permanent-removal".to_owned(),
                        subject_id: turn.subject_id.clone(),
                        scope_id: turn.scope_id.clone(),
                        recycled_at_ns,
                        expires_at_ns: recycled_at_ns + 86_400_000_000_000,
                        reason: vulcan_memorymesh_core::RetentionReasonCode::ManualTurnRecycle,
                        manual_selection: Some(
                            vulcan_memorymesh_core::StorageManualRecycleSelection::Turns {
                                turn_ids: vec![turn.id.clone()],
                            },
                        ),
                        dataset: vulcan_memorymesh_core::ExportDataset {
                            turns: vec![turn],
                            ..vulcan_memorymesh_core::ExportDataset::empty()
                        },
                        analysis_attempts: Vec::new(),
                    },
                );
                Ok(())
            },
        )
        .await
        .expect("move pending turn to recoverable storage");
    for job in &jobs {
        assert_eq!(
            client
                .data_plane
                .load_pending_analysis_job(&job.operation_id)
                .await
                .expect("queue after recycle")
                .as_ref(),
            Some(job),
            "recycle is not permanent removal"
        );
    }
    // Readiness and vectors are not needed to discard an exact retained batch.
    // 丢弃准确保留批次不需要模型就绪或向量调用。
    let maintenance = crate::native_vmm::NativeVmmMaintenance::new(
        client.runtime.clone(),
        client.data_plane.clone(),
        client.operation_gate.clone(),
    );
    // Capture both AI lanes independently around the formal destructive operation.
    // 分别捕获正式破坏性操作前后的两条 AI 调用通道。
    let llm_count = port.requests.lock().expect("LLM capture").len();
    // Embedding absence is asserted separately from the LLM counter.
    // 向量调用缺失独立于 LLM 计数断言。
    let embedding_count = port
        .embedding_requests
        .lock()
        .expect("embedding capture")
        .len();
    // The exact committed version binds both the first operation and its idempotent replay.
    // 准确提交版本同时绑定首次操作及其幂等重放。
    let version = client
        .runtime
        .storage_snapshot()
        .expect("recycled snapshot")
        .storage_version;
    maintenance
        .purge_recycle_batch("queue-formal-purge", "queue-permanent-removal", version)
        .await
        .expect("formal purge with atomic queue removal");
    assert!(
        client
            .data_plane
            .load_pending_analysis_job(&jobs[0].operation_id)
            .await
            .expect("removed job lookup")
            .is_none()
    );
    assert_eq!(
        client
            .data_plane
            .load_pending_analysis_job(&jobs[1].operation_id)
            .await
            .expect("retained job lookup")
            .as_ref(),
        Some(&jobs[1])
    );
    // The relational snapshot and queue loss are both visible after the same committed operation.
    // 相同提交操作之后，关系快照和队列移除同时可见。
    let purged = client.runtime.storage_snapshot().expect("purged snapshot");
    // Simulate a worker that selected the deleted row before the maintenance write lock won.
    // 模拟工作器在维护写锁先完成之前已经选中了被删除行。
    client
        .execute_pending_analysis_job(&jobs[0])
        .await
        .expect("stale execution stops");
    client
        .reschedule_pending_analysis_job(&jobs[0], "stale purge failure")
        .await
        .expect("deleted job is not rescheduled");
    assert_eq!(
        client
            .finalize_failed_pending_analysis_job(
                &jobs[0],
                TurnStatus::Corrupted,
                "stale purge failure"
            )
            .await
            .expect("deleted job is not terminalized"),
        None
    );
    assert_eq!(
        client
            .runtime
            .storage_snapshot()
            .expect("stale operation snapshot"),
        purged
    );
    assert!(
        !purged
            .dataset
            .turns
            .iter()
            .any(|turn| turn.id.as_str() == turn_id)
    );
    assert!(purged.retention_trash_batches.is_empty());
    maintenance
        .purge_recycle_batch("queue-formal-purge", "queue-permanent-removal", version)
        .await
        .expect("purge replay");
    assert_eq!(
        client
            .runtime
            .storage_snapshot()
            .expect("replayed snapshot"),
        purged
    );
    assert_eq!(port.requests.lock().expect("LLM capture").len(), llm_count);
    assert_eq!(
        port.embedding_requests
            .lock()
            .expect("embedding capture")
            .len(),
        embedding_count
    );
    assert!(
        client
            .drain_due_pending_analysis_jobs()
            .await
            .expect("remaining live job")
    );
    assert_eq!(
        client
            .data_plane
            .load_pending_analysis_job(&jobs[1].operation_id)
            .await
            .expect("completed remaining job")
            .expect("remaining audit row")
            .state,
        vulcan_workspace_store::vmm_data_plane::VMM_PENDING_ANALYSIS_COMPLETED
    );
    assert!(
        !client
            .drain_due_pending_analysis_jobs()
            .await
            .expect("no orphan loop")
    );
}

/// Exercises production queue entry points against recoverable pending turns, then restores them.
/// 使用可恢复待处理回合验收生产队列入口，随后恢复原回合。
/// Takes the isolated client, captured AI port and its exact pending jobs; leaves all jobs ready.
/// 接收隔离客户端、AI 捕获端口及其准确待处理作业；结束时全部作业保持就绪。
pub(super) async fn verify(
    client: &NativeVmmClient,
    port: &CapturingAnalysisPort,
    jobs: &[NativeVmmPendingAnalysisJob],
) {
    // The fixture moves selected pending turns only; it does not pretend manual recycle is exposed.
    // 夹具只移动选中待处理回合，不冒充人工回收入口已经开放。
    let before = client.runtime.storage_snapshot().expect("before recycle");
    // Canonical IDs come from real committed intake jobs, never guessed source labels.
    // 规范 ID 来自实际已提交登记作业，绝不猜测来源标签。
    let ids = jobs
        .iter()
        .map(|job| job.turn_id.as_deref().expect("committed turn identity"))
        .collect::<std::collections::BTreeSet<_>>();
    // Freeze AI counters to prove execution rechecks occur before provider route resolution.
    // 冻结 AI 计数，证明执行复查发生在供应商路由解析之前。
    let llm_count = port.requests.lock().expect("LLM capture").len();
    // Embedding calls are independently observed rather than inferred from LLM absence.
    // 单独观测向量调用，不以没有 LLM 调用推断。
    let embedding_count = port
        .embedding_requests
        .lock()
        .expect("embedding capture")
        .len();
    // Retain the first exact owner as a batch invariant after verifying every selected turn below.
    // 保留首个准确归属作为批次不变量，下面校验全部所选回合。
    let first = before
        .dataset
        .turns
        .iter()
        .find(|turn| ids.contains(turn.id.as_str()))
        .expect("selected turn");
    // Use a bounded future deadline from the production clock, not a timeless trash fixture.
    // 使用生产时钟计算有界未来期限，不构造永不过期回收夹具。
    let recycled_at_ns = now_ns().expect("clock");
    client
        .runtime
        .transform_storage_snapshot(
            "queue-recycle-fixture",
            "fixture-only-pending-recycle",
            before.storage_version,
            |snapshot| {
                // Move, do not duplicate, the original pending records into one same-owner batch.
                // 将原始待处理记录移动到同归属批次，不能复制。
                let mut dataset = vulcan_memorymesh_core::ExportDataset::empty();
                snapshot.dataset.turns.retain(|turn| {
                    if ids.contains(turn.id.as_str()) {
                        assert_eq!(turn.status, TurnStatus::Pending);
                        assert_eq!(turn.subject_id, first.subject_id);
                        assert_eq!(turn.scope_id, first.scope_id);
                        dataset.turns.push(turn.clone());
                        false
                    } else {
                        true
                    }
                });
                assert_eq!(dataset.turns.len(), ids.len());
                snapshot.retention_trash_batches.push(
                    vulcan_memorymesh_core::StorageRetentionTrashBatch {
                        batch_id: "queue-recycle-acceptance".to_owned(),
                        subject_id: first.subject_id.clone(),
                        scope_id: first.scope_id.clone(),
                        recycled_at_ns,
                        expires_at_ns: recycled_at_ns + 86_400_000_000_000,
                        reason: vulcan_memorymesh_core::RetentionReasonCode::ColdTurnRecycle,
                        manual_selection: None,
                        dataset,
                        analysis_attempts: Vec::new(),
                    },
                );
                Ok(())
            },
        )
        .await
        .expect("commit fixture recycle");
    // The normal scheduler must neither report runnable work nor consume a failure attempt.
    // 正常调度器既不能报告可运行作业，也不能消耗失败次数。
    let recycled = client
        .runtime
        .storage_snapshot()
        .expect("recycled snapshot");
    assert!(
        !client
            .drain_due_pending_analysis_jobs()
            .await
            .expect("paused drain")
    );
    for job in jobs {
        // Direct execution simulates a job selected before a concurrent recycle acquired the gate.
        // 直接执行模拟作业在并发回收取得维护门之前已被选中。
        client
            .execute_pending_analysis_job(job)
            .await
            .expect("execution recheck");
        client
            .reschedule_pending_analysis_job(job, "stale failure before recycle")
            .await
            .expect("reschedule recheck");
        for terminal in [TurnStatus::AnalysisBypassed, TurnStatus::Corrupted] {
            assert!(
                client
                    .finalize_failed_pending_analysis_job(job, terminal, "stale terminal failure")
                    .await
                    .expect("terminalization recheck")
                    .is_none()
            );
        }
        assert_eq!(
            client
                .data_plane
                .load_pending_analysis_job(&job.operation_id)
                .await
                .expect("durable suspended job")
                .as_ref(),
            Some(job)
        );
    }
    assert_eq!(
        client
            .runtime
            .storage_snapshot()
            .expect("unchanged suspended storage"),
        recycled
    );
    assert_eq!(port.requests.lock().expect("LLM capture").len(), llm_count);
    assert_eq!(
        port.embedding_requests
            .lock()
            .expect("embedding capture")
            .len(),
        embedding_count
    );
    // A forged owner is still rejected by execution; a recycle identity is not an authorization bypass.
    // 执行仍拒绝伪造归属；回收身份不是绕过授权的条件。
    let mut wrong_owner = jobs[0].clone();
    // Parse the sole authoritative job schema rather than editing arbitrary JSON keys.
    // 解析唯一权威作业结构，不编辑任意 JSON 键。
    let mut payload: NativePendingPostActionPayload =
        serde_json::from_str(&wrong_owner.payload_json).expect("typed pending payload");
    payload.intake.subject_id = SubjectId::new("another-subject").expect("other subject");
    payload.analysis.subject_id = payload.intake.subject_id.clone();
    wrong_owner.payload_json = serde_json::to_string(&payload).expect("encode typed payload");
    assert!(matches!(
        client.execute_pending_analysis_job(&wrong_owner).await,
        Err(PendingAnalysisJobError::Corrupted(_))
    ));
    // Restore uses the production maintenance transaction and the same client operation gate.
    // 恢复使用生产维护事务及同一个客户端操作门。
    let maintenance = crate::native_vmm::NativeVmmMaintenance::new(
        client.runtime.clone(),
        client.data_plane.clone(),
        client.operation_gate.clone(),
    );
    maintenance.set_native_backend_enabled(true);
    maintenance
        .restore_recycle_batch(
            "queue-restore-acceptance",
            "queue-recycle-acceptance",
            recycled.storage_version,
        )
        .await
        .expect("production pending-turn restore");
    // Snapshot identity and session watermarks must return exactly; only commit versions advance.
    // 快照身份及会话水位必须准确恢复，只有提交版本递增。
    let mut expected = before;
    expected.storage_version += 2;
    assert_eq!(
        client
            .runtime
            .storage_snapshot()
            .expect("restored snapshot"),
        expected
    );
    for job in jobs {
        assert_eq!(
            client
                .data_plane
                .load_pending_analysis_job(&job.operation_id)
                .await
                .expect("restored pending job")
                .as_ref(),
            Some(job)
        );
    }
    assert_eq!(port.requests.lock().expect("LLM capture").len(), llm_count);
    assert_eq!(
        port.embedding_requests
            .lock()
            .expect("embedding capture")
            .len(),
        embedding_count
    );
    // The caller now runs its existing five-job history assertions through the real scheduler.
    // 调用方随后通过真实调度器继续执行已有五作业历史断言。
}
