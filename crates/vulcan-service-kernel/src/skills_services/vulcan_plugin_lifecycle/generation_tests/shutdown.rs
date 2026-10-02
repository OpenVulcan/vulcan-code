//! Verify shutdown admission at real extension and complete workbench publication boundaries.
//! 验证真实扩展与完整工作台发布边界上的关闭准入。
use super::*;

/// Independent production authorities that close plugin publication admission.
/// 关闭插件发布准入的独立生产权威。
#[derive(Clone, Copy, Debug)]
enum ClosedPublicationAdmission {
    /// Reversible drain ownership.
    /// 可恢复的排空归属。
    Draining,
    /// Irreversible process runtime teardown.
    /// 不可逆的进程运行时拆除。
    IrreversibleTeardown,
    /// Retained recovery failure after drain ownership ends.
    /// 排空归属结束后保留的恢复失败。
    RetainedRecoveryFailure,
    /// Actual failed recovery with its registry still closed.
    /// 注册表仍关闭的实际恢复失败。
    RecoveryFailed,
    /// Independently closed actual session registry admission.
    /// 独立关闭的实际会话注册表准入。
    RegistryClosed,
}

impl ClosedPublicationAdmission {
    /// Close this authority using its production lifecycle or registry method.
    /// 使用生产生命周期或注册表方法关闭此权威。
    /// `state` owns the isolated service; returns no value and performs no runtime teardown.
    /// `state` 持有隔离服务；无返回值且不执行运行时拆除。
    fn close(self, state: &SharedServiceBridgeState) {
        // Use the actual service lock shared by publication and shutdown coordination.
        // 使用发布与关闭协调共同持有的实际服务锁。
        let mut service = state.lock().expect("isolated service lock");
        match self {
            Self::Draining => service.service_lifecycle_state.begin_shutdown_drain(),
            Self::IrreversibleTeardown => {
                service.service_lifecycle_state.block_shutdown_recovery();
            }
            Self::RetainedRecoveryFailure | Self::RecoveryFailed => {
                if matches!(self, Self::RecoveryFailed) {
                    service
                        .session_runtime_registry
                        .begin_shutdown()
                        .expect("retain actual failed recovery registry");
                }
                service.service_lifecycle_state.retain_recovery_failure(
                    vulcan_service_api::ServiceStatusFailure {
                        stage: String::from("shutdown-recovery"),
                        code: String::from("shutdown_recovery_failed"),
                        message: String::from("retained isolated recovery failure"),
                    },
                );
            }
            Self::RegistryClosed => {
                service
                    .session_runtime_registry
                    .begin_shutdown()
                    .expect("close actual registry");
            }
        }
    }
}

/// Complete captured publication surfaces, including independently mutable task catalog state.
/// 完整捕获的发布面，包含可独立变更的任务目录状态。
struct ServicePublicationSnapshot {
    /// Immutable execution extension pointer.
    /// 不可变执行扩展指针。
    extension: Arc<vulcan_agent_runtime::agent_runtime::ExtensionGeneration>,
    /// Immutable bridge subagent catalog pointer.
    /// 不可变桥子代理目录指针。
    subagents: Arc<vulcan_task_session::SubagentCatalog>,
    /// Effective task runtime catalog used by future spawn requests.
    /// 后续派生请求使用的生效任务运行时目录。
    task_subagents: vulcan_task_session::SubagentCatalog,
    /// Complete user-visible projection.
    /// 完整用户可见投影。
    workbench: vulcan_service_api::WorkbenchSnapshot,
    /// Independently owned workbench generation counter.
    /// 独立持有的工作台代次计数。
    workbench_generation: u64,
}

impl ServicePublicationSnapshot {
    /// Capture real pointers and projections under one publication lock.
    /// 在同一发布锁内捕获真实指针与投影。
    /// `state` owns the actual fixture; returns owned snapshots without retaining the service lock.
    /// `state` 持有实际夹具；返回拥有所有权的快照，不保留服务锁。
    fn capture(state: &SharedServiceBridgeState) -> Self {
        // Keep all visible surfaces from the same actual service state.
        // 使全部可见发布面来自同一实际服务状态。
        let service = state.lock().expect("capture publication state");
        // This bridge owns both the immutable extension and mutable task catalog.
        // 此桥同时持有不可变扩展及可变任务目录。
        let bridge = service.agent_runtime.as_ref().expect("fixture bridge");
        Self {
            extension: Arc::clone(&bridge.extension_generation),
            subagents: Arc::clone(&bridge.subagent_catalog),
            task_subagents: bridge
                .task_runtime
                .subagent_catalog_snapshot()
                .expect("task catalog"),
            workbench: service.workbench.clone(),
            workbench_generation: service.workbench_generation,
        }
    }

