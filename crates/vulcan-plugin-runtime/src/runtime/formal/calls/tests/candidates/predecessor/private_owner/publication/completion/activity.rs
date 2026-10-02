//! Real unrelated Published controller activity preserves the original closed and sealed predecessor routing allocations.
//! 真实无关 Published 控制器活动保留原关闭及密封前驱路由分配。

use super::*;
use crate::VulcanPluginRoutingReservation;
use crate::runtime::tests::write_idle_session_package;

/// Add real B before A reserves routing and signs its closed source; return production owners and their retained coordinator.
/// 在 A 预留路由及签发关闭来源之前添加真实 B；返回生产所有者及其保留协调器。
/// No inputs; both plugins use the same genuine formal parent and existing native Events barrier.
/// 无输入；两个插件使用同一真实正式父级及既有原生 Events 屏障。
fn publication_fixture_with_other_plugin() -> (
    Fixture,
    VulcanPluginGenerationStore,
    VulcanPluginActivationTransaction,
    VulcanPluginPredecessorRestoration,
    Arc<VulcanPluginPredecessorRestorationSource>,
    VulcanPluginRoutingReservation,
) {
    // The original actual drain and immutable old pins exist before unrelated B enters discovery.
    // 无关 B 进入发现之前，原实际排空及不可变旧固定句柄已经存在。
    let (fixture, store, mut transaction, mut restoration) = private_owner_fixture();
    write_idle_session_package(&fixture._root.path().join("bundled"));
    // This exact prepublication package is owned by the existing isolated fixture.
    // 此精确发布前插件包由既有隔离夹具拥有。
    let package = fixture._root.path().join("bundled/idle-plugin");
    // The existing stateless tool retains its declared identity; only its native observation grant and finite budget change.
    // 既有无状态工具保留声明身份；仅改变其原生观测授权及有限预算。
    let manifest = std::fs::read_to_string(package.join("plugin.yaml"))
        .expect("actual unrelated package manifest")
        .replace(
            "timeout_ms: 1000",
            &format!("timeout_ms: {}", WAIT.as_millis()),
        );
    std::fs::write(
        package.join("plugin.yaml"),
        format!("{manifest}permissions: [ui_host_context]\n"),
    )
    .expect("actual unrelated observation permission");
    std::fs::write(
        package.join("runtime/plugin.bundle.lua"),
        r#"
-- Each controller invocation owns one real fresh module.
-- 每次控制器调用拥有单个真实全新模块。
local M = {}
-- args is the actual Controller envelope; return success only after the host's native barrier releases.
-- args 为实际 Controller 信封；仅在宿主原生屏障释放后返回成功。
function M.dispatch(args)
    assert(type(args.payload_json) == 'string')
    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='enable',revision=1})
    return '{"ok":true,"result":{"accepted":true}}'
end
-- args is the actual finalizer envelope; return successful physical shutdown without entering another barrier.
-- args 为实际关闭信封；返回成功物理关闭，不进入另一个屏障。
function M.shutdown(args)
    assert(type(args.payload_json) == 'string')
    return '{"ok":true,"result":{"closed":true}}'
