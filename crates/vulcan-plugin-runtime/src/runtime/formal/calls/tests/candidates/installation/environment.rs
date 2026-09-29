//! Actual native candidate work must retain its environment after the foreground observer leaves.
//! 前台观察者退出后，实际原生候选工作必须保留其环境。

use super::*;
use crate::runtime::environment::{
    lock_plugin_execution_environment_cleanup, remove_obsolete_plugin_execution_environments,
    remove_plugin_execution_environment_generation,
};

/// Preparation errors after lease acquisition must release original authority without invoking Lua.
/// 租约获取后的准备错误必须释放原权威，且不调用 Lua。
/// No inputs or return; an invalid actual cache fails production marker validation and permits later cleanup.
/// 无输入与返回值；无效实际缓存未通过生产标记校验，随后允许清理。
#[test]
fn formal_candidate_environment_failed_preparation_releases_lease() {
    // This candidate has real install code, but preparation must fail before executing it.
    // 此候选具有真实安装代码，但准备必须在执行前失败。
    let (fixture, store, mut transaction) = installation_fixture(false, false, false);
    executor().block_on(async {
        // Retain only the immutable package descriptor after the failed operation returns.
        // 失败操作返回后，仅保留不可变插件包描述符。
        let plugin = {
            // Inspect the actual durable request rather than constructing private authority in the test.
            // 检查实际持久请求，而非在测试中构造私有权威。
            let candidate = store
                .inspect_activation_candidate(&transaction)
                .expect("candidate view");
            // The exact target identity selects its existing-cache validation branch.
            // 精确目标身份选择其既有缓存校验分支。
            let plugin = candidate.package().clone();
            // An empty existing directory deliberately lacks the production identity marker.
            // 刻意使现存空目录缺少生产身份标记。
            let environment = fixture
                .manager
                .state
                .plugin_runtime_root
                .join(&plugin.id)
                .join(plugin.version.as_deref().expect("version"))
                .join(&plugin.content_fingerprint);
            std::fs::create_dir_all(environment).expect("incomplete candidate environment");
            // The reported failure must come from real filesystem preparation, not a fabricated Lua result.
            // 报告失败必须来自真实文件系统准备，而非伪造 Lua 结果。
            let error = fixture
                .manager
                .install_activation_candidate(&candidate)
                .await
                .expect_err("incomplete cache rejected");
            assert!(
                error.message.contains("environment marker"),
                "unexpected preparation error: {}",
                error.message
            );
            plugin
        };
        assert!(
            fixture
                .events
                .values
                .lock()
                .expect("native event log")
                .is_empty()
        );
        assert!(
            fixture
                .manager
                .activation_candidate_calls(transaction.record().request_id())
                .expect("retired failed preparation")
                .is_empty()
        );
        remove_plugin_execution_environment_generation(
            &fixture.manager.state,
            &plugin.id,
            plugin.version.as_deref().expect("version"),
            &plugin.content_fingerprint,
        )
        .expect("failed preparation released its shared environment lease");
        store
            .abort_activation_transaction(&mut transaction)
            .expect("failed preparation released original request");
    });
}

/// An actual cleanup owner blocks candidate preparation before any native effect or environment publication.
/// 实际清理所有者在任何原生副作用或环境发布之前阻止候选准备。
/// No inputs or return; release of that exact owner leaves the failed original request cleanly abortable.
/// 无输入与返回值；该精确所有者释放后，原失败请求可正常中止。
#[test]
fn formal_candidate_environment_respects_existing_cleanup_owner() {
    // Actual install code would emit events if admission passed the cleanup fence.
    // 若入场通过清理屏障，实际安装代码会发出事件。
    let (fixture, store, mut transaction) = installation_fixture(false, false, false);
    // Acquire the same permanent exclusive lock used by production recursive deletion.
    // 获取生产递归删除使用的同一永久排他锁。
    let cleanup = lock_plugin_execution_environment_cleanup(&fixture.manager.state, "hook-plugin")
        .expect("original cleanup owner");
    executor().block_on(async {
        // This view retains only the actual request and package selections.
        // 此视图仅保留实际请求及插件包选择。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("candidate view");
        // The exclusive cleanup owner must reject preparation instead of waiting or falling back.
        // 排他清理所有者必须拒绝准备，而非等待或回退。
        let error = fixture
            .manager
            .install_activation_candidate(&candidate)
            .await
            .expect_err("cleanup owns environment");
        assert!(
            error
                .message
                .contains("execution environment is still owned"),
            "unexpected ownership error: {}",
            error.message
        );
    });
    assert!(
        fixture
            .events
            .values
            .lock()
            .expect("native event log")
            .is_empty()
    );
    assert!(
        !fixture
            .manager
            .state
            .plugin_runtime_root
            .join("hook-plugin")
            .exists()
    );
    drop(cleanup);
    store
        .abort_activation_transaction(&mut transaction)
        .expect("failed lock acquisition leaked no request authority");
}

