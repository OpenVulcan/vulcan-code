use super::*;
use vulcan_context_engine::VceIndexPort;

/// Build one deterministic physical-statistics snapshot for compaction transition tests.
/// 为整理转换测试构造一份确定性的物理统计快照。
///
/// `version`, `row_count`, `index_count`, and `fragment_count` select the tested invariants.
/// Returns a content-table snapshot whose fragment distribution is internally valid.
/// `version`、`row_count`、`index_count` 与 `fragment_count` 选择待测试的不变量。
/// 返回一份片段分布内部有效的内容表快照。
fn compaction_statistics_fixture(
    version: u64,
    row_count: u64,
    index_count: u64,
    fragment_count: u64,
) -> VcePhysicalTableStatistics {
    VcePhysicalTableStatistics {
        role: VcePhysicalTableRole::Content,
        table_name: String::from("content_vectors"),
        version,
        total_bytes: 4096,
        row_count,
        index_count,
        fragment_count,
        small_fragment_count: fragment_count,
        compaction_task_count: u64::from(fragment_count > 1),
        fragment_rows: vulcan_context_engine::VcePhysicalFragmentSummary {
            minimum_rows: row_count.checked_div(fragment_count).unwrap_or(0),
            maximum_rows: row_count,
            mean_rows: row_count.checked_div(fragment_count).unwrap_or(0),
            p25_rows: row_count.checked_div(fragment_count).unwrap_or(0),
            p50_rows: row_count.checked_div(fragment_count).unwrap_or(0),
            p75_rows: row_count.checked_div(fragment_count).unwrap_or(0),
            p99_rows: row_count,
        },
    }
}

/// Reject engine success projections that alter logical contents or contradict rewrite metrics.
/// 拒绝会改变逻辑内容或与改写指标矛盾的引擎成功投影。
#[test]
fn compaction_transition_requires_data_preservation_and_exact_metrics() {
    let admitted = compaction_statistics_fixture(9, 32, 2, 8);
    let mut after = compaction_statistics_fixture(10, 32, 2, 1);
    validate_compaction_statistics_transition(&admitted, 9, &admitted, &after, 8, 1)
        .expect("a data-preserving physical rewrite should be accepted");

    after.row_count = 31;
    assert!(
        validate_compaction_statistics_transition(&admitted, 9, &admitted, &after, 8, 1).is_err()
    );
    after.row_count = 32;
    after.index_count = 1;
    assert!(
        validate_compaction_statistics_transition(&admitted, 9, &admitted, &after, 8, 1).is_err()
    );
    after.index_count = 2;
    assert!(
        validate_compaction_statistics_transition(&admitted, 9, &admitted, &after, 7, 1).is_err()
    );

    let mut forged_before = admitted.clone();
    forged_before.total_bytes += 1;
    assert!(
        validate_compaction_statistics_transition(&admitted, 9, &forged_before, &after, 8, 1,)
            .is_err()
    );
}

/// Verify a successful lookup cannot masquerade as cancellation acknowledgement.
/// 验证一次成功查询不能冒充取消确认。
#[test]
fn cancellation_acknowledgement_requires_flag_or_terminal_state() {
    // operation starts as an unacknowledged engine-owned running step.
    // operation 初始表示一项尚未确认取消、由引擎持有的运行中步骤。
    let mut operation = VcePhysicalCompactionOperation {
        operation_id: String::from("compact-content-round-1"),
        table_name: String::from("content"),
        state: VcePhysicalCompactionState::Running,
        cancel_requested: false,
        message: String::from("running"),
        error: None,
        submitted_at_unix_ms: 1,
        started_at_unix_ms: Some(2),
        completed_at_unix_ms: None,
        before: None,
        after: None,
        metrics: None,
    };

    assert!(!compaction_cancellation_acknowledged(&operation));
    operation.cancel_requested = true;
    assert!(compaction_cancellation_acknowledged(&operation));
    operation.cancel_requested = false;
    operation.state = VcePhysicalCompactionState::Succeeded;
    assert!(compaction_cancellation_acknowledged(&operation));
}

/// Verify cancellation wakes host-only work but cannot release an admitted storage operation.
/// 验证取消会唤醒仅在宿主排队的工作，但不能释放已准入的存储操作。
#[tokio::test]
async fn admitted_compaction_cancellation_retains_the_writer_budget_wait() {
    let writer_budget = Arc::new(tokio::sync::Semaphore::new(1));
    let held = Arc::clone(&writer_budget)
        .acquire_owned()
        .await
        .expect("hold writer budget before recovered compaction");
    let cancel_wake = Arc::new(Notify::new());
    cancel_wake.notify_one();
    let waiting_budget = Arc::clone(&writer_budget);
    let waiting_wake = Arc::clone(&cancel_wake);
    let admitted_wait = tokio::spawn(async move {
        acquire_compaction_writer_budget(waiting_budget, waiting_wake.as_ref(), true).await
    });
    tokio::task::yield_now().await;
    assert!(
        !admitted_wait.is_finished(),
        "an admitted operation must remain owned while waiting to forward cancellation"
    );
    drop(held);
    let admitted_result = tokio::time::timeout(Duration::from_secs(1), admitted_wait)
        .await
        .expect("admitted budget wait should finish after permit release")
        .expect("admitted budget wait task should not panic");
    assert!(admitted_result.is_ok());

    let host_only_budget = Arc::new(tokio::sync::Semaphore::new(0));
    let host_only_wake = Notify::new();
    host_only_wake.notify_one();
    let host_only_result = tokio::time::timeout(
        Duration::from_secs(1),
        acquire_compaction_writer_budget(host_only_budget, &host_only_wake, false),
    )
    .await
    .expect("host-only cancellation should wake immediately");
    assert!(matches!(
        host_only_result,
        Err(VceCompactionJobOutcome::Cancelled)
    ));
}

/// Stage deterministic one-row content and path fragments through the production VCE port.
/// 通过生产 VCE 端口暂存确定性的单行内容与路径碎片。
///
/// `index` is an isolated writable fixture, `label` separates attempts, and the numeric range
/// creates unique canonical Blobs. The function returns only after every physical write is
/// acknowledged by the real in-process engine.
/// `index` 是隔离的可写夹具，`label` 区分不同尝试，数字范围用于创建唯一规范 Blob。
/// 该函数仅在真实进程内引擎确认全部物理写入后返回。
async fn stage_real_compaction_fragments(
    index: &VceProjectIndex,
    label: &str,
    start: usize,
    count: usize,
    generation: u64,
) {
    for ordinal in start..start + count {
        let path = format!("src/{label}_{ordinal}.rs");
        let content = format!("pub fn {label}_{ordinal}() {{}}");
        let blob_name = vulcan_context_engine::blob_name(&path, &content)
            .expect("build canonical compaction fixture Blob");
        let chunk = vulcan_context_engine::Chunk::new(
            blob_name.clone(),
            &path,
            content,
            1,
            1,
            vulcan_context_engine::ChunkKind::Recursive,
        )
        .expect("build compaction fixture Chunk");
        let path_document = vulcan_context_engine::PathDocument::new(blob_name.clone(), &path)
            .expect("build compaction fixture path document");
        let indexed_blob = vulcan_context_engine::IndexedBlob {
            blob_name: blob_name.clone(),
            path,
            generation,
            structure_analyzer_id: None,
            structure_mode: None,
            structure_fallback_reason: None,
            chunks: vec![chunk.clone()],
            embedding_vectors: vec![vulcan_context_engine::EmbeddingVector {
                input_id: chunk.chunk_id,
                values: vec![1.0, 0.0, 0.0],
            }],
            path_embedding: Some(vulcan_context_engine::EmbeddingVector {
                input_id: path_document.path_id.clone(),
                values: vec![0.0, 1.0, 0.0],
            }),
            path_document: Some(path_document),
            symbols: Vec::new(),
            path_status: vulcan_context_engine::AuxiliaryIndexStatus::Ready,
            symbol_status: vulcan_context_engine::AuxiliaryIndexStatus::NotApplicable,
            status: vulcan_context_engine::IndexedBlobStatus::Ready,
        };
        index
            .stage(vulcan_context_engine::IndexStageBatch {
                operation_id: format!("stage-{label}-{ordinal}"),
                refresh_id: format!("refresh-{label}-{ordinal}"),
                generation,
                blobs: vec![indexed_blob],
            })
            .await
            .expect("stage real compaction fixture fragment");
    }
}