end
return M
"#,
    )
    .expect("actual unrelated native callback module");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("publish unrelated B before signing A source");
    // Register the first actual native issuer before reserving this still-Prepared transaction's immutable routing.
    // 在预留此仍处于 Prepared 的事务不可变路由之前，登记首个实际原生签发者。
    let first_issuer = fixture.manager.register_native_mcp_host_adapter();
    // The non-cloneable guard is signed before Pending, abort or Source and remains the sole strong coordination owner.
    // 不可克隆守卫在 Pending、中止及 Source 之前签发，并保持为唯一强协调所有者。
    let routing = fixture
        .manager
        .reserve_activation_routing(&first_issuer, &transaction)
        .expect("reserve actual Prepared A routing after the only B refresh");
    fixture
        .manager
        .prepare_activation_predecessor_private_owner(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("actual independent Pending owner");
    fixture
        .manager
        .abort_and_stage_activation_predecessor_restoration(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("actual aborted closed A route with published B");
    // Sign the original actual Source only after B's complete production declaration is frozen in this catalog.
    // 仅在 B 的完整生产声明固定于此目录之后，签发原实际 Source。
    let source = fixture
        .manager
        .prepare_activation_predecessor_source(&transaction, &mut restoration)
        .expect("same original physical A source");
    (fixture, store, transaction, restoration, source, routing)
}

/// Enter one real public B Controller under fixture's same parent; return its original observer after native callback entry.
/// 在 fixture 同一父级下进入单个真实公开 B 控制器；原生回调入场后返回其原观察器。
/// No counter or participant is manufactured; actual registry and live projection independently prove Published admission.
/// 不制造计数或参与者；实际注册表及实时投影独立证明 Published 入场。
async fn enter_other_controller(
    fixture: &Fixture,
) -> tokio::task::JoinHandle<Result<Value, VulcanPluginRuntimeError>> {
    // The public request retains the real fixture workspace and a distinct Controller identity.
    // 公开请求保留真实夹具工作区及不同 Controller 身份。
    let request = crate::VulcanPluginInvocationRequest {
        tool_id: String::from("idle_ping"),
        arguments: json!({}),
        owner_session_id: String::from("unrelated-controller"),
        workspace_root: fixture._root.path().to_path_buf(),
    };
    // Only the same actual manager submits this ordinary Published business.
    // 仅相同实际管理器提交此普通 Published 业务。
    let manager = fixture.manager.clone();
    // The observer owns no replacement Source, Ready evidence or synthetic participant.
    // 观察器不拥有替代 Source、Ready 凭据或合成参与者。
    let call = tokio::spawn(async move { manager.invoke(request).await });
    fixture.entered().await;
    // Observe the existing actual registry by plugin identity, avoiding positional assumptions about retained records.
    // 按插件身份观测既有实际注册表，避免对保留记录的位置作假设。
    let parent = fixture.owner();
    {
        // This short lock is released before any native release or await.
        // 此短锁在任何原生释放或 await 之前解除。
        let calls = parent.calls.lock().expect("actual Published call registry");
        // The production allocator owns B's actual admitted call, separately from A's persistent Hook registry.
        // 生产分配器拥有 B 的实际已入场调用，与 A 的持久 Hook 注册表分离。
        let actual = calls
            .values()
            .find(|actual| actual.plugin.id == "idle-plugin")
            .expect("actual unrelated B admission");
        assert!(matches!(&actual.source, CallExecutionSource::Published));
        assert!(matches!(
            &actual.ownership,
            CallOwner::Controller(id) if id == "unrelated-controller"
        ));
        assert!(!actual.lock().expect("actual B physical state").retired);
    }
    assert_eq!(
        fixture
            .manager
            .active_session_count("idle-plugin")
            .expect("actual B live reservation"),
        1
    );
    // The display-only owned value must reflect the same real count without taking over routing identity.
    // 仅展示自有值必须反映相同真实计数，不接管路由身份。
    let observed = fixture
        .manager
        .observe_catalog(&fixture.manager.snapshot())
        .expect("actual B display projection");
    assert_eq!(observed.plugins["idle-plugin"].active_session_count, 1);
    call
}

/// Release fixture's actual B native callback and await its original observer; return no synthetic retirement evidence.
/// 释放 fixture 实际 B 原生回调并等待其原观察器；不返回合成退役凭据。
/// call belongs to the public request above; real finalization and acknowledgement must release its live reservation.
/// call 属于上述公开请求；真实关闭及确认必须释放其实时预留。
async fn finish_other_controller(
    fixture: &Fixture,
    call: tokio::task::JoinHandle<Result<Value, VulcanPluginRuntimeError>>,
) {
    fixture.events.release();
    assert_eq!(
        tokio::time::timeout(WAIT, call)
            .await
            .expect("finite actual B completion")
            .expect("actual B observer joined")
            .expect("actual B business and physical finalization"),
        json!({"accepted":true})
    );
    assert_eq!(
        fixture
            .manager
            .active_session_count("idle-plugin")
            .expect("actual B released reservation"),
        0
    );
    assert!(
        fixture
            .manager
            .formal_calls("unrelated-controller")
            .expect("actual B acknowledged call history")
            .is_empty()
    );
    // Live display projection also observes real zero after original finalization, while routing stays separately owned.
    // 原关闭之后，实时展示投影同样观测真实零计数，而路由保持独立归属。
    let observed = fixture
        .manager
        .observe_catalog(&fixture.manager.snapshot())
        .expect("actual B retired display projection");
    assert_eq!(observed.plugins["idle-plugin"].active_session_count, 0);
}

/// Published B entry and retirement preserve A's signed closed Arc and permit original Hook initialization once.
/// Published B 入场及退役保留 A 已签发关闭 Arc，并允许原 Hook 仅初始化一次。
/// No inputs or return; real B calls and actual pool/session/initialization IDs prove stable Source ownership.
/// 无输入或返回值；真实 B 调用及实际池／会话／初始化身份证明稳定 Source 归属。
#[test]
fn predecessor_closed_source_survives_real_unrelated_controller_activity() {
    // Both actual plugins precede the retained reservation; its non-cloneable guard spans all real B business and A completion.
    // 两个实际插件先于保留预留权；其不可克隆守卫覆盖全部真实 B 业务及 A 完成。
    let (fixture, _store, transaction, restoration, source, routing) =
        publication_fixture_with_other_plugin();
    // The exact original closed allocation remains the sole routing authority throughout B's live count changes.
    // B 实时计数变化期间，精确原关闭分配保持为唯一路由权威。
    let closed = Arc::clone(source.staged_catalog());
    // Probe only the actual original independent reservation file, never a new lease owner.
    // 仅探测实际原独立预留文件，绝不创建新租约所有者。
    let competing = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            restoration
                .private_owner_test_path(false)
                .expect("actual A lease path"),
        )
        .expect("actual A independent File probe");
    executor().block_on(async {
        // Public B admission must not replace the original closed allocation or its routing revision.
        // 公开 B 入场不得替换原关闭分配或其路由修订。
        let call = enter_other_controller(&fixture).await;
        assert!(Arc::ptr_eq(&closed, &fixture.manager.snapshot()));
        assert_eq!(fixture.manager.snapshot().revision, closed.revision);
        assert!(
            fixture
                .manager
                .prewarm_activation_predecessor_hook(&source)
                .await
                .expect("actual A initialization while Published B runs")
        );
        // Capture A's real private preparation before B retires; no synthetic readiness is accepted.
        // 在 B 退役前捕获 A 真实私有准备；不接受合成就绪。
        let original = initialized_hook(&fixture);
        // These physical identities must remain the same after unrelated B's count returns to zero.
        // 无关 B 计数归零后，这些物理身份必须保持相同。
        let identities = original.target_test_identity();
        assert!(identities.0.is_some() && identities.1.is_some() && identities.2.is_some());
        assert!(identities.3 && !identities.4);
        finish_other_controller(&fixture, call).await;
        assert!(Arc::ptr_eq(&closed, &fixture.manager.snapshot()));
        assert_eq!(fixture.manager.snapshot().revision, closed.revision);
        assert!(matches!(
            competing.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        assert!(
            fixture
                .manager
                .prewarm_activation_predecessor_hook(&source)
                .await
                .expect("same A initialization observation after real B retirement")
        );
        assert_eq!(original.target_test_identity(), identities);
        // The successful genuine publisher and completion transfer the same A instead of reinitializing it.
        // 成功真实发布器及完成转交相同 A，不重新初始化它。
        let publication = ready_publication(&fixture, &transaction, &source).await;
        fixture
            .manager
            .complete_activation_predecessor_handoff(&transaction, &source, &publication)
            .expect("same actual A completes after unrelated B activity");
        // Successful transfer changes only private ownership flags, preserving all actual physical identities.
        // 成功转交仅改变私有归属标志，保留全部实际物理身份。
        let transferred = original.target_test_identity();
        assert_eq!(
            (&transferred.0, &transferred.1, &transferred.2),
            (&identities.0, &identities.1, &identities.2)
        );
        assert!(!transferred.3 && !transferred.4);
        competing
            .try_lock()
            .expect("actual A reservation released after completion");
        competing.unlock().expect("release independent observation");
    });
    // Explicit retirement follows every real B admission/count change and the original successful A handoff.
    // 明确退役位于每个真实 B 入场／计数变化及原成功 A 移交之后。
    drop(routing);
}

/// Real B admission and retirement during Unknown preserve the same sealed A catalog and permit read-only Transferred retry.
/// Unknown 期间真实 B 入场及退役保留相同 A 密封目录，并允许只读 Transferred 重试。
/// No inputs or return; this Core seal fixture proves original Hook/File transfer, without claiming a service Full or native MCP participant.
/// 无输入或返回值；此核心密封夹具证明原 Hook／文件转交，不宣称服务 Full 或原生 MCP 参与者。
#[test]
fn predecessor_unknown_transfer_survives_real_unrelated_controller_activity() {
    // The actual original retains its pre-Source non-cloneable coordinator throughout B activity and Unknown reconciliation.
    // 实际原请求在 B 活动及 Unknown 对账全程保留其 Source 之前签发的不可克隆协调器。
    let (fixture, store, transaction, restoration, source, routing) =
        publication_fixture_with_other_plugin();
    // Capture exact real writer paths before any confirmed transfer consumes its original reservation.
    // 已确认转交消费原预留权之前，捕获精确真实写入器路径。
    let sidecar_path = restoration
        .private_owner_test_path(true)
        .expect("actual A sidecar");
    // This existing independent descriptor observes the original File across both real count changes.
    // 此既有独立描述符跨两个真实计数变化观测原文件。
    let competing = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            restoration
                .private_owner_test_path(false)
                .expect("actual A lease"),
        )
        .expect("actual independent A File probe");
    // The entire actual Aborted activation remains distinct from sidecar transfer evidence.
    // 整个实际 Aborted 激活始终区别于附属转交凭据。
    let aborted = transaction.record().clone();
    executor().block_on(async {
        // Actual A initialization precedes the genuine sealed publisher, independently of B's later call.
        // 实际 A 初始化先于真实密封发布器，独立于 B 随后调用。
        let publication = ready_publication(&fixture, &transaction, &source).await;
        // Only the actual registered persistent Hook supplies these original physical identities.
        // 仅实际已注册持久 Hook 提供这些原物理身份。
        let original = initialized_hook(&fixture);
        // Unknown must preserve this same real pool, session and initialization operation.
        // Unknown 必须保留此相同真实池、会话及初始化操作。
        let identities = original.target_test_identity();
        // The genuine weak domain observer retains no replacement physical owner.
        // 真实弱域观察器不保留替代物理所有者。
        let domain = fixture
            .owner()
            .host
            .target_test_domain_identity(identities.0.as_deref().expect("actual A pool"));
        // This existing controlled fault loses only observation after the real Transferred bytes commit.
        // 此既有受控故障仅在真实 Transferred 字节提交后丢失观测。
        let loss = store.lose_predecessor_handoff_observation_once(source.request_id());
        // The real completion retains all physical participants when its writer cannot confirm immediate readback.
        // 写入器不能确认即时回读时，真实完成保留全部物理参与者。
        let failure = fixture
            .manager
            .complete_activation_predecessor_handoff(&transaction, &source, &publication)
            .expect_err("actual Transferred observation Unknown");
        drop(loss);
        assert!(
            failure
                .message
                .contains("Controlled loss of predecessor handoff immediate readback")
        );
        // Observe the committed complete actual record and bytes before unrelated B enters.
        // 无关 B 入场之前，观测已提交完整实际记录及字节。
        let transferred = store
            .predecessor_handoff_record("hook-plugin", source.request_id())
            .expect("actual Transferred read")
            .expect("actual A handoff exists");
        assert_eq!(
            transferred.phase(),
            crate::VulcanPluginPredecessorHandoffPhase::Transferred
        );
        // Exact bytes must remain unchanged across B activity and the same original read-only retry.
        // 跨 B 活动及相同原只读重试，精确字节必须保持不变。
        let bytes = std::fs::read(&sidecar_path).expect("actual committed A transfer bytes");
        // Enter real B under the same parent while A's actual physical transfer remains unconfirmed.
        // A 实际物理转交仍未确认时，在同一父级下进入真实 B。
        let call = enter_other_controller(&fixture).await;
        assert!(Arc::ptr_eq(
            publication.catalog(),
            &fixture.manager.snapshot()
        ));
        assert_eq!(
            fixture.manager.snapshot().revision,
            publication.catalog().revision
        );
        assert_eq!(original.target_test_identity(), identities);
        assert!(matches!(
            competing.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        finish_other_controller(&fixture, call).await;
        assert!(Arc::ptr_eq(
            publication.catalog(),
            &fixture.manager.snapshot()
        ));
        assert_eq!(
            fixture.manager.snapshot().revision,
            publication.catalog().revision
        );
        assert_eq!(original.target_test_identity(), identities);
        assert!(matches!(
            competing.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        // A real obstruction would reject another write, proving completion reconciles its same Transferred record read-only.
        // 真实阻挡会拒绝另一次写入，证明完成只读对账其相同 Transferred 记录。
        let no_rewrite = store.obstruct_predecessor_handoff_write_once(source.request_id());
        fixture
            .manager
            .complete_activation_predecessor_handoff(&transaction, &source, &publication)
            .expect("same original sealed A completes after real B count changes");
        drop(no_rewrite);
        assert_eq!(
            std::fs::read(&sidecar_path).expect("unchanged A transfer bytes"),
            bytes
        );
        assert_eq!(
            store
                .predecessor_handoff_record("hook-plugin", source.request_id())
                .expect("same actual transfer record")
                .expect("record remains"),
            transferred
        );
        assert_eq!(
            store
                .activation_transaction("hook-plugin", aborted.request_id())
                .expect("same actual Aborted record")
                .expect("original remains"),
            aborted
        );
        assert!(Arc::ptr_eq(
            publication.catalog(),
            &fixture.manager.snapshot()
        ));
        // The actual same registered Hook keeps its physical identities and releases only private ownership.
        // 实际相同已注册 Hook 保留物理身份，仅释放私有归属。
        let confirmed = original.target_test_identity();
        assert_eq!(
            (&confirmed.0, &confirmed.1, &confirmed.2),
            (&identities.0, &identities.1, &identities.2)
        );
        assert!(!confirmed.3 && !confirmed.4);
        assert!(Arc::ptr_eq(&original, &initialized_hook(&fixture)));
        assert_eq!(
            fixture
                .owner()
                .host
                .target_test_domain_identity(identities.0.as_deref().expect("same A pool")),
            domain
        );
        competing
            .try_lock()
            .expect("same original A File released after read-only confirmation");
        competing
            .unlock()
            .expect("release independent A observation");
    });
    // Retire coordination only after original read-only transfer confirmation and all sealed routing identity assertions.
    // 仅在原只读转交确认及全部密封路由身份断言之后退役协调器。
    drop(routing);
}
