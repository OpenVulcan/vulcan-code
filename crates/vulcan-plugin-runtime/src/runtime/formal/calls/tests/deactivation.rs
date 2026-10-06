//! Actual managed removal borrows the original namespace across Lua, native cancellation and explicit retirement.
//! 实际受管移除跨 Lua、原生取消及明确退役借用原命名空间。

use super::*;

mod routing;
use crate::{
    VulcanPluginActivation, VulcanPluginConfigurationSelection, VulcanPluginCredentialRevision,
    VulcanPluginCredentialValues, VulcanPluginDeactivationObservation,
    VulcanPluginDeactivationReservation, VulcanPluginGenerationStore, VulcanPluginStoreErrorCode,
};

/// A real Uninstall publisher can remain physically active after its original observer is cancelled.
/// 真实卸载发布器可以在原观测者取消之后仍保持物理活动。
struct RemovalEvents(Arc<Events>);
impl VulcanPluginUiEventPublisher for RemovalEvents {
    /// Commit event only after the actual worker release; returns synchronization errors without inventing retirement.
    /// 仅在实际工作者释放之后提交 event；返回同步错误，不虚构退役。
    fn publish(&self, event: VulcanPluginUiInvalidation) -> Result<(), String> {
        self.0.entered.store(true, Ordering::Release);
        // The same native worker waits on the fixture's actual release condition.
        // 同一原生工作者等待夹具实际释放条件。
        let mut released = self.0.released.lock().map_err(|error| error.to_string())?;
        while !*released {
            released = self
                .0
                .changed
                .wait(released)
                .map_err(|error| error.to_string())?;
        }
        self.0
            .values
            .lock()
            .map_err(|error| error.to_string())?
            .push(event);
        Ok(())
    }
}

