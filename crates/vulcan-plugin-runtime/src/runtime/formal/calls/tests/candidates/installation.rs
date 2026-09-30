//! Real unpublished installation verifies exact event authority, native effects and retirement ownership.
//! 真实未发布安装验证精确事件权威、原生副作用及退役所有权。

use super::*;

mod environment;
mod receipts;

/// Installation timeout returns while the original native business still prevents transaction mutation.
/// 安装超时返回时，原原生业务仍阻止事务变更。
#[test]
fn formal_candidate_install_deadline_preserves_actual_business_ownership() {
    // A real native callback outlives the invocation's deliberately shortened deadline.
    // 真实原生回调比刻意缩短的调用截止时间存活更久。
    let (fixture, store, mut transaction) = installation_fixture(true, false, false);
    executor().block_on(async {
        // Borrow the actual request throughout business admission.
        // 在业务入场期间借用实际请求。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("inspect timed install");
        // Derive a shorter observation from the shared test wait budget.
        // 从共享测试等待预算派生较短观测期限。
        let deadline = Instant::now() + WAIT / 10;
        assert!(
            fixture
                .manager
                .invoke_formal_candidate_operation(
                    &candidate,
                    CandidateOperation::Install,
                    deadline
                )
                .await
                .expect_err("installation deadline")
                .message
                .contains("timed out")
        );
        assert!(fixture.events.entered.load(Ordering::Acquire));
        drop(candidate);
        assert_eq!(
            store
                .commit_activation_transaction(&mut transaction)
                .expect_err("native business outlives timeout")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        // Query the actual candidate record without extending its ownership.
        // 查询实际候选记录，不延长其所有权。
        let authority = {
            // The retained registry belongs to the original parent.
            // 保留注册表属于原父级。
            let owner = fixture.owner();
            // Select by original request identity instead of relying on collection order.
            // 按原请求身份选择，不依赖集合顺序。
            let calls = owner.calls.lock().expect("original calls");
            // A weak reference observes final resource retirement independently of status snapshots.
            // 弱引用独立于状态快照观测最终资源退役。
            let original = calls
                .values()
                .find_map(|call| {
                    call.activation.as_ref().filter(|authority| {
                        authority.request_id == transaction.record().request_id()
                    })
                })
                .expect("original timed install");
            Arc::downgrade(original)
        };
        fixture.events.release();
        fixture
            .manager
            .drain_activation_candidate_calls(transaction.record().request_id())
            .await
            .expect("drain original timed callback");
        authority_retired(&authority).await;
        store
            .abort_activation_transaction(&mut transaction)
            .expect("actual timed install retired");
    });
}

/// An absent installation declaration succeeds without preparing an environment or attaching a parent.
/// 缺失安装声明直接成功，不准备环境或附加父级。
#[test]
fn formal_candidate_install_absent_callback_does_not_create_runtime() {
    // Preserve a valid Lua package while removing only the optional installation declaration.
    // 保留有效 Lua 插件包，仅移除可选安装声明。
    let fixture = Fixture::new();
    // Both source paths are owned by this isolated fixture.
    // 两个来源路径均属于此隔离夹具。
    let original = fixture._root.path().join("bundled/hook-plugin");
    // The exact declaration is asserted before changing fixture bytes.
    // 在改变夹具字节之前断言精确声明。
    let manifest =
        std::fs::read_to_string(original.join("plugin.yaml")).expect("declared lifecycle manifest");
    assert_eq!(
        manifest.matches("    install: on_lifecycle\n").count(),
        1,
        "optional install fixture declaration may have evolved"
    );
    std::fs::write(
        original.join("plugin.yaml"),
        manifest.replace("    install: on_lifecycle\n", ""),
    )
    .expect("remove optional installation");
    // Discovery must never see the unpublished source.
    // 发现绝不能看到未发布来源。
    let source = fixture._root.path().join("source/hook-plugin");
    std::fs::create_dir_all(source.parent().expect("source parent")).expect("source directory");
    std::fs::rename(original, &source).expect("move exact isolated source");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("empty discovery");
    // Storage preparation remains genuine even though callback execution is unnecessary.
    // 即使不需要回调执行，存储准备仍真实发生。
    let (store, transaction) = transaction_for(&fixture, &source, false);
    // This manager shares only the configured catalog root and deliberately has no formal parent.
    // 此管理器仅共享配置目录根，刻意不具有正式父级。
    let standalone = test_manager(fixture._root.path(), fixture._root.path().join("bundled"));
    assert!(
        !standalone.state.plugin_runtime_root.exists(),
        "no environment before absent callback"
    );
    executor().block_on(async {
        // Bind the same validated target to the standalone host before checking the absent declaration.
        // 在检查缺失声明前，将相同已校验目标绑定至独立宿主。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("inspect no-callback target");
        standalone
            .install_activation_candidate(&candidate)
            .await
            .expect("absent install succeeds");
    });
    assert!(
        !standalone.state.plugin_runtime_root.exists(),
        "absent callback must create no environment or lease files"
    );
    assert!(
        standalone
            .formal_runtime_owner()
            .expect("read parent")
            .is_none()
    );
    assert!(
        fixture
            .events
            .values
            .lock()
            .expect("no callback events")
            .is_empty()
    );
}

/// Build a real installation callback with optional native blocking and business failure.
/// 构建真实安装回调，可选原生阻塞及业务失败。
/// publish_predecessor selects an update; returns the original manager, actual store and prepared request.
/// publish_predecessor 选择更新；返回原管理器、实际存储及准备请求。
fn installation_fixture(
    block: bool,
    fail: bool,
    publish_predecessor: bool,
) -> (
    Fixture,
    VulcanPluginGenerationStore,
    VulcanPluginActivationTransaction,
) {
    // Reuse production lifecycle assertions and its real native process resolver.
    // 复用生产生命周期断言及真实原生进程解析器。
    let fixture = Fixture::new();
    // Both paths remain inside the fixture's own temporary root.
    // 两个路径均保留在夹具自身临时根目录内。
    let original = fixture._root.path().join("bundled/hook-plugin");
    // Keep source outside discovery before immutable preparation.
    // 不可变准备之前将来源保留在发现范围之外。
    let source = fixture._root.path().join("source/hook-plugin");
    std::fs::create_dir_all(source.parent().expect("source parent")).expect("create source parent");
    std::fs::rename(original, &source).expect("move isolated installation source");
    // The declared schema proves callback configuration belongs to the exact target.
    // 声明模式证明回调配置属于精确目标。
    let manifest = std::fs::read_to_string(source.join("plugin.yaml")).expect("installation manifest")
        .replace("permissions: [process_binary, ui_host_context]",
            "permissions: [process_binary, ui_host_context, config]\nconfig:\n  schema_file: config.schema.json\n  defaults_file: config.defaults.json");
    std::fs::write(source.join("plugin.yaml"), manifest)
        .expect("declare installation configuration");
    std::fs::write(source.join("config.schema.json"),
        r#"{"type":"object","properties":{"label":{"type":"string"}},"required":["label"],"additionalProperties":false}"#)
        .expect("installation configuration schema");
    std::fs::write(
        source.join("config.defaults.json"),
        r#"{"label":"package-default"}"#,
    )
    .expect("installation configuration defaults");
    // Insert real native work only inside the manifest-selected installation function.
    // 仅在清单选择的安装函数内部插入真实原生工作。
    let bundle = std::fs::read_to_string(source.join("runtime/plugin.bundle.lua"))
        .expect("original lifecycle bundle");
    // The existing declaration is an exact fixture contract, verified before replacing its body prefix.
    // 既有声明为精确夹具契约，在替换函数体前缀前验证。
    let entry = "function M.on_lifecycle(args)";
    assert_eq!(
        bundle.matches(entry).count(),
        1,
        "lifecycle fixture entry may have evolved"
    );
    // Controlled native blocking does not change the event supplied by the production host.
    // 受控原生阻塞不改变生产宿主提供的事件。
    let body = format!(
        r#"{entry}
    -- Installation consumes the transaction's configuration and the original host event.
    -- 安装消费事务配置及原宿主事件。
    assert(args.event == 'install')
    -- Record the host request through a real capability so tests observe the exact Lua envelope.
    -- 通过真实能力记录宿主请求，使测试观测精确 Lua 信封。
    assert(type(args.request_id) == 'string')
    vulcan.host.call('vulcan.plugin.ui.invalidate', {{topic='install-request:' .. args.request_id,revision=0}})
    local selected = vulcan.host.call('vulcan.plugin.config.get', {{}})
    assert(selected.label == 'selected-candidate', 'wrong install configuration')
    -- A broad process capability never grants the undeclared process-session subtype.
    -- 宽泛进程能力绝不授予未声明的进程会话子类型。
    local session_allowed = pcall(vulcan.host.call, 'vulcan.plugin.process.resolve',
        {{entry_id='fixture',mode='session',options={{cwd=args.environment_root}}}})
    assert(not session_allowed, 'undeclared process_session was admitted')
    if {block} then
        vulcan.host.call('vulcan.plugin.ui.invalidate', {{topic='enable',revision=0}})
    end
    if {fail} then
        vulcan.host.call('vulcan.plugin.ui.invalidate', {{topic='install-failed',revision=0}})
        return '{{"ok":false,"error":"candidate-install-marker"}}'
    end
"#
    );
    std::fs::write(
        source.join("runtime/plugin.bundle.lua"),
        bundle.replace(entry, &body),
    )
    .expect("installation callback fixture");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("remove bundled fixture before candidate inspection");
    // Production staging selects exact credentials and the candidate document.
    // 生产暂存选择精确凭据及候选文档。
    let (store, transaction) = transaction_for(&fixture, &source, publish_predecessor);
    (fixture, store, transaction)
}

/// Installation uses the original envelope, private target configuration and parent without changing public state.
/// 安装使用原信封、私有目标配置及父级，不改变公开状态。
#[test]
fn formal_candidate_install_preserves_original_selection_and_business_result() {
    for publish_predecessor in [false, true] {
        for fail in [false, true] {
            // Native output distinguishes an executed callback from a fake successful no-op.
            // 原生输出区分已执行回调与虚假成功空操作。
            let (fixture, store, mut transaction) =
                installation_fixture(false, fail, publish_predecessor);
            // Retain both public and durable predecessor identities during candidate work.
            // 在候选工作期间保留公开及持久前驱身份。
            let catalog = fixture.manager.snapshot();
            // Physical disk selection remains unchanged until an explicit later commit.
            // 物理磁盘选择保持不变，直到后续明确提交。
            let previous = store.activation("hook-plugin").expect("original selection");
            // Parent identity must not change even for failed callbacks.
            // 即使回调失败，父级身份也不能改变。
            let owner = fixture.owner();
            executor().block_on(async {
                // Inspection creates the sole valid private admission.
                // 检查创建唯一有效私有入场。
                let candidate = store
                    .inspect_activation_candidate(&transaction)
                    .expect("inspect install target");
                // Ordinary public lifecycle cannot execute an unpublished or enabled predecessor as an installation.
                // 普通公开生命周期不能把未发布目标或已启用前驱作为安装执行。
                assert!(
                    fixture
                        .manager
                        .invoke_lifecycle("hook-plugin", Event::Install)
                        .await
                        .is_err()
                );
                // This is the actual public installation API, including its native capability scope.
                // 此处为实际公开安装 API，包含其原生能力作用域。
                let result = fixture
                    .manager
                    .install_activation_candidate(&candidate)
                    .await;
                if fail {
                    assert!(
                        result
                            .expect_err("callback business failure")
                            .message
                            .contains("candidate-install-marker")
                    );
                } else {
                    result.expect("real candidate installation");
                }
                assert!(owner.calls.lock().expect("original calls").is_empty());
                assert_eq!(
                    owner
                        .host
                        .runtime()
                        .resources()
                        .expect("actual resources")
                        .resident,
                    0
                );
                assert!(
                    fixture
                        .manager
                        .formal_capacity_status(
                            "hook-plugin",
                            VulcanPluginCapacityPurpose::Lifecycle
                        )
                        .expect("original lifecycle capacity")
                        .is_some()
                );
            });
            assert!(Arc::ptr_eq(&catalog, &fixture.manager.snapshot()));
            assert_eq!(
                store
                    .activation("hook-plugin")
                    .expect("selection after callback"),
                previous
            );
            assert_eq!(
                fixture.owner().host.runtime().id(),
                owner.host.runtime().id()
            );
            // Exactly one callback effect is observable; no business finalizer or replay is allowed.
            // 恰好可观察到一个回调副作用；不允许业务关闭器或重放。
            let topics = fixture
                .events
                .values
                .lock()
                .expect("native effects")
                .iter()
                .filter(|event| !event.topic.starts_with("install-request:"))
                .map(|event| event.topic.clone())
                .collect::<Vec<_>>();
            assert_eq!(topics, [if fail { "install-failed" } else { "install" }]);
            store
                .abort_activation_transaction(&mut transaction)
                .expect("actual callback ownership retired");
        }
    }
}

/// A retained installation excludes concurrent installation and probing within the same request.
/// 保留的安装排除同一请求内并发安装及探测。
#[test]
fn formal_candidate_install_excludes_overlapping_request_work() {
    // The callback blocks inside a genuine native capability.
    // 回调在真实原生能力内部阻塞。
    let (fixture, store, mut transaction) = installation_fixture(true, false, false);
    executor().block_on(async {
        // Both callers borrow the same actual prepared target.
        // 两个调用方借用同一实际准备目标。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("inspect blocked install");
        // One observer starts business while the other checks exact conflicting admissions.
        // 一个观测者开始业务，另一个检查精确冲突入场。
        let (installed, ()) = tokio::join!(
            fixture.manager.install_activation_candidate(&candidate),
            async {
                fixture.entered().await;
                assert!(
                    fixture
                        .manager
                        .install_activation_candidate(&candidate)
                        .await
                        .expect_err("duplicate live install")
                        .message
                        .contains("already attempted")
                );
                assert!(
                    fixture
                        .manager
                        .probe_activation_candidate(&candidate)
                        .await
                        .expect_err("probe cannot overlap installation")
                        .message
                        .contains("precede lifecycle")
                );
                // Status exposes the original business operation; it never invents an initialization-only receipt.
                // 状态公开原业务操作；绝不虚构仅初始化回执。
                let statuses = fixture
                    .manager
                    .activation_candidate_calls(transaction.record().request_id())
                    .expect("original install status");
                assert_eq!(statuses.len(), 1);
                assert!(statuses.iter().all(|status| status.operation.is_some()
                    && status.initialization.is_none()
                    && !status.retired));
                fixture.events.release();
            }
        );
        installed.expect("original installation finishes once");
    });
    store
        .abort_activation_transaction(&mut transaction)
        .expect("original installation retired");
    assert_eq!(
        fixture
            .events
            .values
            .lock()
            .expect("actual effects")
            .iter()
            .filter(|event| event.topic == "install")
            .count(),
        1
    );
}

/// Cancelled installation observers cannot release native ownership or allow request recovery.
/// 已取消安装观测者不能释放原生所有权或允许请求恢复。
#[test]
fn formal_candidate_install_cancelled_observer_retains_transaction() {
    // Native business has already entered before the observer is cancelled.
    // 在观测者取消前，原生业务已经进入。
    let (fixture, store, transaction) = installation_fixture(true, false, false);
    // Durable identity remains queryable after the original owner leaves.
    // 原所有者退出后，持久身份仍可查询。
    let request_id = transaction.record().request_id();
    executor().block_on(async {
        // The observer owns the original transaction while preparation and native work retain independent ownership.
        // 观测者拥有原事务，准备及原生工作保留独立所有权。
        let manager = fixture.manager.clone();
        // Store clones are namespace access, not transaction lease ownership.
        // 存储克隆是命名空间访问，不是事务租约所有权。
        let probing_store = store.clone();
        // One spawned waiter owns the non-cloneable transaction.
        // 单个派生等待者拥有不可克隆事务。
        let observer = tokio::spawn(async move {
            // The actual view binds private callback admission.
            // 实际视图绑定私有回调入场。
            let candidate = probing_store
                .inspect_activation_candidate(&transaction)
                .expect("inspect owned install");
            manager.install_activation_candidate(&candidate).await
        });
        fixture.entered().await;
        observer.abort();
        assert!(
            observer
                .await
                .expect_err("observer was cancelled")
                .is_cancelled()
        );
        assert_eq!(
            store
                .resume_activation_transaction("hook-plugin", request_id)
                .expect_err("native install retains request")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        // A bounded drain reports actual incompletion without freeing package or transaction ownership.
        // 有界排空报告实际未完成，不释放插件包或事务所有权。
        let blocked = fixture
            .manager
            .drain_activation_candidate_calls(request_id)
            .await
            .expect("bounded install drain");
        assert_eq!(blocked.closed_formal_calls, 0);
        assert!(!blocked.failures.is_empty());
        assert_eq!(
            store
                .abort_activation_request("hook-plugin", request_id)
                .expect_err("blocked callback prevents abort")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        fixture.events.release();
        fixture
            .manager
            .drain_activation_candidate_calls(request_id)
            .await
            .expect("finish original install cleanup");
        // Final supervisor references may outlive the returned drain report by one scheduling turn.
        // 最终监督者引用可能比返回排空报告多存活一个调度轮次。
        tokio::time::timeout(WAIT, async {
            loop {
                match store.abort_activation_request("hook-plugin", request_id) {
                    Ok(_) => break,
                    Err(error) if error.code == VulcanPluginStoreErrorCode::Busy => {
                        tokio::task::yield_now().await
                    }
                    Err(error) => panic!("original install abort failed: {error}"),
                }
            }
        })
        .await
        .expect("original native ownership retires");
        assert!(
            fixture
                .manager
                .activation_candidate_calls(request_id)
                .expect("retired install status")
                .is_empty()
        );
    });
}

/// Typed candidate authority rejects all other lifecycle and public scopes before native admission.
/// 类型化候选权威在原生入场前拒绝全部其他生命周期及公开作用域。
#[test]
fn formal_candidate_authority_cannot_cross_operation_scopes() {
    // No VM is needed to verify the exact private authority boundary.
    // 验证精确私有权威边界不需要 VM。
    let (fixture, store, transaction) = installation_fixture(false, false, false);
    // All authority kinds derive from the same inspected package, so scope alone must distinguish them.
    // 全部权威种类派生自相同已检查插件包，因此必须单独区分作用域。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("inspect scope target");
    for operation in [
        CandidateOperation::Probe,
        CandidateOperation::Install,
        CandidateOperation::Enable,
        CandidateOperation::EnableRollback,
    ] {
        // Bind only the explicitly selected operation.
        // 仅绑定明确选择的操作。
        let authority = ActivationExecution::new(&fixture.manager.state, &candidate, operation)
            .expect("typed authority");
        authority
            .validate(&fixture.manager.state, &authority.plugin, operation.scope())
            .expect("own scope");
        for scope in [
            VulcanPluginBridgeLeaseScope::Enabled,
            VulcanPluginBridgeLeaseScope::EnablingProbe,
            VulcanPluginBridgeLeaseScope::LifecycleInstall,
            VulcanPluginBridgeLeaseScope::LifecycleUninstall,
            VulcanPluginBridgeLeaseScope::LifecycleEnable,
            VulcanPluginBridgeLeaseScope::LifecycleEnableRollback,
            VulcanPluginBridgeLeaseScope::LifecycleDisable,
        ] {
            assert_eq!(
                authority
                    .validate(&fixture.manager.state, &authority.plugin, scope)
                    .is_ok(),
                scope == operation.scope()
            );
        }
    }
    assert!(
        fixture
            .owner()
            .calls
            .lock()
            .expect("no native admission")
            .is_empty()
    );
}