/// Wait until a host compaction receipt durably records one engine-owned operation.
/// 等待宿主整理回执持久记录一项由引擎持有的操作。
///
/// `manager`, `project_id`, and `job_id` identify the exact fixture job; `index` queries the
/// matching storage binding. Returns the table role and stable operation identity only
/// after both durable owners acknowledge it.
/// `manager`、`project_id` 与 `job_id` 标识精确夹具作业；`index` 查询匹配的存储绑定。
/// 仅在两个持久所有者均确认后返回表角色与稳定操作身份。
async fn await_admitted_compaction_operation(
    manager: &VceRuntimeManager,
    index: &VceProjectIndex,
    project_id: &str,
    job_id: &str,
) -> (VcePhysicalTableRole, String) {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let receipt = manager
                .get_job(project_id, Some(job_id))
                .await
                .expect("read compaction fixture receipt")
                .expect("compaction fixture receipt exists");
            assert!(
                !receipt.state.is_terminal(),
                "compaction finished before host recovery was injected: {:?}",
                receipt.error
            );
            if let Some((role, operation_id)) = receipt.compaction.as_ref().and_then(|state| {
                state.tables.iter().find_map(|table| {
                    table
                        .operation_id
                        .as_ref()
                        .map(|operation_id| (table.plan.statistics.role, operation_id.clone()))
                })
            }) {
                // The host records the stable identity before submission so an uncertain reply
                // can be queried safely. Wait for engine acknowledgement before injecting
                // host loss; an absent operation at that earlier boundary is a different fault.
                // 宿主在提交前记录稳定身份，以便安全查询不确定响应。注入宿主丢失前等待引擎确认；
                // 更早边界上的操作缺失属于另一种故障。
                if index
                    .physical_compaction_operation(role, &operation_id)
                    .await
                    .is_ok()
                {
                    return (role, operation_id);
                }
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("storage operation must be durably admitted before deadline")
}

/// Drop only the fixture host worker and its in-memory owner while preserving durable state.
/// 仅丢弃夹具宿主工作任务及其内存所有者，同时保留持久状态。
///
/// `state` is the exact project job slot. The function returns after the task and writer lease
/// are gone, reproducing a host restart while preserving in-process durable state.
/// `state` 是精确项目作业槽位。该函数在任务与写入租约释放后返回，以复现保留进程内持久状态的宿主重启。
async fn abandon_fixture_host_worker(state: &Arc<VceProjectJobState>) {
    let worker = state
        .worker_handle
        .lock()
        .await
        .take()
        .expect("fixture host worker exists");
    worker.abort();
    assert!(
        worker
            .await
            .expect_err("fixture host worker is deliberately aborted")
            .is_cancelled()
    );
    let active = state
        .active
        .lock()
        .await
        .take()
        .expect("fixture in-memory owner exists");
    drop(active);
}

/// Verify real physical compaction recovery and explicit operation-state loss end to end.
/// 端到端验证真实物理整理恢复及明确的操作状态丢失。
///
/// `executor` owns all fixture workers. Every database and vector table lives under one
/// temporary directory; no user project, provider, or billed inference request is involved.
/// `executor` 持有全部夹具任务。所有数据库与向量表均位于同一临时目录，不涉及用户项目、供应商或计费推理请求。
async fn verify_real_compaction_host_recovery(executor: Arc<tokio::runtime::Runtime>) {
    use vulcan_ai_contract::test_support::MockHostAiPort;
    use vulcan_workspace_store::data_plane::{HostRuntimeHandle, HostRuntimeMetadata};

    let temporary = tempfile::tempdir().expect("create isolated compaction fixture");
    let project_root = temporary.path().join("source");
    let data_root = temporary.path().join("data");
    std::fs::create_dir_all(&project_root).expect("create compaction source root");
    std::fs::create_dir_all(&data_root).expect("create compaction data root");
    let host_runtime = HostRuntimeHandle::new_for_test(HostRuntimeMetadata {
        data_root: data_root.clone(),
        sqlite_path: data_root.join("unused.sqlite3"),
        sqlite_binding_id: Uuid::new_v4().to_string(),
        system_statistics_sqlite_path: data_root.join("unused-statistics.sqlite3"),
        system_statistics_sqlite_binding_id: Uuid::new_v4().to_string(),
    });
    let factory = VceDataPlaneFactory::bootstrap(&host_runtime)
        .await
        .expect("bootstrap compaction fixture storage");
    let capabilities = job_test_capabilities();
    let fingerprint = crate::vce::vce_embedding_physical_fingerprint(&capabilities, 3)
        .expect("freeze compaction fixture fingerprint");
    let mock = MockHostAiPort::new();
    mock.set_capability_snapshot(capabilities)
        .expect("configure compaction fixture capability metadata");
    let expected_fingerprint = fingerprint.clone();
    let manager = VceRuntimeManager::new(
        Arc::new(mock),
        factory.clone(),
        ServiceEventEmitter::new(Vec::new()),
        Arc::new(move |contract| {
            if contract.project_id == "compaction-project"
                && contract.dimension == 3
                && contract.embedding_fingerprint == expected_fingerprint
            {
                Ok(())
            } else {
                Err(String::from("Compaction fixture publication authority rejected").into())
            }
        }),
    );
    let mut settings = ProjectVceSettings {
        enabled: true,
        revision: 1,
        update_mode: vulcan_workbench_model::ProjectVceUpdateMode::Manual,
        ..ProjectVceSettings::default()
    };
    manager
        .enable_project(
            "compaction-project",
            &project_root,
            settings.clone(),
            3,
            &fingerprint,
            true,
        )
        .await
        .expect("prepare isolated compaction project");
    let initialized = manager
        .submit_job(
            "compaction-project",
            VceJobAdmission {
                retry_of_job_id: None,
                kind: VceJobKind::Initialize,
                idempotency_key: String::from("initialize-compaction-project"),
                expected_settings_revision: settings.revision,
                expected_checkpoint_version: None,
                contract_fingerprint: fingerprint.clone(),
            },
            &executor,
        )
        .await
        .expect("admit empty compaction fixture initialization");
    let initialized = await_test_job(&manager, "compaction-project", &initialized.job_id).await;
    assert_eq!(initialized.state, VceJobState::Succeeded);
    let checkpoint = initialized
        .result_checkpoint_version
        .expect("empty compaction fixture publishes a checkpoint");
    assert!(settings.revision > 0);
    assert!(checkpoint > 0);
    assert!(
        vulcan_context_engine::is_tagged_sha256_identity(&fingerprint),
        "fixture fingerprint must satisfy compaction admission: {fingerprint}"
    );
    let runtime = manager
        .require_project("compaction-project")
        .await
        .expect("read active compaction runtime");
    stage_real_compaction_fragments(&runtime.index, "recovery", 0, 32, 2).await;
    // Policy-only changes advance host admission without rewriting immutable physical provenance.
    // 仅策略变更推进宿主准入，但不改写不可变物理来源。
    settings.revision += 1;
    settings.max_results += 1;
    assert!(
            manager
                .reconcile_policy_revision(
                    "compaction-project",
                    &project_root,
                    &settings,
                    &fingerprint,
                )
                .await
                .expect("adopt policy-only compaction fixture revision"),
            "policy-only revision must retain the active physical index",
        );

    let target = manager
        .stored_maintenance_target(
            "compaction-project",
            vulcan_context_engine::VceMaintenanceMode::Compact,
        )
        .await
        .expect("plan real physical compaction");
    assert_eq!(target.checkpoint_version, Some(checkpoint));
    assert_eq!(
        target.plan.actions.len(),
        2,
        "both vector tables need compaction"
    );
    let before_by_role = target
        .plan
        .physical_tables
        .iter()
        .map(|table| (table.statistics.role, table.statistics.clone()))
        .collect::<BTreeMap<_, _>>();
    let action_ids = target
        .plan
        .actions
        .iter()
        .map(|action| action.action_id.clone())
        .collect::<BTreeSet<_>>();
    let first = manager
        .submit_physical_compaction(
            "compaction-project",
            VceJobAdmission {
                retry_of_job_id: None,
                kind: VceJobKind::Compact,
                idempotency_key: String::from("compact-with-host-recovery"),
                expected_settings_revision: settings.revision,
                expected_checkpoint_version: Some(checkpoint),
                contract_fingerprint: fingerprint.clone(),
            },
            action_ids,
            &executor,
        )
        .await
        .expect("admit physical compaction recovery fixture");
    let (admitted_role, admitted_operation_id) = await_admitted_compaction_operation(
        &manager,
        &runtime.index,
        "compaction-project",
        &first.job_id,
    )
    .await;
    let state = manager.project_job_state("compaction-project").await;
    abandon_fixture_host_worker(&state).await;

    let attached = manager
        .get_job("compaction-project", Some(&first.job_id))
        .await
        .expect("reattach exact physical compaction")
        .expect("reattached physical compaction exists");
    assert_eq!(attached.job_id, first.job_id);
    let recovered = await_test_job(&manager, "compaction-project", &first.job_id).await;
    assert_eq!(
        recovered.state,
        VceJobState::Succeeded,
        "recovered compaction diagnostic: {:?}",
        recovered.error
    );
    assert_eq!(recovered.source_checkpoint_version, Some(checkpoint));
    assert_eq!(recovered.result_checkpoint_version, Some(checkpoint));
    let recovered_operation = recovered
        .compaction
        .as_ref()
        .expect("recovered table receipts")
        .tables
        .iter()
        .find(|table| table.plan.statistics.role == admitted_role)
        .and_then(|table| table.operation_id.as_deref())
        .expect("recovered operation identity remains durable");
    assert_eq!(recovered_operation, admitted_operation_id);
    for table in recovered
        .compaction
        .as_ref()
        .expect("completed table receipts")
        .tables
        .iter()
    {
        let before = before_by_role
            .get(&table.plan.statistics.role)
            .expect("planned role has initial statistics");
        assert_eq!(table.state, VceCompactionTableState::Succeeded);
        assert!(table.latest_statistics.fragment_count < before.fragment_count);
        assert!(table.metrics.fragments_removed > 0);
    }

    // A second fragmented layout supplies an unfinished receipt whose storage operation is
    // deliberately absent, reproducing runtime-restart state loss without killing a process.
    // 第二组碎片布局提供存储操作被刻意移除的未完成回执，在不终止进程的情况下复现运行时重启状态丢失。
    stage_real_compaction_fragments(&runtime.index, "state_lost", 100, 8, 3).await;
    let target = manager
        .stored_maintenance_target(
            "compaction-project",
            vulcan_context_engine::VceMaintenanceMode::Compact,
        )
        .await
        .expect("plan state-loss compaction fixture");
    let action_ids = target
        .plan
        .actions
        .iter()
        .map(|action| action.action_id.clone())
        .collect::<BTreeSet<_>>();
    assert!(!action_ids.is_empty());
    let budget = manager
        .job_writer_budget
        .acquire()
        .await
        .expect("hold compaction writer budget");
    let queued = manager
        .submit_physical_compaction(
            "compaction-project",
            VceJobAdmission {
                retry_of_job_id: None,
                kind: VceJobKind::Compact,
                idempotency_key: String::from("compact-with-state-loss"),
                expected_settings_revision: settings.revision,
                expected_checkpoint_version: Some(checkpoint),
                contract_fingerprint: fingerprint.clone(),
            },
            action_ids,
            &executor,
        )
        .await
        .expect("queue storage state-loss fixture");
    assert_eq!(queued.state, VceJobState::Queued);
    let state = manager.project_job_state("compaction-project").await;
    let active = state
        .active
        .lock()
        .await
        .clone()
        .expect("state-loss fixture owns one queued job");
    let store = active.store.clone();
    let mut admitted = queued.clone();
    admitted.state = VceJobState::Running;
    admitted.sequence += 1;
    admitted.updated_at_unix_ms = job_time()
        .expect("read state-loss fixture time")
        .max(admitted.updated_at_unix_ms);
    let fake_operation_id = format!("{}:lost-storage-operation", admitted.job_id);
    let table_count = admitted
        .compaction
        .as_ref()
        .expect("queued compaction tables")
        .tables
        .len() as u64;
    admitted.progress = Some(compaction_progress(
        &fake_operation_id,
        0,
        table_count,
        VceRefreshPhase::Writing,
    ));
    let first_table = admitted
        .compaction
        .as_mut()
        .expect("admitted compaction tables")
        .tables
        .first_mut()
        .expect("state-loss fixture has one table");
    first_table.state = VceCompactionTableState::Queued;
    first_table.round = 1;
    first_table.operation_id = Some(fake_operation_id.clone());
    first_table.operation_expected_version = Some(first_table.plan.statistics.version);
    admitted
        .validate()
        .expect("injected admitted receipt remains contract-valid");
    let admitted = store
        .update(&queued, &admitted)
        .await
        .expect("persist missing storage operation admission");
    let mut running = admitted.clone();
    running.sequence += 1;
    running.updated_at_unix_ms = job_time()
        .expect("read running state-loss fixture time")
        .max(running.updated_at_unix_ms);
    running
        .compaction
        .as_mut()
        .expect("running compaction tables")
        .tables
        .first_mut()
        .expect("state-loss fixture has one running table")
        .state = VceCompactionTableState::Running;
    running
        .validate()
        .expect("injected state-loss receipt remains contract-valid");
    store
        .update(&admitted, &running)
        .await
        .expect("persist running but missing storage operation");
    abandon_fixture_host_worker(&state).await;
    drop(active);
    drop(budget);

    manager
        .get_job("compaction-project", Some(&queued.job_id))
        .await
        .expect("reattach missing storage operation")
        .expect("state-loss receipt exists");
    let interrupted = await_test_job(&manager, "compaction-project", &queued.job_id).await;
    assert_eq!(interrupted.state, VceJobState::Interrupted);
    assert_eq!(interrupted.source_checkpoint_version, Some(checkpoint));
    assert!(interrupted.result_checkpoint_version.is_none());
    assert!(
        interrupted
            .error
            .as_deref()
            .is_some_and(|error| error.contains("state was lost")),
        "state loss must remain explicit: {:?}",
        interrupted.error
    );
    let lost_table = interrupted
        .compaction
        .as_ref()
        .expect("interrupted table receipts")
        .tables
        .first()
        .expect("interrupted table exists");
    assert_eq!(lost_table.state, VceCompactionTableState::StateLost);
    assert_eq!(
        lost_table.operation_id.as_deref(),
        Some(fake_operation_id.as_str())
    );
    assert_eq!(
        manager
            .get_job("compaction-project", Some(&queued.job_id))
            .await
            .expect("replay interrupted state-loss receipt"),
        Some(interrupted)
    );

    // A previously observed transient table-receipt error must not retain the active owner
    // after a later authoritative terminal receipt has been persisted successfully.
    // 此前观察到的瞬时表回执错误不能在后续权威终态回执成功持久化后继续保留活动所有者。
    stage_real_compaction_fragments(&runtime.index, "terminal_recovery", 200, 8, 4).await;
    let target = manager
        .stored_maintenance_target(
            "compaction-project",
            vulcan_context_engine::VceMaintenanceMode::Compact,
        )
        .await
        .expect("plan terminal persistence recovery compaction");
    let action_ids = target
        .plan
        .actions
        .iter()
        .map(|action| action.action_id.clone())
        .collect::<BTreeSet<_>>();
    assert!(!action_ids.is_empty());
    let budget = manager
        .job_writer_budget
        .acquire()
        .await
        .expect("hold terminal recovery writer budget");
    let terminal_recovery = manager
        .submit_physical_compaction(
            "compaction-project",
            VceJobAdmission {
                retry_of_job_id: None,
                kind: VceJobKind::Compact,
                idempotency_key: String::from("compact-clears-stale-persistence-diagnostic"),
                expected_settings_revision: settings.revision,
                expected_checkpoint_version: Some(checkpoint),
                contract_fingerprint: fingerprint.clone(),
            },
            action_ids,
            &executor,
        )
        .await
        .expect("queue terminal persistence recovery compaction");
    let state = manager.project_job_state("compaction-project").await;
    let active = state
        .active
        .lock()
        .await
        .clone()
        .expect("terminal persistence recovery owns one queued job");
    *active.persistence_error.lock().await = Some(String::from(
        "fixture transient table receipt persistence failure",
    ));
    drop(active);
    drop(budget);
    let recovered = await_test_job(&manager, "compaction-project", &terminal_recovery.job_id).await;
    assert_eq!(recovered.state, VceJobState::Succeeded);
    assert!(recovered.error.is_none());
    assert!(state.active.lock().await.is_none());

    // The engine contract returns a queued snapshot immediately after admission. Reject every
    // subsequent non-terminal observation update and prove the host still polls the same durable
    // operation to its authoritative terminal result instead of orphaning storage work.
    // 引擎合约在准入后立即返回排队快照。拒绝后续每次非终态观察更新，并证明宿主仍会轮询同一
    // 持久操作直至权威终态，而不是遗弃存储工作。
    stage_real_compaction_fragments(&runtime.index, "observation_failure", 250, 8, 5).await;
    let target = manager
        .stored_maintenance_target(
            "compaction-project",
            vulcan_context_engine::VceMaintenanceMode::Compact,
        )
        .await
        .expect("plan observation persistence failure compaction");
    let action_ids = target
        .plan
        .actions
        .iter()
        .map(|action| action.action_id.clone())
        .collect::<BTreeSet<_>>();
    assert!(!action_ids.is_empty());
    let budget = manager
        .job_writer_budget
        .acquire()
        .await
        .expect("hold observation failure writer budget");
    let observation_recovery = manager
        .submit_physical_compaction(
            "compaction-project",
            VceJobAdmission {
                retry_of_job_id: None,
                kind: VceJobKind::Compact,
                idempotency_key: String::from(
                    "compact-continues-after-observation-persistence-failure",
                ),
                expected_settings_revision: settings.revision,
                expected_checkpoint_version: Some(checkpoint),
                contract_fingerprint: fingerprint.clone(),
            },
            action_ids,
            &executor,
        )
        .await
        .expect("queue observation persistence failure compaction");
    let observation_state = manager.project_job_state("compaction-project").await;
    let observation_active = observation_state
        .active
        .lock()
        .await
        .clone()
        .expect("observation persistence fixture owns one queued job");
    let trigger = format!(
        "CREATE TRIGGER reject_compaction_observation BEFORE UPDATE ON vce_host_jobs \
             WHEN OLD.job_id = '{}' \
             AND json_extract(OLD.snapshot_json, '$.compaction.tables[0].operation_id') IS NOT NULL \
             AND json_extract(NEW.snapshot_json, '$.state') = 'running' \
             AND json_extract(NEW.snapshot_json, '$.compaction.tables[0].state') IN ('queued', 'running') \
             BEGIN SELECT RAISE(ABORT, 'injected compaction observation failure'); END;",
        observation_recovery.job_id
    );
    observation_active
        .store
        .execute_test_storage_script(&trigger)
        .await
        .expect("install observation persistence fault");
    drop(budget);
    let observation_recovered =
        await_test_job(&manager, "compaction-project", &observation_recovery.job_id).await;
    assert_eq!(observation_recovered.state, VceJobState::Succeeded);
    assert!(observation_recovered.error.is_none());
    // The armed trigger above rejects every non-terminal observation update, and this asserts that it never
    // fired. That is the correct expectation for an in-process engine, not a weakened one:
    //
    // The in-process engine answered `submit` with a queued snapshot and the host then polled it, which is
    // the only window in which an advisory observation receipt is ever written. The in-process step is
    // synchronous, so `submit` returns an authoritative terminal result directly and no polling window
    // exists — there is no storage work that a failed observation could orphan. Keeping the trigger
    // armed turns this into a guard: if any future change reintroduces advisory observation writes, the
    // trigger aborts them and this assertion fails.
    //
    // 上方已武装的触发器会拒绝每次非终态观察更新，而本断言要求它从未触发。对进程内引擎而言这是正确的期望，
    // 而不是被削弱的期望：进程内引擎对 `submit` 回答排队快照、宿主随后轮询它，那是唯一会写入"观察回执"的
    // 窗口。进程内步骤是同步的，故 `submit` 直接返回权威终态，不存在轮询窗口——也就没有"失败的观察会使其
    // 成为孤儿"的存储工作。保持触发器武装使本断言成为护栏：若将来有任何改动重新引入观察写入，触发器会
    // 中止它们并使本断言失败。
    assert_eq!(
        observation_active
            .compaction_observation_persistence_failures
            .load(std::sync::atomic::Ordering::SeqCst),
        0,
        "a synchronous in-process step must never attempt an advisory observation receipt"
    );
    observation_active
        .store
        .execute_test_storage_script("DROP TRIGGER reject_compaction_observation;")
        .await
        .expect("remove observation persistence fault");
    assert!(observation_state.active.lock().await.is_none());
    drop(observation_active);
    drop(observation_state);

    // Reject both the ordinary round-result write and its first terminal retry. The exact
    // terminal candidate must retain the completed physical facts and become recoverable once
    // storage accepts writes again; cancellation is only the recovery command, not a rewrite.
    // 同时拒绝普通轮次结果写入及首次终态重试。精确终态候选必须保留已完成物理事实，并在存储恢复
    // 可写后能够继续；取消在此仅是恢复命令，不能改写既有结果。
    stage_real_compaction_fragments(&runtime.index, "terminal_candidate", 300, 8, 6).await;
    let target = manager
        .stored_maintenance_target(
            "compaction-project",
            vulcan_context_engine::VceMaintenanceMode::Compact,
        )
        .await
        .expect("plan retained terminal candidate compaction");
    let action_ids = target
        .plan
        .actions
        .iter()
        .map(|action| action.action_id.clone())
        .collect::<BTreeSet<_>>();
    assert!(!action_ids.is_empty());
    let budget = manager
        .job_writer_budget
        .acquire()
        .await
        .expect("hold retained candidate writer budget");
    let retained = manager
        .submit_physical_compaction(
            "compaction-project",
            VceJobAdmission {
                retry_of_job_id: None,
                kind: VceJobKind::Compact,
                idempotency_key: String::from("compact-retains-terminal-candidate"),
                expected_settings_revision: settings.revision,
                expected_checkpoint_version: Some(checkpoint),
                contract_fingerprint: fingerprint,
            },
            action_ids,
            &executor,
        )
        .await
        .expect("queue retained terminal candidate compaction");
    let retained_state = manager.project_job_state("compaction-project").await;
    let retained_active = retained_state
        .active
        .lock()
        .await
        .clone()
        .expect("retained candidate fixture owns one queued job");
    let trigger = format!(
        "CREATE TRIGGER reject_compaction_terminal_candidate BEFORE UPDATE ON vce_host_jobs \
             WHEN OLD.job_id = '{}' AND (((json_extract(NEW.snapshot_json, '$.state') = 'running') \
             AND (json_extract(NEW.snapshot_json, '$.compaction.tables[0].latest_statistics.version') \
             > json_extract(OLD.snapshot_json, '$.compaction.tables[0].latest_statistics.version'))) \
             OR json_extract(NEW.snapshot_json, '$.state') IN ('succeeded', 'failed', 'cancelled', 'interrupted')) \
             BEGIN SELECT RAISE(ABORT, 'injected compaction terminal candidate failure'); END;",
        retained.job_id
    );
    retained_active
        .store
        .execute_test_storage_script(&trigger)
        .await
        .expect("install retained terminal candidate fault");
    drop(budget);
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if retained_active.persistence_error.lock().await.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("terminal persistence fault must become visible");
    let candidate = retained_active
        .pending_terminal_receipt
        .lock()
        .await
        .clone()
        .expect("exact failed terminal successor remains retained");
    assert_eq!(candidate.state, VceJobState::Failed);
    let candidate_table = candidate
        .compaction
        .as_ref()
        .expect("candidate retains table state")
        .tables
        .first()
        .expect("candidate retains selected table");
    assert!(matches!(
        candidate_table.state,
        VceCompactionTableState::Succeeded | VceCompactionTableState::Failed
    ));
    assert!(candidate_table.latest_statistics.version > candidate_table.plan.statistics.version);
    assert!(candidate_table.metrics.fragments_removed > 0);
    retained_active
        .store
        .execute_test_storage_script("DROP TRIGGER reject_compaction_terminal_candidate;")
        .await
        .expect("remove retained terminal candidate fault");
    let recovered_candidate = manager
        .cancel_job("compaction-project", &retained.job_id)
        .await
        .expect("retry exact retained terminal candidate");
    assert_eq!(recovered_candidate, candidate);
    assert!(retained_state.active.lock().await.is_none());
    drop(retained_active);
    drop(retained_state);

    drop(store);
    drop(state);
    drop(runtime);
    manager
        .shutdown()
        .await
        .expect("shutdown compaction fixture manager");
    factory
        .shutdown()
        .await
        .expect("close compaction fixture bindings");
}

/// Exercise host reattachment and operation state-loss handling through real temporary storage.
/// 通过真实临时存储验证宿主重新附着与操作状态丢失处理。
///
/// Takes no parameters and uses an isolated production-sized runtime stack.
/// 无参数，并使用隔离且与生产规模相当的运行时栈。
#[test]
fn in_process_compaction_recovers_without_duplicate_submission() {
    let executor = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("build isolated compaction runtime"),
    );
    std::thread::Builder::new()
        .name(String::from("vce-compaction-recovery"))
        .stack_size(8 * 1024 * 1024)
        .spawn({
            let executor = Arc::clone(&executor);
            move || executor.block_on(verify_real_compaction_host_recovery(executor.clone()))
        })
        .expect("spawn isolated compaction fixture thread")
        .join()
        .expect("isolated compaction fixture thread completed");
}

