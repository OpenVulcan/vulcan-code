//! Real removal routing preserves original callbacks and rejects replacement physical owners.
//! 真实移除路由保留原回调，并拒绝替代物理所有者。

use super::*;
use crate::runtime::routing::ActivationRoutingOriginal;

/// Create actual managed A and published B, optionally entering B's real Draining transition.
/// 创建实际受管 A 及已发布 B，可选择进入 B 真实排空转换。
/// `draining` selects the actual production transition; returns existing fixtures and B's real descriptor.
/// `draining` 选择实际生产转换；返回既有夹具及 B 真实描述符。
fn fixture_with_published_peer(
    draining: bool,
) -> (
    Fixture,
    VulcanPluginGenerationStore,
    VulcanPluginActivation,
    crate::VulcanPluginDescriptor,
) {
    // Existing fixture publishes the actual disabled managed A before independent B discovery.
    // 既有夹具在独立 B 发现之前发布实际停用受管 A。
    let (fixture, store, selected) = super::fixture(false, false);
    crate::runtime::tests::write_idle_session_package(&fixture._root.path().join("bundled"));
    // Exact B source is amended before any inspection, transition or formal admission.
    // 精确 B 来源在任何检查、转换或正式准入之前修改。
    let package = fixture._root.path().join("bundled/idle-plugin");
    // Real manifest declares this ordinary Disable export and its native observation grant.
    // 真实清单声明此普通停用导出及其原生观测授权。
    let manifest = std::fs::read_to_string(package.join("plugin.yaml")).expect("actual B manifest")
        .replace("lua_runtime:\n", "permissions: [ui_host_context]\nlua_runtime:\n  lifecycle:\n    disable: peer_disable\n");
    std::fs::write(package.join("plugin.yaml"), manifest).expect("real B lifecycle declaration");
    std::fs::write(
        package.join("runtime/plugin.bundle.lua"),
        r#"
-- Actual independent B owns ordinary Controller and lifecycle exports.
-- 实际独立 B 拥有普通控制器及生命周期导出。
local M = {}
-- args is the actual Controller envelope; return real business success without lifecycle effects.
-- args 为实际控制器信封；返回真实业务成功，不产生生命周期效果。
function M.dispatch(args) return '{"ok":true,"result":{"accepted":true}}' end
-- args is the actual finalizer envelope; return successful physical shutdown.
-- args 为实际关闭器信封；返回成功物理关闭。
function M.shutdown(args) return '{"ok":true,"result":{"closed":true}}' end
-- args is the original Disable envelope; signal actual native entry before returning success.
-- args 为原停用信封；返回成功之前发出实际原生入场信号。
function M.peer_disable(args)
    assert(args.plugin_id == 'idle-plugin' and args.event == 'disable')
    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='peer-disable',revision=1})
    return '{"ok":true,"result":{}}'
end
return M
"#,
    )
    .expect("real B native callback");
    fixture
        .manager
        .refresh_catalog(
            &BTreeSet::from([selected.plugin_id.clone()]),
            &BTreeSet::new(),
        )
        .expect("actual A/B catalog");
    if draining {
        fixture
            .manager
            .begin_draining("idle-plugin")
            .expect("real B Draining transition");
    }
    // Only production transition supplies the descriptor used by ordinary lifecycle admission.
    // 仅生产转换提供普通生命周期准入使用的描述符。
    let peer = fixture.manager.snapshot().plugins["idle-plugin"].clone();
    assert_eq!(
        peer.runtime_state,
        if draining {
            crate::VulcanPluginRuntimeState::Draining
        } else {
            crate::VulcanPluginRuntimeState::Enabled
        }
    );
    (fixture, store, selected, peer)
}

