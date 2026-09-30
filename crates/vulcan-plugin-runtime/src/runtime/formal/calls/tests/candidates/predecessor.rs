//! Original update authorization is verified against real catalogs, file locks and native execution.
//! 使用真实目录、文件锁及原生执行验证原更新授权。

use super::*;
mod callbacks;
mod publication;
mod restoration_boundary;
mod restore;

/// Prepare the exact target configuration while preserving the original predecessor and request lock.
/// 准备精确目标配置，同时保留原前驱及请求锁。
/// fixture and store own real files; configuration is the explicit target selection returned in the transaction.
/// fixture 与 store 拥有真实文件；configuration 为事务返回的显式目标选择。
fn prepare_configured(
    fixture: &Fixture,
    store: &VulcanPluginGenerationStore,
    configuration: VulcanPluginConfigurationSelection,
) -> VulcanPluginActivationTransaction {
    // Read the existing source and authoritative current selection before allocating the update.
    // 分配更新之前读取既有来源及权威当前选择。
    let source = fixture._root.path().join("source/hook-plugin");
    // Production inspection fixes the actual source digest and required grants.
    // 生产检查固定实际来源摘要及所需授权。
    let inspected = crate::inspect_user_plugin_package(&source).expect("inspect original source");
    // The baseline uses the actual predecessor manifest, never the target's new defaults.
    // 基线使用实际前驱清单，绝不使用目标新默认值。
    let previous = fixture.manager.snapshot();
    // The managed store owns the complete predecessor selection.
    // 受管存储拥有完整前驱选择。
    let selected = store.activation("hook-plugin").expect("original selection");
    // Freeze one original before any new physical copy.
    // 任何新物理复制之前固定单个原请求。
    let preparation = store
        .begin_activation_update_preparation(
            Uuid::new_v4(),
            &source,
            &inspected.content_fingerprint,
            selected.as_ref(),
            crate::runtime::plugin_required_permissions(&inspected).expect("actual grants"),
            VulcanPluginUpdatePolicy {
                explicitly_enabled: false,
                explicitly_disabled: false,
                predecessor_default_enabled: previous.plugins["hook-plugin"]
                    .manifest
                    .as_ref()
                    .expect("manifest")
                    .default_enabled,
            },
        )
        .expect("original update");
    // Real package preparation and original-bound empty credential staging precede promotion.
    // 真实包准备及绑定原请求的空凭据暂存先于晋级。
    let package = store
        .prepare_activation_package(&preparation, &source)
        .expect("copy target");
    // The empty backend rejects unexpected secret use.
    // 空后端拒绝意外秘密使用。
    let mut writer = store
        .begin_activation_credential_staging(
            &preparation,
            &package,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("staging");
    writer
        .write_and_finish(&NoSecrets, &VulcanPluginCredentialValues::new())
        .expect("finish staging");
    drop(writer);
    store
        .finish_activation_preparation(preparation, configuration)
        .expect("promote original")
}
use crate::{
    VulcanPluginActivationEffect as Effect, VulcanPluginEffectObservation as Observation,
    VulcanPluginUpdatePolicy,
};

/// Legacy flat installations remain actual predecessors even though no managed selection exists.
/// 即使不存在受管选择，旧平铺安装仍是实际前驱。
#[test]
fn formal_predecessor_drain_preserves_legacy_package_identity() {
    // Start with an unselected managed target, then finish only its unused fixture request.
    // 从未选中的受管目标开始，再仅结束未使用夹具请求。
    let (fixture, store, initial) = candidate_fixture(false, false, false);
    // The original journal remains readable after explicit fixture abandonment.
    // 显式放弃夹具后，原日志保持可读。
    let request_id = initial.record().request_id();
    drop(initial);
    store
        .abort_activation_request("hook-plugin", request_id)
        .expect("finish fixture request");
    // Production discovery sees this exact flat package under the actual user root.
    // 生产发现看到实际用户根下此精确平铺包。
    write_hook_package(&fixture.manager.state.user_plugin_root);
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("discover legacy original");
    // The complete legacy grant is derived from its own manifest before authorization.
    // 完整旧格式授权在批准前从其自身清单派生。
    let permissions = crate::runtime::plugin_required_permissions(
        &fixture.manager.snapshot().plugins["hook-plugin"],
    )
    .expect("legacy grants");
    fixture
        .manager
        .authorize_user_plugin("hook-plugin", permissions)
        .expect("authorize exact flat bytes");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("enabled legacy original");
    // There is a real predecessor despite the deliberate absence of managed activation metadata.
    // 尽管刻意没有受管激活元数据，仍存在真实前驱。
    let original = fixture.manager.snapshot();
    assert!(original.plugins["hook-plugin"].managed_package().is_none());
    assert_eq!(
        original.plugins["hook-plugin"].runtime_state,
        VulcanPluginRuntimeState::Enabled
    );
    assert!(
        store
            .activation("hook-plugin")
            .expect("no managed selection")
            .is_none()
    );
    // Freeze the legacy descriptor and policy, then use the same actual update path.
    // 固定旧格式描述符与策略，然后使用相同实际更新路径。
    let transaction = prepare(&fixture, &store);
    // The target remains immutable and unpublished.
    // 目标保持不可变且未发布。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("candidate");
    // Original runtime identity covers legacy bytes independently from target generation.
    // 原运行时身份独立于目标代次覆盖旧格式字节。
    let drain = fixture
        .manager
        .begin_activation_predecessor_drain(&candidate, &original)
        .expect("legacy original owner");
    assert_eq!(
        drain.identity().content_fingerprint,
        original.plugins["hook-plugin"].content_fingerprint
    );
    assert_ne!(
        drain.identity().content_fingerprint,
        candidate.package().content_fingerprint
    );
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .drain_activation_predecessor(&drain)
                .await
                .expect("legacy retirement")
                .failures
                .is_empty()
        );
    });
    assert!(
        store
            .activation("hook-plugin")
            .expect("legacy remains selected")
            .is_none()
    );
    assert_eq!(
        fixture.manager.snapshot().plugins["hook-plugin"].package_root,
        original.plugins["hook-plugin"].package_root
    );
}

