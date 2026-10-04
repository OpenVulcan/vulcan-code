//! Actual private target Hooks transfer original VM state while retaining independent filesystem ownership.
//! 实际私有目标 Hook 转交原 VM 状态，同时保留独立文件系统所有权。

use super::*;
mod mcp_receipts;
mod namespaces;
mod native_sources;
mod receipts;

/// Read actual committed initialization events; fixture owns the real native publisher and returns the count.
/// 读取实际已提交初始化事件；fixture 拥有真实原生发布器并返回数量。
fn target_initializations(fixture: &Fixture) -> usize {
    fixture
        .events
        .values
        .lock()
        .expect("native events")
        .iter()
        .filter(|event| event.topic == "target-init")
        .count()
}

/// Return the retained actual Hook instance; absence is a test failure rather than a fabricated owner.
/// 返回保留实际 Hook 实例；缺失为测试失败，不虚构所有者。
fn original_hook(fixture: &Fixture) -> Arc<crate::runtime::formal::PersistentRuntime> {
    fixture
        .owner()
        .hooks
        .lock()
        .expect("Hook registry")
        .get("hook-plugin")
        .cloned()
        .expect("actual original Hook")
}

/// Open the exact original request lock for a real nonblocking exclusive-lock assertion.
/// 打开精确原请求锁，用于真实非阻塞排他锁断言。
fn original_request_file(fixture: &Fixture, request_id: Uuid) -> std::fs::File {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            fixture
                .manager
                .state
                .user_plugin_root
                .join(".vulcan-generations/locks")
                .join(format!("hook-plugin.transaction.{request_id}.lock")),
        )
        .expect("actual permanent original request file")
}

/// Observe sealed completion using only memory while ordinary no-MCP target preparation holds registration exclusion.
/// 普通无 MCP 目标准备持有注册互斥时，仅使用内存观测封闭完成。
struct CompletionProbe<'a> {
    /// Records the actual confirmation without any durable or external operation.
    /// 记录实际确认，不执行任何持久或外部操作。
    confirmed: &'a AtomicBool,
    /// Expected original request remains separate from instance identity.
    /// 期望原请求保持与实例身份分离。
    request_id: Uuid,
    /// Exact original full target cloned before the durable boundary.
    /// 持久边界之前克隆的精确原完整目标。
    target: crate::VulcanPluginActivation,
    /// Actual original host route generation.
    /// 实际原宿主路由代次。
    generation: u64,
}

/// The prepared probe borrows the actual gate and original fixture observation without retaining resources.
/// 已准备探针借用实际注册门及原夹具观测，不保留资源。
struct CompletionProbeGuard<'gate, 'probe> {
    /// Original fixture-owned observation remains fixed throughout confirmation.
    /// 原夹具拥有观测在确认全程保持固定。
    probe: &'gate CompletionProbe<'probe>,
    /// Actual registration evidence cannot escape its existing MutexGuard.
    /// 实际注册证据不可逃逸既有 MutexGuard。
    _registration: &'gate crate::VulcanPluginActivationTargetRegistration<'gate>,
}

impl<'probe> CompletionProbe<'probe> {
    /// Prepare exact no-MCP expectations before completion; return only a fixture memory adapter.
    /// 完成之前准备精确无 MCP 期望；仅返回夹具内存适配器。
    fn new(
        confirmed: &'probe AtomicBool,
        transaction: &crate::VulcanPluginActivationTransaction,
        catalog: &Arc<crate::VulcanPluginCatalogSnapshot>,
    ) -> Self {
        Self {
            confirmed,
            request_id: transaction.record().request_id(),
            target: transaction.record().target().clone(),
            generation: catalog.plugins["hook-plugin"].generation,
        }
    }
}