    /// Assert refusal preserved every actual publication surface.
    /// 断言拒绝保留了全部实际发布面。
    /// `state` is queried live; returns no value and fails on pointer, revision or projection drift.
    /// `state` 被实时查询；无返回值，指针、修订或投影漂移时失败。
    fn assert_unchanged(&self, state: &SharedServiceBridgeState) {
        // Read the current slot rather than trusting a historical returned snapshot.
        // 读取当前槽，而非信任历史返回快照。
        let current = Self::capture(state);
        assert!(Arc::ptr_eq(&current.extension, &self.extension));
        assert_eq!(current.extension.revision(), self.extension.revision());
        assert!(Arc::ptr_eq(&current.subagents, &self.subagents));
        assert_eq!(current.task_subagents, self.task_subagents);
        assert_eq!(current.workbench, self.workbench);
        assert_eq!(current.workbench_generation, self.workbench_generation);
    }
}

/// Build a provider-free service with a nonempty task catalog observable across forbidden refresh.
/// 构建无供应商服务，使用可观察禁止刷新影响的非空任务目录。
/// No inputs; returns the real shared fixture without writing user configuration.
/// 无输入；返回真实共享夹具，不写用户配置。
fn shutdown_publication_state() -> SharedServiceBridgeState {
    // The production parser owns definition fields; the test does not duplicate that protocol.
    // 定义字段归生产解析器所有；测试不复制该协议。
    let definition = vulcan_task_session::parse_subagent_definition_text(
        std::path::Path::new("shutdown-publication-sentinel.md"),
        "---\nname: Shutdown Publication Sentinel\nenabled: true\n---\nKeep captured routes.",
        vulcan_task_session::SubagentDefinitionScope::System,
    )
    .expect("parse sentinel through production catalog parser");
    // This sentinel differs from filesystem reload even if application resource roots are empty.
    // 即使应用资源根为空，此哨兵也与文件系统重载不同。
    let sentinel = vulcan_task_session::SubagentCatalog {
        definitions: vec![definition],
        ..Default::default()
    };
    // Real Hook, MCP, extension and task owners exist without provider network activity.
    // 真实 Hook、MCP、扩展及任务所有者存在，但不产生供应商网络活动。
    let mut bridge = crate::tests::test_support::test_agent_runtime_bridge();
    bridge
        .task_runtime
        .replace_subagent_catalog(sentinel.clone())
        .expect("install task sentinel");
    bridge.subagent_catalog = Arc::new(sentinel);
    // This exact service owns the production publication slot and recovery registry.
    // 此精确服务持有生产发布槽及恢复注册表。
    let mut service =
        crate::tests::test_support::test_service_state_with_parent_session(SessionId::new_v4());
    service.agent_runtime = Some(bridge);
    Arc::new(Mutex::new(service))
}