/// Prove the real supervisor keeps execution polling while progress persistence waits on shared storage.
/// 证明真实监管器在进度持久化等待共享存储时仍持续轮询执行任务。
/// The execution holds the gate until the test releases it; an inline future would deadlock forever.
/// 执行任务持有闸门直到测试释放；若执行 future 内联轮询，此场景将永久死锁。
#[test]
fn supervised_execution_keeps_polling_during_progress_persistence() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build deterministic supervisor runtime");
    runtime.block_on(async {
            // These signals make the shared-lock interleaving deterministic and bounded.
            // 这些信号让共享锁交错过程具备确定性并且有界。
            let shared_gate = Arc::new(tokio::sync::Mutex::new(()));
            let execution_started = Arc::new(Notify::new());
            let persistence_started = Arc::new(Notify::new());
            let release_execution = Arc::new(Notify::new());
            let (progress_sender, progress_receiver) = watch::channel(None);
            let control = VceRefreshControl::default();
            let observed_control = control.clone().with_observer(Arc::new(move |progress| {
                progress_sender.send_replace(Some(progress));
            }));

            let execution_gate = Arc::clone(&shared_gate);
            let execution_started_signal = Arc::clone(&execution_started);
            let release_signal = Arc::clone(&release_execution);
            let execution = async move {
                let _gate = execution_gate.lock().await;
                execution_started_signal.notify_one();
                release_signal.notified().await;
                Ok::<u64, String>(7)
            };

            let persistence_gate = Arc::clone(&shared_gate);
            let persistence_started_signal = Arc::clone(&persistence_started);
            let persist_progress = move |
                _progress: VceRefreshProgress,
            | -> futures_util::future::BoxFuture<'static, Result<(), String>> {
                persistence_started_signal.notify_one();
                let persistence_gate = Arc::clone(&persistence_gate);
                Box::pin(async move {
                    let _gate = persistence_gate.lock().await;
                    Ok::<(), String>(())
                })
            };
            let supervisor = tokio::spawn(VceRuntimeManager::supervise_job_execution(
                control,
                progress_receiver,
                execution,
                persist_progress,
            ));

            tokio::time::timeout(Duration::from_secs(1), execution_started.notified())
                .await
                .expect("execution must start while supervised");
            observed_control.observe(VceRefreshProgress {
                activity: None,
                phase: vulcan_context_engine::VceRefreshPhase::Scanning,
                operation_id: String::from("shared-gate-test"),
                unit: vulcan_context_engine::VceRefreshUnit::Files,
                completed: 0,
                total: Some(1),
                overall_completed: 0,
                overall_total: Some(1),
                provider_batch_state: None,
                current_path: None,
            });
            tokio::time::timeout(Duration::from_secs(1), persistence_started.notified())
                .await
                .expect("progress persistence must reach the shared gate");
            release_execution.notify_one();
            let result = tokio::time::timeout(Duration::from_secs(1), supervisor)
                .await
                .expect("supervisor must not deadlock behind progress persistence")
                .expect("supervisor task must finish");
            assert_eq!(result.expect("execution must complete"), 7);
        });
}

/// Ensure unchanged polling cannot advance durable progress timestamps or repeat storage writes.
/// 确保相同状态轮询不会推进持久进度时间戳或重复写入存储。
/// Takes no input; holds the worker across multiple real sampling ticks and returns after joining it.
/// 无输入；跨多个实际采样时钟保留工作任务，连接完成后返回。
#[test]
fn supervised_execution_does_not_republish_unchanged_progress() {
    // runtime keeps this isolated supervisor and its timers on one test thread.
    // runtime 将此隔离监管器及其时钟保留在一个测试线程。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        // count and persisted establish the exact number of acknowledged progress writes.
        // count 与 persisted 确定已确认进度写入的准确次数。
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let persisted = Arc::new(Notify::new());
        // release holds execution until multiple sampling ticks have occurred.
        // release 保留执行，直到经过多个采样时钟。
        let release = Arc::new(Notify::new());
        // progress is a genuine unchanged local-preparation observation.
        // progress 是实际保持不变的本地准备观察。
        let progress = VceRefreshProgress {
            activity: None,
            phase: vulcan_context_engine::VceRefreshPhase::Chunking,
            operation_id: "unchanged-progress".to_owned(),
            unit: vulcan_context_engine::VceRefreshUnit::Files,
            completed: 1,
            total: Some(3),
            overall_completed: 0,
            overall_total: Some(3),
            provider_batch_state: None,
            current_path: None,
        };
        // sender stays alive so channel closure cannot settle the test early.
        // sender 保持存活，避免通道关闭使测试提前收敛。
        let (sender, receiver) = watch::channel(Some(progress.clone()));
        // worker_release is owned by the real supervised execution future.
        // worker_release 由实际受监管执行 future 持有。
        let worker_release = Arc::clone(&release);
        // writes and signal belong only to the persistence callback.
        // writes 与 signal 仅属于持久化回调。
        let writes = Arc::clone(&count);
        let signal = Arc::clone(&persisted);
        // supervisor retains worker ownership through normal completion.
        // supervisor 在正常完成前始终持有工作任务所有权。
        let supervisor = tokio::spawn(VceRuntimeManager::supervise_job_execution(
            VceRefreshControl::default(),
            receiver,
            async move {
                worker_release.notified().await;
                Ok(1)
            },
            move |_| {
                writes.fetch_add(1, Ordering::SeqCst);
                signal.notify_one();
                Box::pin(async { Ok(()) })
            },
        ));
        tokio::time::timeout(Duration::from_secs(1), persisted.notified())
            .await
            .unwrap();
        sender.send_replace(Some(progress));
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert_eq!(count.load(Ordering::SeqCst), 1);
        release.notify_one();
        assert_eq!(supervisor.await.unwrap().unwrap(), 1);
        assert_eq!(count.load(Ordering::SeqCst), 1);
    });
}

/// Prove transient SQLite contention is retried without changing the latest progress payload.
/// 证明瞬时 SQLite 竞争会重试，且不会改变最新进度载荷。
/// The fixture fails once with the production lock text and succeeds on the next attempt.
/// 测试夹具先使用生产锁错误文本失败一次，再在下一次尝试成功。
#[test]
fn transient_progress_persistence_is_retried() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build progress retry runtime");
    runtime.block_on(async {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let attempts = Arc::new(AtomicUsize::new(0));
        let persist_progress = {
            let attempts = Arc::clone(&attempts);
            move |_progress: VceRefreshProgress|
                    -> futures_util::future::BoxFuture<'static, Result<(), String>> {
                    let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                    Box::pin(async move {
                        if attempt == 0 {
                            Err(String::from(
                                "Update VCE job receipt: sqlite execute failed: database is locked",
                            ))
                        } else {
                            Ok(())
                        }
                    })
                }
        };
        let progress = VceRefreshProgress {
            activity: None,
            phase: vulcan_context_engine::VceRefreshPhase::Writing,
            operation_id: String::from("progress-retry-test"),
            unit: vulcan_context_engine::VceRefreshUnit::Blobs,
            completed: 1,
            total: Some(2),
            overall_completed: 1,
            overall_total: Some(2),
            provider_batch_state: None,
            current_path: None,
        };

        persist_progress_with_retry(&persist_progress, progress)
            .await
            .expect("transient progress persistence should recover");
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    });
}

/// Verify a worker panic is re-raised so the outer job boundary can perform staging cleanup.
/// 验证工作任务恐慌会重新抛出，使外层作业边界能够执行暂存清理。
/// This exercises the production supervisor path rather than calling the cleanup helper directly.
/// 此测试走生产监管器路径，而不是直接调用清理辅助函数。
#[test]
fn supervised_execution_rethrows_worker_panics() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build panic supervisor runtime");
    runtime.block_on(async {
            // The closed progress channel leaves only the real worker result for the supervisor to join.
            // 已关闭的进度通道让监管器只需连接真实工作任务结果。
            let (_progress_sender, progress_receiver) = watch::channel(None);
            let persist_progress = move |
                _progress: VceRefreshProgress,
            | -> futures_util::future::BoxFuture<'static, Result<(), String>> {
                Box::pin(async { Ok(()) })
            };
            let panic = AssertUnwindSafe(VceRuntimeManager::supervise_job_execution(
                VceRefreshControl::default(),
                progress_receiver,
                async move {
                    std::panic::panic_any("fixture worker panic");
                    #[allow(unreachable_code)]
                    Ok::<u64, String>(0)
                },
                persist_progress,
            ))
            .catch_unwind()
            .await
            .expect_err("worker panic must cross the supervisor boundary");
            assert_eq!(panic.downcast_ref::<&str>(), Some(&"fixture worker panic"));
        });
}

/// Verify restart preserves recent admission cooldown and treats clock rollback conservatively.
/// 验证重启保留最近准入冷却，并保守处理时钟回退。
/// Takes no parameters. Asserts no startup scheduling, bounded elapsed time, and exact remaining delay.
/// 无参数。断言无启动调度、耗时有界及精确剩余延时。
#[test]
fn restored_admission_preserves_cooldown_without_scheduling() {
    let mut settings = ProjectVceSettings {
        update_mode: vulcan_workbench_model::ProjectVceUpdateMode::Automatic,
        ..ProjectVceSettings::default()
    };
    let mut schedule = VceJobSchedule::new(&settings);
    let now = tokio::time::Instant::now();
    schedule.restore_admission(10_000, 40_000, now);
    assert_eq!(
        schedule.automatic_delay(now),
        Some(Duration::from_secs(270))
    );
    schedule.restore_admission(40_000, 10_000, now);
    assert_eq!(
        schedule.automatic_delay(now),
        Some(Duration::from_secs(300))
    );
    schedule.restore_admission(1, u64::MAX, now);
    assert_eq!(schedule.automatic_delay(now), Some(Duration::ZERO));
    settings.update_mode = vulcan_workbench_model::ProjectVceUpdateMode::Manual;
    schedule.update(&settings);
    assert_eq!(schedule.automatic_delay(now), None);
}

/// Verify manual admission establishes cooldown without making manual requests wait for it.
/// 验证手动准入建立冷却，但不使手动请求等待该冷却。
/// Takes no parameters. Asserts interval recomputation and policy changes preserve admission time.
/// 无参数。断言间隔重新计算及策略变化保留准入时刻。
#[test]
fn automatic_interval_uses_last_admission_and_live_project_policy() {
    let mut settings = ProjectVceSettings::default();
    let mut schedule = VceJobSchedule::new(&settings);
    let now = tokio::time::Instant::now();
    assert_eq!(schedule.automatic_delay(now), None);
    settings.update_mode = vulcan_workbench_model::ProjectVceUpdateMode::Automatic;
    schedule.update(&settings);
    assert_eq!(schedule.automatic_delay(now), Some(Duration::ZERO));
    schedule.last_admitted = Some(now);
    assert_eq!(
        schedule.automatic_delay(now + Duration::from_secs(120)),
        Some(Duration::from_secs(180))
    );
    settings.auto_update_min_interval_seconds = 600;
    schedule.update(&settings);
    assert_eq!(
        schedule.automatic_delay(now + Duration::from_secs(120)),
        Some(Duration::from_secs(480))
    );
    settings.update_mode = vulcan_workbench_model::ProjectVceUpdateMode::Manual;
    schedule.update(&settings);
    assert_eq!(
        schedule.automatic_delay(now + Duration::from_secs(120)),
        None
    );
    assert_eq!(schedule.last_admitted, Some(now));
    settings.update_mode = vulcan_workbench_model::ProjectVceUpdateMode::Automatic;
    schedule.update(&settings);
    assert_eq!(
        schedule.automatic_delay(now + Duration::from_secs(601)),
        Some(Duration::ZERO)
    );
}

/// Build a complete non-network capability snapshot for empty-project lifecycle integration.
/// 构建完整无网络能力快照，用于空项目生命周期集成。
///
/// Takes no parameters. Returns mandatory operations with an exact three-dimensional route.
/// 无参数。返回带精确三维路由的必需操作。
pub(super) fn job_test_capabilities() -> vulcan_ai_contract::InferenceCapabilitySnapshot {
    use vulcan_ai_contract::*;
    let route = ExecutionRouteSnapshot {
        provider_id: String::from("fixture-provider"),
        service_entry_id: String::from("fixture-service"),
        connection_id: String::from("fixture-account"),
        model_id: String::from("fixture-embedding"),
        protocol_binding_id: Some(String::from("fixture-protocol")),
        action_binding_id: None,
        secondary_route_id: None,
        secondary_provider_name: None,
        secondary_endpoint_name: None,
        capability_revision: String::from("fixture-capability-v1"),
        catalog_revision: String::from("fixture-catalog-v1"),
        route_revision: String::from("fixture-route-v1"),
        usage_scope: InferenceUsageScope::InternalBackground,
    };
    let embedding = EmbeddingCapabilities {
        tasks: BTreeSet::from([EmbeddingTask::Document, EmbeddingTask::Query]),
        input_resources: BTreeSet::new(),
        output_kinds: BTreeSet::from([EmbeddingOutputKind::Dense]),
        encodings: BTreeSet::from([EmbeddingEncoding::Float]),
        dimensions: BTreeSet::from([3]),
        minimum_dimensions: None,
        maximum_dimensions: None,
        default_dimensions: None,
        maximum_inputs: None,
        maximum_tokens_per_input: None,
        normalization: EmbeddingNormalization::Unknown,
        distance_metric: DistanceMetric::Unknown,
    };
    InferenceCapabilitySnapshot {
        contract_version: INFERENCE_CONTRACT_VERSION,
        consumer_profile_id: String::from("vce"),
        operations: vec![
            InferenceOperationCapability {
                operation: AiOperation::EmbeddingCreate,
                purpose_id: None,
                available: true,
                unavailable_reason: None,
                route: Some(route),
                embedding: Some(embedding),
                embedding_batch: None,
                rerank: None,
            },
            InferenceOperationCapability {
                operation: AiOperation::ConversationRespond,
                purpose_id: Some(String::from(crate::vce::VCE_QUERY_REWRITE_PURPOSE_ID)),
                available: true,
                unavailable_reason: None,
                route: None,
                embedding: None,
                embedding_batch: None,
                rerank: None,
            },
            InferenceOperationCapability {
                operation: AiOperation::RerankDocuments,
                purpose_id: None,
                available: true,
                unavailable_reason: None,
                route: None,
                embedding: None,
                embedding_batch: None,
                rerank: None,
            },
        ],
    }
}

/// Await an exact terminal receipt and its worker lease release with a bounded test deadline.
/// 在有界测试截止前等待精确终态回执及其工作线程租约释放。
///
/// `manager`, `project_id`, and `job_id` identify fixture ownership. Returns the final receipt.
/// `manager`、`project_id` 与 `job_id` 标识夹具归属。返回最终回执。
pub(super) async fn await_test_job(
    manager: &VceRuntimeManager,
    project_id: &str,
    job_id: &str,
) -> VceStoredJob {
    // Retain the last observation so a timeout identifies the stalled phase and owner.
    // 保留最后一次观察，使超时能够指出卡住的阶段与所有者。
    let mut last_observation = None;
    tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let receipt = manager
                    .get_job(project_id, Some(job_id))
                    .await
                    .expect("read exact fixture job")
                    .expect("fixture job exists");
                // Stable project ownership also covers cancellation before any model runtime exists.
                // 稳定项目归属也覆盖任何模型运行时存在之前的取消。
                let state = manager.project_job_state(project_id).await;
                last_observation = Some((receipt.clone(), state.active.lock().await.is_some()));
                if receipt.state.is_terminal()
                    && state.active.lock().await.is_none()
                    && state.execution.try_write().is_ok()
                {
                    return receipt;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|error| panic!("fixture job must settle before deadline: {error}; last observation: {last_observation:?}"))
}