/// Register an actual ordinary Disable without starting its existing production pending input.
/// 登记实际普通停用，不启动其既有生产待处理输入。
/// `fixture`/`peer` own the actual parent and transitioned source; returns the real retained formal record.
/// `fixture`／`peer` 拥有实际父级及已转换来源；返回真实保留正式记录。
fn register_peer_disable(
    fixture: &Fixture,
    peer: &crate::VulcanPluginDescriptor,
) -> Result<Arc<FormalCall>, VulcanPluginRuntimeError> {
    // Trusted Disable event supplies the same scope used by the real lifecycle entry.
    // 可信停用事件提供真实生命周期入口使用的相同作用域。
    let scope = crate::VulcanPluginBridgeLeaseScope::from(Event::Disable);
    // Export is obtained from the real published manifest rather than guessed admission metadata.
    // 导出从真实已发布清单取得，而非猜测的准入元数据。
    let export = peer
        .manifest
        .as_ref()
        .expect("real B manifest")
        .lua_runtime
        .as_ref()
        .expect("real B runtime")
        .lifecycle
        .disable
        .clone()
        .expect("real declared Disable");
    fixture.manager.register_owned_formal_call(
        &fixture.owner(),
        peer.clone(),
        Preparation {
            workspace: CallWorkspace::LifecycleEnvironment,
            arguments: json!({"event":"disable","plugin_id":peer.id}),
            finalizer: None,
            admission: CallAdmission::Lifecycle { export },
            scope,
            security_partition: format!("lifecycle:{}:{}", scope.as_str(), peer.generation),
            deadline: Instant::now()
                .checked_add(WAIT)
                .expect("finite actual deadline"),
        },
    )
}