/// Refuse all shutdown authorities through both real extension publishers and complete projection refresh.
/// 通过两种真实扩展发布器及完整投影刷新拒绝全部关闭权威。
/// No inputs or return value; detects partial pointer, task catalog and workbench mutations.
/// 无输入或返回值；检测指针、任务目录及工作台的局部变更。
#[tokio::test]
async fn service_plugin_publication_and_projection_reject_every_shutdown_authority() {
    for reason in [
        ClosedPublicationAdmission::Draining,
        ClosedPublicationAdmission::IrreversibleTeardown,
        ClosedPublicationAdmission::RetainedRecoveryFailure,
        ClosedPublicationAdmission::RecoveryFailed,
        ClosedPublicationAdmission::RegistryClosed,
    ] {
        // Each reason receives an independent actual owner rather than reset flags on one fixture.
        // 每个原因使用独立实际所有者，而非重置同一夹具的标志。
        let state = shutdown_publication_state();
        // Distinct catalog identity makes accidental publication observable.
        // 不同目录身份使意外发布可被观察。
        let replacement = service_vulcan_plugin_runtime(&state)
            .expect("fixture manager")
            .refresh_catalog(&Default::default(), &Default::default())
            .expect("replacement catalog");
        reason.close(&state);
        if matches!(reason, ClosedPublicationAdmission::RecoveryFailed) {
            assert_eq!(
                crate::service_status_snapshot_from_state(&state).lifecycle,
                vulcan_service_api::ServiceLifecycle::RecoveryFailed,
            );
        }
        // Capture complete publication state after establishing this independent authority.
        // 建立此独立权威后捕获完整发布状态。
        let before = ServicePublicationSnapshot::capture(&state);
        assert!(
            publish_extension_generation(
                &state,
                "shutdown-publication-test",
                Arc::clone(&replacement),
                before.extension.mcp_runtime(),
            )
            .expect_err("reject ordinary publication")
            .contains("不能发布插件扩展代次"),
            "{reason:?}"
        );
        before.assert_unchanged(&state);
        // The current-MCP preparation closure must remain unreachable before admission succeeds.
        // 准入成功前，使用当前 MCP 的准备闭包必须不可到达。
        let prepared = std::sync::atomic::AtomicBool::new(false);
        assert!(
            publish_extension_generation_with_current_mcp(
                &state,
                "shutdown-publication-test",
                Arc::clone(&replacement),
                |current| {
                    prepared.store(true, std::sync::atomic::Ordering::SeqCst);
                    Ok(current.clone())
                },
            )
            .is_err(),
            "{reason:?}"
        );
        assert!(!prepared.load(std::sync::atomic::Ordering::SeqCst));
        before.assert_unchanged(&state);
        assert!(
            publish_disabled_plugin_extension_generation(
                &state,
                "shutdown-publication-test",
                replacement,
            )
            .is_err(),
            "{reason:?}"
        );
        before.assert_unchanged(&state);
        assert!(
            crate::settings_runtime_projection::refresh_plugin_catalog_local_state(&state)
                .await
                .err()
                .expect("reject complete projection refresh")
                .contains("不能发布插件扩展代次"),
            "{reason:?}"
        );
        before.assert_unchanged(&state);
    }
}

/// Require real pointer replacement and complete projection commit under open admission.
/// 要求开放准入时真实替换指针并完整提交投影。
/// `state` owns an admitted fixture; returns no value and checks actual catalog identities and revisions.
/// `state` 持有允许准入的夹具；无返回值，检查实际目录身份及修订。
async fn assert_service_publication_admitted(state: &SharedServiceBridgeState) {
    // Keep the original extension and independent workbench counter for comparison.
    // 保留原扩展及独立工作台计数以供比较。
    let before = ServicePublicationSnapshot::capture(state);
    // The manager produces the exact catalog supplied to the actual publisher.
    // 管理器生成交给实际发布器的精确目录。
    let catalog = service_vulcan_plugin_runtime(state)
        .expect("fixture manager")
        .refresh_catalog(&Default::default(), &Default::default())
        .expect("admitted catalog");
    publish_extension_generation(
        state,
        "shutdown-publication-test",
        Arc::clone(&catalog),
        before.extension.mcp_runtime(),
    )
    .expect("open admission permits real extension publication");
    // Inspect the live slot after publication, without trusting copied bridge state.
    // 发布后检查存活槽，不信任复制的桥状态。
    let published = ServicePublicationSnapshot::capture(state);
    assert!(!Arc::ptr_eq(&published.extension, &before.extension));
    assert_eq!(
        published.extension.revision(),
        before.extension.revision() + 1
    );
    assert!(Arc::ptr_eq(
        &published
            .extension
            .plugin_catalog()
            .expect("published catalog"),
        &catalog
    ));
    // Production refresh commits every workbench domain and replaces the sentinel task catalog.
    // 生产刷新提交全部工作台领域，并替换哨兵任务目录。
    let update = crate::settings_runtime_projection::refresh_plugin_catalog_local_state(state)
        .await
        .expect("open admission permits real projection refresh");
    // Read complete committed surfaces from the real service owner.
    // 从真实服务所有者读取完整已提交发布面。
    let refreshed = ServicePublicationSnapshot::capture(state);
    assert!(!Arc::ptr_eq(&refreshed.extension, &published.extension));
    assert_eq!(
        refreshed.workbench_generation,
        before.workbench_generation + 1
    );
    assert_eq!(refreshed.workbench.revision, update.revision);
    assert_eq!(refreshed.workbench_generation, update.revision);
    assert_eq!(refreshed.workbench.lua_skills, update.lua_skills);
    assert_eq!(refreshed.workbench.plugins, update.plugins);
    assert_eq!(refreshed.workbench.agent_skills, update.agent_skills);
    assert_eq!(
        refreshed.workbench.subagent_catalog,
        update.subagent_catalog
    );
    assert_eq!(refreshed.task_subagents, *refreshed.subagents);
    assert!(
        !refreshed
            .task_subagents
            .is_enabled("shutdown-publication-sentinel")
    );
}