impl<'probe> crate::VulcanPluginNativeMcpHandoffAdapter for CompletionProbe<'probe> {
    type Guard<'gate>
        = CompletionProbeGuard<'gate, 'probe>
    where
        Self: 'gate;
    /// This no-MCP probe cannot supply any registered native host authority.
    /// 此无 MCP 探针不可提供任何已注册原生宿主权威。
    fn authority(&self) -> Option<&crate::VulcanPluginRegisteredNativeMcpHostAuthority> {
        None
    }
    /// Validate the real held registration gate and return its lifetime-bound observation guard.
    /// 校验真实已持有注册门，并返回受其寿命约束的观测守卫。
    fn prepare<'gate>(
        &'gate self,
        manager: &crate::VulcanPluginRuntimeManager,
        _transaction: &crate::VulcanPluginActivationTransaction,
        _catalog: &Arc<crate::VulcanPluginCatalogSnapshot>,
        registration: &'gate crate::VulcanPluginActivationTargetRegistration<'gate>,
    ) -> Result<Self::Guard<'gate>, crate::VulcanPluginRuntimeError> {
        assert!(registration.matches_manager(manager));
        assert!(
            manager.state.session_registration_gate.try_lock().is_err(),
            "registration precedes participant acquisition"
        );
        Ok(CompletionProbeGuard {
            probe: self,
            _registration: registration,
        })
    }
}

impl crate::VulcanPluginActivationTargetHandoffParticipant for CompletionProbeGuard<'_, '_> {
    /// This actual no-MCP fixture cannot furnish sealed MCP resource evidence.
    /// 此实际无 MCP 夹具无法提供封闭 MCP 资源证据。
    fn mcp_evidence(&self) -> Option<&crate::VulcanPluginMcpTargetHandoffEvidence> {
        None
    }
    /// Confirm only exact original witness identities and an infallible memory flag.
    /// 仅确认精确原凭据身份及不可失败内存标志。
    fn confirm_completed(&mut self, completion: &crate::VulcanPluginActivationTargetCompletion) {
        assert_eq!(completion.request_id(), self.probe.request_id);
        assert_eq!(completion.target(), &self.probe.target);
        assert_eq!(completion.generation(), self.probe.generation);
        assert!(completion.mcp_instance_ids().is_none());
        self.probe.confirmed.store(true, Ordering::Release);
    }
}