/// Probe and installation deadlines cannot authorize deletion while native work remains live.
/// 原生工作仍存活时，探测与安装截止时间不能授权删除。
/// No inputs or return; production cleanup is excluded by actual file ownership until final retirement.
/// 无输入与返回值；生产清理被实际文件所有权排除，直至最终退役。
#[test]
fn formal_candidate_environment_outlives_observer_until_actual_retirement() {
    for operation in [CandidateOperation::Probe, CandidateOperation::Install] {
        // Both fixtures block inside an actual native callback, independently of async observation.
        // 两种夹具均阻塞于实际原生回调，独立于异步观察。
        let (fixture, store, mut transaction) = match operation {
            CandidateOperation::Probe => candidate_fixture(true, false, false),
            CandidateOperation::Install => installation_fixture(true, false, false),
        };
        executor().block_on(async {
            // This descriptor pins package bytes only, not the request's later environment ownership.
            // 此描述符仅固定插件包字节，不拥有请求后续环境所有权。
            let plugin = {
                // Production inspection freezes the exact version and fingerprint before admission.
                // 生产检查在入场前固定精确版本及指纹。
                let candidate = store
                    .inspect_activation_candidate(&transaction)
                    .expect("exact candidate");
                // A copied package descriptor permits target cleanup checks after the borrowed view ends.
                // 复制插件包描述符，允许在借用视图结束后检查目标清理。
                let plugin = candidate.package().clone();
                fixture
                    .manager
                    .invoke_formal_candidate_operation(
                        &candidate,
                        operation,
                        Instant::now() + WAIT / 10,
                    )
                    .await
                    .expect_err("observer deadline precedes actual callback release");
                plugin
            };
            assert!(
                fixture.events.entered.load(Ordering::Acquire),
                "native callback must actually run"
            );
            // The environment path follows the exact production version/fingerprint directory contract.
            // 环境路径遵循精确生产版本及指纹目录契约。
            let environment = fixture
                .manager
                .state
                .plugin_runtime_root
                .join(&plugin.id)
                .join(plugin.version.as_deref().expect("declared version"))
                .join(&plugin.content_fingerprint);
            assert!(
                environment.is_dir(),
                "actual prepared environment must exist"
            );
            // Probe the actual cleanup lock first, so a regression cannot delete a running fixture's files.
            // 先探测实际清理锁，使回归不会删除正在运行夹具的文件。
            let excluded =
                lock_plugin_execution_environment_cleanup(&fixture.manager.state, &plugin.id);
            assert!(
                excluded.is_err(),
                "actual native candidate must retain its environment lease"
            );
            assert!(
                remove_plugin_execution_environment_generation(
                    &fixture.manager.state,
                    &plugin.id,
                    plugin.version.as_deref().expect("version"),
                    &plugin.content_fingerprint,
                )
                .is_err(),
                "targeted production cleanup must remain excluded"
            );
            assert!(
                remove_obsolete_plugin_execution_environments(
                    &fixture.manager.state,
                    &plugin.id,
                    "2.0.0",
                    "other-generation",
                )
                .is_err(),
                "version cleanup must respect the same lease"
            );
            assert!(environment.is_dir());
            // Observe authority weakly so the test never extends the lifetime it is measuring.
            // 弱观察权威，使测试绝不延长所测寿命。
            let authority = {
                // The original formal call is still executing or initializing in its actual pool.
                // 原正式调用仍在其实际池中执行或初始化。
                let call = fixture.admitted().await;
                Arc::downgrade(call.candidate.as_ref().expect("original native candidate"))
            };
            fixture.events.release();
            fixture
                .manager
                .drain_activation_candidate_calls(transaction.record().request_id())
                .await
                .expect("drain original native work");
            authority_retired(&authority).await;
            remove_plugin_execution_environment_generation(
                &fixture.manager.state,
                &plugin.id,
                plugin.version.as_deref().expect("version"),
                &plugin.content_fingerprint,
            )
            .expect("actual final release permits target cleanup");
            assert!(
                !environment.exists(),
                "production cleanup removed the exact retired environment"
            );
            store
                .abort_activation_transaction(&mut transaction)
                .expect("original request no longer owned");
        });
    }
}
