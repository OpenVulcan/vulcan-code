//! Actual production timers consume settings saved by the live HTTP policy component.
//! 实际生产计时器消费联网 HTTP 策略组件保存的设置。

use super::*;
use crate::native_vmm::maintenance::{NativeVmmMaintenance, NativeVmmRetentionStatus};

/// Reads the real worker status until the requested completed state, with a forty-second bound.
/// 读取真实工作器状态直到要求的完成状态，最多等待四十秒。
/// Takes the private maintenance owner and a status predicate; returns the observed quiescent status.
/// 接收私有维护所有者与状态谓词；返回观察到的静止状态。
async fn observe(
    maintenance: &NativeVmmMaintenance,
    predicate: impl Fn(&NativeVmmRetentionStatus) -> bool,
) -> NativeVmmRetentionStatus {
    // Use real monotonic time rather than pausing or replacing production clocks.
    // 使用真实单调时间，不暂停或替换生产时钟。
    let deadline = tokio::time::Instant::now() + Duration::from_secs(40);
    loop {
        // Status is projected by the same service handle that receives HTTP configuration updates.
        // 状态由接收 HTTP 配置更新的同一个服务句柄投影。
        let status = maintenance
            .retention_status()
            .await
            .expect("HTTP worker status");
        assert!(status.last_error.is_none(), "{:?}", status.last_error);
        assert!(
            status.profile_expiry_last_error.is_none(),
            "{:?}",
            status.profile_expiry_last_error
        );
        if !status.running && !status.profile_expiry_running && predicate(&status) {
            return status;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "HTTP worker timed out: {status:?}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Checks one independent scheduler generation while preserving all six data-retention fields.
/// 检查一个独立调度代次，同时保持全部六个数据保留字段。
/// Takes the real runtime and exact switch/period values; returns after whole-group equality succeeds.
/// 接收真实运行时与准确开关／周期值；完整组相等后返回。
async fn settings(test: &Acceptance, enabled: bool, minutes: u32, seconds: u32) {
    // The input expectation is independent of the frontend response and production resolver.
    // 输入预期独立于前端响应及生产解析器。
    let mut expected = global(2);
    expected.enabled = enabled;
    expected.scan_interval_minutes = minutes;
    expected.profile_expiry_interval_seconds = seconds;
    assert_eq!(
        test.runtime
            .retention_settings()
            .await
            .expect("HTTP scheduler policy"),
        expected
    );
}

/// Executes the real enabled/lengthened/shortened/disabled timer lifecycle after actual page saves.
/// 在实际页面保存后执行真实启用／延长／缩短／禁用计时生命周期。
/// Takes isolated acceptance state, handshake directory, service owner and stable project ID; joins owned tasks before return.
/// 接收隔离验收状态、握手目录、服务所有者及稳定项目 ID；返回前回收持有任务。
pub(super) async fn verify(
    test: &Acceptance,
    root: &Path,
    maintenance: &NativeVmmMaintenance,
    project: u64,
) {
    settings(test, false, 3, 5).await;
    // Retain every earlier record, profile and trash batch so scheduler execution cannot silently change them.
    // 保留所有前序记录、画像及回收批次，避免调度执行静默改变它们。
    let original = test
        .runtime
        .storage_snapshot()
        .expect("HTTP schedule initial storage");
    // Scheduling and retention must never create inference work.
    // 调度及保留绝不能创建推理工作。
    let calls = test
        .port
        .requests
        .lock()
        .expect("HTTP scheduler inference baseline")
        .len();
    // A distinct numeric range prevents collisions with all preceding manual cleanup samples.
    // 不同数值区间避免与全部前序人工清洗样本冲突。
    let sample = seed(
        test,
        project,
        9_999_000,
        original
            .dataset
            .memories
            .first()
            .expect("HTTP schedule template"),
    )
    .await;
    // Only this JoinSet owns the two production workers; panic drops and aborts them too.
    // 仅此任务集合持有两个生产工作器；异常析构同样中止它们。
    let mut workers =
        crate::native_vmm::maintenance::retention_worker_tests::start_http_workers(maintenance);
    // Capture the disabled baseline before crossing the configured five-second profile period.
    // 在跨越已配置五秒画像周期之前捕获禁用基线。
    let disabled = observe(maintenance, |_| true).await;
    tokio::time::sleep(Duration::from_secs(6)).await;
    // An enabled backend with the global switch off must not run either loop.
    // 后端启用但全局开关关闭时，两个循环都不能运行。
    let off = observe(maintenance, |_| true).await;
    assert_eq!(
        off.last_finished_at_unix_ms,
        disabled.last_finished_at_unix_ms
    );
    assert_eq!(
        off.profile_expiry_last_finished_at_unix_ms,
        disabled.profile_expiry_last_finished_at_unix_ms
    );
    assert!(
        test.runtime
            .storage_snapshot()
            .expect("HTTP seed still active")
            .dataset
            .memories
            .iter()
            .any(|memory| memory.id == sample.memories[0].id)
    );
    wait_phase(root, "schedule-setup").await;
    acknowledge(root, "schedule-setup");

    wait_phase(root, "schedule-enabled").await;
    settings(test, true, 1, 1).await;
    // The full loop runs on enable; the separate profile loop runs after its actual first one-second period.
    // 完整循环在启用时执行；独立画像循环在真实首个一秒周期后执行。
    let enabled = observe(maintenance, |status| {
        status.last_finished_at_unix_ms > off.last_finished_at_unix_ms
            && status.profile_expiry_last_finished_at_unix_ms
                > off.profile_expiry_last_finished_at_unix_ms
    })
    .await;
    assert!(
        enabled
            .last_result
            .as_ref()
            .expect("HTTP automatic cleanup receipt")
            .project_failures
            .is_empty()
    );
    // This is actual automatic physical cleanup, not just a status timestamp or policy projection.
    // 这是实际自动物理清洗，而不只是状态时间戳或策略投影。
    let after = test
        .runtime
        .storage_snapshot()
        .expect("HTTP automatic storage");
    for (index, memory) in sample.memories.iter().enumerate() {
        assert_eq!(
            after
                .dataset
                .memories
                .iter()
                .find(|item| item.id == memory.id),
            if index < 6 { None } else { Some(memory) }
        );
        if index < 6 {
            // Every selected terminal record keeps its complete vector-bearing content and frozen four-day deadline.
            // 每条选中的终态记录保留完整向量正文及冻结的四天期限。
            let batch = after
                .retention_trash_batches
                .iter()
                .find(|batch| {
                    batch
                        .dataset
                        .memories
                        .iter()
                        .any(|item| item.id == memory.id)
                })
                .expect("HTTP automatic retained memory");
            assert_eq!(
                batch
                    .dataset
                    .memories
                    .iter()
                    .find(|item| item.id == memory.id),
                Some(memory)
            );
            assert_eq!(
                batch.expires_at_ns - batch.recycled_at_ns,
                4 * i64::try_from(DAY).expect("day fits") * 1_000_000
            );
        }
    }
    for (index, turn) in sample.turns.iter().enumerate() {
        assert_eq!(
            after.dataset.turns.iter().find(|item| item.id == turn.id),
            if index == 0 { None } else { Some(turn) }
        );
        if index == 0 {
            // Automatic cold-turn removal must retain the same complete bilingual source as manual cleanup.
            // 自动冷回合移除必须与人工清洗一样保留完整双语来源。
            let batch = after
                .retention_trash_batches
                .iter()
                .find(|batch| batch.dataset.turns.iter().any(|item| item.id == turn.id))
                .expect("HTTP automatic retained source");
            assert_eq!(
                batch.dataset.turns.iter().find(|item| item.id == turn.id),
                Some(turn)
            );
            assert_eq!(
                batch.expires_at_ns - batch.recycled_at_ns,
                4 * i64::try_from(DAY).expect("day fits") * 1_000_000
            );
        }
    }
    for memory in &original.dataset.memories {
        assert_eq!(
            after
                .dataset
                .memories
                .iter()
                .find(|item| item.id == memory.id),
            Some(memory)
        );
    }
    for turn in &original.dataset.turns {
        assert_eq!(
            after.dataset.turns.iter().find(|item| item.id == turn.id),
            Some(turn)
        );
    }
    assert_eq!(after.dataset.profile_nodes, original.dataset.profile_nodes);
    for batch in &original.retention_trash_batches {
        assert_eq!(
            after
                .retention_trash_batches
                .iter()
                .find(|item| item.batch_id == batch.batch_id),
            Some(batch)
        );
    }
    acknowledge(root, "schedule-enabled");

    wait_phase(root, "schedule-lengthened").await;
    settings(test, true, 2, 180).await;
    // A just-admitted profile pass may finish while the page saves; freeze the actual settled baseline.
    // 页面保存期间刚准入的画像轮次可能完成；冻结实际稳定后的基线。
    let lengthened = observe(maintenance, |_| true).await;
    assert_eq!(
        lengthened.last_finished_at_unix_ms,
        enabled.last_finished_at_unix_ms
    );
    // Freeze storage after any pass already admitted before the longer settings were saved.
    // 在保存较长设置之前已准入的轮次完成后冻结存储。
    let lengthened_storage = test
        .runtime
        .storage_snapshot()
        .expect("HTTP lengthened storage baseline");
    // Cross the old one-minute full deadline and repeated policy refresh using real time.
    // 使用真实时间跨越旧的一分钟完整截止及反复策略刷新。
    tokio::time::sleep(Duration::from_secs(31)).await;
    tokio::time::sleep(Duration::from_secs(31)).await;
    // Neither longer timer is due, while both old periods have definitely elapsed.
    // 两个更长周期均未到期，而两个旧周期均已确定经过。
    let still = observe(maintenance, |_| true).await;
    assert_eq!(
        still.last_finished_at_unix_ms,
        lengthened.last_finished_at_unix_ms
    );
    assert_eq!(
        still.profile_expiry_last_finished_at_unix_ms,
        lengthened.profile_expiry_last_finished_at_unix_ms
    );
    assert_eq!(
        test.runtime
            .storage_snapshot()
            .expect("HTTP longer timers preserve storage"),
        lengthened_storage
    );
    acknowledge(root, "schedule-lengthened");

    wait_phase(root, "schedule-shortened").await;
    settings(test, true, 1, 1).await;
    // Both are already due relative to their last completion; shortening must not restart a fresh period.
    // 相对上次完成时间两者均已到期；缩短不能重新开始一个完整周期。
    let shortened = observe(maintenance, |status| {
        status.last_finished_at_unix_ms > lengthened.last_finished_at_unix_ms
            && status.profile_expiry_last_finished_at_unix_ms
                > lengthened.profile_expiry_last_finished_at_unix_ms
    })
    .await;
    assert!(
        shortened.last_started_at_unix_ms.expect("HTTP short start")
            - enabled
                .last_finished_at_unix_ms
                .expect("HTTP enabled finish")
            >= 60_000
    );
    assert!(
        shortened
            .last_result
            .as_ref()
            .expect("HTTP shortened receipt")
            .project_failures
            .is_empty()
    );
    acknowledge(root, "schedule-shortened");

    wait_phase(root, "schedule-disabled").await;
    settings(test, false, 1, 1).await;
    // Disabling permits an admitted durable transaction to finish, then prevents further passes.
    // 禁用允许已准入耐久事务完成，随后阻止后续轮次。
    tokio::time::sleep(Duration::from_millis(1_250)).await;
    let paused = observe(maintenance, |_| true).await;
    tokio::time::sleep(Duration::from_millis(2_100)).await;
    let paused_again = observe(maintenance, |_| true).await;
    assert_eq!(
        paused_again.last_finished_at_unix_ms,
        paused.last_finished_at_unix_ms
    );
    assert_eq!(
        paused_again.profile_expiry_last_finished_at_unix_ms,
        paused.profile_expiry_last_finished_at_unix_ms
    );
    acknowledge(root, "schedule-disabled");

    wait_phase(root, "schedule-restored").await;
    settings(test, false, 3, 5).await;
    workers.abort_all();
    while let Some(outcome) = workers.join_next().await {
        assert!(
            outcome
                .expect_err("HTTP owned worker aborted")
                .is_cancelled()
        );
    }
    assert_eq!(
        test.port
            .requests
            .lock()
            .expect("HTTP scheduler no inference")
            .len(),
        calls
    );
    acknowledge(root, "schedule-restored");
}
