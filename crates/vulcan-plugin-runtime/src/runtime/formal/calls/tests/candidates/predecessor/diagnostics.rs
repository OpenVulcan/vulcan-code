//! Original-request observations use actual retained drain authority and ordinary call ownership.
//! 原请求观测使用实际保留排空权威及普通调用所有权。

use super::*;

/// A real original update includes ordinary predecessor calls and never substitutes another request or current generation.
/// 真实原更新包含普通前驱调用，绝不替代为其他请求或当前代次。
#[test]
fn formal_predecessor_resource_diagnostics_preserve_original_request_and_selection() {
    // The existing fixture publishes an actual selected predecessor before preparing the real update.
    // 既有夹具在准备真实更新前发布实际选中的前驱。
    let (fixture, store) = finalizer_fixture();
    // The original catalog descriptor fixes the admitted call's genuine routing identity.
    // 原目录描述符固定已接纳调用的真实路由身份。
    let original = fixture.manager.snapshot();
    // Retain the sole already-attached parent rather than create another runtime for management.
    // 保留唯一已经附加的父级，不为管理创建另一运行时。
    let owner = fixture.owner();
    // An admitted but not submitted internal invocation owns real host retirement work.
    // 已接纳但尚未提交的内部调用拥有真实宿主退役工作。
    let call = fixture
        .manager
        .register_owned_formal_call(
            &owner,
            original.plugins["hook-plugin"].clone(),
            Preparation {
                workspace: CallWorkspace::LifecycleEnvironment,
                arguments: json!({}),
                finalizer: None,
                scope: VulcanPluginBridgeLeaseScope::Enabled,
                security_partition: "diagnostic-original".into(),
                deadline: Instant::now() + WAIT,
                admission: CallAdmission::Lifecycle {
                    export: "dispatch".into(),
                },
            },
        )
        .expect("admit original ordinary call");
    // Contention on the real original state cannot turn into an empty or falsely retired observation.
    // 真实原状态争用不能变成空观测或虚假退役观测。
    let state_guard = call.lock().expect("hold original state");
    assert_eq!(
        fixture
            .manager
            .formal_plugin_resource_diagnostics("hook-plugin", None)
            .expect_err("state must be explicitly busy")
            .message,
        "Plugin resource diagnostics are busy."
    );
    drop(state_guard);
    // Real preparation takes the original request lock and freezes the actual immutable candidate.
    // 真实准备取得原请求锁，并冻结实际不可变候选。
    let transaction = prepare(&fixture, &store);
    // Candidate authority comes from the actual original transaction.
    // 候选权威来自实际原事务。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("actual candidate");
    // Beginning the real predecessor drain closes ordinary admission and retains its exact request.
    // 开始真实前驱排空关闭普通准入，并保留精确请求。
    let drain = fixture
        .manager
        .begin_activation_predecessor_drain(&candidate, &original)
        .expect("actual drain");
    // Querying uses the original live owner rather than today's same-name catalog descriptor.
    // 查询使用原实时所有者，而非当前同名目录描述符。
    let diagnostic = fixture
        .manager
        .activation_predecessor_drain_diagnostics("hook-plugin", drain.request_id())
        .expect("original management read")
        .expect("live original");
    assert_eq!(diagnostic.request_id, drain.request_id());
    assert_eq!(diagnostic.service_epoch, drain.identity().service_epoch);
    assert_eq!(
        diagnostic.predecessor_generation,
        drain.identity().predecessor_generation.to_string()
    );
    assert_eq!(
        diagnostic.draining_generation,
        drain.identity().draining_generation.to_string()
    );
    // Live blockers include the actual ordinary call, not only activation-specific callback receipts.
    // 实时阻塞者包含实际普通调用，不仅包含启用专属回调回执。
    let resources = diagnostic.resources.expect("existing parent");
    assert_eq!(resources.pinned_session_count, 0);
    // Stable host call identity identifies the original record regardless of registry ordering.
    // 无论注册表顺序如何，稳定宿主调用身份标识原记录。
    let row = resources
        .resources
        .iter()
        .find(|row| row.resource_id == call.id)
        .expect("expected ordinary predecessor call may have changed");
    assert_eq!(row.resource_kind, VulcanPluginResourceKind::FormalCall);
    assert_eq!(
        row.plugin_generation,
        original.plugins["hook-plugin"].generation.to_string()
    );
    assert!(
        row.owner_session_id.is_none(),
        "internal calls do not invent a conversation owner"
    );
    assert!(row.is_drain_blocking);
    assert!(
        fixture
            .manager
            .activation_predecessor_drain_diagnostics("hook-plugin", Uuid::new_v4())
            .expect("unknown original")
            .is_none()
    );
    assert!(
        fixture
            .manager
            .activation_predecessor_drain_diagnostics("another-plugin", drain.request_id())
            .is_err()
    );
    drop(candidate);
    drop(transaction);
    // The real drain retires the unstarted call through existing cleanup, never through this diagnostic reader.
    // 真实排空通过既有清理退役尚未启动调用，绝不通过此诊断读取器。
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .drain_activation_predecessor(&drain)
                .await
                .expect("actual retirement")
                .failures
                .is_empty()
        );
    });
    assert!(
        fixture
            .manager
            .activation_predecessor_drain_diagnostics("hook-plugin", drain.request_id())
            .expect("completed original absence")
            .is_none()
    );
}
