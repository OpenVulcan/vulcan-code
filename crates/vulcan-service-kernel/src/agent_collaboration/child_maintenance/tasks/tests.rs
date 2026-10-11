//! Real registered maintenance workers with deterministic publication and cancellation boundaries.
//! 使用确定发布及取消边界的真实已登记维护工作任务。

use super::*;

/// Root/project lifecycle and maintenance registration exclude each other in both arrival orders.
/// 根及项目生命周期与维护登记在两种到达顺序下都互相排除。
/// Exercises real service guards and worker ownership, including an empty retained project session list.
/// 验证真实服务守卫和工作任务归属，包括保留项目会话列表为空的情况。
#[tokio::test]
async fn maintenance_storage_lifecycle_exclusion_covers_root_project_and_late_registration() {
    // The fixture owns the actual child profile and registered indexed root.
    // 夹具持有实际子档案及已登记索引根。
    let (state, handle, source, _backend, _runtime) = super::super::tests::fixture();
    // Both arrival orders use the same canonical registry.
    // 两种到达顺序使用同一规范注册表。
    let registry = state.lock().unwrap().session_runtime_registry.clone();
    // Root identity comes from the durable child creation record.
    // 根身份来自可靠子创建记录。
    let profile = source.profile().clone();
    // Project ownership comes from the real locator, independent from document display tags and the UI list.
    // 项目归属来自真实定位器，独立于文档展示分区及界面列表。
    let scope = MaintenanceStorageScope::capture(&handle, &registry).unwrap();
    // Lifecycle first must block registration without ever polling submitted work.
    // 生命周期先到时必须阻止登记，绝不轮询提交的工作。
    let root_guard = crate::project_conversation_services::begin_session_lifecycle_transition(
        &state,
        [scope.root_session_id],
        "archive",
    )
    .unwrap();
    assert!(
        spawn(&state, &registry, &handle, profile.clone(), async {
            panic!("lifecycle-excluded work must not run")
        })
        .await
        .is_err()
    );
    drop(root_guard);
    // Project reservation remains authoritative even if the UI did not retain this root.
    // 即使界面未保留此根，项目预留仍是权威。
    let project_guard = crate::project_conversation_services::begin_project_lifecycle_transition(
        &state,
        &scope.project_id,
        [],
        "remove",
    )
    .unwrap();
    assert!(
        spawn(&state, &registry, &handle, profile.clone(), async {
            panic!("project-excluded work must not run")
        })
        .await
        .is_err()
    );
    drop(project_guard);
    // MainOnly Stop owns root run admission, but must not suppress independent child maintenance.
    // MainOnly 停止持有根运行准入，但不能压制独立子维护。
    let root_stop_guard = crate::project_conversation_services::begin_session_run_stop_exclusion(
        &state,
        scope.root_session_id,
    )
    .unwrap();
    // Worker first must prevent both root archival and whole-project removal until actual cleanup.
    // 工作任务先到时，实际清理前必须阻止根归档及整个项目移除。
    let result = spawn(&state, &registry, &handle, profile.clone(), async {
        std::future::pending().await
    })
    .await
    .unwrap();
    drop(root_stop_guard);
    assert!(
        crate::project_conversation_services::begin_session_lifecycle_transition(
            &state,
            [scope.root_session_id],
            "archive",
        )
        .is_err()
    );
    assert!(
        crate::project_conversation_services::begin_project_lifecycle_transition(
            &state,
            &scope.project_id,
            [],
            "remove",
        )
        .is_err()
    );
    // Unrelated projects remain available while this child owns maintenance.
    // 此子任务持有维护期间，无关项目仍可操作。
    drop(
        crate::project_conversation_services::begin_project_lifecycle_transition(
            &state,
            "unrelated-maintenance-project",
            [],
            "archive",
        )
        .unwrap(),
    );
    stop(&state, handle.session_id()).await.unwrap();
    assert!(result.await.unwrap().is_err());
    // An archived root is unregistered by the real lifecycle protocol; late maintenance cannot reuse a retained child.
    // 真实生命周期协议会卸载已归档根；迟到维护不能复用保留的子任务。
    registry
        .unregister_lifecycle_session(scope.root_session_id)
        .unwrap();
    assert!(
        spawn(&state, &registry, &handle, profile, async {
            panic!("unregistered root must not authorize maintenance")
        })
        .await
        .unwrap_err()
        .contains("session")
    );
}