/// Observe the sole real physical-retirement predicate on `call`, without acknowledgement or synthetic state.
/// 观测 `call` 唯一真实物理退役判定，不确认或合成状态。
/// Returns after actual retirement within the existing fixture deadline.
/// 在既有夹具截止时间内实际退役后返回。
async fn retired_original(call: &Arc<FormalCall>) {
    tokio::time::timeout(WAIT, async {
        while !call
            .lock()
            .expect("actual retained state")
            .can_acknowledge_retirement()
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("actual physical retirement");
}

/// Real ordinary pending lifecycle excludes A's routing until actual retirement and explicit acknowledgement.
/// 真实普通待处理生命周期排除 A 路由，直至实际退役及明确确认。
/// No parameters; old production proves its post-reservation native-effect window before this assertion fails.
/// 无参数；旧生产在此断言失败之前证明其预留后原生效果窗口。
#[test]
fn deactivation_routing_original_excludes_real_pending_published_lifecycle_until_acknowledged() {
    // Real production transitions create B Draining while A remains actually disabled.
    // 真实生产转换创建 B 排空状态，同时 A 保持实际停用。
    let (fixture, store, selected, peer) = fixture_with_published_peer(true);
    // Actual registration creates one unstarted ordinary Lifecycle with a Published source.
    // 实际登记创建单个未启动普通生命周期，来源为已发布。
    let pending = register_peer_disable(&fixture, &peer).expect("real pending B lifecycle");
    assert!(matches!(&pending.ownership, CallOwner::Lifecycle));
    assert!(matches!(&pending.source, CallExecutionSource::Published));
    assert!(
        pending
            .lock()
            .expect("real pending input")
            .pending
            .is_some()
    );
    assert!(
        fixture
            .events
            .values
            .lock()
            .expect("zero native effects")
            .is_empty()
    );
    // Actual generation-store producer creates A's physical removal original.
    // 实际代次存储生产者创建 A 物理移除原实例。
    let original = reserve(&store, &selected);
    // Capture the production result before scheduling any pending native work.
    // 调度任何待处理原生工作之前捕获生产结果。
    let attempted = fixture.manager.reserve_deactivation_routing(&original);
    // Existing executor runs only real registered work, never synthetic state assignments.
    // 既有执行器仅运行真实已登记工作，绝不合成状态赋值。
    let runtime = executor();
    if let Ok(routing) = attempted {
        runtime.block_on(async {
            fixture
                .manager
                .start_registered_formal_call(&fixture.owner(), &pending)
                .expect("old production starts Published B after A routing");
            fixture.entered().await;
        });
        assert!(
            pending
                .lock()
                .expect("actual started input")
                .pending
                .is_none()
        );
        fixture.events.release();
        runtime.block_on(retired_original(&pending));
        assert_eq!(
            fixture
                .events
                .values
                .lock()
                .expect("actual late native effects")
                .len(),
            1
        );
        fixture
            .manager
            .acknowledge_original_internal_call(&fixture.owner(), &pending)
            .expect("real reproduction cleanup")
            .expect("real native result");
        drop(routing);
        panic!(
            "Routing accepted a real pending Published Lifecycle; its native callback actually started after reservation."
        );
    }
    assert!(
        pending
            .lock()
            .expect("retained pending input after Busy")
            .pending
            .is_some()
    );
    assert!(
        fixture
            .events
            .values
            .lock()
            .expect("still zero effects")
            .is_empty()
    );
    runtime.block_on(async {
        fixture
            .manager
            .start_registered_formal_call(&fixture.owner(), &pending)
            .expect("start actual B after rejected A reservation");
        fixture.entered().await;
    });
    assert!(
        fixture
            .manager
            .reserve_deactivation_routing(&original)
            .is_err()
    );
    fixture.events.release();
    runtime.block_on(retired_original(&pending));
    assert!(!pending.acknowledged.load(Ordering::Acquire));
    assert!(
        fixture
            .manager
            .reserve_deactivation_routing(&original)
            .is_err()
    );
    fixture
        .manager
        .acknowledge_original_internal_call(&fixture.owner(), &pending)
        .expect("actual physical acknowledgement")
        .expect("real callback result");
    // Only actual acknowledgement releases the existing call-map exclusion.
    // 仅实际确认解除既有调用映射互斥。
    let routing = fixture
        .manager
        .reserve_deactivation_routing(&original)
        .expect("A routing after actual B acknowledgement");
    assert_eq!(
        fixture
            .events
            .values
            .lock()
            .expect("one real lifecycle effect")
            .len(),
        1
    );
    drop(routing);
}

/// A routing reservation acquired first rejects actual ordinary Lifecycle registration without consuming input.
/// 先取得的路由预留拒绝实际普通生命周期登记，不消费输入。
/// No parameters; uses the real B transition and zero native effects to prove the reverse ordering.
/// 无参数；使用真实 B 转换及零原生效果证明反向排序。
#[test]
fn deactivation_routing_original_acquired_first_rejects_real_published_lifecycle_registration() {
    // Both plugin states are produced by actual catalog and lifecycle entries.
    // 两个插件状态均由实际目录及生命周期入口生产。
    let (fixture, store, selected, peer) = fixture_with_published_peer(true);
    // Actual A original owns the namespace before its routing guard is published.
    // 路由守卫发布之前，实际 A 原实例拥有命名空间。
    let original = reserve(&store, &selected);
    // Sole weak coordination wins the original existing registration gate.
    // 唯一弱协调先取得原既有登记门。
    let routing = fixture
        .manager
        .reserve_deactivation_routing(&original)
        .expect("routing before lifecycle registration");
    assert!(register_peer_disable(&fixture, &peer).is_err());
    assert!(
        fixture
            .owner()
            .calls
            .lock()
            .expect("actual retained calls")
            .is_empty()
    );
    assert!(
        fixture
            .events
            .values
            .lock()
            .expect("zero native effects")
            .is_empty()
    );
    drop(routing);
}

/// Actual Published Controller ownership remains available while A reserves routing.
/// A 预留路由期间，实际已发布控制器所有权保持可用。
/// No parameters; verifies a real registered pending Controller neither blocks reservation nor loses business admission.
/// 无参数；验证真实已登记待处理控制器既不阻挡预留，也不丢失业务准入。
#[test]
fn deactivation_routing_original_allows_real_retained_published_controller_business() {
    // Actual B stays Enabled; no ordinary lifecycle transition or effect is synthesized.
    // 实际 B 保持启用；不合成普通生命周期转换或效果。
    let (fixture, store, selected, peer) = fixture_with_published_peer(false);
    // Real manifest is the execution export/finalizer authority for this Controller registration.
    // 真实清单为此控制器登记的执行导出／关闭器权威。
    let runtime_manifest = peer
        .manifest
        .as_ref()
        .expect("actual manifest")
        .lua_runtime
        .as_ref()
        .expect("actual runtime");
    // Existing parent owns the genuine pending Controller before A routing is acquired.
    // A 路由取得之前，既有父级拥有真实待处理控制器。
    let owner = fixture.owner();
    // Preparation uses the same production envelope and finalizer as invoke_formal_stateless.
    // 准备使用与 invoke_formal_stateless 相同的生产信封及关闭器。
    let pending = fixture
        .manager
        .register_owned_formal_call(
            &owner,
            peer.clone(),
            Preparation {
                workspace: CallWorkspace::Provided(fixture._root.path().to_path_buf()),
                arguments: fixture
                    .manager
                    .formal_envelope(json!({}), 1000)
                    .expect("actual business envelope"),
                finalizer: Some(ModuleFinalizer {
                    export: runtime_manifest.shutdown_function.clone(),
                    arguments: fixture
                        .manager
                        .formal_envelope(
                            plugin_payload("shutdown", Map::new()).expect("real finalizer payload"),
                            owner.settings.shutdown_timeout_ms,
                        )
                        .expect("actual finalizer envelope"),
                    timeout_ms: owner.settings.shutdown_timeout_ms,
                }),
                admission: CallAdmission::Controller {
                    tool_id: "idle_ping".to_owned(),
                    export: runtime_manifest.invoke_function.clone(),
                    owner_session_id: "published-business".to_owned(),
                },
                scope: crate::VulcanPluginBridgeLeaseScope::Enabled,
                security_partition: "tool:published-business".to_owned(),
                deadline: Instant::now()
                    .checked_add(WAIT)
                    .expect("bounded actual Controller deadline"),
            },
        )
        .expect("real pending Controller");
    assert!(matches!(&pending.source, CallExecutionSource::Published));
    assert!(matches!(&pending.ownership, CallOwner::Controller(id) if id == "published-business"));
    // Real A retains its physical original separately from B's Controller ownership.
    // 真实 A 独立于 B 控制器所有权保留物理原实例。
    let original = reserve(&store, &selected);
    // Only ordinary lifecycle originals participate in routing exclusion.
    // 仅普通生命周期原实例参与路由互斥。
    let routing = fixture
        .manager
        .reserve_deactivation_routing(&original)
        .expect("retained Controller does not exclude routing");
    // Existing executor starts and physically retires the actual original Controller under unchanged routing.
    // 既有执行器在未变路由内启动并物理退役实际原控制器。
    let runtime = executor();
    runtime.block_on(async {
        fixture
            .manager
            .start_registered_formal_call(&owner, &pending)
            .expect("actual Controller starts under routing reservation");
        retired_original(&pending).await;
    });
    assert_eq!(
        pending
            .lock()
            .expect("actual business state")
            .result
            .clone()
            .expect("actual result")
            .expect("real business success"),
        json!({"accepted":true})
    );
    fixture
        .manager
        .forget_formal_call(&pending.id, "published-business")
        .expect("actual Controller acknowledgement");
    assert!(
        fixture
            .events
            .values
            .lock()
            .expect("zero lifecycle effects")
            .is_empty()
    );
    drop(routing);
}

/// Real native Uninstall survives ordinary refresh attempts under the same removal reservation.
/// 相同移除预留内，真实原生卸载跨普通刷新尝试存续。
/// No parameters; asserts actual callback, same namespace and exact tombstone projection through retirement.
/// 无参数；断言实际回调、相同命名空间及跨退役的精确停用投影。
#[test]
fn deactivation_routing_original_keeps_native_callback_and_exact_tombstone_projection() {
    // Existing production fixture owns the actual selected managed package.
    // 既有生产夹具拥有实际已选受管包。
    let (fixture, store, selected) = fixture(true, false);
    // Same original remains strongly retained before any routing or callback effect.
    // 任何路由或回调效果之前，相同原实例保持强保留。
    let mut removal = reserve(&store, &selected);
    // Sole coordination retains only metadata and weak namespace identity.
    // 唯一协调仅保留元数据及弱命名空间身份。
    let routing = fixture
        .manager
        .reserve_deactivation_routing(&removal)
        .expect("real removal routing");
    assert_eq!(removal.namespace_identity().strong_count(), 1);
    // Exact original routing is frozen before real native registration.
    // 真实原生登记之前固定精确原路由。
    let catalog = fixture.manager.snapshot();
    assert!(
        fixture
            .manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .is_err()
    );
    assert!(Arc::ptr_eq(&catalog, &fixture.manager.snapshot()));
    // Actual source admission registers one retained callback under the same namespace.
    // 实际来源准入在相同命名空间内登记单个保留回调。
    let mut callback = fixture
        .manager
        .prepare_deactivation_lifecycle_call(&removal)
        .expect("same original callback");
    // Observe the real callback with the existing bounded executor.
    // 用既有有界执行器观测真实回调。
    let runtime = executor();
    runtime.block_on(async {
        callback.start().expect("same original start");
        callback.start().expect("repeated start does not replay");
        fixture.entered().await;
    });
    assert!(
        fixture
            .manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .is_err()
    );
    assert!(Arc::ptr_eq(&catalog, &fixture.manager.snapshot()));
    assert!(store.continue_deactivation_retained(&mut removal).is_err());
    // The actual admitted record stays retained through true physical retirement.
    // 实际已准入记录跨真实物理退役保持保留。
    let admitted = runtime.block_on(fixture.admitted());
    fixture.events.release();
    runtime
        .block_on(callback.observe())
        .expect("actual native result");
    runtime
        .block_on(callback.drain_cleanup())
        .expect("same callback cleanup");
    runtime.block_on(async {
        tokio::time::timeout(WAIT, async {
            while !admitted
                .lock()
                .expect("actual call state")
                .can_acknowledge_retirement()
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("true original retirement");
    });
    callback
        .confirm_retired()
        .expect("actual retirement")
        .expect("actual business result");
    drop(admitted);
    assert_eq!(removal.namespace_identity().strong_count(), 1);
    assert_eq!(
        fixture.events.values.lock().expect("actual effects").len(),
        1
    );
    assert_eq!(
        store
            .continue_deactivation_retained(&mut removal)
            .expect("same original publication after pre-intent rejection"),
        VulcanPluginDeactivationObservation::Published
    );
    assert_eq!(
        store
            .continue_deactivation_retained(&mut removal)
            .expect("same admitted intent continues read-only"),
        VulcanPluginDeactivationObservation::Published
    );
    // Only the actual published original may perform the removal projection.
    // 仅实际已发布原实例可以执行移除投影。
    let removed = fixture
        .manager
        .refresh_catalog_with_deactivation(
            &BTreeSet::from([selected.plugin_id.clone()]),
            &BTreeSet::new(),
            &removal,
        )
        .expect("same actual tombstone projection");
    assert!(!removed.plugins.contains_key(&selected.plugin_id));
    drop(routing);
    drop(removal);
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("ordinary refresh after actual release");
}

/// Same UUID and complete record cannot replace the original namespace allocation after actual File release.
/// 实际文件释放之后，相同 UUID 及完整记录不能替代原命名空间分配。
/// No parameters; asserts rejection of real same-store replacement and another actual manager.
/// 无参数；断言拒绝真实同存储替代及另一实际管理器。
#[test]
fn deactivation_routing_original_rejects_same_record_replacement_file_and_foreign_manager() {
    // Use verified absent callbacks to isolate real physical identity from execution ownership.
    // 使用已核验缺失回调，将真实物理身份与执行所有权隔离。
    let (fixture, store, selected) = fixture(false, false);
    // This nonnil request is allocated once and reused only by the real contender test.
    // 此非空请求仅分配一次，仅由真实争用测试复用。
    let request = Uuid::new_v4();
    // Actual producer retains the real original namespace File.
    // 实际生产者保留真实原命名空间文件。
    let original = store
        .reserve_deactivation(request, &selected)
        .expect("actual original");
    // Coordinator does not keep that File alive after physical owner loss.
    // 物理所有者丢失之后，协调器不使该文件存活。
    let routing = fixture
        .manager
        .reserve_deactivation_routing(&original)
        .expect("original routing");
    // Original Weak keeps allocation identity, not the namespace lock.
    // 原 Weak 保留分配身份，而非命名空间锁。
    let identity = original.namespace_identity();
    drop(original);
    assert_eq!(identity.strong_count(), 0);
    // Real namespace acquisition succeeds with exactly the same request and complete selection.
    // 使用完全相同请求及完整选择，真实命名空间获取成功。
    let replacement = store
        .reserve_deactivation(request, &selected)
        .expect("new actual File allocation");
    assert_eq!(replacement.original(), &selected);
    assert!(!replacement.matches_namespace_identity(&identity));
    {
        // Existing manager gate guards the production coordination predicate.
        // 既有管理器门保护生产协调判定。
        let registration = fixture
            .manager
            .state
            .session_registration_gate
            .lock()
            .expect("actual gate");
        assert!(
            fixture
                .manager
                .require_routing_publication_locked(
                    &registration,
                    Some(ActivationRoutingOriginal::Deactivation(&replacement)),
                )
                .is_err()
        );
    }
    assert!(
        fixture
            .manager
            .prepare_deactivation_lifecycle_call(&replacement)
            .is_err()
    );
    // Another actual manager points at a different canonical production store.
    // 另一实际管理器指向不同规范生产存储。
    let (foreign, _, _) = super::fixture(false, false);
    assert!(
        foreign
            .manager
            .reserve_deactivation_routing(&replacement)
            .is_err()
    );
    drop(routing);
    fixture
        .manager
        .reserve_deactivation_routing(&replacement)
        .expect("new original only after coordinator retirement");
}