/// Validate ordinary admission so rejection tests cannot pass merely because preparation is broken.
/// 验证普通准入，避免拒绝测试仅因准备流程损坏而误通过。
/// No inputs or return value; invokes the same actual production operations with open lifecycle state.
/// 无输入或返回值；在开放生命周期状态下调用相同实际生产操作。
#[tokio::test]
async fn service_plugin_publication_and_projection_allow_open_admission() {
    // Production default lifecycle and registry admission are open in this isolated service.
    // 此隔离服务中的生产默认生命周期及注册表准入为开放。
    let state = shutdown_publication_state();
    assert_service_publication_admitted(&state).await;
}

/// Recover a retained reversible service through the real service resume path before publishing again.
/// 再次发布前，通过真实服务恢复路径恢复保留的可恢复服务。
/// No inputs or return value; covers failed window close without claiming to restore stopped VCE or MCP runtimes.
/// 无输入或返回值；覆盖窗口关闭失败，不声称恢复已停止的 VCE 或 MCP 运行时。
#[tokio::test]
async fn service_plugin_publication_and_projection_resume_after_recoverable_shutdown() {
    // No process teardown occurs: the host retained usable runtimes after window close failed.
    // 不发生进程拆除：窗口关闭失败后，宿主保留可用运行时。
    let state = shutdown_publication_state();
    ClosedPublicationAdmission::Draining.close(&state);
    ClosedPublicationAdmission::RegistryClosed.close(&state);
    ClosedPublicationAdmission::RecoveryFailed.close(&state);
    assert_eq!(
        crate::service_status_snapshot_from_state(&state).lifecycle,
        vulcan_service_api::ServiceLifecycle::RecoveryFailed,
    );
    // Retain actual identities before recovery instead of manually clearing lifecycle fields.
    // 恢复前保留实际身份，而非手动清除生命周期字段。
    let before = ServicePublicationSnapshot::capture(&state);
    assert!(
        state
            .lock()
            .expect("closed service")
            .session_runtime_registry
            .shutdown_admission_is_closed()
    );
    assert!(
        crate::settings_runtime_projection::refresh_plugin_catalog_local_state(&state)
            .await
            .is_err()
    );
    before.assert_unchanged(&state);
    crate::service_resume_after_failed_app_shutdown(&state)
        .expect("actual recovery reopens retained reversible service");
    assert_eq!(
        crate::service_status_snapshot_from_state(&state).lifecycle,
        vulcan_service_api::ServiceLifecycle::Ready,
    );
    before.assert_unchanged(&state);
    {
        // Actual recovery must open the registry and clear its retained lifecycle failure together.
        // 实际恢复必须共同开放注册表并清除保留的生命周期失败。
        let service = state.lock().expect("recovered service");
        assert!(
            !service
                .session_runtime_registry
                .shutdown_admission_is_closed()
        );
        assert!(!service.service_lifecycle_state.shutdown_drain_active);
        assert!(!service.service_lifecycle_state.shutdown_recovery_blocked);
        assert!(
            service
                .service_lifecycle_state
                .last_recovery_failure
                .is_none()
        );
    }
    assert_service_publication_admitted(&state).await;
}
