//! Actual managed Uninstall lifecycle tests retain namespace ownership through native work.
//! 实际受管卸载生命周期测试在原生工作期间保留命名空间所有权。

use super::*;
use crate::{
    VulcanPluginActivation, VulcanPluginDeactivationReservation, VulcanPluginGenerationStore,
    VulcanPluginStoreErrorCode,
};

/// Control real removal callbacks using the existing native synchronization fixture.
/// 使用既有原生同步夹具控制真实卸载回调。
struct RemovalEvents(Arc<Events>);

impl VulcanPluginUiEventPublisher for RemovalEvents {
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

/// Select real bytes as a managed package, replacing the bundled fixture namespace.
/// 将真实字节选择为受管插件包，替换内置夹具命名空间。
/// Returns a disabled actual UserInstalled descriptor, the original formal parent and the complete selection.
/// 返回停用实际用户安装描述符、原正式父级及完整选择。
fn user_fixture() -> (Fixture, VulcanPluginActivation) {
    // Own the isolated actual user package and its production manager.
    // 持有隔离实际用户包及其生产管理器。
    let fixture = Fixture::new();
    // Select the isolated source parent outside catalog discovery.
    // 选择目录发现之外的隔离源码父目录。
    let source = fixture._root.path().join("source");
    std::fs::create_dir(&source).expect("source parent");
    let package = source.join("hook-plugin");
    std::fs::rename(fixture._root.path().join("bundled/hook-plugin"), &package)
        .expect("move actual source bytes");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("remove bundled selection");
    let selected =
        crate::install_user_package_for_test(&fixture.manager.state.user_plugin_root, &package);
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
            ui_events: Some(Arc::new(RemovalEvents(Arc::clone(&fixture.events)))),
        })
        .expect("actual native observer");
    (fixture, selected)
}

/// Reserve the original removal of the complete selection; the reservation owns the namespace File.
/// 预留完整选择的原移除；预留拥有命名空间文件。
fn removal(
    fixture: &Fixture,
    selected: &VulcanPluginActivation,
) -> VulcanPluginDeactivationReservation {
    VulcanPluginGenerationStore::open(&fixture.manager.state.user_plugin_root)
        .expect("original store")
        .reserve_deactivation(Uuid::new_v4(), selected)
        .expect("original removal reservation")
}

/// Acquire one real namespace owner over the unchanged selection, proving the namespace is free.
/// 对照未变选择取得单个真实命名空间所有者，证明命名空间空闲。
fn guard(
    fixture: &Fixture,
    selected: &VulcanPluginActivation,
) -> Arc<crate::VulcanPluginNamespaceGuard> {
    Arc::new(
        VulcanPluginGenerationStore::open(&fixture.manager.state.user_plugin_root)
            .expect("original store")
            .acquire_namespace_guard("hook-plugin", Some(selected))
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
        .acquire_namespace_guard("hook-plugin", None)
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
fn managed_uninstall_original_guard_spans_registration_native_callback_and_retirement() {
    let (fixture, selected) = user_fixture();
    // The original reservation retains the exact namespace before formal registration.
    // 原预留在正式注册前保留精确命名空间。
    let original_removal = removal(&fixture, &selected);
    // Retain the original public lifecycle handle until true retirement acknowledgement.
    // 保留原公开生命周期句柄，直至真实退役确认。
    let mut handle = fixture
        .manager
        .prepare_deactivation_lifecycle_call(&original_removal)
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
    drop(original_removal);
    // Verify the namespace becomes available only after all original ownership is released.
    // 核验仅全部原所有权释放后命名空间才可获取。
    drop(guard(&fixture, &selected));
    // Inspect committed real native effects to rule out callback replay.
    // 检查已提交真实原生副作用以排除回调重放。
    let events = fixture.events.values.lock().expect("actual native effects");
    assert_eq!(events.len(), 1);
    assert_eq!(
        events.first().expect("one native effect").topic,
        "uninstall"
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
/// owner is the exact shared parent; returns its sole removal source record or fails the test.
/// owner 为精确共享父级；返回其唯一移除来源记录，否则测试失败。
fn original_removal_call(owner: &ManagedFormalRuntime) -> Arc<FormalCall> {
    owner
        .calls
        .lock()
        .expect("actual registry")
        .values()
        .find(|call| matches!(&call.source, CallExecutionSource::RemovalLifecycle(_)))
        .cloned()
        .expect("actual removal record")
}

/// Original confirmation ignores unrelated acknowledged records even when their actual core forget fails.
/// 即使无关已确认记录的实际核心遗忘失败，原确认仍忽略它们。
/// Takes no input and returns unit after real callbacks and a real core NotFound checkpoint.
/// 不接收输入；真实回调及真实核心未找到检查点后返回空值。
#[test]
fn managed_uninstall_exact_confirmation_ignores_foreign_forget_failure() {
    // Actual user bytes and native effects provide every operation used by this regression.
    // 实际用户字节及原生副作用提供此回归使用的每个操作。
    let (fixture, selected) = user_fixture();
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
        let original_removal = removal(&fixture, &selected);
        let mut handle = fixture
            .manager
            .prepare_deactivation_lifecycle_call(&original_removal)
            .expect("original guarded call");
        // Inspect the original shared parent without registering a replacement.
        // 检查原共享父级，不登记替代调用。
        let owner = fixture.owner();
        // Preserve exact Arc identity for the actual own record.
        // 为实际自身记录保留精确 Arc 身份。
        let original = original_removal_call(&owner);
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
fn managed_uninstall_failed_own_confirmation_retains_exact_record_and_guard() {
    // Own actual user package bytes and release real native work normally.
    // 持有实际用户插件包字节，并正常释放真实原生工作。
    let (fixture, selected) = user_fixture();
    fixture.events.release();
    // The removal reservation must survive both callback retirement and acknowledgement failure.
    // 移除预留必须跨回调退役及确认失败存续。
    let original_removal = removal(&fixture, &selected);
    // Retain the sole actual lifecycle handle without fabricated terminal evidence.
    // 保留唯一实际生命周期句柄，不伪造终态证据。
    let mut handle = fixture
        .manager
        .prepare_deactivation_lifecycle_call(&original_removal)
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
        let original = original_removal_call(&owner);
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
                .prepare_deactivation_lifecycle_call(&original_removal)
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