/// Real closed initialization, failed publisher and confirmed completion preserve the same pool/session/domain and VM state.
/// 真实关闭初始化、失败发布器及已确认完成保留相同池／会话／域及 VM 状态。
/// No inputs or return value; request and environment file locks prove independent post-transfer lifetimes.
/// 无输入与返回值；请求及环境文件锁证明转交后的独立寿命。
#[test]
fn formal_private_target_hook_handoff_preserves_original_core_and_environment() {
    // Use the actual production-selected original fixture and request.
    // 使用实际生产选择的原夹具及请求。
    let (fixture, store, mut transaction) = published_enabled_target();
    // Keep the actual closed staging Arc issued by production routing.
    // 保留生产路由签发的实际关闭暂存 Arc。
    let staged = fixture
        .manager
        .stage_activation_target_handoff(
            &transaction,
            &BTreeSet::new(),
            &BTreeSet::from(["hook-plugin".to_owned()]),
        )
        .expect("actual closed target");
    // Capture actual native initialization count before private prewarming.
    // 在私有预热前捕获实际原生初始化次数。
    let initializations = target_initializations(&fixture);
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .prewarm_activation_target_hook(&transaction, &staged)
                .await
                .expect("actual target config initialization")
        );
        assert!(
            !fixture
                .manager
                .prewarm_plugin_runtime("hook-plugin")
                .await
                .expect("closed ordinary prewarming returns no admission"),
            "ordinary Hook remains ineligible"
        );
        assert!(
            fixture
                .manager
                .ensure_formal_hook(
                    "hook-plugin",
                    staged.plugins["hook-plugin"].generation,
                    Instant::now() + Duration::from_millis(VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS)
                )
                .await
                .is_err(),
            "actual ordinary Hook admission remains closed"
        );
        assert!(
            fixture
                .manager
                .prewarm_activation_target_hook(&transaction, &staged)
                .await
                .expect("same private original")
        );
        assert_eq!(target_initializations(&fixture), initializations + 1);
        assert_eq!(
            store
                .target_hook_initialization_receipt(
                    "hook-plugin",
                    transaction.record().request_id()
                )
                .expect("actual durable receipt")
                .expect("actual original attempt")
                .phase(),
            crate::VulcanPluginTargetHookInitializationPhase::Succeeded
        );
        // Retain the exact original instance or authority rather than creating a replacement.
        // 保留精确原实例或权威，不创建替代物。
        let original = original_hook(&fixture);
        // Capture original pool, session and initialization identities before transfer.
        // 转交前捕获原池、会话及初始化身份。
        let identities = original.target_test_identity();
        // Select the recorded immutable original core pool identity.
        // 选择已记录不可变原核心池身份。
        let pool = identities.0.as_ref().expect("actual pool");
        // Observe or retain only the actual original core domain.
        // 仅观测或保留实际原核心域。
        let domain = fixture.owner().host.target_test_domain_identity(pool);
        assert!(identities.3 && !identities.4);
        // Keep the exact original UUID for real disk and file-lock checks.
        // 保留精确原 UUID，用于真实磁盘及文件锁检查。
        let request_id = transaction.record().request_id();
        // Open the original permanent request file for an actual lock assertion.
        // 打开原永久请求文件，用于实际锁断言。
        let request = original_request_file(&fixture, request_id);
        assert!(matches!(
            request.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        assert!(
            crate::runtime::environment::lock_plugin_execution_environment_cleanup(
                &fixture.manager.state,
                "hook-plugin"
            )
            .is_err()
        );
        assert!(
            fixture
                .manager
                .publish_activation_target_handoff(&transaction, &staged, |_| Err(
                    "controlled publisher failure".into()
                ))
                .is_err()
        );
        assert!(Arc::ptr_eq(&staged, &fixture.manager.snapshot()));
        assert_eq!(original.target_test_identity(), identities);
        // Retain the actual final Arc accepted by the synchronous publisher.
        // 保留同步发布器接受的实际最终 Arc。
        let publication = fixture
            .manager
            .publish_activation_target_handoff(&transaction, &staged, |_| Ok(()))
            .expect("actual full extension handoff");
        // Read only the exact final Arc sealed by this successful publication.
        // 仅读取此次成功发布封闭的精确最终 Arc。
        let catalog = Arc::clone(publication.catalog());
        // A real foreign manager cannot consume another manager's sealed publication.
        // 真实不同管理器不能消费另一管理器的封闭发布。
        let foreign = Fixture::new();
        assert!(
            foreign
                .manager
                .complete_activation_target_handoff(&mut transaction, &publication)
                .is_err(),
            "foreign manager cannot complete this original publisher"
        );
        // Participant preparation must run after the same actual registration gate has been acquired.
        // 参与者准备必须在相同实际注册门已取得之后执行。
        let confirmed = AtomicBool::new(false);
        // The GAT adapter can return only guards borrowing this completion's actual registration scope.
        // GAT 适配器仅可返回借用此次完成实际注册作用域的守卫。
        let probe = CompletionProbe::new(&confirmed, &transaction, &catalog);
        fixture
            .manager
            .complete_activation_target_handoff_with(&mut transaction, &publication, &probe)
            .expect("atomic original handoff completion");
        assert!(confirmed.load(Ordering::Acquire));
        // Observe identities after transfer without retaining physical owners.
        // 转交后观测身份，不保留物理所有者。
        let transferred = original.target_test_identity();
        assert_eq!(
            (&transferred.0, &transferred.1, &transferred.2),
            (&identities.0, &identities.1, &identities.2)
        );
        assert!(!transferred.3 && !transferred.4);
        assert_eq!(
            fixture.owner().host.target_test_domain_identity(pool),
            domain
        );
        // Retain the sole formal parent for direct original session observation.
        // 保留唯一正式父级，以直接观测原会话。
        let core = fixture.owner();
        // Query the same original core session instead of inferring readiness.
        // 查询相同原核心会话，不推断就绪。
        let session = core
            .host
            .runtime()
            .session(transferred.1.as_ref().expect("same session"))
            .expect("actual session");
        assert_eq!(
            session.phase,
            luaskills::runtime::embedded::EmbeddedSessionPhase::Ready
        );
        assert!(session.active_operation.is_none());
        assert_eq!(session.queued_calls, 0);
        // Select the declared target Hook by semantic plugin identity.
        // 按语义插件身份选择声明目标 Hook。
        let hook = catalog
            .enabled_hooks
            .values()
            .find(|hook| hook.plugin_id == "hook-plugin")
            .expect("actual target Hook");
        for expected in [1, 2] {
            // Preserve the actual preparation or business result without inventing completion.
            // 保留实际准备或业务结果，不虚构完成。
            let result = fixture
                .manager
                .invoke_formal_hook(
                    hook,
                    json!({"payload_json":"{}", "host_timeout_ms":1000}),
                    None,
                    Instant::now() + WAIT,
                )
                .await
                .expect("same original business VM");
            assert_eq!(result["count"], expected);
        }
        assert_eq!(target_initializations(&fixture), initializations + 1);
        drop(transaction);
        request
            .try_lock()
            .expect("original transaction no longer Busy while Hook lives");
        drop(request);
        assert!(
            crate::runtime::environment::lock_plugin_execution_environment_cleanup(
                &fixture.manager.state,
                "hook-plugin"
            )
            .is_err(),
            "same domain retains independent environment ownership"
        );
        fixture
            .manager
            .begin_draining("hook-plugin")
            .expect("normal target drain");
        retire_original_hook(&fixture).await;
        drop(original);
        crate::runtime::environment::lock_plugin_execution_environment_cleanup(
            &fixture.manager.state,
            "hook-plugin",
        )
        .expect("environment releases only after actual original retirement");
    });
}