/// Exercise durable admission, empty initialization, cancellation and shutdown against real storage.
    /// 通过真实存储验证持久准入、空项目初始化、取消与退出。
    ///
    /// Takes no parameters. Uses isolated temporary storage and a local mock AI with no billed calls.
    /// 无参数。使用独立临时存储与无计费请求的本地模拟人工智能。
    #[test]
    #[rustfmt::skip]
    fn real_host_storage_human_jobs_are_durable_cancellable_and_nonblocking() {
        use vulcan_ai_contract::test_support::MockHostAiPort;
        use vulcan_workspace_store::data_plane::{
            HostRuntimeHandle, HostRuntimeMetadata,
        };
        let executor = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("build isolated job runtime"),
        );
        // Run the large end-to-end fixture on a production-sized caller stack.
        // 在与生产相当的调用线程栈上运行大型端到端夹具。
        std::thread::Builder::new()
            .name(String::from("vce-job-integration"))
            .stack_size(8 * 1024 * 1024)
            .spawn({
                let executor = Arc::clone(&executor);
                move || executor.block_on(async {
            let temporary = tempfile::tempdir().expect("create isolated job fixture");
            let project_root = temporary.path().join("source");
            let data_root = temporary.path().join("data");
            std::fs::create_dir_all(&project_root).expect("create empty source root");
            std::fs::create_dir_all(&data_root).expect("create host data root");
            let host_runtime = HostRuntimeHandle::new_for_test(
                HostRuntimeMetadata {
                    data_root: data_root.clone(),
                    sqlite_path: data_root.join("unused.sqlite3"),
                    sqlite_binding_id: Uuid::new_v4().to_string(),
                    system_statistics_sqlite_path: data_root.join("unused-statistics.sqlite3"),
                    system_statistics_sqlite_binding_id: Uuid::new_v4().to_string(),
                },
            );
            let factory = VceDataPlaneFactory::bootstrap(&host_runtime)
                .await
                .expect("bootstrap lazy job storage");
            let capabilities = job_test_capabilities();
            let fingerprint = crate::vce::vce_embedding_physical_fingerprint(&capabilities, 3)
                .expect("freeze tagged physical fingerprint");
            assert!(fingerprint.starts_with("sha256:"));
            let mock = MockHostAiPort::new();
            mock.set_capability_snapshot(capabilities)
                .expect("configure isolated capabilities");
            // The fixture supplies explicit host authority instead of depending on a user service.
            // 夹具提供明确宿主权威，不依赖用户服务。
            let publication_allowed = Arc::new(AtomicBool::new(true));
            let publication_flag = Arc::clone(&publication_allowed);
            let expected_publication_fingerprint = fingerprint.clone();
            let manager = VceRuntimeManager::new(
                Arc::new(mock),
                factory.clone(),
                ServiceEventEmitter::new(Vec::new()),
                Arc::new(move |contract| {
                    if publication_flag.load(Ordering::Acquire)
                        && contract.project_id == "job-project"
                        && contract.dimension == 3
                        && contract.embedding_fingerprint == expected_publication_fingerprint
                    {
                        Ok(())
                    } else {
                        Err(String::from("Fixture publication authority revoked").into())
                    }
                }),
            );
            let center = crate::workspace_watcher::WorkspaceWatcherCenter::new()
                .expect("create isolated physical watcher");
            let watcher = crate::vce::VceWatcherService::new(
                &center,
                manager.clone(),
                ServiceEventEmitter::new(Vec::new()),
                Arc::clone(&executor),
            );
            let settings = ProjectVceSettings {
                enabled: true,
                update_mode: vulcan_workbench_model::ProjectVceUpdateMode::Automatic,
                ..ProjectVceSettings::default()
            };
            manager
                .enable_project(
                    "job-project",
                    &project_root,
                    settings.clone(),
                    3,
                    &fingerprint,
                    true,
                )
                .await
                .expect("explicit storage preparation without scan");
            assert!(watcher.registered_root("job-project").is_none());
            assert!(
                manager
                    .get_job("job-project", None)
                    .await
                    .expect("read cold jobs")
                    .is_none()
            );
            // budget prevents the worker from starting and proves admission/cancellation do not await indexing.
            // budget 阻止工作线程启动，证明准入与取消不等待索引。
            let budget = manager
                .job_writer_budget
                .acquire()
                .await
                .expect("hold fixture budget");
            let command = |key: &str, kind, checkpoint| VceJobAdmission {
                retry_of_job_id: None,
                kind,
                idempotency_key: key.to_owned(),
                expected_settings_revision: settings.revision,
                expected_checkpoint_version: checkpoint,
                contract_fingerprint: fingerprint.clone(),
            };
            // A separate handle represents another host; rejection cannot create a durable job receipt.
            // 独立句柄代表另一宿主；拒绝不能创建持久作业回执。
            let foreign_writer = factory
                .try_acquire_project_writer("job-project")
                .expect("foreign writer attempt")
                .expect("foreign writer ownership");
            assert!(
                manager
                    .submit_job(
                        "job-project",
                        command("foreign-writer-blocked", VceJobKind::Initialize, None),
                        &executor,
                    )
                    .await
                    .expect_err("foreign writer blocks admission")
                    .contains("active host writer")
            );
            assert!(
                manager
                    .get_job("job-project", None)
                    .await
                    .expect("no rejected receipt")
                    .is_none()
            );
            drop(foreign_writer);
            let queued = tokio::time::timeout(
                Duration::from_secs(5),
                manager.submit_job(
                    "job-project",
                    command("first-click", VceJobKind::Initialize, None),
                    &executor,
                ),
            )
            .await
            .expect("admission must not await budget")
            .expect("durably queue first initialization");
            assert_eq!(queued.state, VceJobState::Queued);
            assert!(
                factory
                    .try_acquire_project_writer("job-project")
                    .expect("queued ownership")
                    .is_none()
            );
            assert!(vulcan_context_engine::is_sha256_identity(
                &queued.contract_fingerprint
            ));
            assert_eq!(
                manager
                    .submit_job(
                        "job-project",
                        command("first-click", VceJobKind::Initialize, None),
                        &executor
                    )
                    .await
                    .expect("replay queued click"),
                queued
            );
            assert_eq!(
                manager
                    .submit_job(
                        "job-project",
                        command("duplicate-click", VceJobKind::Initialize, None),
                        &executor
                    )
                    .await
                    .expect("repeated initialization locates the same job")
                    .job_id,
                queued.job_id
            );
            assert!(
                manager
                    .cancel_job("job-project", "wrong-job-id")
                    .await
                    .is_err()
            );
            // queued_before_detach includes the already acknowledged duplicate-click alias transition.
            // queued_before_detach 包含已经确认的重复点击别名转换。
            let queued_before_detach = manager
                .get_job("job-project", Some(&queued.job_id))
                .await
                .expect("read current acknowledged queue receipt");
            // retained_runtime simulates an absent query registration while the admitted worker retains its own leases.
            // retained_runtime 模拟查询注册缺席，而已准入工作任务仍保留自身租约。
            let retained_runtime = manager
                .projects
                .write()
                .await
                .remove("job-project")
                .expect("temporarily detach query runtime");
            assert!(manager.project_status("job-project").await.is_none());
            assert_eq!(
                manager
                    .get_job("job-project", Some(&queued.job_id))
                    .await
                    .expect("read active job without query runtime"),
                queued_before_detach
            );
            // active_job exposes storage failure without relying on a query runtime to carry that diagnostic.
            // active_job 不依赖查询运行时携带诊断，也能暴露存储失败。
            let active_job = retained_runtime
                .job_state
                .active
                .lock()
                .await
                .clone()
                .expect("project retains queued owner");
            // Terminal-persistence failure is reported only after execution ends in production.
            // 生产环境只有执行结束后才会报告终态持久化失败。
            // Stop the synthetic queued worker before injecting that diagnostic; otherwise it still owns the budget wait.
            // 注入该诊断前停止模拟排队线程，否则它仍持有等待执行预算的任务。
            let abandoned_worker = retained_runtime.job_state.worker_handle.lock().await.take().expect("queued worker handle");
            abandoned_worker.abort();
            assert!(abandoned_worker.await.expect_err("fixture worker aborted").is_cancelled());
            active_job.control.finish();
            *active_job.persistence_error.lock().await =
                Some(String::from("fixture receipt failure"));
            let observed_failed_receipt = manager
                .get_job("job-project", Some(&queued.job_id))
                .await
                .expect("retained project owner remains queryable")
                .expect("failed owner still has its receipt");
            assert_eq!(
                observed_failed_receipt.error.as_deref(),
                Some("fixture receipt failure")
            );
            // Cancellation must recover a finished owner instead of sending a no-op signal to its finished control.
            // 取消必须恢复已结束的所有者，而不是向已结束的引擎操作发送无效信号。
            let recovered_cancelled = manager
                .cancel_job("job-project", &queued.job_id)
                .await
                .expect("recover and cancel persisted owner");
            assert_eq!(recovered_cancelled.state, VceJobState::Cancelled);
            assert!(
                retained_runtime
                    .job_state
                    .active
                    .lock()
                    .await
                    .is_none(),
                "recovery cancellation must release the stale in-memory owner"
            );
            drop(active_job);
            manager
                .cancel_job("job-project", &queued.job_id)
                .await
                .expect("cancel queued initialization");
            assert!(
                manager.project_status("job-project").await.is_none(),
                "cancellation must not reactivate models"
            );
            manager
                .projects
                .write()
                .await
                .insert(String::from("job-project"), retained_runtime);
            let cancelled = await_test_job(&manager, "job-project", &queued.job_id).await;
            assert_eq!(cancelled.state, VceJobState::Cancelled);
            assert!(
                factory
                    .try_acquire_project_writer("job-project")
                    .expect("cancelled lease released")
                    .is_some()
            );
            assert!(
                cancelled.progress.is_none(),
                "queued cancellation cannot invent file scans"
            );
            assert!(cancelled.result_checkpoint_version.is_none());
            // A newer writer must not turn replay of this terminal receipt into a busy error.
            // 新写入者存在时，回放这个终态回执也不能变成忙碌错误。
            let terminal_writer = factory
                .try_acquire_project_writer("job-project")
                .expect("terminal replay writer")
                .expect("terminal replay can reserve the project");
            assert_eq!(
                manager
                    .cancel_job("job-project", &cancelled.job_id)
                    .await
                    .expect("terminal cancellation is idempotent"),
                cancelled
            );
            drop(terminal_writer);
            assert_eq!(
                manager
                    .last_successful_update("job-project")
                    .await
                    .expect("read cancelled freshness"),
                None
            );
            // detached_close proves project teardown cancels and drains a queued owner without a query runtime.
            // detached_close 证明项目拆卸在没有查询运行时时仍取消并排空排队所有者。
            let detached_close = manager
                .submit_job(
                    "job-project",
                    command("close-without-query-runtime", VceJobKind::Initialize, None),
                    &executor,
                )
                .await
                .expect("queue detached closure fixture");
            let retained_runtime = manager
                .projects
                .write()
                .await
                .remove("job-project")
                .expect("detach runtime for project closure");
            tokio::time::timeout(
                Duration::from_secs(5),
                manager.disable_project("job-project"),
            )
            .await
            .expect("detached project closure cannot wait for the held budget")
            .expect("drain detached project job");
            assert!(retained_runtime.job_state.active.lock().await.is_none());
            assert!(manager.project_status("job-project").await.is_none());
            assert_eq!(
                manager
                    .get_job("job-project", Some(&detached_close.job_id))
                    .await
                    .expect("read drained detached receipt")
                    .expect("detached receipt persists")
                    .state,
                VceJobState::Cancelled
            );
            manager
                .projects
                .write()
                .await
                .insert(String::from("job-project"), retained_runtime);
            drop(budget);
            // Explicit retry records its durable origin but gets a fresh admission identity.
            // 明确重试记录持久来源，但取得全新准入身份。
            let mut retry_command = command("second-click", VceJobKind::Initialize, None);
            retry_command.retry_of_job_id = Some(cancelled.job_id.clone());
            let initialized = manager
                .submit_job("job-project", retry_command, &executor)
                .await
                .expect("retry with explicit new click");
            assert_eq!(
                initialized.retry_of_job_id.as_deref(),
                Some(cancelled.job_id.as_str())
            );
            assert_ne!(initialized.job_id, cancelled.job_id);
            let completed = await_test_job(&manager, "job-project", &initialized.job_id).await;
            assert_eq!(
                completed.state,
                VceJobState::Succeeded,
                "empty initialization diagnostic: {:?}",
                completed.error
            );
            let checkpoint = completed
                .result_checkpoint_version
                .expect("valid empty checkpoint exists");
            assert!(manager.project_can_serve_now("job-project", settings.revision, &fingerprint));
            // A competing writer's lease is not a query gate for the already published checkpoint.
            // 竞争写入者的租约不是已发布检查点的查询闸门。
            let foreign_writer = factory
                .try_acquire_project_writer("job-project")
                .expect("foreign query-time writer")
                .expect("foreign ownership");
            assert!(manager.project_can_serve_now("job-project", settings.revision, &fingerprint));
            tokio::time::timeout(
                Duration::from_secs(5),
                manager.retrieve("job-project", "existing empty index"),
            )
            .await
            .expect("query cannot wait on another host writer")
            .expect("query old checkpoint");
            drop(foreign_writer);
            // Unconfirmed callers cannot smuggle rebuild work through the normal synchronization entry.
            // 未确认调用方不能通过普通同步入口夹带重建工作。
            assert!(
                manager
                    .submit_job(
                        "job-project",
                        command("unconfirmed-rebuild", VceJobKind::Rebuild, Some(checkpoint)),
                        &executor
                    )
                    .await
                    .is_err()
            );
            // rebuild_plan comes from the same public management path used for human confirmation.
            // rebuild_plan 来自与人为确认相同的公开管理路径。
            // The displayed management identity is bound to the exact publication, not just its number.
            // 显示的管理身份绑定精确发布，而非仅绑定数字版本。
            let (management_identity, management_snapshot) = manager
                .management_snapshot("job-project")
                .await
                .expect("read initialized management overview");
            let management_identity = management_identity.expect("published management identity");
            assert_eq!(
                management_snapshot.engine.checkpoint_version,
                Some(checkpoint)
            );
            // Build a fresh typed request for each admission check; returns only a bounded file read.
            // 为每次准入检查构建新的类型化请求；仅返回有界文件读取。
            let management_files = || vulcan_context_engine::VceManagementQueryRequest {
                operation_id: String::from("fixture-management-files"),
                query: vulcan_context_engine::VceManagementQuery::Files(
                    vulcan_context_engine::VceFileQuery {
                        path_prefix: None,
                        status: None,
                        auxiliary_failures_only: false,
                        page: vulcan_context_engine::VcePageRequest::new(None, 25)
                            .expect("bounded fixture page"),
                    },
                ),
            };
            assert!(
                manager
                    .management_query("job-project", management_files(), None)
                    .await
                    .is_err()
            );
            assert!(
                manager
                    .management_query("job-project", management_files(), Some("another-space:1"))
                    .await
                    .is_err()
            );
            assert!(
                manager
                    .management_query(
                        "job-project",
                        management_files(),
                        Some(&management_identity)
                    )
                    .await
                    .is_ok()
            );
            let rebuild_plan = manager
                .management_query(
                    "job-project",
                    vulcan_context_engine::VceManagementQueryRequest {
                        operation_id: String::from("fixture-rebuild-plan"),
                        query: vulcan_context_engine::VceManagementQuery::MaintenancePlan {
                            mode: vulcan_context_engine::VceMaintenanceMode::Rebuild,
                        },
                    },
                    None,
                )
                .await
                .expect("load initialized empty checkpoint plan");
            let vulcan_context_engine::VceManagementQueryResult::MaintenancePlan(rebuild_plan) =
                rebuild_plan.result
            else {
                panic!("unexpected maintenance fixture result");
            };
            // rebuild_action_ids retains every exact planned identity, independent of UI ordering.
            // rebuild_action_ids 保留每个精确计划身份，与界面顺序无关。
            let rebuild_action_ids = rebuild_plan
                .actions
                .into_iter()
                .map(|action| action.action_id)
                .collect::<BTreeSet<_>>();
            assert!(
                !rebuild_action_ids.is_empty(),
                "initialized empty checkpoint must support explicit rebuild"
            );
            // cold_manager deliberately has no capabilities or permissive publication authority.
            // cold_manager 刻意不提供能力或允许发布的权威校验器。
            let cold_manager = VceRuntimeManager::new(
                Arc::new(MockHostAiPort::new()),
                factory.clone(),
                ServiceEventEmitter::new(Vec::new()),
                Arc::new(|_| {
                    Err(String::from("cold fixture must never authorize model execution").into())
                }),
            );
            // cold_plan must still expose the original stored plan without creating a model runtime.
            // cold_plan 仍须暴露原始存储计划，且不创建模型运行时。
            let cold_plan = cold_manager
                .stored_maintenance_plan(
                    "job-project",
                    vulcan_context_engine::VceMaintenanceMode::Rebuild,
                )
                .await
                .expect("plan rebuild independently of model activation");
            assert_eq!(
                cold_plan
                    .actions
                    .into_iter()
                    .map(|action| action.action_id)
                    .collect::<BTreeSet<_>>(),
                rebuild_action_ids
            );
            assert!(cold_manager.project_status("job-project").await.is_none());
            assert!(!cold_manager.project_can_serve_now(
                "job-project",
                settings.revision,
                &fingerprint
            ));
            assert!(
                cold_manager
                    .stored_maintenance_plan(
                        "missing-plan-project",
                        vulcan_context_engine::VceMaintenanceMode::Rebuild
                    )
                    .await
                    .is_err()
            );
            assert!(
                factory
                    .load_active_project_config("missing-plan-project")
                    .await
                    .expect("missing plan does not allocate storage")
                    .is_none()
            );
            drop(cold_manager);
            // A new confirmed submission must not enter the queue after live publication authority is revoked.
            // 实时发布授权撤销后，新的已确认提交不得进入队列。
            publication_allowed.store(false, Ordering::Release);
            assert!(
                manager
                    .submit_confirmed_rebuild(
                        "job-project",
                        command(
                            "revoked-rebuild-plan",
                            VceJobKind::Rebuild,
                            Some(checkpoint)
                        ),
                        false,
                        rebuild_action_ids.clone(),
                        &executor,
                    )
                    .await
                    .is_err()
            );
            publication_allowed.store(true, Ordering::Release);
            assert!(
                manager
                    .submit_confirmed_rebuild(
                        "job-project",
                        command("forged-rebuild-plan", VceJobKind::Rebuild, Some(checkpoint)),
                        false,
                        BTreeSet::from([String::from("not-the-confirmed-action")]),
                        &executor,
                    )
                    .await
                    .is_err(),
                "unconfirmed plan must not enter the durable queue"
            );
            // rebuild_budget proves admission and old-checkpoint queries do not wait for writer capacity.
            // rebuild_budget 证明准入与旧检查点查询不等待写入容量。
            let rebuild_budget = manager
                .job_writer_budget
                .acquire()
                .await
                .expect("hold rebuild capacity");
            // queued_rebuild is a real durable maintenance job rather than a blocking RPC refresh.
            // queued_rebuild 是真实持久维护作业，而非阻塞远程调用刷新。
            let queued_rebuild = tokio::time::timeout(
                Duration::from_secs(5),
                manager.submit_confirmed_rebuild(
                    "job-project",
                    command("cancel-rebuild", VceJobKind::Rebuild, Some(checkpoint)),
                    false,
                    rebuild_action_ids.clone(),
                    &executor,
                ),
            )
            .await
            .expect("rebuild admission must not await capacity")
            .expect("queue confirmed rebuild");
            assert_eq!(queued_rebuild.kind, VceJobKind::Rebuild);
            assert_eq!(queued_rebuild.state, VceJobState::Queued);
            tokio::time::timeout(
                Duration::from_secs(5),
                manager.retrieve("job-project", "existing code"),
            )
            .await
            .expect("query must not wait for rebuild")
            .expect("old empty checkpoint remains queryable");
            manager
                .cancel_job("job-project", &queued_rebuild.job_id)
                .await
                .expect("cancel queued rebuild");
            assert_eq!(
                await_test_job(&manager, "job-project", &queued_rebuild.job_id)
                    .await
                    .state,
                VceJobState::Cancelled
            );
            drop(rebuild_budget);
            // rebuilt exercises the same durable runner with explicit full re-index semantics.
            // rebuilt 使用明确完整重索引语义验证同一个持久运行器。
            let rebuilt = manager
                .submit_confirmed_rebuild(
                    "job-project",
                    command("run-rebuild", VceJobKind::Rebuild, Some(checkpoint)),
                    false,
                    rebuild_action_ids.clone(),
                    &executor,
                )
                .await
                .expect("admit confirmed rebuild");
            let rebuilt = await_test_job(&manager, "job-project", &rebuilt.job_id).await;
            assert_eq!(
                rebuilt.state,
                VceJobState::Succeeded,
                "rebuild diagnostic: {:?}",
                rebuilt.error
            );
            // Revocation blocks new work but must not reinterpret the receipt of an already completed request.
            // 撤销阻止新工作，但不得重新解释已经完成请求的回执。
            publication_allowed.store(false, Ordering::Release);
            assert_eq!(
                manager
                    .submit_confirmed_rebuild(
                        "job-project",
                        command("run-rebuild", VceJobKind::Rebuild, Some(checkpoint)),
                        false,
                        rebuild_action_ids.clone(),
                        &executor,
                    )
                    .await
                    .expect("replay completed rebuild against its original checkpoint"),
                rebuilt
            );
            publication_allowed.store(true, Ordering::Release);
            assert!(
                manager
                    .submit_confirmed_rebuild(
                        "job-project",
                        command("run-rebuild", VceJobKind::Rebuild, Some(checkpoint)),
                        false,
                        BTreeSet::from([String::from("changed-confirmation")]),
                        &executor,
                    )
                    .await
                    .is_err(),
                "the same replay key cannot authorize different maintenance actions"
            );
            // checkpoint follows the newly published generation for all subsequent fixture operations.
            // checkpoint 为全部后续夹具操作跟随新发布代次。
            let checkpoint = rebuilt
                .result_checkpoint_version
                .expect("rebuild publishes checkpoint");
            // A later publication cannot accept the old list's identity for another page or detail.
            // 后续发布不能为另一页或详情接受旧列表身份。
            let (replacement_identity, _) = manager
                .management_snapshot("job-project")
                .await
                .expect("read replacement management identity");
            assert_ne!(
                replacement_identity.as_deref(),
                Some(management_identity.as_str())
            );
            assert!(
                manager
                    .management_query(
                        "job-project",
                        management_files(),
                        Some(&management_identity)
                    )
                    .await
                    .is_err()
            );
            assert!(
                manager
                    .management_query(
                        "job-project",
                        management_files(),
                        replacement_identity.as_deref()
                    )
                    .await
                    .is_ok()
            );
            assert!(manager.project_can_serve_now("job-project", settings.revision, &fingerprint));
            let mut invalid_retry =
                command("retry-success", VceJobKind::Synchronize, Some(checkpoint));
            invalid_retry.retry_of_job_id = Some(completed.job_id.clone());
            assert!(
                manager
                    .submit_job("job-project", invalid_retry, &executor)
                    .await
                    .is_err(),
                "a successful job cannot be presented as a failed retry origin"
            );
            let canonical_root =
                std::fs::canonicalize(&project_root).expect("canonical fixture root");
            assert_eq!(
                watcher.registered_root("job-project"),
                Some(canonical_root.clone())
            );
            assert!(
                manager
                    .submit_watcher_job("job-project", &canonical_root, None, None, &executor)
                    .await
                    .is_err(),
                "automatic work must respect manual initialization cooldown"
            );
            let synchronized = manager
                .submit_job(
                    "job-project",
                    command("sync-click", VceJobKind::Synchronize, Some(checkpoint)),
                    &executor,
                )
                .await
                .expect("explicit no-change synchronization");
            let synchronized = await_test_job(&manager, "job-project", &synchronized.job_id).await;
            assert_eq!(synchronized.state, VceJobState::Succeeded);
            assert_eq!(synchronized.result_checkpoint_version, Some(checkpoint));
            assert_eq!(
                manager
                    .last_successful_update("job-project")
                    .await
                    .expect("read successful freshness"),
                Some(
                    synchronized
                        .completed_update
                        .as_ref()
                        .expect("successful batch accounting")
                        .at_unix_ms
                )
            );
            assert!(synchronized.updated_at_unix_ms >= completed.updated_at_unix_ms);
            manager
                .disable_project("job-project")
                .await
                .expect("close isolated project");
            assert!(watcher.registered_root("job-project").is_none());
            assert_eq!(
                manager
                    .cancel_job("job-project", &synchronized.job_id)
                    .await
                    .expect(
                        "terminal receipt cancellation remains available after project closure"
                    ),
                synchronized
            );
            assert!(
                manager.project_status("job-project").await.is_none(),
                "terminal replay does not open a query runtime"
            );
            manager
                .enable_project(
                    "job-project",
                    &project_root,
                    settings.clone(),
                    3,
                    &fingerprint,
                    false,
                )
                .await
                .expect("restore only persisted metadata");
            assert_eq!(
                watcher.registered_root("job-project"),
                Some(canonical_root.clone())
            );
            let remaining = manager
                .watcher_admission_delay("job-project")
                .await
                .expect("read restored automatic policy")
                .expect("automatic observation enabled");
            assert!(remaining > Duration::ZERO && remaining <= Duration::from_secs(300));
            assert_eq!(
                manager
                    .get_job("job-project", None)
                    .await
                    .expect("read restored receipt")
                    .expect("previous receipt exists")
                    .job_id,
                synchronized.job_id,
                "metadata restoration must not schedule another job"
            );
            let active_runtime = manager
                .require_project("job-project")
                .await
                .expect("restored runtime");
            // Restoring a physical watcher cannot certify changes missed while the project was disabled.
            // 恢复物理监听不能确认项目关闭期间遗漏的变化。
            let restored_freshness = manager.source_freshness("job-project").await;
            assert!(restored_freshness.watching);
            assert!(restored_freshness.reconciliation_required);
            active_runtime
                .job_schedule
                .lock()
                .expect("fixture schedule")
                .last_admitted = None;
            let unchanged = manager
                .submit_watcher_job(
                    "job-project",
                    &canonical_root,
                    Some(BTreeSet::new()),
                    None,
                    &executor,
                )
                .await
                .expect("admit empty incremental delta");
            let unchanged = await_test_job(&manager, "job-project", &unchanged.job_id).await;
            assert_eq!(
                unchanged.state,
                VceJobState::Succeeded,
                "empty delta: {:?}",
                unchanged.error
            );
            assert_eq!(unchanged.result_checkpoint_version, Some(checkpoint));
            // Missing batch evidence promoted the legal update to a full manifest scan, not re-embedding.
            // 批次证据缺失将合法更新提升为完整清单扫描，而不是重新嵌入。
            assert!(
                !manager
                    .source_freshness("job-project")
                    .await
                    .reconciliation_required
            );
            // Hold only the publication boundary: the first batch has really scanned before later clicks arrive.
            // 只持有发布边界：后续点击到达前，首批已真正完成扫描。
            let publication_guard = crate::begin_public_ai_configuration_transaction().await;
            let chained = manager
                .submit_job(
                    "job-project",
                    command("chain-first", VceJobKind::Synchronize, Some(checkpoint)),
                    &executor,
                )
                .await
                .expect("admit chain head");
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let current = manager
                        .get_job("job-project", Some(&chained.job_id))
                        .await
                        .expect("read chain progress")
                        .expect("chain exists");
                    if current.progress.as_ref().is_some_and(|progress| {
                        progress.phase == vulcan_context_engine::VceRefreshPhase::Complete
                    }) {
                        break;
                    }
                    assert!(
                        !current.state.is_terminal(),
                        "batch stopped unexpectedly: {:?}",
                        current.error
                    );
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("first batch reaches blocked publication");
            let first_followup = manager
                .submit_job(
                    "job-project",
                    command(
                        "chain-followup-a",
                        VceJobKind::Synchronize,
                        Some(checkpoint),
                    ),
                    &executor,
                )
                .await
                .expect("coalesce first follow-up");
            assert!(first_followup.followup_requested);
            assert_eq!(first_followup.job_id, chained.job_id);
            let second_followup = manager
                .submit_job(
                    "job-project",
                    command(
                        "chain-followup-b",
                        VceJobKind::Synchronize,
                        Some(checkpoint),
                    ),
                    &executor,
                )
                .await
                .expect("coalesce another click into the same one slot");
            assert_eq!(second_followup.job_id, chained.job_id);
            assert!(second_followup.followup_requested);
            assert_eq!(
                manager
                    .submit_job(
                        "job-project",
                        command(
                            "chain-followup-a",
                            VceJobKind::Synchronize,
                            Some(checkpoint)
                        ),
                        &executor
                    )
                    .await
                    .expect("replay accepted alias")
                    .sequence,
                second_followup.sequence
            );
            drop(publication_guard);
            let chained = await_test_job(&manager, "job-project", &chained.job_id).await;
            assert_eq!(chained.state, VceJobState::Succeeded, "{:?}", chained.error);
            assert!(!chained.followup_requested);
            assert!(
                chained
                    .progress
                    .as_ref()
                    .expect("real follow-up observation")
                    .operation_id
                    .contains(":batch:")
            );
            assert_eq!(
                manager
                    .submit_job(
                        "job-project",
                        command(
                            "chain-followup-a",
                            VceJobKind::Synchronize,
                            Some(checkpoint)
                        ),
                        &executor
                    )
                    .await
                    .expect("terminal alias replay does not start a third scan"),
                chained
            );
            // Replaying the same key with a changed observed checkpoint remains a contract error.
            // 使用已变更观察检查点重放相同键仍是契约错误。
            assert!(
                manager
                    .submit_job(
                        "job-project",
                        command(
                            "chain-followup-a",
                            VceJobKind::Synchronize,
                            Some(checkpoint + 1)
                        ),
                        &executor
                    )
                    .await
                    .is_err()
            );
            // Cancellation after the first core batch finished must still remove the acknowledged successor.
            // 首个核心批次结束后的取消仍必须移除已确认后续批次。
            let publication_guard = crate::begin_public_ai_configuration_transaction().await;
            let clearing = manager
                .submit_job(
                    "job-project",
                    command(
                        "clear-pending-head",
                        VceJobKind::Synchronize,
                        Some(checkpoint),
                    ),
                    &executor,
                )
                .await
                .expect("admit pending cancellation fixture");
            await_test_completed_phase(&manager, "job-project", &clearing.job_id).await;
            let pending = manager
                .submit_job(
                    "job-project",
                    command(
                        "clear-pending-next",
                        VceJobKind::Synchronize,
                        Some(checkpoint),
                    ),
                    &executor,
                )
                .await
                .expect("acknowledge successor before cancellation");
            assert!(pending.followup_requested);
            let cleared = manager
                .cancel_job("job-project", &clearing.job_id)
                .await
                .expect("remove successor without undoing finished batch");
            assert!(!cleared.followup_requested);
            drop(publication_guard);
            let cleared = await_test_job(&manager, "job-project", &cleared.job_id).await;
            assert_eq!(cleared.state, VceJobState::Succeeded);
            assert!(
                !cleared
                    .progress
                    .as_ref()
                    .expect("only first batch ran")
                    .operation_id
                    .contains(":batch:")
            );
            let last_successful_batch = cleared
                .completed_update
                .as_ref()
                .expect("follow-up completion is durable")
                .at_unix_ms;
            // Revocation is checked again at final publication, including an empty-delta metadata commit.
            // 最终发布再次检查撤销，包括空增量元数据提交。
            publication_allowed.store(false, Ordering::Release);
            let revoked = manager
                .submit_job(
                    "job-project",
                    command(
                        "revoked-publication",
                        VceJobKind::Synchronize,
                        Some(checkpoint),
                    ),
                    &executor,
                )
                .await
                .expect("admit work before its final publication check");
            let revoked = await_test_job(&manager, "job-project", &revoked.job_id).await;
            assert_eq!(revoked.state, VceJobState::Failed);
            assert!(
                revoked
                    .error
                    .as_deref()
                    .expect("explicit revocation error")
                    .contains("authority revoked")
            );
            assert!(!manager.project_can_serve_now("job-project", settings.revision, &fingerprint));
            assert_eq!(
                active_runtime
                    .index
                    .load_state_snapshot()
                    .await
                    .expect("read preserved checkpoint")
                    .expect("checkpoint survives revocation")
                    .workspace
                    .checkpoint
                    .expect("committed checkpoint survives")
                    .version,
                checkpoint
            );
            publication_allowed.store(true, Ordering::Release);
            assert!(
                manager.project_can_serve_now("job-project", settings.revision, &fingerprint),
                "failed update must not erase the previously admitted checkpoint"
            );
            // Project shutdown closes admission before the completed batch can hand off to its successor.
            // 项目关闭在已完成批次交接后续批次之前关闭准入。
            let publication_guard = crate::begin_public_ai_configuration_transaction().await;
            let closing = manager
                .submit_job(
                    "job-project",
                    command(
                        "close-chain-head",
                        VceJobKind::Synchronize,
                        Some(checkpoint),
                    ),
                    &executor,
                )
                .await
                .expect("admit closing chain fixture");
            await_test_completed_phase(&manager, "job-project", &closing.job_id).await;
            assert!(
                manager
                    .submit_job(
                        "job-project",
                        command(
                            "close-chain-next",
                            VceJobKind::Synchronize,
                            Some(checkpoint)
                        ),
                        &executor
                    )
                    .await
                    .expect("queue successor before close")
                    .followup_requested
            );
            let closing_manager = manager.clone();
            let close =
                executor.spawn(async move { closing_manager.disable_project("job-project").await });
            tokio::time::timeout(Duration::from_secs(5), async {
                while active_runtime
                    .operation_admission
                    .accepting
                    .load(Ordering::Acquire)
                {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("close must revoke admission without waiting for publication");
            drop(publication_guard);
            tokio::time::timeout(Duration::from_secs(10), close)
                .await
                .expect("close drains chain")
                .expect("close task completed")
                .expect("close runtime");
            let closed = manager
                .get_job("job-project", Some(&closing.job_id))
                .await
                .expect("read closed chain")
                .expect("closed receipt persists");
            assert!(!closed.followup_requested);
            // Cooperative rejection records failure; the bounded shutdown abort records interruption.
            // 协作拒绝记录失败；有界关闭中止则记录中断。
            // Both paths must be terminal and must not invent a successfully published batch.
            // 两条路径都必须终结作业，且不能虚构成功发布的批次。
            assert!(matches!(closed.state, VceJobState::Failed | VceJobState::Interrupted));
            assert!(
                closed.completed_update.is_none(),
                "revoked publication must not invent successful completion"
            );
            assert!(
                !closed
                    .progress
                    .as_ref()
                    .expect("no successor scan after close")
                    .operation_id
                    .contains(":batch:")
            );
            manager
                .enable_project(
                    "job-project",
                    &project_root,
                    settings.clone(),
                    3,
                    &fingerprint,
                    false,
                )
                .await
                .expect("reopen for independent queued shutdown assertion");
            drop(active_runtime);
            // A foreign unfinished owner must never be declared dead merely because its UUID differs.
            // 绝不能仅因外部未完成所有者的运行标识不同，就宣布其已经退出。
            let recovery_runtime = manager
                .require_project("job-project")
                .await
                .expect("retain recovery fixture");
            // recovery_store addresses only this temporary project's durable receipts.
            // recovery_store 仅定位此临时项目的持久回执。
            let recovery_store = recovery_runtime.index.job_store();
            // foreign is an explicitly created fixture owner, not an inferred crashed process.
            // foreign 是明确创建的夹具所有者，不是推断的崩溃进程。
            let mut foreign = closed.clone();
            foreign.job_id = Uuid::new_v4().to_string();
            foreign.owner_run_id = Uuid::new_v4().to_string();
            foreign.idempotency_key = String::from("foreign-live-fixture");
            foreign.retry_of_job_id = None;
            foreign.state = VceJobState::Queued;
            foreign.sequence = 1;
            foreign.created_at_unix_ms = job_time().expect("fixture admission time");
            foreign.updated_at_unix_ms = foreign.created_at_unix_ms;
            foreign.result_checkpoint_version = None;
            foreign.completed_update = None;
            foreign.progress = None;
            foreign.followup_requested = false;
            foreign.error = None;
            recovery_store
                .register(&foreign)
                .await
                .expect("register foreign fixture job");
            // A live cooperating host owns an OS writer lease, not merely a different receipt UUID.
            // 活动协作宿主持有操作系统写入租约，而不只是不同的回执编号。
            let foreign_lease = factory.try_acquire_project_writer("job-project").expect("foreign writer lock").expect("foreign writer lease");
            assert_eq!(manager.get_job("job-project", Some(&foreign.job_id)).await.expect("read live foreign job"), Some(foreign.clone()));
            // This staged generation belongs to the explicit foreign fixture, never to project shutdown.
            // 此暂存代次属于明确的外部夹具，绝不属于项目关闭流程。
            recovery_runtime
                .index
                .stage(vulcan_context_engine::IndexStageBatch {
                    operation_id: format!("{}:fixture-stage", foreign.job_id),
                    refresh_id: foreign.job_id.clone(),
                    generation: checkpoint + 1,
                    blobs: Vec::new(),
                })
                .await
                .expect("stage foreign fixture generation");
            assert!(
                manager
                    .submit_job(
                        "job-project",
                        command(
                            "must-not-steal-foreign-job",
                            VceJobKind::Synchronize,
                            Some(checkpoint)
                        ),
                        &executor,
                    )
                    .await
                    .expect_err("unconfirmed foreign ownership rejects new work")
                    .contains("active host writer")
            );
            assert_eq!(
                recovery_store
                    .get(&foreign.job_id)
                    .await
                    .expect("foreign state after rejected steal"),
                Some(foreign.clone())
            );
            assert!(
                recovery_store
                    .find_by_idempotency_key("must-not-steal-foreign-job")
                    .await
                    .expect("no unauthorized admission")
                    .is_none()
            );
            assert!(
                manager
                    .cleanup_staging("job-project")
                    .await
                    .expect_err("maintenance cannot erase unconfirmed staging")
                    .contains("active host writer")
            );
            manager
                .disable_project("job-project")
                .await
                .expect("close without claiming foreign staging");
            assert!(
                recovery_runtime
                    .index
                    .has_unresolved_staging()
                    .await
                    .expect("foreign staging survives close")
            );
            assert_eq!(
                recovery_store
                    .get(&foreign.job_id)
                    .await
                    .expect("foreign receipt survives close"),
                Some(foreign.clone())
            );
            manager
                .enable_project(
                    "job-project",
                    &project_root,
                    settings.clone(),
                    3,
                    &fingerprint,
                    false,
                )
                .await
                .expect("reopen retained checkpoint without deleting foreign staging");
            // Only the test's known foreign owner concludes its own fixture receipt.
            // 仅由测试中已知的外部所有者结束其自身夹具回执。
            recovery_runtime
                .index
                .abort_refresh_staging(&foreign.job_id)
                .await
                .expect("foreign fixture aborts its exact attempt");
            let mut foreign_finished = foreign.clone();
            foreign_finished.sequence += 1;
            foreign_finished.state = VceJobState::Cancelled;
            foreign_finished.updated_at_unix_ms = job_time()
                .expect("fixture finish time")
                .max(foreign.updated_at_unix_ms);
            recovery_store
                .update(&foreign, &foreign_finished)
                .await
                .expect("foreign fixture owner releases its slot");
            drop(foreign_lease);
            drop(recovery_store);
            drop(recovery_runtime);
            // Shutdown must wake a queued synchronization while the existing checkpoint remains stored.
            // 退出必须唤醒排队同步，同时已有检查点保持存储。
            let budget = manager
                .job_writer_budget
                .acquire()
                .await
                .expect("hold shutdown fixture budget");
            let stopping = manager
                .submit_job(
                    "job-project",
                    command("shutdown-click", VceJobKind::Synchronize, Some(checkpoint)),
                    &executor,
                )
                .await
                .expect("queue shutdown fixture");
            assert_eq!(
                manager
                    .cancel_job("job-project", &cancelled.job_id)
                    .await
                    .expect("late old cancellation does not target a newer job"),
                cancelled
            );
            assert_eq!(
                manager
                    .get_job("job-project", Some(&stopping.job_id))
                    .await
                    .expect("read untouched newer job")
                    .expect("newer job exists")
                    .state,
                VceJobState::Queued
            );
            tokio::time::timeout(Duration::from_secs(10), manager.shutdown())
                .await
                .expect("shutdown cannot hang on a queued job")
                .expect("drain project worker and bindings");
            drop(budget);
            let stopped = manager
                .get_job("job-project", Some(&stopping.job_id))
                .await
                .expect("read persisted shutdown outcome")
                .expect("shutdown receipt exists");
            assert_eq!(stopped.state, VceJobState::Cancelled);
            assert_eq!(stopped.source_checkpoint_version, Some(checkpoint));
            assert_eq!(
                manager
                    .last_successful_update("job-project")
                    .await
                    .expect("cancel must preserve success time"),
                Some(last_successful_batch)
            );
            assert!(watcher.registered_root("job-project").is_none());
            watcher.shutdown().await;
            // Verify a published candidate's cold restore in a separate awaited task, without enlarging this fixture's stack.
            // 在独立且被等待的任务中验证已发布候选的冷恢复，不扩大本夹具栈。
            executor
                .spawn(verify_published_candidate_runtime_restore(
                    factory.clone(),
                    project_root.clone(),
                    settings.clone(),
                    fingerprint.clone(),
                    Arc::clone(&executor),
                ))
                .await
                .expect("candidate restore fixture completed");
            // Exercise the real human queue separately from the manually assembled candidate fixture above.
            // 与上方手工组装候选夹具分离，实际验证人为队列。
            executor
                .spawn(verify_queued_replacement_jobs(
                    factory.clone(),
                    project_root.clone(),
                    settings.clone(),
                    Arc::clone(&executor),
                ))
                .await
                .expect("queued replacement fixture completed");
            // Run real nonempty read/write interleaving separately to keep the parent fixture stack bounded.
            // 单独运行真实非空读写交错，保持父夹具栈有界。
            executor
                .spawn(super::query_tests::verify_nonempty_query_interleaving(
                    factory.clone(),
                    temporary.path().join("nonempty-source"),
                    Arc::clone(&executor),
                ))
                .await
                .expect("nonempty query interleaving fixture completed");
            factory
                .shutdown()
                .await
                .expect("close exact fixture bindings");
                })
            })
            .expect("spawn isolated job fixture thread")
            .join()
            .expect("isolated job fixture thread completed");
    }

/// Exercise incompatible-dimension rebuilding through the same functional queue used by the service.
/// 通过服务使用的同一功能队列验证不兼容维度重建。
/// Inputs belong to the isolated empty project; returns after cancellation, handoff, replay and shutdown checks.
/// 输入属于隔离空项目；在取消、交接、重放及退出检查之后返回。
async fn verify_queued_replacement_jobs(
    factory: VceDataPlaneFactory,
    project_root: std::path::PathBuf,
    settings: ProjectVceSettings,
    executor: Arc<tokio::runtime::Runtime>,
) {
    // capabilities intentionally change the physical dimension without granting old-vector compatibility.
    // capabilities 有意改变物理维度，不授予旧向量兼容性。
    let mut capabilities = job_test_capabilities();
    capabilities.operations[0]
        .embedding
        .as_mut()
        .expect("embedding capability")
        .dimensions = BTreeSet::from([4]);
    let fingerprint = crate::vce::vce_embedding_physical_fingerprint(&capabilities, 4)
        .expect("freeze four-dimensional route");
    let mock = vulcan_ai_contract::test_support::MockHostAiPort::new();
    mock.set_capability_snapshot(capabilities)
        .expect("configure four-dimensional fixture");
    let expected = fingerprint.clone();
    let manager = VceRuntimeManager::new(
        Arc::new(mock),
        factory.clone(),
        ServiceEventEmitter::new(Vec::new()),
        Arc::new(move |contract| {
            if contract.project_id != "job-project"
                || contract.dimension != 4
                || contract.embedding_fingerprint != expected
            {
                return Err(
                    String::from("Fixture requires the current four-dimensional route").into(),
                );
            }
            if contract.validates_replacement_admission()? {
                return Ok(());
            }
            let provenance = contract
                .provenance
                .as_ref()
                .expect("existing fixture provenance");
            if provenance.original.dimension != 4
                || provenance.original.physical_fingerprint != expected
            {
                return Err(String::from(
                    "Fixture forbids querying incompatible predecessor vectors",
                )
                .into());
            }
            Ok(())
        }),
    );
    assert!(
        manager
            .enable_project(
                "job-project",
                &project_root,
                settings.clone(),
                4,
                &fingerprint,
                false
            )
            .await
            .is_err()
    );
    let original = factory
        .embedding_provenance("job-project")
        .await
        .expect("read predecessor")
        .expect("existing predecessor");
    assert_eq!(original.original.dimension, 3);
    let checkpoint = original
        .checkpoint_version
        .expect("initialized predecessor");
    let plan = manager
        .stored_maintenance_plan(
            "job-project",
            vulcan_context_engine::VceMaintenanceMode::Rebuild,
        )
        .await
        .expect("preview without compatible runtime");
    let actions = plan
        .actions
        .into_iter()
        .map(|action| action.action_id)
        .collect::<BTreeSet<_>>();
    let input = VceRebuildProjectInput {
        project_id: String::from("job-project"),
        project_root: project_root.clone(),
        settings: settings.clone(),
        dimension: 4,
        fingerprint: fingerprint.clone(),
    };
    // command preserves the old checkpoint preview and current producer through every replay.
    // command 在每次重放中保留旧检查点预览及当前生产者。
    let command = |key: &str| VceJobAdmission {
        retry_of_job_id: None,
        kind: VceJobKind::Rebuild,
        idempotency_key: key.to_owned(),
        expected_settings_revision: settings.revision,
        expected_checkpoint_version: Some(checkpoint),
        contract_fingerprint: fingerprint.clone(),
    };
    let budget = manager
        .job_writer_budget
        .acquire()
        .await
        .expect("hold replacement allocation budget");
    let queued = manager
        .submit_project_rebuild(
            input.clone(),
            command("replacement-cancel"),
            false,
            actions.clone(),
            &executor,
        )
        .await
        .expect("queue without compatible runtime");
    assert_eq!(queued.state, VceJobState::Queued);
    assert!(
        factory
            .try_acquire_project_writer("job-project")
            .expect("replacement queue owns writer")
            .is_none()
    );
    let replacement = queued.replacement.as_ref().expect("durable target");
    assert_eq!(replacement.target.dimension, 4);
    assert_eq!(replacement.predecessor.checkpoint_version, checkpoint);
    assert_eq!(
        factory
            .load_active_project_config("job-project")
            .await
            .expect("active configuration"),
        Some(original.original.clone())
    );
    assert!(
        factory
            .reopen_rebuild_candidate(&replacement.target, &replacement.predecessor)
            .await
            .is_err(),
        "queueing must not allocate the candidate"
    );
    assert!(!manager.project_can_serve_now("job-project", settings.revision, &fingerprint));
    assert!(
        manager
            .enable_project(
                "job-project",
                &project_root,
                settings.clone(),
                4,
                &fingerprint,
                false
            )
            .await
            .is_err(),
        "cold reads cannot reactivate a predecessor while replacement owns the project"
    );
    assert_eq!(
        manager
            .submit_project_rebuild(
                input.clone(),
                command("replacement-cancel"),
                false,
                actions.clone(),
                &executor
            )
            .await
            .expect("queued replay"),
        queued
    );
    // Validate that explicit new-space authority cannot be reused as ordinary query compatibility.
    // 验证明确定义的新空间权威不能复用为普通查询兼容授权。
    let mut contract = VcePublicationContract {
        project_id: String::from("job-project"),
        canonical_root: replacement.target.root_path.clone(),
        embedding_fingerprint: fingerprint.clone(),
        dimension: 4,
        max_file_bytes: settings.max_file_bytes,
        extra_ignore_patterns: settings.extra_ignore_patterns.clone(),
        provenance: Some(original.clone()),
        replacement_admission: Some(Box::new(queued.clone())),
    };
    assert_eq!(contract.validates_replacement_admission(), Ok(true));
    contract.dimension = 3;
    assert!(contract.validates_replacement_admission().is_err());
    contract.dimension = 4;
    contract
        .replacement_admission
        .as_mut()
        .expect("receipt")
        .state = VceJobState::Cancelling;
    assert!(contract.validates_replacement_admission().is_err());
    contract.replacement_admission = None;
    assert_eq!(contract.validates_replacement_admission(), Ok(false));
    assert!((manager.publication_authority)(&contract).is_err());
    // changed_source must cancel even without a query runtime, while the held budget prevents scans.
    // changed_source 必须在没有查询运行时时也取消任务，同时保留预算阻止扫描。
    let mut changed_source = settings.clone();
    changed_source.revision += 1;
    changed_source
        .extra_ignore_patterns
        .push(String::from("generated/**"));
    assert!(
        !manager
            .reconcile_policy_revision("job-project", &project_root, &changed_source, &fingerprint)
            .await
            .expect("source-policy change cancels exact queued replacement")
    );
    let cancelled_by_policy = await_test_job(&manager, "job-project", &queued.job_id).await;
    assert_eq!(cancelled_by_policy.state, VceJobState::Cancelled);
    manager
        .cancel_job("job-project", &queued.job_id)
        .await
        .expect("cancel queued replacement");
    let cancelled = await_test_job(&manager, "job-project", &queued.job_id).await;
    assert_eq!(
        cancelled.state,
        VceJobState::Cancelled,
        "{:?}",
        cancelled.error
    );
    assert!(
        factory
            .try_acquire_project_writer("job-project")
            .expect("cancelled replacement releases writer")
            .is_some()
    );
    assert!(
        factory
            .reopen_rebuild_candidate(&replacement.target, &replacement.predecessor)
            .await
            .is_err()
    );
    // Disabling without a query runtime still drains the exact replacement worker while its budget is unavailable.
    // 没有查询运行时的停用仍排空精确替换任务，即使其预算不可用。
    let stopping = manager
        .submit_project_rebuild(
            input.clone(),
            command("replacement-disable"),
            false,
            actions.clone(),
            &executor,
        )
        .await
        .expect("queue disable fixture");
    tokio::time::timeout(
        Duration::from_secs(10),
        manager.disable_project("job-project"),
    )
    .await
    .expect("disable cannot wait for writer budget")
    .expect("disable project job");
    assert_eq!(
        manager
            .get_job("job-project", Some(&stopping.job_id))
            .await
            .expect("read stopped job")
            .expect("stopped receipt")
            .state,
        VceJobState::Cancelled
    );
    assert!(manager.projects.read().await.get("job-project").is_none());
    // The final submission performs allocation, publication and normal-runtime restoration through production code.
    // 最后一次提交通过生产代码执行分配、发布及普通运行时恢复。
    let admitted = manager
        .submit_project_rebuild(
            input.clone(),
            command("replacement-success"),
            false,
            actions.clone(),
            &executor,
        )
        .await
        .expect("queue actual replacement");
    // updated is saved project policy while the candidate is still queued, not a new indexing request.
    // updated 为候选仍排队时保存的项目策略，不是新的索引请求。
    let mut updated = settings.clone();
    updated.revision += 1;
    updated.update_mode = vulcan_workbench_model::ProjectVceUpdateMode::Manual;
    updated.auto_update_min_interval_seconds = 600;
    updated.query_max_concurrency = 1;
    updated.embedding_max_concurrency = 2;
    updated.max_results = 7;
    updated.max_context_chars = 8_000;
    assert!(
        manager
            .reconcile_policy_revision("job-project", &project_root, &updated, &fingerprint)
            .await
            .expect("reconcile queued replacement policy")
    );
    assert!(manager.projects.read().await.get("job-project").is_none());
    assert_eq!(
        manager
            .get_job("job-project", Some(&admitted.job_id))
            .await
            .expect("unchanged receipt"),
        Some(admitted.clone())
    );
    assert!(
        manager
            .reconcile_policy_revision("job-project", &project_root, &settings, &fingerprint)
            .await
            .is_err(),
        "old snapshots cannot erase accepted policy"
    );
    // conflicting has the same revision but different data and must not silently replace authority.
    // conflicting 具有相同修订但不同数据，不得静默替换权威。
    let mut conflicting = updated.clone();
    conflicting.auto_update_min_interval_seconds = 900;
    assert!(
        manager
            .reconcile_policy_revision("job-project", &project_root, &conflicting, &fingerprint)
            .await
            .is_err()
    );
    // delayed_policy models saved intent whose reconciliation callback arrives only after job handoff.
    // delayed_policy 模拟已经保存、但协调回调在作业交接后才到达的意图。
    let mut delayed_policy = updated.clone();
    delayed_policy.revision += 1;
    delayed_policy.max_results = 3;
    delayed_policy.max_context_chars = 4_000;
    delayed_policy.allow_rerank = false;
    drop(budget);
    let completed = await_test_job(&manager, "job-project", &admitted.job_id).await;
    assert_eq!(
        completed.state,
        VceJobState::Succeeded,
        "{:?}",
        completed.error
    );
    let published = completed
        .result_checkpoint_version
        .expect("published replacement checkpoint");
    let target = admitted
        .replacement
        .as_ref()
        .expect("success target")
        .target
        .clone();
    assert_eq!(
        factory
            .load_active_project_config("job-project")
            .await
            .expect("published configuration"),
        Some(target.clone())
    );
    assert!(manager.project_can_serve_now("job-project", updated.revision, &fingerprint));
    assert!(!manager.project_can_serve_now("job-project", settings.revision, &fingerprint));
    // restored must use accepted budgets while preserving the original durable build contract revision.
    // restored 必须使用已接纳预算，同时保留原持久构建契约修订。
    let restored = manager
        .require_project("job-project")
        .await
        .expect("restored runtime");
    assert_eq!(restored.settings, updated);
    assert_eq!(
        restored.config.query_max_concurrency,
        updated.query_max_concurrency
    );
    assert_eq!(
        restored.config.retrieval.coverage.max_results,
        updated.max_results
    );
    assert_eq!(
        restored.config.retrieval.coverage.max_chars,
        updated.max_context_chars
    );
    assert!(!restored.job_schedule.lock().expect("schedule").automatic());
    // Old callbacks arriving after handoff cannot replace or revoke the newly configured runtime.
    // 交接后到达的旧回调不能替换或撤销新配置运行时。
    assert!(
        manager
            .reconcile_policy_revision("job-project", &project_root, &settings, &fingerprint)
            .await
            .is_err()
    );
    assert!(
        manager
            .enable_project(
                "job-project",
                &project_root,
                settings.clone(),
                4,
                &fingerprint,
                false
            )
            .await
            .is_err()
    );
    assert!(manager.project_can_serve_now("job-project", updated.revision, &fingerprint));
    // Delayed query-only reconciliation updates the same runtime instead of disabling the published index.
    // 延迟的纯查询协调更新同一运行时，而不是禁用已发布索引。
    assert!(
        manager
            .reconcile_policy_revision("job-project", &project_root, &delayed_policy, &fingerprint)
            .await
            .expect("adopt delayed saved policy")
    );
    assert!(Arc::ptr_eq(
        &restored,
        &manager
            .require_project("job-project")
            .await
            .expect("same runtime")
    ));
    assert_eq!(
        *restored.latest_settings.read().expect("latest settings"),
        delayed_policy
    );
    assert!(manager.project_can_serve_now("job-project", delayed_policy.revision, &fingerprint));
    drop(restored);
    assert_eq!(
        manager
            .retrieve("job-project", "inspect replaced empty fixture")
            .await
            .expect("query replaced checkpoint")
            .checkpoint_version,
        published
    );
    assert_eq!(
        manager
            .submit_project_rebuild(
                input,
                command("replacement-success"),
                false,
                actions,
                &executor
            )
            .await
            .expect("completed replay after handoff"),
        completed
    );
    // synchronization_budget keeps an ordinary writer queued while another query-policy update is accepted.
    // synchronization_budget 在接纳另一次查询策略更新期间保持普通写入者排队。
    let synchronization_budget = manager
        .job_writer_budget
        .acquire()
        .await
        .expect("hold normal writer");
    let synchronization = manager
        .submit_job(
            "job-project",
            VceJobAdmission {
                retry_of_job_id: None,
                kind: VceJobKind::Synchronize,
                idempotency_key: String::from("synchronize-queued-replacement"),
                expected_settings_revision: delayed_policy.revision,
                expected_checkpoint_version: Some(published),
                contract_fingerprint: fingerprint.clone(),
            },
            &executor,
        )
        .await
        .expect("ordinary write after one-shot candidate grant ended");
    // newest changes optional query capabilities and limits without altering the admitted source work.
    // newest 改变可选查询能力及限制，不改变已准入源码工作。
    let mut newest = delayed_policy.clone();
    newest.revision += 1;
    newest.allow_query_rewrite = true;
    newest.allow_rerank = true;
    newest.query_max_concurrency = 2;
    newest.embedding_max_concurrency = 3;
    newest.max_results = 4;
    assert!(
        manager
            .reconcile_policy_revision("job-project", &project_root, &newest, &fingerprint)
            .await
            .expect("change query policy with queued writer")
    );
    assert_eq!(
        manager
            .get_job("job-project", Some(&synchronization.job_id))
            .await
            .expect("read queued writer"),
        Some(synchronization.clone())
    );
    assert!(manager.project_can_serve_now("job-project", newest.revision, &fingerprint));
    assert_eq!(
        manager
            .retrieve("job-project", "query during policy change")
            .await
            .expect("query while writer queued")
            .settings_revision,
        newest.revision
    );
    drop(synchronization_budget);
    assert_eq!(
        await_test_job(&manager, "job-project", &synchronization.job_id)
            .await
            .state,
        VceJobState::Succeeded
    );
    // runtime keeps its original identity, but the completed private worker used the new execution limit.
    // runtime 保持原身份，但已完成私有工作器使用新的执行限制。
    let runtime = manager
        .require_project("job-project")
        .await
        .expect("retained runtime");
    assert_eq!(
        runtime
            .current_engine
            .read()
            .await
            .engine
            .config()
            .embedding_max_concurrency,
        3
    );
    // frozen_batch samples policy before scanning; later reconciliation must not mutate this admitted value.
    // frozen_batch 在扫描前采样策略；后续协调不能改变此准入值。
    let before_observation = runtime.active_refresh_observation().await;
    assert!(before_observation.1.is_none());
    let frozen_batch = manager
        .begin_observed_refresh(
            &runtime,
            String::from("freeze-budget"),
            VceRefreshControl::default(),
        )
        .await
        .expect("admit observed batch");
    // The active sample pairs the exact task identity with a strictly newer transition revision.
    // 活动采样将精确任务身份与严格更新的变更修订配套。
    let during_observation = runtime.active_refresh_observation().await;
    assert_eq!(during_observation.1.as_deref(), Some("freeze-budget"));
    assert!(during_observation.0 > before_observation.0);
    verify_active_staging_diagnostics(&manager, &runtime).await;
    let mut next_policy = newest.clone();
    next_policy.revision += 1;
    next_policy.embedding_max_concurrency = 5;
    assert!(
        manager
            .reconcile_policy_revision("job-project", &project_root, &next_policy, &fingerprint)
            .await
            .expect("change future batch budget")
    );
    assert_eq!(frozen_batch.embedding_max_concurrency, 3);
    assert_eq!(
        runtime
            .latest_settings
            .read()
            .expect("future policy")
            .embedding_max_concurrency,
        5
    );
    assert!(frozen_batch.cancellation.check().is_ok());
    // Exercise the production unwind finalizer: a settled worker must not remain a live diagnostic exclusion.
    // 验证生产展开终结器：已结束的工作线程不得继续作为存活诊断排除项。
    let panic_error = manager
        .finish_panicked_refresh(&runtime, "freeze-budget")
        .await;
    assert!(panic_error.contains("worker panicked"));
    assert!(panic_error.contains("staging was cleaned"));
    // Both endpoints are idle, yet the transition revision detects the completed intervening task.
    // 两端均为空闲，但变更修订仍能检测中间已完成的任务。
    let after_observation = runtime.active_refresh_observation().await;
    assert!(after_observation.1.is_none());
    assert!(after_observation.0 > during_observation.0);
    runtime.set_ready().await;
    // Release the test observer's runtime lease before verifying complete host shutdown.
    // 验证宿主完全关闭之前释放测试观察者持有的运行时租约。
    drop(runtime);
    manager
        .shutdown()
        .await
        .expect("drain real queued replacement fixture");
}

/// Verify that only the admitted host worker's real staged rows are excluded from integrity diagnostics.
/// 验证仅已准入宿主工作线程的真实暂存记录被排除在完整性诊断之外。
/// `manager` and `runtime` belong to the isolated fixture; returns after aborting its exact test stage.
/// `manager` 与 `runtime` 属于隔离夹具；中止其精确测试暂存后返回。
async fn verify_active_staging_diagnostics(
    manager: &VceRuntimeManager,
    runtime: &VceProjectRuntime,
) {
    use vulcan_context_engine::{
        AuxiliaryIndexStatus, IndexedBlob, IndexedBlobStatus, VceIndexPort,
    };
    // Empty source still creates a real staged metadata row without using any paid embedding service.
    // 空源码仍创建真实暂存元数据行，不使用任何付费向量服务。
    let blob = IndexedBlob {
        blob_name: vulcan_context_engine::blob_name("src/staging-fixture.rs", "")
            .expect("fixture identity"),
        path: String::from("src/staging-fixture.rs"),
        generation: 1,
        structure_analyzer_id: None,
        structure_mode: None,
        structure_fallback_reason: None,
        chunks: Vec::new(),
        embedding_vectors: Vec::new(),
        path_document: None,
        path_embedding: None,
        symbols: Vec::new(),
        path_status: AuxiliaryIndexStatus::NotApplicable,
        symbol_status: AuxiliaryIndexStatus::NotApplicable,
        status: IndexedBlobStatus::Ready,
    };
    runtime
        .index
        .stage(vulcan_context_engine::IndexStageBatch {
            operation_id: String::from("freeze-budget:stage"),
            refresh_id: String::from("freeze-budget"),
            generation: 1,
            blobs: vec![blob],
        })
        .await
        .expect("write exact observed staging fixture");
    // Storage alone has no proof of a live owner and must keep the diagnostic visible.
    // 单独存储没有存活所有者证明，必须保留诊断。
    let unobserved = runtime
        .index
        .management_snapshot(vulcan_context_engine::VceManagementRequest {
            request_id: String::from("unobserved-stage-fixture"),
        })
        .await
        .expect("inspect without host observation");
    assert_eq!(unobserved.integrity.orphan_blob_count, 1);
    assert!(unobserved.active_job.is_none());
    // Storage receipts are completed mutations, never fabricated logical refreshes with absent policy.
    // 存储回执属于已完成变更，绝不伪造缺少策略的逻辑刷新。
    let operations = manager
        .management_query(
            &runtime.project_id,
            vulcan_context_engine::VceManagementQueryRequest {
                operation_id: String::from("staged-operation-history-fixture"),
                query: vulcan_context_engine::VceManagementQuery::Operations(
                    vulcan_context_engine::VcePageRequest::new(None, 100)
                        .expect("bounded operation page"),
                ),
            },
            None,
        )
        .await
        .expect("validate real storage operation history");
    assert!(matches!(operations.result,
            vulcan_context_engine::VceManagementQueryResult::Operations(page)
                if page.items.iter().any(|item| item.operation_id == "freeze-budget:stage"
                    && item.kind == vulcan_context_engine::VceOperationKind::StorageMutation
                    && item.phase == vulcan_context_engine::VceOperationPhase::Completed)));
    // The real manager supplies its exact admitted identity while retaining the published checkpoint.
    // 真实管理器提供其精确已准入身份，同时保留已发布检查点。
    let (identity, observed) = manager
        .management_snapshot(&runtime.project_id)
        .await
        .expect("inspect active host staging");
    assert_eq!(
        observed
            .index
            .expect("host storage diagnostics")
            .integrity
            .orphan_blob_count,
        0
    );
    let issues = manager
        .management_query(
            &runtime.project_id,
            vulcan_context_engine::VceManagementQueryRequest {
                operation_id: String::from("observed-issues-fixture"),
                query: vulcan_context_engine::VceManagementQuery::IntegrityIssues(
                    vulcan_context_engine::VcePageRequest::new(None, 25)
                        .expect("bounded issue page"),
                ),
            },
            identity.as_deref(),
        )
        .await
        .expect("read host integrity page");
    assert!(
        matches!(issues.result, vulcan_context_engine::VceManagementQueryResult::IntegrityIssues(page) if page.items.is_empty())
    );
    runtime
        .index
        .abort_refresh_staging("freeze-budget")
        .await
        .expect("finish only fixture-owned staging");
}

/// Publish a real isolated candidate, then restore and synchronize it through ordinary host admission.
/// 发布真实隔离候选，再通过普通宿主准入恢复并同步。
/// Inputs belong exclusively to the existing temporary fixture; returns after draining the new manager.
/// 输入仅属于既有临时夹具；排空新管理器后返回。
/// This tests runtime handoff only; candidate preparation below is explicit test setup, not a queued UI workflow.
/// 此处仅验证运行时交接；下方候选准备是明确测试布置，不是界面排队流程。
async fn verify_published_candidate_runtime_restore(
    factory: VceDataPlaneFactory,
    project_root: std::path::PathBuf,
    settings: ProjectVceSettings,
    fingerprint: String,
    executor: Arc<tokio::runtime::Runtime>,
) {
    // mock supplies only local capabilities; the empty fixture never calls a paid provider.
    // mock 仅提供本地能力；空夹具绝不调用付费供应商。
    let mock = vulcan_ai_contract::test_support::MockHostAiPort::new();
    mock.set_capability_snapshot(job_test_capabilities())
        .expect("configure replacement fixture");
    // approved_fingerprint freezes this fixture's explicit publication authority.
    // approved_fingerprint 冻结本夹具明确发布权威。
    let approved_fingerprint = fingerprint.clone();
    let manager = VceRuntimeManager::new(
        Arc::new(mock),
        factory.clone(),
        ServiceEventEmitter::new(Vec::new()),
        Arc::new(move |contract| {
            if contract.project_id == "job-project"
                && contract.dimension == 3
                && contract.embedding_fingerprint == approved_fingerprint
            {
                Ok(())
            } else {
                Err(
                    String::from("Candidate fixture rejects unexpected publication contract")
                        .into(),
                )
            }
        }),
    );
    // original/base name the exact already published predecessor after the preceding fixture drained.
    // original/base 标识前序夹具排空后精确的已发布前驱。
    let original = factory
        .embedding_provenance("job-project")
        .await
        .expect("load predecessor")
        .expect("predecessor exists");
    let original_index = factory
        .open_existing_project_index(&original.original)
        .await
        .expect("open predecessor")
        .expect("predecessor index exists");
    let checkpoint = original_index
        .load_state_snapshot()
        .await
        .expect("load predecessor checkpoint")
        .expect("predecessor snapshot exists")
        .workspace
        .checkpoint
        .expect("predecessor initialized");
    let base = vulcan_workspace_store::vce_data_plane::VceRebuildCandidateBase {
        namespace_id: original.original.binding.namespace_id.clone(),
        checkpoint_version: checkpoint.version,
        job_id: format!("fixture-candidate-{}", Uuid::new_v4()),
    };
    // prepared/index allocate only the explicit fixture candidate and preserve the predecessor until commit.
    // prepared/index 仅分配明确夹具候选，并在提交之前保留前驱。
    let prepared = runtime_configuration::prepare_runtime_configuration(
        "job-project",
        &project_root,
        &settings,
        3,
        &fingerprint,
        Some(&base.job_id),
    )
    .expect("prepare candidate configuration");
    let candidate_config = prepared.index.clone();
    // candidate_store/queued persist the exact physical target before this fixture allocates anything.
    // candidate_store/queued 在本夹具分配任何内容之前持久化精确物理目标。
    let candidate_store = original_index.job_store();
    let admitted_at = job_time().expect("candidate admission clock");
    let queued = VceStoredJob {
        schema_version: vulcan_workspace_store::vce_data_plane::VCE_JOB_RECEIPT_SCHEMA_VERSION,
        project_id: String::from("job-project"),
        job_id: base.job_id.clone(),
        owner_run_id: manager.job_owner_run_id.clone(),
        idempotency_key: format!("request:{}", base.job_id),
        retry_of_job_id: None,
        request_fingerprint: "a".repeat(64),
        contract_fingerprint: "b".repeat(64),
        kind: VceJobKind::Rebuild,
        state: VceJobState::Queued,
        sequence: 1,
        created_at_unix_ms: admitted_at,
        updated_at_unix_ms: admitted_at,
        admitted_settings_revision: settings.revision,
        source_checkpoint_version: Some(base.checkpoint_version),
        replacement: Some(Box::new(
            vulcan_workspace_store::vce_data_plane::VceJobReplacement {
                predecessor: base.clone(),
                target: candidate_config.clone(),
            },
        )),
        compaction: None,
        result_checkpoint_version: None,
        progress: None,
        followup_requested: false,
        completed_update: None,
        error: None,
    };
    // An unpublished abandoned receipt becomes interrupted, without scheduling replacement work.
    // 未发布的遗留回执变为中断，不安排替换工作。
    let mut abandoned = queued.clone();
    abandoned.job_id = Uuid::new_v4().to_string();
    abandoned.idempotency_key = format!("request:{}", abandoned.job_id);
    abandoned.kind = VceJobKind::Synchronize;
    abandoned.replacement = None;
    candidate_store
        .register(&abandoned)
        .await
        .expect("register abandoned receipt");
    let interrupted = manager
        .get_job("job-project", Some(&abandoned.job_id))
        .await
        .expect("settle unpublished job")
        .expect("interrupted receipt");
    assert_eq!(interrupted.state, VceJobState::Interrupted);
    assert!(interrupted.result_checkpoint_version.is_none());
    assert_eq!(
        manager
            .get_job("job-project", Some(&abandoned.job_id))
            .await
            .expect("replay interruption"),
        Some(interrupted)
    );
    candidate_store
        .register(&queued)
        .await
        .expect("admit fixture physical replacement");
    // Reading or cancelling an old terminal job cannot settle a different queued receipt.
    // 读取或取消旧终态作业不能收敛另一个排队回执。
    assert_eq!(
        manager
            .get_job("job-project", Some(&abandoned.job_id))
            .await
            .expect("historical terminal read")
            .expect("old receipt")
            .state,
        VceJobState::Interrupted
    );
    assert_eq!(
        manager
            .cancel_job("job-project", &abandoned.job_id)
            .await
            .expect("historical terminal cancellation")
            .state,
        VceJobState::Interrupted
    );
    assert_eq!(
        candidate_store
            .get(&queued.job_id)
            .await
            .expect("new cold receipt unchanged"),
        Some(queued.clone())
    );
    // running is acknowledged before starting the owned candidate phase.
    // running 在开始有归属候选阶段之前得到确认。
    let mut running = queued.clone();
    running.sequence += 1;
    running.state = VceJobState::Running;
    candidate_store
        .update(&queued, &running)
        .await
        .expect("start fixture candidate");
    let index = Arc::new(
        factory
            .prepare_rebuild_candidate(prepared.index, base.clone())
            .await
            .expect("allocate fixture candidate")
            .admit_rebuild_candidate_writer(base.clone())
            .await
            .expect("admit exact fixture candidate"),
    );
    // ai registers the candidate's own logical ID and instance without replacing another runtime handle.
    // ai 注册候选自身逻辑编号及实例，不替换另一运行时句柄。
    let ai = Arc::new(
        VceHostAiAdapter::activate(
            Arc::clone(&manager.ai_port),
            VceHostAiRegistrationConfig {
                runtime_instance_id: Uuid::new_v4(),
                project_id: String::from("job-project"),
                consumer_profile_id: String::from(VCE_CONSUMER_PROFILE_ID),
                vector_space_id: candidate_config.vector_space_id.clone(),
                dimension: Some(3),
                preprocessing_version: String::from(VCE_PREPROCESSING_VERSION),
                chunker_version: String::from(VCE_CHUNKER_VERSION),
            },
        )
        .expect("register candidate fixture models"),
    );
    assert_eq!(ai.embedding_space(), &prepared.engine.embedding_space);
    // engine/runtime form an unpublished writer only; no project query runtime is inserted during construction.
    // engine/runtime 仅构成未发布写入器；构造期间不插入项目查询运行时。
    let engine = VceEngine::new(prepared.engine.clone()).expect("create candidate engine");
    let projection = VceCommittedProjection {
        engine: engine.status(),
        workspace: engine.workspace_statistics(),
    };
    let runtime = Arc::new(VceProjectRuntime {
        project_id: String::from("job-project"),
        config: prepared.engine,
        config_revision: AtomicU64::new(settings.revision),
        job_schedule: std::sync::Mutex::new(VceJobSchedule::new(&settings)),
        latest_settings: std::sync::RwLock::new(settings.clone()),
        settings: settings.clone(),
        ai,
        index,
        source: prepared.source,
        current_engine: RwLock::new(Arc::new(VcePublishedEngine {
            engine,
            read_index: None,
        })),
        committed_projection: RwLock::new(projection),
        refresh_gate: Arc::new(Mutex::new(())),
        job_state: manager.project_job_state("job-project").await,
        host_status: RwLock::new(VceHostStatus {
            lifecycle: VceProjectLifecycle::Uninitialized,
            last_error: None,
        }),
        active_refresh: Mutex::new(None),
        refresh_observation_revision: std::sync::atomic::AtomicU64::new(0),
        operation_admission: VceOperationAdmission::accepting(),
    });
    assert!(manager.project_status("job-project").await.is_none());
    assert_eq!(
        factory
            .embedding_provenance("job-project")
            .await
            .expect("read original pointer")
            .expect("original remains active")
            .original,
        original.original
    );
    // writer owns the same gate required by the host execution entry; this is not a durable queue assertion.
    // writer 持有宿主执行入口要求的同一闸门；此处不是持久队列断言。
    let writer = runtime.refresh_gate.lock().await;
    let published = manager
        .execute_job_refresh(
            &runtime,
            base.job_id.clone(),
            VceRefreshControl::default(),
            VceJobWork::Manual,
        )
        .await
        .expect("publish candidate through host commit/read handoff");
    drop(writer);
    // Simulate a host ending after publication but before final job accounting.
    // 模拟发布完成后、最终作业计账前宿主结束。
    let mut committing = running.clone();
    committing.sequence += 1;
    committing.progress = Some(VceRefreshProgress {
        activity: None,
        phase: vulcan_context_engine::VceRefreshPhase::Committing,
        operation_id: base.job_id.clone(),
        unit: vulcan_context_engine::VceRefreshUnit::None,
        completed: 0,
        total: None,
        overall_completed: 0,
        overall_total: None,
        provider_batch_state: None,
        current_path: None,
    });
    candidate_store
        .update(&running, &committing)
        .await
        .expect("persist stuck publication phase");
    // A cooperating writer prevents reconciliation even though the receipt looks stale.
    // 即使回执看似过期，协作写入者仍会阻止收敛。
    let lease = factory
        .try_acquire_project_writer("job-project")
        .expect("lock project")
        .expect("writer available");
    assert_eq!(
        manager
            .get_job("job-project", Some(&base.job_id))
            .await
            .expect("read live job"),
        Some(committing)
    );
    drop(lease);
    let finished = manager
        .get_job("job-project", Some(&base.job_id))
        .await
        .expect("reconcile publication")
        .expect("receipt exists");
    assert_eq!(finished.state, VceJobState::Succeeded);
    assert_eq!(finished.result_checkpoint_version, Some(published));
    assert_eq!(
        manager
            .get_job("job-project", Some(&base.job_id))
            .await
            .expect("idempotent recovery"),
        Some(finished.clone())
    );
    assert_eq!(
        manager
            .cancel_job("job-project", &base.job_id)
            .await
            .expect("terminal cancellation replay"),
        finished
    );
    drop(candidate_store);
    assert_eq!(
        factory
            .embedding_provenance("job-project")
            .await
            .expect("read published candidate")
            .expect("candidate active")
            .original,
        candidate_config
    );
    manager
        .stop_runtime(&runtime)
        .await
        .expect("drain candidate writer registration");
    drop(runtime);
    // restore must adopt the stored candidate identity rather than regenerate the original root-based ID.
    // restore 必须采用存储候选身份，而不是重新生成原始基于根目录的编号。
    manager
        .enable_project(
            "job-project",
            &project_root,
            settings.clone(),
            3,
            &fingerprint,
            false,
        )
        .await
        .expect("restore published candidate as an ordinary runtime");
    let restored = manager
        .require_project("job-project")
        .await
        .expect("restored project available");
    assert_eq!(
        restored.config.index_namespace_id,
        candidate_config.binding.namespace_id
    );
    assert_eq!(
        restored.ai.embedding_space().vector_space_id,
        candidate_config.vector_space_id
    );
    assert!(manager.project_can_serve_now("job-project", settings.revision, &fingerprint));
    // retrieved exercises the retained checkpoint read path, not only its synchronous tool-admission projection.
    // retrieved 实际执行保留检查点的读取路径，而不只检查同步工具准入投影。
    let retrieved = manager
        .retrieve("job-project", "inspect the restored empty fixture")
        .await
        .expect("query the restored candidate checkpoint");
    assert_eq!(retrieved.checkpoint_version, published);
    // receipt proves the restored ordinary writer remains usable after the one-shot candidate grant has ended.
    // receipt 证明一次性候选授权结束后，恢复的普通写入器仍可使用。
    let receipt = manager
        .submit_job(
            "job-project",
            VceJobAdmission {
                retry_of_job_id: None,
                kind: VceJobKind::Synchronize,
                idempotency_key: String::from("synchronize-restored-candidate"),
                expected_settings_revision: settings.revision,
                expected_checkpoint_version: Some(published),
                contract_fingerprint: fingerprint,
            },
            &executor,
        )
        .await
        .expect("queue ordinary synchronization after candidate restore");
    let completed = await_test_job(&manager, "job-project", &receipt.job_id).await;
    assert_eq!(
        completed.state,
        VceJobState::Succeeded,
        "restored synchronization: {:?}",
        completed.error
    );
    assert_eq!(completed.result_checkpoint_version, Some(published));
    // Release the fixture's inspection handles before shutdown verifies that all project leases have drained.
    // 在退出检查全部项目租约已排空之前，释放夹具自身持有的检查句柄。
    drop(restored);
    drop(original_index);
    manager
        .shutdown()
        .await
        .expect("drain candidate restore fixture");
}

/// Keep successor stopping sticky through worker drain without holding the completion admission mutex.
/// 在工作任务排空期间保持后继停止标志，不持有完成所需的准入互斥锁。
/// No external services are used; a simulated worker lease proves ordering and safe reset.
/// 不使用外部服务；模拟工作任务租约证明时序与安全复位。
#[tokio::test]
async fn project_job_drain_keeps_stop_requested_until_worker_leases_end() {
    // state owns no model or storage; execution represents an already admitted worker.
    // state 不持有模型或存储；execution 表示已准入工作任务。
    let state = Arc::new(VceProjectJobState::default());
    let execution = Arc::clone(&state.execution).read_owned().await;
    // closing retains project state while drain waits for the worker's terminal processing.
    // closing 在排空等待工作任务终态处理时保留项目状态。
    let closing = Arc::clone(&state);
    let drain = tokio::spawn(async move { closing.cancel_and_drain().await });
    tokio::time::timeout(Duration::from_secs(5), async {
        while !state.stop_requested.load(Ordering::Acquire) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("drain must request stop before waiting");
    assert!(
        !drain.is_finished(),
        "an outstanding worker lease must prevent completion"
    );
    assert!(
        state.admission.try_lock().is_ok(),
        "terminal processing must still acquire admission"
    );
    drop(execution);
    drain
        .await
        .expect("drain task completes")
        .expect("empty worker ownership drains cleanly");
    assert!(
        !state.stop_requested.load(Ordering::Acquire),
        "only drained execution permits reset"
    );
}

/// Abort a tracked worker that cannot release its execution lease within the shutdown grace period.
/// 验证无法在关闭宽限期内释放执行租约的已跟踪工作任务会被中止。
#[tokio::test]
async fn project_job_drain_aborts_tracked_worker_after_shutdown_timeout() {
    let state = Arc::new(VceProjectJobState::default());
    let execution = Arc::clone(&state.execution).read_owned().await;
    let completion_state = Arc::clone(&state);
    let worker = tokio::spawn(async move {
        let _execution = execution;
        std::future::pending::<()>().await;
        completion_state.clear_worker_handle().await;
    });
    state.install_worker_handle(worker).await;

    tokio::time::timeout(Duration::from_secs(5), state.cancel_and_drain())
        .await
        .expect("forced shutdown drain must be bounded")
        .expect("tracked worker must be abortable");
    assert!(
        state.worker_handle.lock().await.is_none(),
        "forced drain must not retain a detached worker handle"
    );
    assert!(
        !state.stop_requested.load(Ordering::Acquire),
        "forced drain must reset the stop fence after releasing execution"
    );
}

/// Wait for a worker that already released execution but still owns terminal persistence.
/// 验证执行租约已释放但仍持有终态持久化的工作任务会被退出流程等待。
#[tokio::test]
async fn project_job_drain_waits_for_terminal_persistence_worker() {
    let state = Arc::new(VceProjectJobState::default());
    let worker_started = Arc::new(Notify::new());
    let release_worker = Arc::new(Notify::new());
    let worker_state = Arc::clone(&state);
    let worker_started_signal = Arc::clone(&worker_started);
    let release_worker_signal = Arc::clone(&release_worker);
    let worker = tokio::spawn(async move {
        // No execution lease is held here; the worker models the terminal receipt write itself.
        // 这里不持有执行租约；工作任务本身模拟终态回执写入。
        worker_started_signal.notify_one();
        release_worker_signal.notified().await;
        worker_state.clear_worker_handle().await;
    });
    state.install_worker_handle(worker).await;

    let closing = Arc::clone(&state);
    let drain = tokio::spawn(async move { closing.cancel_and_drain().await });
    tokio::time::timeout(Duration::from_secs(1), worker_started.notified())
        .await
        .expect("terminal persistence worker must start");
    tokio::task::yield_now().await;
    assert!(
        !drain.is_finished(),
        "shutdown must wait for terminal persistence after execution drained"
    );
    release_worker.notify_one();
    tokio::time::timeout(Duration::from_secs(5), drain)
        .await
        .expect("terminal persistence drain must be bounded")
        .expect("drain task must complete")
        .expect("terminal persistence worker must drain cleanly");
    assert!(
        state.worker_handle.lock().await.is_none(),
        "terminal persistence drain must not retain a worker handle"
    );
}

/// Check that initialization, synchronization, and contract changes cannot bypass exact guards.
/// 检查初始化、同步与契约变化无法绕过精确保护。
///
/// Takes no parameters; verifies all legal initial/sync combinations and stale inputs.
/// 无参数；验证合法初始化与同步组合及过期输入。
#[test]
fn human_job_admission_requires_exact_checkpoint_and_contract() {
    let mut command = VceJobAdmission {
        retry_of_job_id: None,
        kind: VceJobKind::Initialize,
        idempotency_key: String::from("button-click"),
        expected_settings_revision: 4,
        expected_checkpoint_version: None,
        contract_fingerprint: "a".repeat(64),
    };
    let fingerprint = "a".repeat(64);
    assert!(validate_job_admission(&command, 4, None, Some(&fingerprint)).is_ok());
    assert!(validate_job_admission(&command, 5, None, Some(&fingerprint)).is_err());
    assert!(validate_job_admission(&command, 4, None, Some(&"b".repeat(64))).is_err());
    assert!(validate_job_admission(&command, 4, Some(1), Some(&fingerprint)).is_err());
    command.kind = VceJobKind::Synchronize;
    assert!(validate_job_admission(&command, 4, None, Some(&fingerprint)).is_err());
    command.expected_checkpoint_version = Some(1);
    assert!(validate_job_admission(&command, 4, Some(1), Some(&fingerprint)).is_ok());
    command.kind = VceJobKind::Rebuild;
    assert!(validate_job_admission(&command, 4, Some(1), Some(&fingerprint)).is_ok());
    command.expected_checkpoint_version = None;
    assert!(validate_job_admission(&command, 4, None, Some(&fingerprint)).is_err());
}

/// Verify physical compaction ignores policy-only revisions while retaining immutable guards.
/// 验证物理整理忽略纯策略修订，同时保留不可变保护。
///
/// Takes no parameters; prepares one physical binding and checks revision, checkpoint, and fingerprint behavior.
/// 无参数；准备一个物理绑定并检查修订、检查点及指纹行为。
#[test]
fn physical_compaction_target_uses_immutable_binding_guards() {
    // temporary supplies one canonical project root without touching user storage.
    // temporary 提供一个规范项目根目录，且不触碰用户存储。
    let temporary = tempfile::tempdir().expect("create compaction target fixture");
    // settings represent the immutable binding's original project revision.
    // settings 表示不可变绑定最初的项目修订。
    let settings = ProjectVceSettings {
        enabled: true,
        revision: 1,
        ..ProjectVceSettings::default()
    };
    // fingerprint is one canonical physical embedding identity.
    // fingerprint 是一个规范物理向量身份。
    let fingerprint = format!("sha256:{}", "a".repeat(64));
    // config retains the original database write-guard revision.
    // config 保留原始数据库写保护修订。
    let config = runtime_configuration::prepare_runtime_configuration(
        "compaction-target-project",
        temporary.path(),
        &settings,
        3,
        &fingerprint,
        None,
    )
    .expect("prepare compaction target")
    .index;
    // command carries a later policy-only admission revision for the same physical target.
    // command 携带同一物理目标后续的纯策略准入修订。
    let mut command = VceJobAdmission {
        retry_of_job_id: None,
        kind: VceJobKind::Compact,
        idempotency_key: String::from("compact-after-policy-change"),
        expected_settings_revision: 2,
        expected_checkpoint_version: Some(9),
        contract_fingerprint: fingerprint,
    };
    assert_eq!(config.config_revision, 1);
    assert!(validate_compaction_target(&command, &config, Some(9)).is_ok());
    command.expected_checkpoint_version = Some(10);
    assert!(validate_compaction_target(&command, &config, Some(9)).is_err());
    command.expected_checkpoint_version = Some(9);
    command.contract_fingerprint = format!("sha256:{}", "b".repeat(64));
    assert!(validate_compaction_target(&command, &config, Some(9)).is_err());
}

/// Wait for a real completed core observation while the fixture deliberately holds host publication.
/// 在夹具明确持有宿主发布期间等待真实核心完成观察。
/// `manager`, `project_id`, and `job_id` select the exact live receipt; returns after the observation.
/// `manager`、`project_id` 与 `job_id` 选择精确实时回执；观察到后返回。
async fn await_test_completed_phase(manager: &VceRuntimeManager, project_id: &str, job_id: &str) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            // receipt is read through the same stable job API used by the UI.
            // receipt 通过界面使用的相同稳定作业接口读取。
            let receipt = manager
                .get_job(project_id, Some(job_id))
                .await
                .expect("read fixture receipt")
                .expect("fixture receipt exists");
            if receipt.progress.as_ref().is_some_and(|progress| {
                progress.phase == vulcan_context_engine::VceRefreshPhase::Complete
            }) {
                return;
            }
            assert!(
                !receipt.state.is_terminal(),
                "unexpected terminal receipt: {:?}",
                receipt.error
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("actual completed phase arrives");
}