/// Build one declared shutdown that blocks inside a real native callback before managed publication.
/// 在受管发布之前构建单个声明关闭函数，使其阻塞于真实原生回调。
fn finalizer_fixture() -> (Fixture, VulcanPluginGenerationStore) {
    // Use the known Lua fixture before copying or approving any package bytes.
    // 在复制或批准任何包字节之前使用已知 Lua 夹具。
    let fixture = Fixture::transient(false, false, 1000);
    // Modify the exact declared shutdown implementation, preserving its exported identity.
    // 修改精确声明关闭实现，保留其导出身份。
    let package = fixture._root.path().join("bundled/hook-plugin");
    // These source bytes are not yet a user-approved or immutable managed package.
    // 这些来源字节尚非用户批准或不可变受管包。
    let bundle = std::fs::read_to_string(package.join("runtime/plugin.bundle.lua")).expect("fixture Lua")
        .replace("error('transient call must not invoke tool or Hook shutdown')",
            "vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='enable',revision=0})\n    return '{\"ok\":true,\"result\":{}}'");
    std::fs::write(package.join("runtime/plugin.bundle.lua"), bundle)
        .expect("declared native blocking shutdown");
    // Both paths are concrete descendants of the fixture-owned temporary root.
    // 两个路径均为夹具自有临时根的具体后代。
    let source = fixture._root.path().join("source/hook-plugin");
    std::fs::create_dir_all(source.parent().expect("source parent")).expect("source directory");
    std::fs::rename(package, &source).expect("move isolated source out of bundled discovery");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("remove bundled publication");
    // Approval and immutable preparation consume the final modified bytes through production APIs.
    // 批准及不可变准备通过生产接口消费最终修改字节。
    let (store, initial) = transaction_for(&fixture, &source, true);
    // Retire the unused fixture request before creating the real update request.
    // 创建真实更新请求之前退役未使用夹具请求。
    let request_id = initial.record().request_id();
    drop(initial);
    store
        .abort_activation_request("hook-plugin", request_id)
        .expect("finish fixture original");
    (fixture, store)
}