/// Waiting on unpublished business results is cancellable and a dropped result receiver cannot orphan the worker.
/// 等待未发布业务结果可以取消，丢弃结果接收端不能使工作任务失去归属。
#[tokio::test]
async fn maintenance_worker_stop_cancels_publication_wait_and_rejects_late_restart() {
    // A real completed child and task registry provide exact business ownership.
    // 实际已完成子任务和任务注册表提供精确业务归属。
    let (state, handle, source, _backend, runtime) = super::super::tests::fixture();
    // Registration rechecks this captured profile under the canonical gate.
    // 登记在规范门内重新检查此捕获档案。
    let profile = source.profile().clone();
    // Use the actual service registry, not a synthetic cancellation registry.
    // 使用实际服务注册表，而非合成取消注册表。
    let registry = state.lock().unwrap().session_runtime_registry.clone();
    runtime
        .expect_durable_task_return(handle.session_id(), &profile.business_execution_id)
        .unwrap();
    // Observe the actual worker entering its unpublished-result wait.
    // 观察实际工作任务进入未发布结果等待。
    let (entered, waiting) = tokio::sync::oneshot::channel();
    // The runtime is shared with assertions on the real auxiliary exclusion.
    // 运行时与真实辅助互斥断言共享。
    let work_runtime = runtime.clone();
    // Capture the original execution without moving the registration proof.
    // 捕获原执行，不移动登记凭据。
    let business = profile.business_execution_id.clone();
    // This stable child key selects both its runtime and service ownership.
    // 此稳定子任务键同时选择其运行时和服务归属。
    let child = handle.session_id();
    // The receiver is deliberately dropped; service ownership must remain until Stop completes.
    // 刻意丢弃接收端；服务所有权必须保持到停止完成。
    let result = spawn(&state, &registry, &handle, profile.clone(), async move {
        let _admission = work_runtime
            .try_reserve_child_context_maintenance(child, &business)?
            .unwrap();
        entered.send(()).unwrap();
        work_runtime
            .wait_for_durable_task_return(child, &business)
            .await?;
        Ok(None)
    })
    .await
    .unwrap();
    waiting.await.unwrap();
    drop(result);
    assert!(
        state
            .lock()
            .unwrap()
            .child_maintenance_tasks
            .contains_key(&child)
    );
    assert!(
        runtime
            .try_reserve_child_question_execution(child)
            .unwrap()
            .is_none()
    );
    // Tree selection includes an already completed child under its ordinary main parent.
    // 树选择包含普通主父会话下已经完成的子任务。
    stop_tree(
        &state,
        &runtime,
        Uuid::parse_str(&profile.parent_session_id).unwrap(),
    )
    .await
    .unwrap();
    assert!(state.lock().unwrap().child_maintenance_tasks.is_empty());
    assert!(state.lock().unwrap().child_maintenance_owners.is_empty());
    assert!(
        runtime
            .try_reserve_child_question_execution(child)
            .unwrap()
            .is_some()
    );
    assert!(
        handle
            .read(|document| document.metadata.context_maintenance.is_none())
            .unwrap()
    );
    assert!(
        spawn(&state, &registry, &handle, profile, async {
            panic!("stopped work must never be polled")
        })
        .await
        .is_err()
    );
}