/// Ordinary full publication cannot complete a declared Hook without the original private initialization.
/// 普通完整发布在缺少原私有初始化时不能完成声明 Hook。
/// No inputs or return value; disk stays Published and a copied staged Arc never starts a VM.
/// 无输入与返回值；磁盘保持 Published，复制暂存 Arc 绝不启动 VM。
#[test]
fn formal_private_target_hook_rejects_wrong_route_and_missing_original_initialization() {
    // Use the actual production-selected original fixture and request.
    // 使用实际生产选择的原夹具及请求。
    let (fixture, store, mut transaction) = published_enabled_target();
    // Keep the actual closed staging Arc issued by production routing.
    // 保留生产路由签发的实际关闭暂存 Arc。
    let staged = fixture
        .manager
        .stage_activation_target_handoff(
            &transaction,
            &BTreeSet::new(),
            &BTreeSet::from(["hook-plugin".into()]),
        )
        .expect("staged original");
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .prewarm_activation_target_hook(&transaction, &Arc::new((*staged).clone()))
                .await
                .is_err()
        );
    });
    assert!(
        fixture
            .owner()
            .hooks
            .lock()
            .expect("actual registry")
            .is_empty()
    );
    // Retain the actual final Arc accepted by the synchronous publisher.
    // 保留同步发布器接受的实际最终 Arc。
    let publication = fixture
        .manager
        .publish_activation_target_handoff(&transaction, &staged, |_| Ok(()))
        .expect("published complete extension pointer");
    assert!(
        fixture
            .manager
            .complete_activation_target_handoff(&mut transaction, &publication)
            .is_err()
    );
    assert_eq!(
        store
            .activation_request("hook-plugin", transaction.record().request_id())
            .expect("actual disk request")
            .expect("original record")
            .phase(),
        crate::VulcanPluginActivationTransactionPhase::Published
    );
}