/// Actual core finalizers retain update ownership even outside the host's logical call registry.
/// 即使位于宿主逻辑调用注册表之外，实际核心关闭函数仍保留更新所有权。
#[test]
fn formal_predecessor_drain_waits_for_actual_core_finalizer_and_pool() {
    use luaskills::runtime::embedded::{
        EmbeddedPluginConfig, ExecutionBackend, InstanceReuse, ModuleFinalizer, OperationPhase,
        PluginPoolConfig, PoolKind,
    };
    // A real core pool bypasses logical call receipts so physical ownership is independently tested.
    // 真实核心池不经过逻辑调用回执，从而独立验证物理所有权。
    let (fixture, store) = finalizer_fixture();
    // Register under the existing single parent, with limits derived from its actual settings.
    // 在既有单个父级下注册，限制派生自实际设置。
    let owner = fixture.owner();
    // Keep aggregate settings separate from module-local policy.
    // 聚合设置与模块局部策略保持分离。
    let parent = &owner.settings.parent;
    owner
        .host
        .runtime()
        .register_plugin(
            "hook-plugin".into(),
            EmbeddedPluginConfig {
                max_registered_pools: parent.max_registered_pools,
                max_sessions: parent.max_sessions,
                max_resident_vms: parent.max_resident_vms,
                max_running_calls: parent.max_running_calls,
                max_queued_calls: parent.max_queued_calls,
                max_queued_bytes: parent.max_queued_bytes,
                max_operations: parent.max_operations,
            },
        )
        .expect("actual aggregate");
    // Capture the enabled original before any update closes admission.
    // 在任何更新关闭入场之前捕获已启用原请求。
    let original = fixture.manager.snapshot();
    // The declared finalizer enters the controlled actual native callback only during core finalization.
    // 声明关闭函数仅在核心关闭阶段进入受控实际原生回调。
    let pool = owner
        .host
        .register_bundle_pool(crate::VulcanPluginBundlePoolRequest {
            plugin_id: "hook-plugin".into(),
            plugin_generation: original.plugins["hook-plugin"].generation,
            workspace_root: fixture._root.path().to_path_buf(),
            security_partition: "original-core-drain".into(),
            scope: VulcanPluginBridgeLeaseScope::Enabled,
            policy: PluginPoolConfig {
                kind: PoolKind::Shared,
                min_resident_vms: 0,
                max_resident_vms: 1,
                max_running_calls: 1,
                max_queued_calls: parent.max_queued_calls,
                reuse: InstanceReuse::Session,
                serial: true,
                backend: ExecutionBackend::InProcess,
                idle_ttl_ms: None,
                max_uses: None,
            },
            finalizer: Some(ModuleFinalizer {
                export: "shutdown".into(),
                arguments: json!({}),
                timeout_ms: WAIT.as_millis() as u64,
            }),
        })
        .expect("actual core-bound owner");
    // Initialization must finish before update ownership attaches to this core pool.
    // 更新所有权附加至此核心池前，初始化必须完成。
    let session = owner
        .host
        .runtime()
        .open_session(&pool, WAIT)
        .expect("session");
    assert_eq!(
        session.operation.wait(WAIT).expect("initialized").phase,
        OperationPhase::Succeeded
    );
    // The update takes the original request lock and grants only core finalization.
    // 更新取得原请求锁，且仅授权核心关闭。
    let transaction = prepare(&fixture, &store);
    // Borrowed inspection proves the candidate belongs to this exact request.
    // 借用检查证明候选属于此精确请求。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("candidate");
    // Retain the explicit original handle while releasing ordinary transaction ownership.
    // 释放普通事务所有权时保留显式原句柄。
    let drain = fixture
        .manager
        .begin_activation_predecessor_drain(&candidate, &original)
        .expect("owned drain");
    // Query identity remains independent from the borrow lifetime.
    // 查询身份独立于借用寿命。
    let request_id = drain.request_id();
    drop(candidate);
    drop(transaction);
    executor().block_on(async {
        assert!(
            !fixture
                .manager
                .drain_activation_predecessor(&drain)
                .await
                .expect("untracked core still exists")
                .failures
                .is_empty()
        );
        assert_eq!(
            store
                .resume_activation_transaction("hook-plugin", request_id)
                .expect_err("real pool retained")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        fixture
            .manager
            .request_formal_runtime_close()
            .expect("start actual core finalization");
        fixture.entered().await;
        assert!(
            !fixture
                .manager
                .drain_activation_predecessor(&drain)
                .await
                .expect("actual finalizer still running")
                .failures
                .is_empty()
        );
        assert_eq!(
            store
                .abort_activation_request("hook-plugin", request_id)
                .expect_err("finalizer retains lock")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        fixture.events.release();
        tokio::time::timeout(WAIT, async {
            while !fixture
                .manager
                .poll_formal_runtime_closed()
                .expect("poll core")
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("real termination");
        assert!(
            fixture
                .manager
                .drain_activation_predecessor(&drain)
                .await
                .expect("physical retirement")
                .failures
                .is_empty()
        );
        assert_eq!(
            store
                .activation_effects("hook-plugin", request_id)
                .expect("effects")
                .expect("receipt")
                .observations(),
            vec![(Effect::DrainPredecessorLua, Observation::Succeeded)]
        );
        assert_eq!(
            fixture
                .events
                .values
                .lock()
                .expect("native effects")
                .iter()
                .filter(|event| event.topic == "enable")
                .count(),
            1
        );
        drop(
            store
                .resume_activation_transaction("hook-plugin", request_id)
                .expect("retired actual lock"),
        );
    });
}

/// Return a genuine enabled managed predecessor without any unfinished fixture request.
/// 返回真正启用的受管前驱，不保留任何未完成夹具请求。
fn fixture() -> (Fixture, VulcanPluginGenerationStore) {
    // Reuse actual native services and immutable managed publication.
    // 复用实际原生服务及不可变受管发布。
    let (fixture, store, initial) = candidate_fixture(false, false, true);
    // Only the fixture's unused original is ended; the published predecessor remains selected.
    // 仅结束夹具未使用的原请求；已发布前驱保持选中。
    let request_id = initial.record().request_id();
    drop(initial);
    store
        .abort_activation_request("hook-plugin", request_id)
        .expect("retire fixture original");
    assert_eq!(
        fixture.manager.snapshot().plugins["hook-plugin"].runtime_state,
        VulcanPluginRuntimeState::Enabled
    );
    (fixture, store)
}

/// Prepare a policy-bearing original from fixture's exact predecessor; return its actual transaction owner.
/// 从 fixture 的精确前驱准备携带策略的原请求；返回实际事务所有者。
fn prepare(
    fixture: &Fixture,
    store: &VulcanPluginGenerationStore,
) -> VulcanPluginActivationTransaction {
    prepare_configured(
        fixture,
        store,
        VulcanPluginConfigurationSelection::NotDeclared {},
    )
}

/// Exact original ownership changes only the predecessor and records retirement without publishing the candidate.
/// 精确原所有权仅改变前驱，并记录退役，不发布候选。
#[test]
fn formal_predecessor_drain_binds_identity_and_keeps_candidate_private() {
    // Keep real independent stores to test namespace and manager-incarnation rejection.
    // 保留真实独立存储，以测试命名空间及管理器实例拒绝。
    let (fixture, store) = fixture();
    // Original snapshot comes from the actual manager before preparation.
    // 原快照来自准备前的实际管理器。
    let original = fixture.manager.snapshot();
    // Full original owner remains borrowed throughout this attempted transition.
    // 此转换尝试全程借用完整原所有者。
    let transaction = prepare(&fixture, &store);
    // The candidate is immutable, private and generation zero.
    // 候选不可变、私有且代次为零。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("candidate");
    for change in ["epoch", "generation", "root"] {
        // Each stale request preserves current publication and produces no durable effect.
        // 每个陈旧请求保留当前发布，不产生持久副作用。
        let mut stale = (*original).clone();
        match change {
            "epoch" => stale.service_epoch = Uuid::new_v4().to_string(),
            "generation" => {
                stale
                    .plugins
                    .get_mut("hook-plugin")
                    .expect("original")
                    .generation += 1
            }
            "root" => {
                stale
                    .plugins
                    .get_mut("hook-plugin")
                    .expect("original")
                    .package_root = fixture._root.path().join("foreign")
            }
            _ => unreachable!(),
        }
        assert!(
            fixture
                .manager
                .begin_activation_predecessor_drain(&candidate, &stale)
                .is_err()
        );
        assert!(Arc::ptr_eq(&original, &fixture.manager.snapshot()));
        assert!(
            store
                .activation_effects("hook-plugin", transaction.record().request_id())
                .expect("effects")
                .is_none()
        );
    }
    assert!(fixture.manager.begin_draining("hook-plugin").is_err());
    // A handle is granted only after the exact original durable evidence exists.
    // 仅在精确原持久证据存在后授予句柄。
    let drain = fixture
        .manager
        .begin_activation_predecessor_drain(&candidate, &original)
        .expect("owned drain");
    assert_eq!(
        drain.identity().predecessor_generation,
        original.plugins["hook-plugin"].generation
    );
    assert!(drain.identity().draining_generation > drain.identity().predecessor_generation);
    assert_eq!(
        fixture.manager.snapshot().plugins["hook-plugin"].runtime_state,
        VulcanPluginRuntimeState::Draining
    );
    assert!(fixture.manager.snapshot().enabled_tools.is_empty());
    assert_eq!(candidate.package().generation, 0);
    assert_eq!(
        candidate.package().runtime_state,
        VulcanPluginRuntimeState::Disabled
    );
    assert!(
        fixture
            .manager
            .begin_activation_predecessor_drain(&candidate, &original)
            .is_err()
    );
    assert!(
        fixture
            .manager
            .finish_lifecycle_transition("hook-plugin", &BTreeSet::new(), &BTreeSet::new(), vec![])
            .is_err()
    );
    assert_eq!(
        store
            .activation_effects("hook-plugin", drain.request_id())
            .expect("effects")
            .expect("receipt")
            .observations(),
        vec![(Effect::DrainPredecessorLua, Observation::Started)]
    );
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .invoke_lifecycle("hook-plugin", crate::VulcanPluginLifecycleEvent::Disable)
                .await
                .expect_err("ordinary callback cannot borrow update state")
                .message
                .contains("unfinished original")
        );
        assert!(
            fixture
                .manager
                .probe_activation_candidate(&candidate)
                .await
                .is_err()
        );
        assert!(
            fixture
                .manager
                .drain_activation_predecessor(&drain)
                .await
                .expect("actual empty retirement")
                .failures
                .is_empty()
        );
        assert!(
            fixture
                .manager
                .invoke_lifecycle("hook-plugin", crate::VulcanPluginLifecycleEvent::Disable)
                .await
                .expect_err("retired Lua does not release original lifecycle ownership")
                .message
                .contains("unfinished original")
        );
    });
    assert_eq!(
        store
            .activation_effects("hook-plugin", drain.request_id())
            .expect("effects")
            .expect("receipt")
            .observations(),
        vec![(Effect::DrainPredecessorLua, Observation::Succeeded)]
    );
    assert!(
        fixture
            .manager
            .activation_predecessor_drain(drain.request_id())
            .expect("live lookup")
            .is_none()
    );
    assert!(
        fixture
            .manager
            .finish_lifecycle_transition("hook-plugin", &BTreeSet::new(), &BTreeSet::new(), vec![])
            .is_err()
    );
    assert_eq!(
        store.activation("hook-plugin").expect("selection").as_ref(),
        transaction.record().predecessor()
    );
}

/// Generation exhaustion rejects before Started or a retained owner can be created.
/// 代次耗尽在创建 Started 或保留所有者前拒绝。
#[test]
fn formal_predecessor_drain_rejects_exhaustion_before_effects() {
    // The real original remains valid while only the process-local counter is exhausted.
    // 仅进程内计数器耗尽，真实原请求保持有效。
    let (fixture, store) = fixture();
    // Freeze the genuine published predecessor.
    // 固定真正已发布前驱。
    let original = fixture.manager.snapshot();
    // This request has an actual on-disk lock.
    // 此请求具有实际磁盘锁。
    let transaction = prepare(&fixture, &store);
    // Candidate inspection remains production code.
    // 候选检查仍使用生产代码。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("candidate");
    fixture
        .manager
        .state
        .plugin_generations
        .lock()
        .expect("generations")
        .insert("hook-plugin".into(), u64::MAX);
    assert!(
        fixture
            .manager
            .begin_activation_predecessor_drain(&candidate, &original)
            .expect_err("exhausted")
            .to_string()
            .contains("exhausted")
    );
    assert!(Arc::ptr_eq(&original, &fixture.manager.snapshot()));
    assert!(
        store
            .activation_effects("hook-plugin", transaction.record().request_id())
            .expect("effects")
            .is_none()
    );
    assert!(
        fixture
            .manager
            .activation_predecessor_drain(transaction.record().request_id())
            .expect("owners")
            .is_none()
    );
}

/// Cancellation and finite drain observation never release original ownership while native work is live.
/// 取消及有限排空观测绝不在原生工作存活时释放原所有权。
#[test]
fn formal_predecessor_drain_cancelled_observer_retains_native_request() {
    // Begin with an enabled actual predecessor and no pending request.
    // 从实际启用前驱及无待处理请求开始。
    let (fixture, store) = fixture();
    executor().block_on(async {
        // The original running Action enters before the update freezes its admission.
        // 原运行 Action 在更新固定其入场之前进入。
        let original = fixture.manager.snapshot();
        // Workspace is real and owned by this test.
        // 工作区真实存在并由此测试拥有。
        let workspace = fixture._root.path().join("request");
        std::fs::create_dir_all(&workspace).expect("workspace");
        // Fixed caller identity is independent from update identity.
        // 固定调用方身份独立于更新身份。
        let caller = crate::VulcanPluginActionIdentity {
            client_id: Uuid::new_v4().to_string(),
            invocation_id: Uuid::new_v4().to_string(),
        };
        // The action uses the exact old routing generation.
        // Action 使用精确旧路由代次。
        let generation = format!(
            "{}:{}",
            original.service_epoch, original.plugins["hook-plugin"].generation
        );
        // Retain this manager only for the actual admitted Action observer.
        // 仅为实际已入场 Action 观察者保留此管理器。
        let manager = fixture.manager.clone();
        // A native host callback, not a Tokio sleep, deliberately blocks execution.
        // 实际原生宿主回调刻意阻塞执行，而非 Tokio 休眠。
        let call = tokio::spawn(async move {
            manager
                .invoke_ui_action(
                    crate::VulcanPluginActionRequest {
                        plugin_id: "hook-plugin".into(),
                        action_id: "ui-action".into(),
                        arguments: json!({"block":true}),
                        caller: crate::VulcanPluginActionCaller::PluginUi,
                        workspace_root: workspace,
                        reuse_enabled_runtime: false,
                    },
                    &generation,
                    caller,
                )
                .await
        });
        fixture.entered().await;
        // Prepare and close admission while the real worker still owns its native callback.
        // 真实工作者仍拥有原生回调时准备并关闭入场。
        let transaction = prepare(&fixture, &store);
        // The genuine candidate supplies the exclusive original request lock.
        // 真正候选提供排他原请求锁。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("candidate");
        // Freeze the only owner capable of finalizing predecessor drain evidence.
        // 固定唯一能够完成前驱排空证据的所有者。
        let drain = fixture
            .manager
            .begin_activation_predecessor_drain(&candidate, &original)
            .expect("owned drain");
        // Original UUID remains queryable after the foreground handles are gone.
        // 前台句柄消失后，原 UUID 保持可查询。
        let request_id = drain.request_id();
        // Poll once to launch the real owned supervisor, then drop its external waiter.
        // 轮询一次以启动实际自有监督任务，再销毁其外部等待者。
        let mut observer = Box::pin(fixture.manager.drain_activation_predecessor(&drain));
        std::future::poll_fn(|context| {
            assert!(std::future::Future::poll(observer.as_mut(), context).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        drop(observer);
        drop(candidate);
        drop(transaction);
        drop(drain);
        // Read-only original lookup recovers existing ownership; it creates no new attempt.
        // 只读原查询恢复既有所有权，不创建新尝试。
        let retained = fixture
            .manager
            .activation_predecessor_drain(request_id)
            .expect("query original")
            .expect("retained");
        // The second observer waits for the first supervisor and reports real finite cleanup failure.
        // 第二观察者等待首个监督器，并报告真实有限清理失败。
        let pending = fixture
            .manager
            .drain_activation_predecessor(&retained)
            .await
            .expect("finite drain");
        assert!(!pending.failures.is_empty());
        assert_eq!(
            store
                .resume_activation_transaction("hook-plugin", request_id)
                .expect_err("native lock retained")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        assert_eq!(
            store
                .abort_activation_request("hook-plugin", request_id)
                .expect_err("abort blocked")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        assert_eq!(
            store
                .activation_effects("hook-plugin", request_id)
                .expect("effects")
                .expect("receipt")
                .observations(),
            vec![(Effect::DrainPredecessorLua, Observation::Started)]
        );
        fixture.events.release();
        let _ = call.await.expect("original Action observer");
        fixture.empty().await;
        assert!(
            !fixture
                .manager
                .drain_activation_predecessor(&retained)
                .await
                .expect("retired observation")
                .failures
                .is_empty()
        );
        assert!(
            fixture
                .manager
                .activation_predecessor_drain(request_id)
                .expect("query retired")
                .is_none()
        );
        assert_eq!(
            store
                .activation_effects("hook-plugin", request_id)
                .expect("effects")
                .expect("receipt")
                .observations(),
            vec![(Effect::DrainPredecessorLua, Observation::ObservationFailed)]
        );
        // Physical retirement releases the file lock even while the caller keeps its completed handle.
        // 即使调用方保留已完成句柄，物理退役仍释放文件锁。
        drop(
            store
                .resume_activation_transaction("hook-plugin", request_id)
                .expect("actual lock released"),
        );
        store
            .abort_activation_request("hook-plugin", request_id)
            .expect("explicit abandonment after retirement");
    });
}