/// Cancellation during admitted blocking publication waits for ownership transfer and closes only maintenance.
/// 已准入阻塞发布期间的取消等待所有权转交，且仅闭合维护。
#[tokio::test]
async fn maintenance_worker_stop_survives_cancelled_stop_caller_and_blocked_admission() {
    // This backend pauses actual Core start publication before service ownership transfer.
    // 此后端在服务所有权转交之前暂停实际 Core 起始发布。
    let (state, handle, source, backend, runtime) = super::super::tests::fixture();
    // The original business must survive this auxiliary cancellation unchanged.
    // 原业务必须在本辅助取消后保持不变。
    let profile = source.profile().clone();
    // The same canonical registry also drains the blocked writer in production.
    // 生产中同一规范注册表也负责排空阻塞写入器。
    let registry = state.lock().unwrap().session_runtime_registry.clone();
    // Two channels distinguish entry into storage from permission to finish it.
    // 两个通道区分进入存储及允许完成存储。
    let (entered, observed) = tokio::sync::oneshot::channel();
    // Release only the deliberately paused publication.
    // 仅释放刻意暂停的发布。
    let (release, blocked) = std::sync::mpsc::channel();
    *backend.gate.lock().unwrap() = Some((entered, blocked));
    // Work owns its own service reference while it can mutate domain state.
    // 工作在能够修改领域状态期间持有自己的服务引用。
    let work_state = state.clone();
    // Blocking admission retains this registry after work cancellation.
    // 工作取消后阻塞准入保留此注册表。
    let work_registry = registry.clone();
    // Keep the exact child handle inside the admitted work.
    // 在已准入工作内保留精确子句柄。
    let work_handle = handle.clone();
    // The real runtime guard excludes replacement work throughout failed or cancelled publication.
    // 真实运行时守卫在失败或取消发布期间排除替代工作。
    let admission = runtime
        .try_reserve_child_context_maintenance(handle.session_id(), &profile.business_execution_id)
        .unwrap()
        .unwrap();
    // The worker must be cancelled before this future can perform any further effects.
    // 此 Future 继续产生效果之前必须取消工作任务。
    let result = spawn(&state, &registry, &handle, profile.clone(), async move {
        prepare(
            &work_state,
            &work_registry,
            &work_handle,
            admission,
            source,
            10,
        )
        .await?;
        std::future::pending().await
    })
    .await
    .unwrap();
    observed.await.unwrap();
    // Subscribe before Stop to prove cancellation was signalled even while the writer holds its gate.
    // 停止前订阅，证明写入器持有门时仍发出了取消通知。
    let mut cancelled = state.lock().unwrap().child_maintenance_tasks[&handle.session_id()]
        .cancel
        .subscribe();
    // The caller is disposable; actual cleanup must remain service-owned.
    // 调用者可以丢弃；实际清理必须保持服务所有。
    let stop_state = state.clone();
    // This exact ID cannot select the parent or another child.
    // 此精确 ID 不能选择父会话或其他子任务。
    let child = handle.session_id();
    // Abort the caller only after it has definitely sent the cancellation request.
    // 仅在调用者确定发送取消请求之后中止它。
    let caller = tokio::spawn(async move { stop(&stop_state, child).await });
    cancelled.changed().await.unwrap();
    assert!(*cancelled.borrow());
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    assert!(
        runtime
            .try_reserve_child_question_execution(child)
            .unwrap()
            .is_none()
    );
    release.send(()).unwrap();
    assert!(result.await.unwrap().unwrap_err().contains("stopped"));
    handle
        .read(|document| {
            assert_eq!(
                document.metadata.child_execution_profile.as_ref(),
                Some(&profile)
            );
            assert_eq!(
                document
                    .metadata
                    .context_maintenance
                    .as_ref()
                    .unwrap()
                    .terminal
                    .as_ref()
                    .unwrap()
                    .reason,
                TurnClosureReason::Stopped
            );
            assert_eq!(document.memory_revision(), document.durable_revision());
        })
        .unwrap();
    assert!(state.lock().unwrap().child_maintenance_tasks.is_empty());
    assert!(state.lock().unwrap().child_maintenance_owners.is_empty());
    assert!(
        runtime
            .try_reserve_child_question_execution(child)
            .unwrap()
            .is_some()
    );
}

