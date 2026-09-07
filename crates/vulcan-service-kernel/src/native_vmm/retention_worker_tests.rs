//! Real worker lifecycle acceptance using the isolated controller owned by the calling test.
//! 使用调用测试持有的隔离控制器验收真实工作器生命周期。

use super::*;

/// Starts both production cleanup loops for the isolated HTTP fixture and returns their abort-on-drop owner.
/// 为隔离 HTTP 夹具启动两个生产清洗循环，并返回析构即中止的所有者。
/// The supplied maintenance handle must belong to the private test service; no user worker is accessed.
/// 传入维护句柄必须属于私有测试服务；不访问任何用户工作器。
pub(in crate::native_vmm) fn start_http_workers(
    maintenance: &NativeVmmMaintenance,
) -> tokio::task::JoinSet<()> {
    // Keep both task lifetimes scoped to the caller, including assertion failures.
    // 将两个任务的生命周期限定于调用方，包括断言失败情况。
    let mut workers = tokio::task::JoinSet::new();
    workers.spawn(maintenance.clone().run_retention_worker());
    workers.spawn(maintenance.clone().run_profile_expiry_worker());
    workers
}

/// Waits for a concrete process-local worker observation, with a finite diagnostic deadline.
/// 等待具体的进程内工作器观察结果，具有有限诊断截止时间。
/// `predicate` reads actual worker status; timeout reports the last authoritative state.
/// `predicate` 读取真实工作器状态；超时报告最后的权威状态。
async fn observe(
    maintenance: &NativeVmmMaintenance,
    predicate: impl Fn(&NativeVmmRetentionStatus) -> bool,
) -> NativeVmmRetentionStatus {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(40);
    loop {
        let status = maintenance.retention_last_status.read().await.clone();
        if predicate(&status) {
            return status;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "worker observation timeout: {status:?}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Exercises both production loops without changing their clocks or substituting their storage effects.
/// 执行两个生产循环，不修改其时钟或替换其存储效果。
/// `maintenance` belongs exclusively to a temporary controller; workers are aborted and joined on exit.
/// `maintenance` 专属于临时控制器；退出时中止并等待工作器收回。
pub(in crate::native_vmm) async fn verify_worker_lifecycle(
    maintenance: NativeVmmMaintenance,
    scheduled_memory: vulcan_memorymesh_core::MemoryRecord,
) {
    let mut settings = maintenance
        .runtime
        .retention_settings()
        .await
        .expect("settings");
    settings.enabled = false;
    settings.scan_interval_minutes = 1;
    settings.profile_expiry_interval_seconds = 1;
    maintenance
        .update_retention_settings(settings.clone())
        .await
        .expect("disable schedule");
    maintenance.set_native_backend_enabled(false);
    // JoinSet owns only workers created here and also aborts them if an assertion panics.
    // JoinSet 仅持有本处创建的工作器，在断言异常时也会中止它们。
    let mut workers = tokio::task::JoinSet::new();
    workers.spawn(maintenance.clone().run_retention_worker());
    workers.spawn(maintenance.clone().run_profile_expiry_worker());
    let initial = maintenance.retention_last_status.read().await.clone();
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        maintenance
            .retention_last_status
            .read()
            .await
            .last_finished_at_unix_ms,
        initial.last_finished_at_unix_ms
    );
    maintenance.set_native_backend_enabled(true);
    tokio::time::sleep(Duration::from_millis(1_250)).await;
    assert_eq!(
        maintenance
            .retention_last_status
            .read()
            .await
            .last_finished_at_unix_ms,
        initial.last_finished_at_unix_ms
    );
    assert!(
        maintenance
            .retention_last_status
            .read()
            .await
            .profile_expiry_last_finished_at_unix_ms
            .is_none()
    );

    settings.enabled = true;
    maintenance
        .update_retention_settings(settings.clone())
        .await
        .expect("enable schedule");
    let enabled = observe(&maintenance, |status| {
        status.last_finished_at_unix_ms > initial.last_finished_at_unix_ms
            && status.profile_expiry_last_finished_at_unix_ms.is_some()
    })
    .await;
    assert!(enabled.last_error.is_none(), "{:?}", enabled.last_error);
    assert!(
        enabled.profile_expiry_last_error.is_none(),
        "{:?}",
        enabled.profile_expiry_last_error
    );
    verify_steady_cleanup(&maintenance, &enabled, scheduled_memory).await;
    verify_full_cadence_changes(&maintenance).await;

    settings.enabled = false;
    maintenance
        .update_retention_settings(settings.clone())
        .await
        .expect("pause schedule");
    // Allow an already admitted pass to finish; disabling does not cancel its durable transaction.
    // 允许已经准入的轮次完成；禁用不取消其持久事务。
    tokio::time::sleep(Duration::from_millis(1_250)).await;
    let paused = maintenance.retention_last_status.read().await.clone();
    tokio::time::sleep(Duration::from_millis(2_100)).await;
    let still_paused = maintenance.retention_last_status.read().await.clone();
    assert_eq!(
        still_paused.last_finished_at_unix_ms,
        paused.last_finished_at_unix_ms
    );
    assert_eq!(
        still_paused.profile_expiry_last_finished_at_unix_ms,
        paused.profile_expiry_last_finished_at_unix_ms
    );

    settings.enabled = true;
    settings.profile_expiry_interval_seconds = 10;
    maintenance
        .update_retention_settings(settings.clone())
        .await
        .expect("long profile period");
    let resumed = observe(&maintenance, |status| {
        status.last_finished_at_unix_ms > paused.last_finished_at_unix_ms
    })
    .await;
    tokio::time::sleep(Duration::from_millis(1_250)).await;
    assert_eq!(
        maintenance
            .retention_last_status
            .read()
            .await
            .profile_expiry_last_finished_at_unix_ms,
        paused.profile_expiry_last_finished_at_unix_ms
    );
    settings.profile_expiry_interval_seconds = 1;
    maintenance
        .update_retention_settings(settings)
        .await
        .expect("shorten profile period");
    let shortened = observe(&maintenance, |status| {
        status.profile_expiry_last_finished_at_unix_ms
            > paused.profile_expiry_last_finished_at_unix_ms
    })
    .await;
    assert_eq!(
        shortened.last_finished_at_unix_ms,
        resumed.last_finished_at_unix_ms
    );

    // Mutate only the already identified temporary user file, with no notification to either worker.
    // 仅修改已确定的临时用户文件，不向任一工作器发送通知。
    let config_root = maintenance.data_plane.data_root.join("config");
    let path = config_root.join("runtime-policy.json");
    let valid = std::fs::read(&path).expect("persisted policy");
    std::fs::write(&path, b"{").expect("simulate invalid user edit");
    let rejected = observe(&maintenance, |status| {
        status
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("解析原生 VMM"))
            && status
                .profile_expiry_last_error
                .as_deref()
                .is_some_and(|error| error.contains("解析原生 VMM"))
    })
    .await;
    tokio::time::sleep(Duration::from_millis(1_250)).await;
    let no_fallback = maintenance.retention_last_status.read().await.clone();
    assert_eq!(
        no_fallback.last_finished_at_unix_ms,
        rejected.last_finished_at_unix_ms
    );
    assert_eq!(
        no_fallback.profile_expiry_last_finished_at_unix_ms,
        rejected.profile_expiry_last_finished_at_unix_ms
    );
    std::fs::write(&path, &valid).expect("repair user policy without notification");
    let repaired = observe(&maintenance, |status| {
        status.last_finished_at_unix_ms > rejected.last_finished_at_unix_ms
            && status.profile_expiry_last_finished_at_unix_ms
                > rejected.profile_expiry_last_finished_at_unix_ms
            && status.last_error.is_none()
            && status.profile_expiry_last_error.is_none()
    })
    .await;
    assert!(
        repaired
            .last_result
            .as_ref()
            .expect("completed cleanup")
            .project_failures
            .is_empty()
    );
    maintenance.set_native_backend_enabled(false);
    tokio::time::sleep(Duration::from_millis(1_250)).await;
    let off = maintenance.retention_last_status.read().await.clone();
    tokio::time::sleep(Duration::from_millis(2_100)).await;
    assert_eq!(
        maintenance
            .retention_last_status
            .read()
            .await
            .profile_expiry_last_finished_at_unix_ms,
        off.profile_expiry_last_finished_at_unix_ms
    );
    workers.abort_all();
    while let Some(outcome) = workers.join_next().await {
        assert!(outcome.expect_err("aborted worker").is_cancelled());
    }
}

/// Verifies lengthening suppresses the old deadline and shortening retains the last-completion anchor.
/// 验证延长周期会抑制旧截止时间，缩短周期仍保留上次完成时间锚点。
/// Uses the running isolated workers and real elapsed time; returns after the shortened full pass finishes.
/// 使用正在运行的隔离工作器及真实流逝时间，在缩短后的完整扫描完成后返回。
async fn verify_full_cadence_changes(maintenance: &NativeVmmMaintenance) {
    // The preceding steady-period assertion completed a real full scan, establishing the timer anchor.
    // 前序稳态周期断言完成了一次真实完整扫描，建立计时锚点。
    let baseline = maintenance.retention_last_status.read().await.clone();
    let mut settings = maintenance
        .runtime
        .retention_settings()
        .await
        .expect("current cadence");
    settings.scan_interval_minutes = 2;
    maintenance
        .update_retention_settings(settings.clone())
        .await
        .expect("lengthen full cadence");
    // Cross both the policy refresh and the old one-minute deadline without substituting a clock.
    // 不替换时钟，跨过策略刷新及旧的一分钟截止时间。
    tokio::time::sleep(Duration::from_secs(31)).await;
    assert_eq!(
        maintenance
            .retention_last_status
            .read()
            .await
            .last_finished_at_unix_ms,
        baseline.last_finished_at_unix_ms
    );
    maintenance
        .update_retention_settings(settings.clone())
        .await
        .expect("unchanged settings wake");
    tokio::time::sleep(Duration::from_secs(31)).await;
    let lengthened = maintenance.retention_last_status.read().await.clone();
    assert_eq!(
        lengthened.last_finished_at_unix_ms,
        baseline.last_finished_at_unix_ms
    );
    assert!(
        lengthened.profile_expiry_last_finished_at_unix_ms
            > baseline.profile_expiry_last_finished_at_unix_ms
    );
    // More than a minute has passed since completion, so shortening must be due without another minute.
    // 距离完成已超过一分钟，因此缩短后必须到期，不能再等一分钟。
    settings.scan_interval_minutes = 1;
    maintenance
        .update_retention_settings(settings)
        .await
        .expect("shorten full cadence");
    let shortened = observe(maintenance, |status| {
        status.last_finished_at_unix_ms > baseline.last_finished_at_unix_ms
    })
    .await;
    assert!(
        shortened.last_started_at_unix_ms.expect("shortened start")
            - baseline.last_finished_at_unix_ms.expect("baseline finish")
            >= 60_000
    );
    assert!(shortened.last_error.is_none(), "{:?}", shortened.last_error);
    assert!(
        shortened
            .last_result
            .expect("shortened scan result")
            .project_failures
            .is_empty()
    );
    assert!(!maintenance.host_ai_status().checked);
}

/// Verifies a genuine second full cadence processes data admitted after the eager first pass.
/// 验证真实第二次完整周期处理首轮立即扫描之后准入的数据。
/// The fixture memory belongs to the isolated controller; clocks, worker bodies and storage are unchanged.
/// 夹具记忆属于隔离控制器；不修改时钟、工作器主体或存储实现。
async fn verify_steady_cleanup(
    maintenance: &NativeVmmMaintenance,
    first: &NativeVmmRetentionStatus,
    memory: vulcan_memorymesh_core::MemoryRecord,
) {
    // Keep an independent expected vector record and timestamp before the production transaction consumes it.
    // 在生产事务消费前保留独立预期向量记录和时间戳。
    let expected = memory.clone();
    let first_finished = first
        .last_finished_at_unix_ms
        .expect("initial full pass finished");
    {
        // Use canonical catalog-before-storage ordering and never race the profile worker's transaction.
        // 使用目录先于存储的规范锁序，绝不与画像工作器事务竞争。
        let _catalog_guard = maintenance.project_mutation_gate.lock().await;
        let _operation_guard = maintenance.operation_gate.write().await;
        let version = maintenance
            .runtime
            .storage_snapshot()
            .expect("before scheduled seed")
            .storage_version;
        maintenance
            .runtime
            .transform_storage_snapshot(
                "scheduled-after-first-pass",
                "test",
                version,
                move |snapshot| {
                    snapshot.dataset.memories.push(memory);
                    Ok(())
                },
            )
            .await
            .expect("persist after first pass");
    }
    // Cross the thirty-second policy poll without modifying configuration or sending a wake notification.
    // 跨过三十秒策略轮询，不修改配置或发送唤醒通知。
    tokio::time::sleep(Duration::from_secs(35)).await;
    let halfway = maintenance.retention_last_status.read().await.clone();
    assert_eq!(
        halfway.last_finished_at_unix_ms,
        first.last_finished_at_unix_ms
    );
    assert!(
        halfway.profile_expiry_last_finished_at_unix_ms
            > first.profile_expiry_last_finished_at_unix_ms
    );
    assert!(
        maintenance
            .runtime
            .storage_snapshot()
            .expect("not prematurely recycled")
            .dataset
            .memories
            .iter()
            .any(|record| record.id == expected.id)
    );
    // Existing bounded observation now spans the remaining twenty-five seconds of the real minute.
    // 既有有界观察现在覆盖真实一分钟周期剩余的二十五秒。
    let second = observe(maintenance, |status| {
        status.last_finished_at_unix_ms > first.last_finished_at_unix_ms
    })
    .await;
    assert!(second.last_started_at_unix_ms.expect("second start") - first_finished >= 60_000);
    assert!(second.last_error.is_none(), "{:?}", second.last_error);
    let result = second.last_result.expect("second pass result");
    assert!(
        result.project_failures.is_empty(),
        "{:?}",
        result.project_failures
    );
    assert_eq!(result.recycled_memories, 1);
    let stored: StorageSnapshot = serde_json::from_str(
        &maintenance
            .data_plane
            .load_core_snapshot_json()
            .await
            .expect("SQLite read")
            .expect("SQLite snapshot"),
    )
    .expect("typed persisted snapshot");
    assert!(
        !stored
            .dataset
            .memories
            .iter()
            .any(|record| record.id == expected.id)
    );
    let batch = stored
        .retention_trash_batches
        .iter()
        .find(|batch| {
            batch
                .dataset
                .memories
                .iter()
                .any(|record| record.id == expected.id)
        })
        .expect("scheduled durable batch");
    assert!(
        batch.recycled_at_ns / 1_000_000 >= second.last_started_at_unix_ms.expect("second start")
    );
    let policy = maintenance
        .runtime
        .retention_settings()
        .await
        .expect("unchanged rules");
    assert_eq!(
        batch.expires_at_ns - batch.recycled_at_ns,
        (policy.trash_retention_ms * 1_000_000) as i64
    );
    let actual = maintenance
        .runtime
        .storage_snapshot()
        .expect("vector-bearing snapshot")
        .retention_trash_batches
        .into_iter()
        .flat_map(|batch| batch.dataset.memories)
        .find(|record| record.id == expected.id)
        .expect("retained memory");
    assert_eq!(actual, expected);
    assert!(!maintenance.host_ai_status().checked);
}