/// Exact persisted Completed readback confirms transfer after a lost checkpoint observation without replaying Lua.
/// 精确持久 Completed 回读在检查点观察丢失后确认转交，不重放 Lua。
/// No inputs or return value; the real store writes Completed before test-only loss of write/readback observation.
/// 无输入与返回值；真实存储在仅测试丢失写入／回读观测之前写入 Completed。
#[test]
fn formal_private_target_hook_reconciles_exact_completed_readback_without_replay() {
    // Use the actual production-selected original fixture and request.
    // 使用实际生产选择的原夹具及请求。
    let (fixture, store, mut transaction) = published_enabled_target();
    // Keep the actual closed staging Arc issued by production routing.
    // 保留生产路由签发的实际关闭暂存 Arc。
    let staged = fixture
        .manager
        .stage_activation_target_handoff(
            &transaction,
            &BTreeSet::new(),
            &BTreeSet::from(["hook-plugin".into()]),
        )
        .expect("actual stage");
    executor().block_on(async {
        fixture
            .manager
            .prewarm_activation_target_hook(&transaction, &staged)
            .await
            .expect("actual private initialization");
        // Retain the exact original instance or authority rather than creating a replacement.
        // 保留精确原实例或权威，不创建替代物。
        let original = original_hook(&fixture);
        // Capture original pool, session and initialization identities before transfer.
        // 转交前捕获原池、会话及初始化身份。
        let identities = original.target_test_identity();
        // Capture actual native initialization count to detect replay.
        // 捕获实际原生初始化次数，以检测重放。
        let count = target_initializations(&fixture);
        // Retain the actual final Arc accepted by the synchronous publisher.
        // 保留同步发布器接受的实际最终 Arc。
        let publication = fixture
            .manager
            .publish_activation_target_handoff(&transaction, &staged, |_| Ok(()))
            .expect("full extension handoff");
        // Keep the exact original UUID for real disk and file-lock checks.
        // 保留精确原 UUID，用于真实磁盘及文件锁检查。
        let request_id = transaction.record().request_id();
        // Scope only this actual request's next successful write and immediate indexed readback observation.
        // 仅将此实际请求下一次成功写入及即时索引回读观测限定在作用域内。
        let fault = store.lose_target_completion_observation_once(request_id);
        assert!(
            fixture
                .manager
                .complete_activation_target_handoff(&mut transaction, &publication)
                .expect_err("post-write result remains Unknown")
                .message
                .contains("Controlled loss of immediate Completed indexed readback")
        );
        drop(fault);
        assert_eq!(
            transaction.record().phase(),
            crate::VulcanPluginActivationTransactionPhase::Published
        );
        assert_eq!(
            store
                .activation_request("hook-plugin", request_id)
                .expect("canonical read is unaffected by immediate observation loss")
                .expect("real written original record")
                .phase(),
            crate::VulcanPluginActivationTransactionPhase::Completed
        );
        assert!(Arc::ptr_eq(&staged, &fixture.manager.snapshot()));
        assert_eq!(original.target_test_identity(), identities);
        // The real original request and environment remain locked while private memory ownership is retained.
        // 保留私有内存所有权时，真实原请求及环境保持锁定。
        let request = original_request_file(&fixture, request_id);
        assert!(matches!(
            request.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        assert!(
            crate::runtime::environment::lock_plugin_execution_environment_cleanup(
                &fixture.manager.state,
                "hook-plugin"
            )
            .is_err()
        );
        assert!(
            fixture
                .manager
                .ensure_formal_hook(
                    "hook-plugin",
                    staged.plugins["hook-plugin"].generation,
                    Instant::now() + WAIT
                )
                .await
                .is_err(),
            "written Completed alone never opens retained private Hook business"
        );
        fixture
            .manager
            .prewarm_activation_target_hook(&transaction, &staged)
            .await
            .expect("observe same successful original after Unknown without replay");
        assert_eq!(target_initializations(&fixture), count);
        assert_eq!(original.target_test_identity(), identities);
        // Restore actual complete extension publication before reconciling the original persisted successor.
        // 核对原持久后继之前恢复实际完整扩展发布。
        let publication = fixture
            .manager
            .publish_activation_target_handoff(&transaction, &staged, |_| Ok(()))
            .expect("same original extension republished after Unknown");
        fixture
            .manager
            .complete_activation_target_handoff(&mut transaction, &publication)
            .expect("exact disk readback confirms original transfer");
        assert_eq!(
            transaction.record().phase(),
            crate::VulcanPluginActivationTransactionPhase::Completed
        );
        assert_eq!(
            store
                .activation_request("hook-plugin", request_id)
                .expect("readback")
                .expect("actual original")
                .phase(),
            crate::VulcanPluginActivationTransactionPhase::Completed
        );
        // Observe identities after transfer without retaining physical owners.
        // 转交后观测身份，不保留物理所有者。
        let transferred = original.target_test_identity();
        assert_eq!(
            (&identities.0, &identities.1, &identities.2),
            (&transferred.0, &transferred.1, &transferred.2)
        );
        assert_eq!(target_initializations(&fixture), count);
        fixture
            .manager
            .begin_draining("hook-plugin")
            .expect("normal transferred drain");
        retire_original_hook(&fixture).await;
    });
}

/// Map only target initialization into the existing real blocking publisher; no VM or capability result is mocked.
/// 仅将目标初始化映射至既有真实阻塞发布器；不伪造 VM 或能力结果。
struct BlockingTargetInitialization {
    /// Fixture destruction releases this exact callback, including assertion-unwind cleanup.
    /// 夹具析构释放此精确回调，包含断言展开清理。
    events: Arc<Events>,
}

impl VulcanPluginUiEventPublisher for BlockingTargetInitialization {
    /// Forward event after renaming the initialization marker to the fixture's exact blocking topic.
    /// 将初始化标记改名为夹具精确阻塞主题后转发 event。
    /// Return the actual publisher result; cancellation cannot manufacture physical callback completion.
    /// 返回实际发布器结果；取消不能虚构物理回调完成。
    fn publish(&self, mut event: VulcanPluginUiInvalidation) -> Result<(), String> {
        if event.topic == "target-init" {
            event.topic = "enable".to_owned();
        }
        self.events.publish(event)
    }
}

/// Cancelling the initializer preserves actual original pool/session/domain and locks until native work really retires.
/// 取消初始化方保留实际原池／会话／域及锁，直至原生工作真实退役。
/// No inputs or return value; the original request cannot resume while the cancelled native callback is still blocked.
/// 无输入与返回值；已取消原生回调仍阻塞时，原请求不能恢复。
#[test]
fn formal_private_target_hook_cancel_retains_original_until_actual_retirement() {
    // Use the actual production-selected original fixture and request.
    // 使用实际生产选择的原夹具及请求。
    let (mut fixture, store, transaction) = published_enabled_target();
    // Keep the actual closed staging Arc issued by production routing.
    // 保留生产路由签发的实际关闭暂存 Arc。
    let staged = fixture
        .manager
        .stage_activation_target_handoff(
            &transaction,
            &BTreeSet::new(),
            &BTreeSet::from(["hook-plugin".into()]),
        )
        .expect("closed actual target");
    // Install a fresh physical blocking service only after all original candidate lifecycle effects completed.
    // 仅在全部原候选生命周期副作用完成后安装全新物理阻塞服务。
    fixture.events = Arc::new(Events::default());
    fixture
        .manager
        .state
        .host_services
        .write()
        .expect("actual host services")
        .ui_events = Some(Arc::new(BlockingTargetInitialization {
        events: Arc::clone(&fixture.events),
    }));
    executor().block_on(async {
        {
            // Observe the one original core initialization operation.
            // 观测唯一原核心初始化操作。
            let initialization = fixture.manager.prewarm_activation_target_hook(&transaction, &staged);
            tokio::pin!(initialization);
            tokio::select! {
                result = &mut initialization => panic!("blocked actual initialization must still be retained: {result:?}"),
                _ = tokio::time::timeout(WAIT, async {
                    while !fixture.events.entered.load(Ordering::Acquire) { tokio::task::yield_now().await; }
                }) => assert!(fixture.events.entered.load(Ordering::Acquire), "real target native entry"),
            }
        }
        // Retain the exact original instance or authority rather than creating a replacement.
        // 保留精确原实例或权威，不创建替代物。
        let original = original_hook(&fixture);
        // Capture original pool, session and initialization identities before transfer.
        // 转交前捕获原池、会话及初始化身份。
        let identities = original.target_test_identity();
        assert!(identities.0.is_some() && identities.1.is_some() && identities.2.is_some() && identities.3 && !identities.4);
        assert!(fixture.manager.prewarm_activation_target_hook(&transaction, &staged).await.is_err(), "cancelled initialization cannot replay");
        // Keep the exact original UUID for real disk and file-lock checks.
        // 保留精确原 UUID，用于真实磁盘及文件锁检查。
        let request_id = transaction.record().request_id();
        // Reopening the actual store preserves Started while the same native operation is still blocked.
        // 相同原生操作仍阻塞时，重开实际存储保留 Started。
        let reopened = crate::VulcanPluginGenerationStore::open_existing(&fixture.manager.state.user_plugin_root).expect("reopen actual store").expect("existing original store");
        assert_eq!(reopened.target_hook_initialization_receipt("hook-plugin", request_id).expect("actual Started receipt").expect("original attempt").phase(), crate::VulcanPluginTargetHookInitializationPhase::Started);
        drop(transaction);
        assert_eq!(store.resume_activation_transaction("hook-plugin", request_id).expect_err("actual cancelled owner still holds request").code, crate::VulcanPluginStoreErrorCode::Busy);
        assert!(crate::runtime::environment::lock_plugin_execution_environment_cleanup(&fixture.manager.state, "hook-plugin").is_err());
        // Observe actual original drain diagnostics before releasing blocked native work.
        // 释放阻塞原生工作前观测实际原排空诊断。
        let report = fixture.manager.drain_plugin_sessions("hook-plugin").await.expect("same original drain request");
        assert!(!report.failures.is_empty(), "native work has not physically exited");
        assert_eq!(original.target_test_identity(), identities);
        fixture.events.release();
        retire_original_hook(&fixture).await;
        assert!(!original.target_test_identity().3 && original.target_test_identity().4);
        drop(original);
        // Recover the same durable original only after actual ownership retires.
        // 仅在实际所有权退役后恢复相同持久原请求。
        let resumed = store.resume_activation_transaction("hook-plugin", request_id).expect("original request available after actual retirement");
        assert_eq!(resumed.record().request_id(), request_id);
        assert!(fixture.manager.prewarm_activation_target_hook(&resumed, &staged).await.is_err(), "actual retired original initialization cannot replay");
        assert!(fixture.owner().hooks.lock().expect("actual registry after retired replay refusal").is_empty());
        crate::runtime::environment::lock_plugin_execution_environment_cleanup(&fixture.manager.state, "hook-plugin").expect("actual environment retirement");
    });
}

/// An actual no-Hook target completes the original handoff without fabricating a persistent pool or session.
/// 实际无 Hook 目标完成原交接，不虚构持久池或会话。
/// No inputs or return value; source inspection, candidate effects and selection remain real production operations.
/// 无输入与返回值；来源检查、候选副作用及选择仍为真实生产操作。
#[test]
fn formal_private_target_hook_no_hook_target_completes_without_instance() {
    // Use the actual production-selected original fixture and request.
    // 使用实际生产选择的原夹具及请求。
    let (fixture, store, mut transaction) = published_enabled_target_hooks(false);
    // Keep the actual closed staging Arc issued by production routing.
    // 保留生产路由签发的实际关闭暂存 Arc。
    let staged = fixture
        .manager
        .stage_activation_target_handoff(
            &transaction,
            &BTreeSet::new(),
            &BTreeSet::from(["hook-plugin".into()]),
        )
        .expect("actual no-Hook target stage");
    assert!(staged.plugins["hook-plugin"].hooks.is_empty());
    executor().block_on(async {
        assert!(
            !fixture
                .manager
                .prewarm_activation_target_hook(&transaction, &staged)
                .await
                .expect("actual no-Hook empty path")
        );
    });
    assert!(
        fixture
            .owner()
            .hooks
            .lock()
            .expect("actual Hook registry")
            .is_empty()
    );
    // Retain the actual final Arc accepted by the synchronous publisher.
    // 保留同步发布器接受的实际最终 Arc。
    let publication = fixture
        .manager
        .publish_activation_target_handoff(&transaction, &staged, |_| Ok(()))
        .expect("actual no-Hook extension handoff");
    fixture
        .manager
        .complete_activation_target_handoff(&mut transaction, &publication)
        .expect("original no-Hook completion");
    assert_eq!(
        store
            .activation_request("hook-plugin", transaction.record().request_id())
            .expect("real request")
            .expect("original record")
            .phase(),
        crate::VulcanPluginActivationTransactionPhase::Completed
    );
    assert!(
        store
            .target_hook_initialization_receipt("hook-plugin", transaction.record().request_id())
            .expect("actual no-Hook inspection")
            .is_none()
    );
    assert!(
        fixture
            .owner()
            .hooks
            .lock()
            .expect("actual Hook registry")
            .is_empty()
    );
}