/// Normal worker errors settle admitted maintenance and shutdown rejects unregistered replacement work.
/// 正常工作错误结算已准入维护，关闭拒绝未登记替代工作。
#[tokio::test]
async fn maintenance_worker_failure_settles_and_shutdown_rejects_registration() {
    for panic_work in [false, true] {
        // Start with the same published business used by actual service admission.
        // 从实际服务准入使用的同一已发布业务开始。
        let (state, handle, source, _backend, runtime) = super::super::tests::fixture();
        // Preserve exact registration proof for the later shutdown rejection.
        // 为后续关闭拒绝保留精确登记凭据。
        let profile = source.profile().clone();
        // Real lifecycle gates remain shared with the worker's error cleanup.
        // 真实生命周期门仍与工作任务错误清理共享。
        let registry = state.lock().unwrap().session_runtime_registry.clone();
        // Hold auxiliary admission until it is transferred to the canonical maintenance owner.
        // 持有辅助准入，直到其转交规范维护所有者。
        let admission = runtime
            .try_reserve_child_context_maintenance(
                handle.session_id(),
                &profile.business_execution_id,
            )
            .unwrap()
            .unwrap();
        // Service dependencies remain on the real worker future.
        // 服务依赖保留在真实工作 Future 上。
        let work_state = state.clone();
        // Publication and error settlement use this registry.
        // 发布和错误结算使用此注册表。
        let work_registry = registry.clone();
        // Never substitute the parent handle for the child.
        // 绝不以父句柄替代子句柄。
        let work_handle = handle.clone();
        // This controlled compactor failure happens only after actual domain admission.
        // 此可控压缩器失败仅在实际领域准入之后发生。
        let result = spawn(&state, &registry, &handle, profile.clone(), async move {
            prepare(
                &work_state,
                &work_registry,
                &work_handle,
                admission,
                source,
                10,
            )
            .await?;
            if panic_work {
                panic!("injected maintenance work panic");
            }
            Err(String::from("injected compactor failure"))
        })
        .await
        .unwrap()
        .await
        .unwrap();
        assert_eq!(
            result.unwrap_err(),
            if panic_work {
                "Child maintenance work panicked."
            } else {
                "injected compactor failure"
            }
        );
        assert_eq!(
            handle
                .read(|document| document
                    .metadata
                    .context_maintenance
                    .as_ref()
                    .unwrap()
                    .terminal
                    .as_ref()
                    .unwrap()
                    .reason)
                .unwrap(),
            TurnClosureReason::Failed
        );
        assert!(state.lock().unwrap().child_maintenance_owners.is_empty());
        state
            .lock()
            .unwrap()
            .service_lifecycle_state
            .begin_shutdown_drain();
        stop_all(&state).await.unwrap();
        assert!(
            spawn(&state, &registry, &handle, profile, async {
                panic!("shutdown work must never be polled")
            })
            .await
            .is_err()
        );
    }
}

