//! Actual user-package lifecycle tests retain namespace ownership through native work.
//! 实际用户包生命周期测试在原生工作期间保留命名空间所有权。

use super::*;
use crate::{VulcanPluginGenerationStore, VulcanPluginStoreErrorCode};

mod routing;

/// Control real installation/removal callbacks using the existing native synchronization fixture.
/// 使用既有原生同步夹具控制真实安装或卸载回调。
struct LegacyEvents(Arc<Events>);

impl VulcanPluginUiEventPublisher for LegacyEvents {
    /// Block the actual native worker, then record its original event without fabrication.
    /// 阻塞实际原生工作者，随后记录其原事件，不伪造结果。
    fn publish(&self, event: VulcanPluginUiInvalidation) -> Result<(), String> {
        self.0.entered.store(true, Ordering::Release);
        // Wait on the actual native worker release condition.
        // 等待实际原生工作者释放条件。
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

/// Install real bytes through the production flat installer, replacing the bundled fixture namespace.
/// 通过生产平铺安装器安装真实字节，替换内置夹具命名空间。
/// Returns a disabled actual UserInstalled descriptor and the original formal parent.
/// 返回停用实际用户安装描述符及原正式父级。
fn user_fixture() -> Fixture {
    user_fixture_with_declaration(true)
}

/// Install an actually authorized source with or without its lifecycle declarations.
/// 安装实际授权源码，可包含或缺失生命周期声明。
fn user_fixture_with_declaration(declared: bool) -> Fixture {
    // Own the isolated actual user package and its production manager.
    // 持有隔离实际用户包及其生产管理器。
    let fixture = Fixture::new();
    // Select the isolated source parent for production installation.
    // 选择用于生产安装的隔离源码父目录。
    let source = fixture._root.path().join("source");
    std::fs::create_dir(&source).expect("source parent");
    // Identify the exact source package whose bytes will be authorized.
    // 标识将被授权字节的精确源码包。
    let package = source.join("hook-plugin");
    std::fs::rename(fixture._root.path().join("bundled/hook-plugin"), &package)
        .expect("move actual source bytes");
    if !declared {
        // Select the real source manifest before authorization.
        // 授权前选择真实源码清单。
        let manifest_path = package.join("plugin.yaml");
        // Read the actual manifest before selecting declarations.
        // 选择声明前读取实际清单。
        let manifest = std::fs::read_to_string(&manifest_path)
            .expect("real source manifest")
            .replace("    install: on_lifecycle\n", "")
            .replace("    uninstall: on_lifecycle\n", "");
        std::fs::write(manifest_path, manifest).expect("source without lifecycle declaration");
    }
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("remove bundled selection");
    // Inspect all actual source permissions and the exact content fingerprint.
    // 检查全部实际源码权限及精确内容指纹。
    let preview = fixture
        .manager
        .inspect_local_user_plugin(&package)
        .expect("actual permission inspection");
    fixture
        .manager
        .install_local_user_plugin(
            &package,
            &preview.content_fingerprint,
            preview.permissions,
            false,
        )
        .expect("production user installation");
    fixture
        .manager
        .refresh_catalog(
            &BTreeSet::from(["hook-plugin".to_owned()]),
            &BTreeSet::new(),
        )
        .expect("disabled user catalog");
    assert_eq!(
        fixture.manager.snapshot().plugins["hook-plugin"].origin,
        crate::VulcanPluginPackageOrigin::UserInstalled
    );
    fixture
        .manager
        .install_host_services(VulcanPluginHostServices {
            storage: None,
            vldb: None,
            ui_events: Some(Arc::new(LegacyEvents(Arc::clone(&fixture.events)))),
        })
        .expect("actual native observer");
    fixture
}

/// Acquire one real namespace owner from the fixture's actual user root.
/// 从夹具实际用户根取得单个真实命名空间所有者。
fn guard(fixture: &Fixture) -> Arc<crate::VulcanPluginLegacyMutationGuard> {
    Arc::new(
        VulcanPluginGenerationStore::open(&fixture.manager.state.user_plugin_root)
            .expect("original store")
            .acquire_legacy_mutation_guard("hook-plugin", None)
            .expect("original namespace guard"),
    )
}

/// Verify another real store cannot acquire the namespace retained by the original call.
/// 核验另一个真实存储无法获取原调用保留的命名空间。
fn assert_busy(fixture: &Fixture) {
    // Open an independent real store for cross-owner namespace contention.
    // 打开独立真实存储用于跨所有者命名空间争用。
    let contender = VulcanPluginGenerationStore::open(&fixture.manager.state.user_plugin_root)
        .expect("independent store");
    // Preserve the actual contention error without fabricating Busy evidence.
    // 保留实际争用错误，不伪造占用证据。
    let error = contender
        .acquire_legacy_mutation_guard("hook-plugin", None)
        // Discard unexpected successful ownership before asserting the error, without requiring the guard to implement Debug.
        // 断言错误前释放意外成功的所有权，不要求守卫实现 Debug。
        .map(|_| ())
        .expect_err("original namespace remains busy");
    assert_eq!(error.code, VulcanPluginStoreErrorCode::Busy);
}

/// Observe sole real retirement without acknowledging or substituting another call.
/// 观测唯一真实退役，不确认或替换为另一个调用。
async fn retired(call: &Arc<FormalCall>) {
    tokio::time::timeout(WAIT, async {
        while !call
            .lock()
            .expect("actual state")
            .can_acknowledge_retirement()
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("original physical retirement");
}

/// Inert registration and repeated starts keep one actual user callback and its original namespace.
/// 未启动注册及重复启动保持单个实际用户回调及原命名空间。
#[test]
fn legacy_lifecycle_original_guard_spans_registration_native_callback_and_retirement() {
    for event in [
        VulcanPluginLegacyLifecycleEvent::Install,
        VulcanPluginLegacyLifecycleEvent::Uninstall,
    ] {
        // Own the isolated actual user package and its production manager.
        // 持有隔离实际用户包及其生产管理器。
        let fixture = user_fixture();
        // Retain the exact original namespace guard before formal registration.
        // 正式注册前保留精确原命名空间护栏。
        let original_guard = guard(&fixture);
        // Retain the original public lifecycle handle until true retirement acknowledgement.
        // 保留原公开生命周期句柄，直至真实退役确认。
        let mut handle = fixture
            .manager
            .prepare_legacy_lifecycle_call("hook-plugin", event, Arc::clone(&original_guard))
            .expect("inert original call");
        // Own a deterministic observer executor separate from actual native workers.
        // 持有独立于实际原生工作者的确定性观测执行器。
        let runtime = executor();
        // Retain the actual registered call across observer cancellation or executor loss.
        // 跨观测取消或执行器丢失保留实际已注册调用。
        let original = runtime.block_on(fixture.admitted());
        assert!(original.lock().expect("inert state").pool.is_none());
        assert!(!fixture.events.entered.load(Ordering::Acquire));
        assert_busy(&fixture);
        assert!(
            fixture
                .manager
                .prepare_legacy_lifecycle_call("hook-plugin", event, Arc::clone(&original_guard))
                .is_err()
        );
        drop(original_guard);
        runtime.block_on(async {
            handle.start().expect("original start");
            handle.start().expect("repeat start observes original");
            fixture.entered().await;
            assert_busy(&fixture);
            assert_eq!(
                fixture
                    .manager
                    .active_session_count("hook-plugin")
                    .expect("actual live reservation count"),
                0
            );
            assert!(handle.confirm_retired().is_err());
            fixture.events.release();
            handle.observe().await.expect("real callback result");
            retired(&original).await;
            assert_busy(&fixture);
            handle
                .confirm_retired()
                .expect("physical acknowledgement")
                .expect("business success");
            assert!(handle.start().is_err());
        });
        drop(original);
        fixture
            .manager
            .reap_internal_calls(&fixture.owner())
            .expect("original reaper");
        drop(handle);
        // Verify the namespace becomes available only after all original ownership is released.
        // 核验仅全部原所有权释放后命名空间才可获取。
        let available = guard(&fixture);
        drop(available);
        // Inspect committed real native effects to rule out callback replay.
        // 检查已提交真实原生副作用以排除回调重放。
        let events = fixture.events.values.lock().expect("actual native effects");
        assert_eq!(events.len(), 1);
        assert_eq!(
            events.first().expect("one native effect").topic,
            event.key()
        );
    }
}

/// Cancelled observation and bounded unknown cleanup never acknowledge the original guard.
/// 观测取消及有界未知清理绝不确认原护栏。
#[test]
fn legacy_lifecycle_cancelled_observer_and_interrupted_supervisor_recover_only_original() {
    for stop_executor in [false, true] {
        // Own the isolated actual user package and its production manager.
        // 持有隔离实际用户包及其生产管理器。
        let fixture = user_fixture();
        // Retain the original public lifecycle handle until true retirement acknowledgement.
        // 保留原公开生命周期句柄，直至真实退役确认。
        let handle = Arc::new(
            fixture
                .manager
                .prepare_legacy_lifecycle_call(
                    "hook-plugin",
                    VulcanPluginLegacyLifecycleEvent::Install,
                    guard(&fixture),
                )
                .expect("original callback"),
        );
        // Own a deterministic observer executor separate from actual native workers.
        // 持有独立于实际原生工作者的确定性观测执行器。
        let mut runtime = executor();
        // Share only the same original handle with the cancellable observer.
        // 仅将同一原句柄与可取消观测者共享。
        let observer_handle = Arc::clone(&handle);
        // Observe the original call independently of its retained registry ownership.
        // 独立于保留注册表所有权观测原调用。
        let waiter = runtime.spawn(async move {
            observer_handle.start()?;
            observer_handle.observe().await
        });
        runtime.block_on(fixture.entered());
        // Retain the actual registered call across observer cancellation or executor loss.
        // 跨观测取消或执行器丢失保留实际已注册调用。
        let original = runtime.block_on(fixture.admitted());
        // Capture the core-issued original operation identity before interruption.
        // 中断前捕获核心签发原操作身份。
        let operation = original
            .lock()
            .expect("actual state")
            .operation
            .clone()
            .expect("actual operation");
        if stop_executor {
            std::mem::replace(&mut runtime, executor()).shutdown_timeout(WAIT);
        } else {
            waiter.abort();
            assert!(
                runtime
                    .block_on(waiter)
                    .expect_err("observer aborted")
                    .is_cancelled()
            );
        }
        assert!(!original.acknowledged.load(Ordering::Acquire));
        assert_busy(&fixture);
        // Inspect bounded actual cleanup evidence while native work remains blocked.
        // 原生工作仍被阻塞时检查有界实际清理证据。
        let report = runtime
            .block_on(handle.drain_cleanup())
            .expect("bounded original cleanup");
        assert!(!report.failures.is_empty());
        assert!(!original.lock().expect("retained state").retired);
        assert_busy(&fixture);
        fixture.events.release();
        runtime.block_on(async {
            let _ = handle
                .drain_cleanup()
                .await
                .expect("same-call cleanup retry");
            retired(&original).await;
        });
        assert_eq!(
            original
                .lock()
                .expect("original operation")
                .operation
                .as_ref()
                .expect("retained operation")
                .id(),
            operation.id()
        );
        assert_eq!(
            fixture.events.values.lock().expect("native effects").len(),
            1
        );
        // Retain the original public lifecycle handle until true retirement acknowledgement.
        // 保留原公开生命周期句柄，直至真实退役确认。
        let mut handle = Arc::try_unwrap(handle)
            .ok()
            .expect("sole original observer owner");
        let _ = handle
            .confirm_retired()
            .expect("only true retirement acknowledges");
        drop(original);
        drop(handle);
        drop(guard(&fixture));
    }
}

/// Actual missing declaration needs no formal parent, while bundled/foreign authority is rejected.
/// 实际缺失声明无需正式父级，而内置或异属权威被拒绝。
#[test]
fn legacy_lifecycle_missing_declaration_keeps_guard_without_formal_parent() {
    // Own the isolated actual user package and its production manager.
    // 持有隔离实际用户包及其生产管理器。
    let fixture = user_fixture_with_declaration(false);
    // A new actual manager discovers the user package without attaching an executor.
    // 新实际管理器发现用户包，不附加执行器。
    let manager = test_manager(fixture._root.path(), fixture._root.path().join("bundled"));
    manager
        .refresh_catalog(
            &BTreeSet::from(["hook-plugin".to_owned()]),
            &BTreeSet::new(),
        )
        .expect("actual no-parent catalog");
    // Retain the original public lifecycle handle until true retirement acknowledgement.
    // 保留原公开生命周期句柄，直至真实退役确认。
    let mut handle = manager
        .prepare_legacy_lifecycle_call(
            "hook-plugin",
            VulcanPluginLegacyLifecycleEvent::Install,
            guard(&fixture),
        )
        .expect("absent declaration");
    assert!(
        manager
            .formal_runtime_owner()
            .expect("parent query")
            .is_none()
    );
    handle.start().expect("no executor needed");
    assert_busy(&fixture);
    handle
        .confirm_retired()
        .expect("no resources to retire")
        .expect("no callback result");
    drop(guard(&fixture));
}

/// Full user identity and the actual manager pointer fence another manager and bundled precedence.
/// 完整用户身份及实际管理器指针阻止另一管理器及内置优先级绕过。
#[test]
fn legacy_lifecycle_rejects_foreign_manager_and_bundled_descriptor() {
    // Own the isolated actual user package and its production manager.
    // 持有隔离实际用户包及其生产管理器。
    let fixture = user_fixture();
    // Retain the exact original namespace guard before formal registration.
    // 正式注册前保留精确原命名空间护栏。
    let original_guard = guard(&fixture);
    // Freeze the actual selected user descriptor for this original call.
    // 为此原调用冻结实际选定用户描述符。
    let plugin = fixture.manager.snapshot().plugins["hook-plugin"].clone();
    // Retain the same original checked execution authority.
    // 保留同一原已校验执行权威。
    let authority = LegacyLifecycleExecution::new(
        &fixture.manager.state,
        plugin.clone(),
        VulcanPluginLegacyLifecycleEvent::Install,
        Arc::clone(&original_guard),
    )
    .expect("original authority");
    // Construct a different actual manager to test pointer identity rejection.
    // 构造不同实际管理器以测试指针身份拒绝。
    let other = test_manager(fixture._root.path(), fixture._root.path().join("bundled"));
    assert!(
        authority
            .validate(
                &other.state,
                &plugin,
                VulcanPluginBridgeLeaseScope::LifecycleInstall
            )
            .is_err()
    );
    assert!(
        authority
            .validate(
                &fixture.manager.state,
                &plugin,
                VulcanPluginBridgeLeaseScope::LifecycleUninstall
            )
            .is_err()
    );
    // Change only supplied origin metadata to test strict authority matching.
    // 仅改变传入来源元数据以测试严格权威匹配。
    let mut foreign = plugin;
    foreign.origin = crate::VulcanPluginPackageOrigin::Bundled;
    assert!(
        authority
            .validate(
                &fixture.manager.state,
                &foreign,
                VulcanPluginBridgeLeaseScope::LifecycleInstall
            )
            .is_err()
    );
    // Use a real bundled catalog to verify bundled precedence cannot become user authority.
    // 使用真实内置目录核验内置优先级不能成为用户权威。
    let bundled = Fixture::new();
    // Hold the same-root guard while attempting forbidden bundled execution.
    // 尝试被禁止的内置执行时持有同根护栏。
    let bundled_guard = guard(&bundled);
    assert!(
        bundled
            .manager
            .prepare_legacy_lifecycle_call(
                "hook-plugin",
                VulcanPluginLegacyLifecycleEvent::Install,
                bundled_guard
            )
            .is_err()
    );
}

/// Run one actual published callback while retaining its unacknowledged internal record.
/// 运行单个实际已发布回调，同时保留其未确认内部记录。
/// fixture supplies real package/native services; returns the original retired call without acknowledging it.
/// fixture 提供真实插件包及原生服务；返回原已退役调用，不确认它。
async fn retained_published_call(fixture: &Fixture) -> Arc<FormalCall> {
    // The original parent is shared with the later private lifecycle call.
    // 原父级与之后的私有生命周期调用共享。
    let owner = fixture.owner();
    // Freeze the actual current user package before namespace ownership is retained.
    // 保留命名空间所有权前冻结实际当前用户插件包。
    let plugin = fixture.manager.snapshot().plugins["hook-plugin"].clone();
    // Select the fixture's actual manifest declaration rather than inventing an export.
    // 选择夹具实际清单声明，不虚构导出。
    let export = plugin
        .manifest
        .as_ref()
        .expect("actual manifest")
        .lua_runtime
        .as_ref()
        .expect("declared runtime")
        .lifecycle
        .install
        .clone()
        .expect("actual install declaration");
    // Register the actual callback through the same production bounded registration entry.
    // 通过同一生产有界注册入口登记实际回调。
    let call = fixture
        .manager
        .register_owned_formal_call(
            &owner,
            plugin,
            Preparation {
                workspace: CallWorkspace::LifecycleEnvironment,
                arguments: json!({"event":"install", "plugin_id":"hook-plugin"}),
                finalizer: None,
                admission: CallAdmission::Lifecycle { export },
                scope: VulcanPluginBridgeLeaseScope::LifecycleInstall,
                security_partition: format!("retained-published:{}", Uuid::new_v4()),
                deadline: Instant::now() + WAIT,
            },
        )
        .expect("actual retained published registration");
    fixture
        .manager
        .start_registered_formal_call(&owner, &call)
        .expect("actual published start");
    retired(&call).await;
    assert!(
        call.lock()
            .expect("actual business result")
            .result
            .as_ref()
            .expect("terminal result")
            .is_ok()
    );
    assert!(!call.acknowledged.load(Ordering::Acquire));
    call
}

/// Find the actual private lifecycle record registered beside ordinary retained calls.
/// 查找与普通保留调用共同登记的实际私有生命周期记录。
/// owner is the exact shared parent; returns its sole Legacy source record or fails the test.
/// owner 为精确共享父级；返回其唯一旧式来源记录，否则测试失败。
fn original_legacy_call(owner: &ManagedFormalRuntime) -> Arc<FormalCall> {
    owner
        .calls
        .lock()
        .expect("actual registry")
        .values()
        .find(|call| matches!(&call.source, CallExecutionSource::LegacyLifecycle(_)))
        .cloned()
        .expect("actual legacy record")
}

/// Original confirmation ignores unrelated acknowledged records even when their actual core forget fails.
/// 即使无关已确认记录的实际核心遗忘失败，原确认仍忽略它们。
/// Takes no input and returns unit after real callbacks and a real core NotFound checkpoint.
/// 不接收输入；真实回调及真实核心未找到检查点后返回空值。
#[test]
fn legacy_lifecycle_exact_confirmation_ignores_foreign_forget_failure() {
    // Actual user bytes and native effects provide every operation used by this regression.
    // 实际用户字节及原生副作用提供此回归使用的每个操作。
    let fixture = user_fixture();
    fixture.events.release();
    // Drive actual supervisors independently of their retained acknowledgement records.
    // 独立于保留确认记录驱动实际监督者。
    let runtime = executor();
    runtime.block_on(async {
        // Two actual unrelated internal records precede the original guarded lifecycle call.
        // 两个实际无关内部记录先于原受护栏生命周期调用。
        let foreign_success = retained_published_call(&fixture).await;
        // This operation will lose its core acknowledgement while its host record stays retained.
        // 此操作丢失核心确认时，其宿主记录仍被保留。
        let foreign_failure = retained_published_call(&fixture).await;
        // The private call uses the actual original namespace owner after ordinary callbacks retire.
        // 普通回调退役后，私有调用使用实际原命名空间所有者。
        let mut handle = fixture
            .manager
            .prepare_legacy_lifecycle_call(
                "hook-plugin",
                VulcanPluginLegacyLifecycleEvent::Install,
                guard(&fixture),
            )
            .expect("original guarded call");
        // Inspect the original shared parent without registering a replacement.
        // 检查原共享父级，不登记替代调用。
        let owner = fixture.owner();
        // Preserve exact Arc identity for the actual own record.
        // 为实际自身记录保留精确 Arc 身份。
        let original = original_legacy_call(&owner);
        handle.start().expect("original start");
        handle.observe().await.expect("real own callback");
        retired(&original).await;
        // A real terminal core operation supplies the lost-acknowledgement condition.
        // 真实终态核心操作提供丢失确认条件。
        let operation = foreign_failure
            .lock()
            .expect("foreign evidence")
            .operation
            .clone()
            .expect("actual foreign operation");
        owner
            .host
            .runtime()
            .forget_operation(operation.id())
            .expect("actual prior foreign core acknowledgement");
        assert_eq!(
            owner
                .host
                .runtime()
                .forget_operation(operation.id())
                .expect_err("actual foreign forget now fails")
                .code,
            EmbeddedErrorCode::NotFound
        );
        foreign_success.acknowledged.store(true, Ordering::Release);
        foreign_failure.acknowledged.store(true, Ordering::Release);
        handle
            .confirm_retired()
            .expect("only original confirmation")
            .expect("original business result");
        {
            // Own removal and both untouched foreign identities are checked under the same registry.
            // 在同一注册表下检查自身移除及两个未变更异属身份。
            let calls = owner.calls.lock().expect("actual registry");
            assert!(!calls.contains_key(&original.id));
            assert!(Arc::ptr_eq(
                calls
                    .get(&foreign_success.id)
                    .expect("foreign success retained"),
                &foreign_success
            ));
            assert!(Arc::ptr_eq(
                calls
                    .get(&foreign_failure.id)
                    .expect("foreign failure retained"),
                &foreign_failure
            ));
        }
        assert!(
            fixture.manager.reap_internal_calls(&owner).is_err(),
            "unrelated real failure still belongs to ordinary housekeeping"
        );
        assert_eq!(
            fixture
                .events
                .values
                .lock()
                .expect("actual committed callbacks")
                .len(),
            3
        );
    });
}

/// Failed own core acknowledgement retains the same slot and prevents same-guard callback replay.
/// 自身核心确认失败时保留同一槽位，并阻止同护栏回调重放。
/// Takes no input and returns unit after actual core forgetting proves the exact own failure boundary.
/// 不接收输入；实际核心遗忘证明精确自身失败边界后返回空值。
#[test]
fn legacy_lifecycle_failed_own_confirmation_retains_exact_record_and_guard() {
    // Own actual user package bytes and release real native work normally.
    // 持有实际用户插件包字节，并正常释放真实原生工作。
    let fixture = user_fixture();
    fixture.events.release();
    // External mutation ownership must survive both callback retirement and acknowledgement failure.
    // 外部变更所有权必须跨回调退役及确认失败存续。
    let original_guard = guard(&fixture);
    // Retain the sole actual lifecycle handle without fabricated terminal evidence.
    // 保留唯一实际生命周期句柄，不伪造终态证据。
    let mut handle = fixture
        .manager
        .prepare_legacy_lifecycle_call(
            "hook-plugin",
            VulcanPluginLegacyLifecycleEvent::Install,
            Arc::clone(&original_guard),
        )
        .expect("original guarded call");
    // Drive the actual formal supervisor through real native callbacks.
    // 通过真实原生回调驱动实际正式监督者。
    let runtime = executor();
    runtime.block_on(async {
        // The same parent and original record remain available after core acknowledgement failure.
        // 核心确认失败后，同一父级及原记录仍然可用。
        let owner = fixture.owner();
        // Capture the exact registered original Arc before starting it.
        // 启动前捕获精确已注册原 Arc。
        let original = original_legacy_call(&owner);
        handle.start().expect("real start");
        handle.observe().await.expect("real result");
        retired(&original).await;
        // Perform a genuine prior core forget; the next host acknowledgement receives actual NotFound.
        // 执行真实先前核心遗忘；下次宿主确认收到实际未找到错误。
        let operation = original
            .lock()
            .expect("actual own evidence")
            .operation
            .clone()
            .expect("actual own operation");
        owner
            .host
            .runtime()
            .forget_operation(operation.id())
            .expect("actual prior own acknowledgement");
        assert!(handle.confirm_retired().is_err());
        assert!(!original.acknowledged.load(Ordering::Acquire));
        assert!(Arc::ptr_eq(
            owner
                .calls
                .lock()
                .expect("actual registry")
                .get(&original.id)
                .expect("same original slot remains"),
            &original
        ));
        assert_busy(&fixture);
        assert!(
            fixture
                .manager
                .prepare_legacy_lifecycle_call(
                    "hook-plugin",
                    VulcanPluginLegacyLifecycleEvent::Install,
                    Arc::clone(&original_guard)
                )
                .is_err()
        );
        assert!(handle.confirm_retired().is_err());
        assert_eq!(
            fixture
                .events
                .values
                .lock()
                .expect("actual committed callback")
                .len(),
            1
        );
    });
}