/// Publish a real configured immutable package before reserving removal, independently of the flat installer.
/// 在预留移除之前发布真实已配置不可变包，独立于平铺安装器。
/// declared/configured choose actual source declarations; returns the manager, store and complete selected record.
/// declared／configured 选择实际来源声明；返回管理器、存储及完整已选记录。
fn fixture(
    declared: bool,
    configured: bool,
) -> (Fixture, VulcanPluginGenerationStore, VulcanPluginActivation) {
    // Reuse actual Lua, process resolution and finite physical-drain budgets.
    // 复用实际 Lua、进程解析及有限物理排空预算。
    let fixture = Fixture::new();
    // Move only this isolated test's real source outside catalog discovery.
    // 仅将此隔离测试真实来源移至目录发现之外。
    let source = fixture._root.path().join("source/hook-plugin");
    std::fs::create_dir_all(source.parent().expect("owned source parent")).expect("source parent");
    std::fs::rename(fixture._root.path().join("bundled/hook-plugin"), &source)
        .expect("actual source move");
    // Source declarations are changed before inspection, preparation and authorization.
    // 来源声明在检查、准备及授权之前变更。
    let manifest_path = source.join("plugin.yaml");
    // Freeze the actual complete manifest, never a hand-built managed descriptor.
    // 固定实际完整清单，绝不手工构造受管描述符。
    let mut manifest = std::fs::read_to_string(&manifest_path).expect("source manifest");
    if !declared {
        manifest = manifest
            .replace("    install: on_lifecycle\n", "")
            .replace("    uninstall: on_lifecycle\n", "");
    }
    if configured {
        manifest = manifest.replace("permissions: [process_binary, ui_host_context]",
            "permissions: [process_binary, ui_host_context, config]\nconfig:\n  schema_file: config.schema.json\n  defaults_file: config.defaults.json");
        std::fs::write(source.join("config.schema.json"), r#"{"type":"object","properties":{"label":{"type":"string"}},"required":["label"],"additionalProperties":false}"#).expect("real schema");
        std::fs::write(
            source.join("config.defaults.json"),
            r#"{"label":"package-default"}"#,
        )
        .expect("real defaults");
        // Read and refused-write evidence comes from real native capabilities in the actual Lua callback.
        // 读取及拒绝写入证据来自实际 Lua 回调中的真实原生能力。
        let bundle_path = source.join("runtime/plugin.bundle.lua");
        // Preserve the production fixture envelope and process checks while adding selected-document verification.
        // 保留生产夹具信封及进程检查，同时添加已选文档验证。
        let bundle = std::fs::read_to_string(&bundle_path).expect("actual lifecycle bundle");
        std::fs::write(bundle_path, bundle.replace("function M.on_lifecycle(args)",
            "function M.on_lifecycle(args)\n    -- Read frozen selection and reject mutable replacement before any file write.\n    -- 读取固定选择，并在任何文件写入之前拒绝可变替换。\n    local selected = vulcan.host.call('vulcan.plugin.config.get', {})\n    assert(selected.label == 'original-removal-document')\n    local changed = pcall(vulcan.host.call, 'vulcan.plugin.config.update', {value={label='must-not-write'}})\n    assert(not changed, 'immutable selection cannot be mutated by uninstall')")).expect("native configured callback");
    }
    std::fs::write(&manifest_path, manifest).expect("final actual source declaration");
    // The sole inspector supplies complete fingerprint and required permissions.
    // 唯一检查器提供完整指纹及必需权限。
    let inspected = crate::inspect_user_plugin_package(&source).expect("inspect actual source");
    // This real store belongs to the actual manager's configured user root.
    // 此真实存储属于实际管理器配置的用户根。
    let store = VulcanPluginGenerationStore::open(&fixture.manager.state.user_plugin_root)
        .expect("actual store");
    // Physical immutable preparation is the package identity producer.
    // 物理不可变准备是包身份生产者。
    let package = store
        .prepare(&source, &inspected.content_fingerprint)
        .expect("actual immutable preparation");
    // Stage a real empty credential inventory rather than fabricating a bound revision.
    // 暂存真实空凭据清单，而非虚构绑定修订。
    let credentials = VulcanPluginCredentialRevision::allocate();
    store
        .begin_credential_staging(
            &package,
            credentials.clone(),
            &VulcanPluginCredentialValues::new(),
        )
        .expect("begin actual empty credentials")
        .write_and_finish(&candidates::NoSecrets, &VulcanPluginCredentialValues::new())
        .expect("actual empty credential completion");
    // Configuration absence is explicitly declared; configured selection differs from package defaults.
    // 配置缺失为明确声明；已配置选择与包默认值不同。
    let configuration = if configured {
        VulcanPluginConfigurationSelection::Document {
            value: json!({"label":"original-removal-document"}),
        }
    } else {
        VulcanPluginConfigurationSelection::NotDeclared {}
    };
    // Production selection includes complete approvals, credential ownership and configuration authority.
    // 生产选择包含完整批准、凭据所有权及配置权威。
    let selected = store
        .activate_configured(
            &package,
            0,
            crate::runtime::plugin_required_permissions(&inspected)
                .expect("actual complete required permissions"),
            credentials,
            configuration,
        )
        .expect("actual configured selection");
    fixture
        .manager
        .refresh_catalog(
            &BTreeSet::from([selected.plugin_id.clone()]),
            &BTreeSet::new(),
        )
        .expect("actual disabled catalog");
    assert!(
        fixture.manager.snapshot().plugins["hook-plugin"]
            .managed_package()
            .expect("real managed descriptor")
            .matches_activation(&selected)
    );
    fixture
        .manager
        .install_host_services(VulcanPluginHostServices {
            storage: None,
            vldb: None,
            ui_events: Some(Arc::new(RemovalEvents(Arc::clone(&fixture.events)))),
        })
        .expect("actual removal observer");
    (fixture, store, selected)
}

/// Reserve one original request using the exact production selection; returns the real retained namespace owner.
/// 使用精确生产选择预留单个原请求；返回真实保留命名空间所有者。
fn reserve(
    store: &VulcanPluginGenerationStore,
    selected: &VulcanPluginActivation,
) -> VulcanPluginDeactivationReservation {
    store
        .reserve_deactivation(Uuid::new_v4(), selected)
        .expect("original removal reservation")
}

/// Original callback ownership prevents selection writes; cancellation never creates a replacement callback or namespace.
/// 原回调所有权阻止选择写入；取消绝不创建替代回调或命名空间。
#[test]
fn deactivation_lifecycle_real_native_cancellation_keeps_same_selection_and_original_call() {
    // Stop either one observer or its actual executor while the native worker remains held.
    // 原生工作者仍被持有时，停止单个观测者或其实际执行器。
    for stop_executor in [false, true] {
        let (fixture, store, selected) = fixture(true, false);
        // Original reservation survives every foreground interruption.
        // 原预留跨全部前台中断存续。
        let mut removal = reserve(&store, &selected);
        // Strongly retain the inert original handle before any actual start.
        // 在任何实际启动之前，强保留未启动原句柄。
        let handle = Arc::new(
            fixture
                .manager
                .prepare_deactivation_lifecycle_call(&removal)
                .expect("inert managed uninstall"),
        );
        assert_eq!(
            store
                .publish_deactivation_retained(&mut removal)
                .expect_err("borrowed lifecycle blocks write")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        assert_eq!(
            store
                .reconcile_deactivation_retained(&removal)
                .expect("unchanged selected bytes"),
            VulcanPluginDeactivationObservation::Original
        );
        assert!(fixture.events.values.lock().expect("effects").is_empty());
        // Use a separate observer executor; the actual operation is owned by the formal parent.
        // 使用独立观测执行器；实际操作由正式父级拥有。
        let mut runtime = executor();
        // Borrow only this original handle in the cancellable foreground.
        // 在可取消前台中仅借用此原句柄。
        let observer_handle = Arc::clone(&handle);
        // The one observer starts and waits for the original registered input.
        // 单个观测者启动并等待原已注册输入。
        let waiter = runtime.spawn(async move {
            observer_handle.start()?;
            observer_handle.observe().await
        });
        runtime.block_on(fixture.entered());
        // Inspect the actual original operation identity before losing the observer.
        // 丢失观测者之前检查实际原操作身份。
        let original = runtime.block_on(fixture.admitted());
        // The same core operation remains through original cleanup; no synthetic operation ID is introduced.
        // 相同核心操作跨原清理存续；不引入合成操作标识。
        let operation = original
            .lock()
            .expect("actual state")
            .operation
            .clone()
            .expect("core original operation");
        if stop_executor {
            std::mem::replace(&mut runtime, executor()).shutdown_timeout(WAIT);
        } else {
            waiter.abort();
            assert!(
                runtime
                    .block_on(waiter)
                    .expect_err("observer cancellation")
                    .is_cancelled()
            );
        }
        assert!(store.publish_deactivation_retained(&mut removal).is_err());
        assert!(!original.acknowledged.load(Ordering::Acquire));
        // Actual bounded drain must report the still-physical native worker.
        // 实际有界排空必须报告仍物理存续的原生工作者。
        let report = runtime
            .block_on(handle.drain_cleanup())
            .expect("bounded actual cleanup");
        assert!(!report.failures.is_empty());
        assert_eq!(
            store
                .reconcile_deactivation_retained(&removal)
                .expect("original after cancellation"),
            VulcanPluginDeactivationObservation::Original
        );
        fixture.events.release();
        runtime.block_on(async {
            handle
                .drain_cleanup()
                .await
                .expect("same actual cleanup retry");
            tokio::time::timeout(WAIT, async {
                while !original
                    .lock()
                    .expect("original retirement")
                    .can_acknowledge_retirement()
                {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("actual retirement");
        });
        assert_eq!(
            original
                .lock()
                .expect("original identity")
                .operation
                .as_ref()
                .expect("original operation retained")
                .id(),
            operation.id()
        );
        // Acknowledge physical retirement on this same public handle, without retrying the factory.
        // 在此同一公开句柄确认物理退役，不重试工厂。
        let mut handle = Arc::try_unwrap(handle)
            .ok()
            .expect("original observer no longer owns handle");
        let _business = handle
            .confirm_retired()
            .expect("sole physical retirement confirmation");
        assert_eq!(fixture.events.values.lock().expect("effects").len(), 1);
        // Even a diagnostic Arc retains the original source; publication waits for its explicit release too.
        // 即使诊断 Arc 也保留原来源；发布也等待其明确释放。
        assert_eq!(
            store
                .publish_deactivation_retained(&mut removal)
                .expect_err("retained diagnostic source blocks write")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        drop(original);
        assert_eq!(
            store
                .publish_deactivation_retained(&mut removal)
                .expect("same original publication"),
            VulcanPluginDeactivationObservation::Published
        );
        assert_eq!(removal.intended().revision, selected.revision + 1);
        assert!(
            fixture
                .manager
                .prepare_deactivation_lifecycle_call(&removal)
                .is_err()
        );
        drop(removal);
        assert!(
            store
                .activation("hook-plugin")
                .expect("exact actual tombstone")
                .expect("selected tombstone")
                .generation
                .is_none()
        );
    }
}

/// Absent declarations retain the actual namespace but require no formal call, Lua startup or native effects.
/// 缺失声明保留实际命名空间，但不要求正式调用、Lua 启动或原生副作用。
#[test]
fn deactivation_lifecycle_absent_declaration_retains_guard_until_explicit_confirmation() {
    let (fixture, store, selected) = fixture(false, false);
    // The original removal remains strongly retained before any possible effect.
    // 原移除在任何可能效果之前保持强保留。
    let mut removal = reserve(&store, &selected);
    // Actual manifest absence creates a checked original no-declaration branch.
    // 实际清单缺失创建已校验原无声明分支。
    let mut handle = fixture
        .manager
        .prepare_deactivation_lifecycle_call(&removal)
        .expect("actual no-declaration handle");
    assert!(
        fixture
            .owner()
            .calls
            .lock()
            .expect("actual registry")
            .is_empty()
    );
    handle.start().expect("no executor needed");
    assert!(store.publish_deactivation_retained(&mut removal).is_err());
    handle
        .confirm_retired()
        .expect("original no-declaration confirmation")
        .expect("no callback failure");
    assert!(handle.start().is_err());
    assert!(
        fixture
            .events
            .values
            .lock()
            .expect("actual effects")
            .is_empty()
    );
    assert_eq!(
        store
            .publish_deactivation_retained(&mut removal)
            .expect("same original write"),
        VulcanPluginDeactivationObservation::Published
    );
}

/// Managed removal never borrows flat Install authority or grants ordinary published execution while its original File is held.
/// 受管移除绝不借用平铺安装权威，也不在原文件持有时授予普通已发布执行。
#[test]
fn deactivation_lifecycle_refuses_flat_entry_wrong_root_and_scope_before_native_effects() {
    let (fixture, store, selected) = fixture(true, false);
    // Hold exact original managed removal ownership throughout every rejected authority attempt.
    // 在每个被拒绝权威尝试期间持有精确原受管移除所有权。
    let removal = reserve(&store, &selected);
    for event in [
        VulcanPluginLegacyLifecycleEvent::Install,
        VulcanPluginLegacyLifecycleEvent::Uninstall,
    ] {
        assert!(
            fixture
                .manager
                .prepare_legacy_lifecycle_call(
                    "hook-plugin",
                    event,
                    removal.lifecycle_guard().expect("same actual guard")
                )
                .is_err()
        );
    }
    // Another actual manager has an unrelated root and no right to consume this original.
    // 另一个实际管理器具有无关根，无权消费此原实例。
    let other = Fixture::new();
    assert!(
        other
            .manager
            .prepare_deactivation_lifecycle_call(&removal)
            .is_err()
    );
    // The typed managed authority is fixed to Uninstall even when the same manifest declares Install.
    // 即使相同清单声明安装，类型化受管权威仍固定为卸载。
    let plugin = fixture.manager.snapshot().plugins["hook-plugin"].clone();
    // This is the actual factory's checked authority, not a fabricated record or raw File.
    // 此为实际工厂的已校验权威，而非伪造记录或原始文件。
    let authority = LegacyLifecycleExecution::for_deactivation(
        &fixture.manager.state,
        plugin.clone(),
        &removal,
    )
    .expect("original managed scope");
    assert!(
        authority
            .validate(
                &fixture.manager.state,
                &plugin,
                VulcanPluginBridgeLeaseScope::LifecycleInstall
            )
            .is_err()
    );
    assert!(fixture.events.values.lock().expect("effects").is_empty());
    assert!(fixture.owner().calls.lock().expect("registry").is_empty());
    assert_eq!(
        store
            .reconcile_deactivation_retained(&removal)
            .expect("exact original"),
        VulcanPluginDeactivationObservation::Original
    );
}

/// A stale catalog cannot register callbacks against a fresh complete original even when package bytes are unchanged.
/// 即使包字节未变，过期目录也不能针对新完整原记录登记回调。
#[test]
fn deactivation_lifecycle_rejects_stale_complete_catalog_selection_without_startup() {
    let (fixture, store, selected) = fixture(true, false);
    // Obtain the actual pinned generation before changing selection; never guess an immutable package path.
    // 变更选择之前取得实际固定代次；绝不猜测不可变包路径。
    let package = fixture.manager.snapshot().plugins["hook-plugin"]
        .managed_package()
        .expect("actual managed selection")
        .generation()
        .clone();
    // Advance only the persistent selection, deliberately retaining a stale in-process catalog.
    // 仅推进持久选择，刻意保留过期进程内目录。
    let next = store
        .activate_configured(
            &package,
            selected.revision,
            selected.approved_permissions().to_vec(),
            selected.credential_revision().clone(),
            selected.configuration().clone(),
        )
        .expect("actual next record");
    // This real reservation owns the fresh full selection, while the stale descriptor remains unauthorized.
    // 此真实预留拥有新完整选择，同时过期描述符仍未获授权。
    let removal = reserve(&store, &next);
    assert!(
        fixture
            .manager
            .prepare_deactivation_lifecycle_call(&removal)
            .is_err()
    );
    assert!(
        fixture
            .owner()
            .calls
            .lock()
            .expect("actual registry")
            .is_empty()
    );
    assert!(fixture.events.values.lock().expect("effects").is_empty());
    assert_eq!(
        store
            .reconcile_deactivation_retained(&removal)
            .expect("fresh original remains intact"),
        VulcanPluginDeactivationObservation::Original
    );
}

/// Real Lua reads the exact configured selection and refuses mutable writes, preserving full cleanup bindings.
/// 真实 Lua 读取精确已配置选择并拒绝可变写入，保留完整清理绑定。
#[test]
fn deactivation_lifecycle_actual_configured_callback_preserves_original_document_and_credentials() {
    let (fixture, store, selected) = fixture(true, true);
    fixture.events.release();
    // Keep the same original selection owner throughout callback admission and physical retirement.
    // 在回调准入及物理退役期间保持同一原选择所有者。
    let mut removal = reserve(&store, &selected);
    // An unadmitted removal cannot replace the manager's actual snapshot or invalidate callback identity.
    // 未准入移除不能替换管理器实际快照或使回调身份失效。
    let previous = fixture.manager.snapshot();
    assert!(
        fixture
            .manager
            .refresh_catalog_with_deactivation(&BTreeSet::new(), &BTreeSet::new(), &removal)
            .is_err()
    );
    assert!(Arc::ptr_eq(&previous, &fixture.manager.snapshot()));
    // Retain the checked original callback before launching actual Lua and native services.
    // 启动实际 Lua 及原生服务之前保留已校验原回调。
    let mut handle = fixture
        .manager
        .prepare_deactivation_lifecycle_call(&removal)
        .expect("configured original callback");
    executor().block_on(async {
        handle.start().expect("start original");
        handle
            .observe()
            .await
            .expect("actual configured Lua assertions");
        handle.drain_cleanup().await.expect("actual native cleanup");
    });
    handle
        .confirm_retired()
        .expect("actual physical acknowledgement")
        .expect("actual business success");
    assert_eq!(
        fixture.events.values.lock().expect("actual effects").len(),
        1
    );
    assert!(
        !fixture
            .manager
            .state
            .plugin_state_root
            .join("hook-plugin/config.json")
            .exists()
    );
    assert_eq!(removal.original(), &selected);
    assert_eq!(
        store
            .publish_deactivation_retained(&mut removal)
            .expect("same configured original write"),
        VulcanPluginDeactivationObservation::Published
    );
    assert_eq!(removal.intended().configuration(), selected.configuration());
    assert_eq!(
        removal.intended().credential_revision(),
        selected.credential_revision()
    );
    // The same original now authorizes manager projection while retaining the physical namespace File.
    // 相同原实例现在授权管理器投影，同时保留物理命名空间文件。
    let removed = fixture
        .manager
        .refresh_catalog_with_deactivation(&BTreeSet::new(), &BTreeSet::new(), &removal)
        .expect("actual same-owner removal projection");
    assert!(Arc::ptr_eq(&removed, &fixture.manager.snapshot()));
    assert!(!removed.plugins.contains_key("hook-plugin"));
    assert_eq!(removed.service_epoch, previous.service_epoch);
    assert!(
        removed
            .enabled_tools
            .values()
            .all(|tool| tool.plugin_id != "hook-plugin")
    );
}