/// Shutdown reaches every worker before a blocked child's publication can delay settlement.
/// 关闭在阻塞子任务的发布延迟结算之前到达每个工作任务。
#[tokio::test]
async fn maintenance_shutdown_signals_all_workers_before_waiting_for_storage() {
    // Both actual children are registered under one service, with independently blocked storage.
    // 两个实际子任务登记到同一服务，存储独立阻塞。
    let (state, first, first_source, first_backend, first_runtime) = super::super::tests::fixture();
    // The second fixture contributes only its canonical child and real runtime admission.
    // 第二个夹具仅提供其规范子任务及真实运行时准入。
    let (second_state, second, second_source, second_backend, second_runtime) =
        super::super::tests::fixture();
    // One registry proves both writers are still owned while shutdown is waiting.
    // 一个注册表证明关闭等待期间两个写入器仍有归属。
    let registry = state.lock().unwrap().session_runtime_registry.clone();
    registry.register(second.clone()).unwrap();
    // The second root also owns its embedded directory and must participate in admission.
    // 第二个根同样拥有其嵌入目录，必须参与准入。
    let second_root = second_state
        .lock()
        .unwrap()
        .session_runtime_registry
        .get(Uuid::parse_str(&second_source.profile().root_session_id).unwrap())
        .unwrap();
    registry.register(second_root).unwrap();
    // Retain explicit releases so the test controls when either disk write may finish.
    // 保留显式释放端，使测试控制任一磁盘写入何时可以完成。
    let mut releases = Vec::new();
    // Completion receivers observe the real worker outcomes after shutdown cleanup.
    // 完成接收端观察关闭清理后的真实工作结果。
    let mut results = Vec::new();
    // Cancellation observers are subscribed before shutdown begins.
    // 取消观察者在关闭开始之前订阅。
    let mut cancellations = Vec::new();
    for (handle, source, backend, runtime) in [
        (first, first_source, first_backend, first_runtime),
        (second, second_source, second_backend, second_runtime),
    ] {
        // Every worker carries its own immutable profile and auxiliary guard.
        // 每个工作任务携带自己的不可变档案及辅助守卫。
        let profile = source.profile().clone();
        // The admitted write reports entry before the test starts shutdown.
        // 已准入写入在测试开始关闭前报告进入。
        let (entered, observed) = tokio::sync::oneshot::channel();
        // Neither write can finish until the test has observed both cancellation signals.
        // 测试观察到两个取消信号之前，两次写入都不能完成。
        let (release, blocked) = std::sync::mpsc::channel();
        *backend.gate.lock().unwrap() = Some((entered, blocked));
        releases.push(release);
        // Use the runtime guard corresponding to this exact child.
        // 使用本精确子任务对应的运行时守卫。
        let admission = runtime
            .try_reserve_child_context_maintenance(
                handle.session_id(),
                &profile.business_execution_id,
            )
            .unwrap()
            .unwrap();
        // Each worker retains the actual service during its domain admission.
        // 每个工作任务在领域准入期间保留实际服务。
        let work_state = state.clone();
        // Cancellation drains this same registry's blocking publication.
        // 取消排空同一注册表的阻塞发布。
        let work_registry = registry.clone();
        // The handle stays exact while its caller is cancellable.
        // 调用者可取消期间，句柄保持精确。
        let work_handle = handle.clone();
        results.push(
            spawn(&state, &registry, &handle, profile, async move {
                prepare(
                    &work_state,
                    &work_registry,
                    &work_handle,
                    admission,
                    source,
                    10,
                )
                .await?;
                std::future::pending().await
            })
            .await
            .unwrap(),
        );
        observed.await.unwrap();
        cancellations.push(
            state.lock().unwrap().child_maintenance_tasks[&handle.session_id()]
                .cancel
                .subscribe(),
        );
    }
    state
        .lock()
        .unwrap()
        .service_lifecycle_state
        .begin_shutdown_drain();
    // The actual shutdown future must wait for both reliable settlements.
    // 实际关闭 Future 必须等待两个可靠结算。
    let shutdown_state = state.clone();
    // Spawn the real shutdown entry while both persistence calls remain blocked.
    // 在两个持久化调用仍阻塞时启动真实关闭入口。
    let shutdown = tokio::spawn(async move { stop_all(&shutdown_state).await });
    for cancellation in &mut cancellations {
        tokio::time::timeout(std::time::Duration::from_secs(2), cancellation.changed())
            .await
            .unwrap()
            .unwrap();
        assert!(*cancellation.borrow());
    }
    assert!(!shutdown.is_finished());
    for release in releases {
        release.send(()).unwrap();
    }
    shutdown.await.unwrap().unwrap();
    for result in results {
        assert!(result.await.unwrap().unwrap_err().contains("stopped"));
    }
    assert!(state.lock().unwrap().child_maintenance_tasks.is_empty());
    assert!(state.lock().unwrap().child_maintenance_owners.is_empty());
}
