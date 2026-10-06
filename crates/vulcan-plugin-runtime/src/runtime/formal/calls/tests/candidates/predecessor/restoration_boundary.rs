//! Restoration staging uses real old callbacks, actual request locks and immutable selected configuration.
//! 恢复暂存使用真实旧回调、实际请求锁及不可变选定配置。

use super::callbacks::callback_fixture;
use super::*;
use crate::runtime::update_drain::restoration::VulcanPluginPredecessorRestoreEvidence;

/// Failed old disable followed by one successful old restore can stage only the actual closed predecessor.
/// 旧停用失败后一次旧恢复成功，仅能暂存实际关闭前驱。
/// No inputs or return value; assertions cover old configuration, permanent target denial and exact closed Arc reuse.
/// 无输入与返回值；断言覆盖旧配置、目标永久拒绝及精确关闭 Arc 复用。
#[test]
fn restoration_boundary_failed_disable_restores_once_and_stages_closed_old() {
    // Retain actual fixture evidence and ownership for this boundary assertion.
    // 保留实际 fixture 证据及所有权，供此边界断言使用。
    let (fixture, store, mut transaction, drain) = callback_fixture(false, true, true);
    // Retain actual candidate evidence and ownership for this boundary assertion.
    // 保留实际 candidate 证据及所有权，供此边界断言使用。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("original candidate");
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .disable_activation_predecessor(&candidate, &drain)
                .await
                .is_err()
        );
        fixture.events.release();
        fixture
            .manager
            .restore_activation_predecessor(&candidate, &drain)
            .await
            .expect("actual old restore");
        assert!(
            fixture
                .manager
                .restore_activation_predecessor(&candidate, &drain)
                .await
                .is_err()
        );
    });
    // Retain actual restoration evidence and ownership for this boundary assertion.
    // 保留实际 restoration 证据及所有权，供此边界断言使用。
    let mut restoration = fixture
        .manager
        .prepare_activation_predecessor_restoration(&candidate, drain)
        .expect("typed successful old restoration");
    assert_eq!(
        restoration.evidence(),
        VulcanPluginPredecessorRestoreEvidence::RestoreSucceeded
    );
    drop(candidate);
    assert!(
        store
            .commit_activation_transaction(&mut transaction)
            .is_err()
    );
    // Retain actual staged evidence and ownership for this boundary assertion.
    // 保留实际 staged 证据及所有权，供此边界断言使用。
    let staged = fixture
        .manager
        .abort_and_stage_activation_predecessor_restoration(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("exclusive abort before closed old staging");
    assert_eq!(
        transaction.record().phase(),
        crate::VulcanPluginActivationTransactionPhase::Aborted
    );
    assert_eq!(
        staged.plugins["hook-plugin"].runtime_state,
        VulcanPluginRuntimeState::Enabling
    );
    assert!(staged.enabled_tools.is_empty());
    assert!(staged.enabled_hooks.is_empty());
    assert!(staged.enabled_mcp_servers.is_empty());
    assert_eq!(
        staged.plugins["hook-plugin"]
            .managed_package()
            .expect("actual old pin")
            .configuration()
            .value(),
        Some(&json!({"label":"old-published"}))
    );
    assert!(Arc::ptr_eq(&staged, &fixture.manager.snapshot()));
    assert!(Arc::ptr_eq(
        &staged,
        &fixture
            .manager
            .abort_and_stage_activation_predecessor_restoration(
                &mut transaction,
                &mut restoration,
                false,
                false,
            )
            .expect("same closed staging")
    ));
    assert_eq!(
        fixture
            .events
            .values
            .lock()
            .expect("actual events")
            .iter()
            .filter(|event| event.topic == "enable")
            .count(),
        1
    );
}

/// No old enable declaration produces typed absence without inventing RestoreSucceeded or running Lua.
/// 无旧启用声明产生类型化缺失，不虚构 RestoreSucceeded 或运行 Lua。
/// No inputs or return value; the actual receipt contains only confirmed resource drain.
/// 无输入与返回值；实际回执仅包含已确认资源排空。
#[test]
fn restoration_boundary_no_declaration_is_explicit_and_does_not_execute() {
    // Retain actual fixture evidence and ownership for this boundary assertion.
    // 保留实际 fixture 证据及所有权，供此边界断言使用。
    let (fixture, store, mut transaction, drain) = callback_fixture(false, false, false);
    // Retain actual candidate evidence and ownership for this boundary assertion.
    // 保留实际 candidate 证据及所有权，供此边界断言使用。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("original candidate");
    // Retain actual restoration evidence and ownership for this boundary assertion.
    // 保留实际 restoration 证据及所有权，供此边界断言使用。
    let mut restoration = fixture
        .manager
        .prepare_activation_predecessor_restoration(&candidate, drain)
        .expect("no old declaration");
    assert_eq!(
        restoration.evidence(),
        VulcanPluginPredecessorRestoreEvidence::NoDeclaration
    );
    drop(candidate);
    fixture
        .manager
        .abort_and_stage_activation_predecessor_restoration(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("no callback staging");
    assert!(
        fixture
            .events
            .values
            .lock()
            .expect("actual events")
            .is_empty()
    );
    assert_eq!(
        store
            .activation_effects("hook-plugin", restoration.request_id())
            .expect("effects")
            .expect("actual drain")
            .observations(),
        vec![(Effect::DrainPredecessorLua, Observation::Succeeded)]
    );
}

/// A retained real readiness lease makes abort fail and preserves the same closed old owner for later retry.
/// 保留真实就绪租约使中止失败，并保留相同关闭旧所有者供稍后重试。
/// No inputs or return value; failure cannot publish Enabling or consume restoration authority.
/// 无输入与返回值；失败不能发布启用中状态或消耗恢复权威。
#[test]
fn restoration_boundary_abort_busy_preserves_closed_catalog_and_owners() {
    // Retain actual fixture evidence and ownership for this boundary assertion.
    // 保留实际 fixture 证据及所有权，供此边界断言使用。
    let (fixture, store, mut transaction, drain) = callback_fixture(false, false, false);
    // Retain actual candidate evidence and ownership for this boundary assertion.
    // 保留实际 candidate 证据及所有权，供此边界断言使用。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("candidate");
    // Clone actual readiness before releasing the borrow so exclusive transaction mutation remains testable.
    // 释放借用前克隆实际就绪状态，使排他事务变更仍可测试。
    let retained = candidate.retain_readiness();
    // Retain actual restoration evidence and ownership for this boundary assertion.
    // 保留实际 restoration 证据及所有权，供此边界断言使用。
    let mut restoration = fixture
        .manager
        .prepare_activation_predecessor_restoration(&candidate, drain)
        .expect("typed no declaration");
    drop(candidate);
    // Retain actual closed evidence and ownership for this boundary assertion.
    // 保留实际 closed 证据及所有权，供此边界断言使用。
    let closed = fixture.manager.snapshot();
    // Retain actual failure evidence and ownership for this boundary assertion.
    // 保留实际 failure 证据及所有权，供此边界断言使用。
    let failure = fixture
        .manager
        .abort_and_stage_activation_predecessor_restoration(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect_err("actual request lease is busy");
    assert!(failure.message.contains("readiness"), "{}", failure.message);
    assert!(restoration.staged_catalog().is_none());
    assert!(Arc::ptr_eq(&closed, &fixture.manager.snapshot()));
    assert_eq!(
        transaction.record().phase(),
        crate::VulcanPluginActivationTransactionPhase::Prepared
    );
    drop(retained);
    fixture
        .manager
        .abort_and_stage_activation_predecessor_restoration(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("same original owner after actual lease retirement");
}

/// Foreign manager and changed explicit membership cannot obtain or consume original restoration authority.
/// 外来管理器及已变明确成员关系不能取得或消耗原恢复权威。
/// No inputs or return value; preparation errors return the actual supplied drain and staging errors preserve it.
/// 无输入与返回值；准备错误返回实际提供排空句柄，暂存错误保留它。
#[test]
fn restoration_boundary_foreign_manager_and_changed_policy_fail_closed() {
    // Retain actual fixture evidence and ownership for this boundary assertion.
    // 保留实际 fixture 证据及所有权，供此边界断言使用。
    let (fixture, store, mut transaction, drain) = callback_fixture(false, false, false);
    // Retain actual candidate evidence and ownership for this boundary assertion.
    // 保留实际 candidate 证据及所有权，供此边界断言使用。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("original candidate");
    // Retain actual foreign evidence and ownership for this boundary assertion.
    // 保留实际 foreign 证据及所有权，供此边界断言使用。
    let foreign = Fixture::new();
    // Retain actual failure evidence and ownership for this boundary assertion.
    // 保留实际 failure 证据及所有权，供此边界断言使用。
    let failure = foreign
        .manager
        .prepare_activation_predecessor_restoration(&candidate, drain)
        .expect_err("foreign incarnation");
    assert_eq!(
        failure.drain.request_id(),
        transaction.record().request_id()
    );
    // Retain actual restoration evidence and ownership for this boundary assertion.
    // 保留实际 restoration 证据及所有权，供此边界断言使用。
    let mut restoration = fixture
        .manager
        .prepare_activation_predecessor_restoration(&candidate, failure.drain)
        .expect("original manager retains actual authority");
    drop(candidate);
    // Retain actual closed evidence and ownership for this boundary assertion.
    // 保留实际 closed 证据及所有权，供此边界断言使用。
    let closed = fixture.manager.snapshot();
    assert!(
        fixture
            .manager
            .abort_and_stage_activation_predecessor_restoration(
                &mut transaction,
                &mut restoration,
                true,
                false,
            )
            .expect_err("changed policy membership")
            .message
            .contains("policy changed")
    );
    assert!(Arc::ptr_eq(&closed, &fixture.manager.snapshot()));
    assert!(restoration.staged_catalog().is_none());
}

/// Real Aborted publication with lost write/readback observation remains recoverable by its same actual owner.
/// 丢失写入及回读观测的真实 Aborted 发布仍可由相同实际所有者恢复。
/// No inputs or return value; later confirmation preserves a newer unrelated catalog namespace and never replays restore.
/// 无输入与返回值；稍后确认保留较新无关目录命名空间，并绝不重放恢复。
#[test]
fn restoration_boundary_uncertain_abort_confirms_later_without_replaying_or_stale_catalog() {
    // Real old disable fails after its side effect, then exactly one old restore succeeds.
    // 真实旧停用在其副作用后失败，随后精确一次旧恢复成功。
    let (fixture, store, mut transaction, drain) = callback_fixture(false, true, true);
    // Borrow only the same unpublished original before exclusive abort can run.
    // 仅在排他中止可执行前借用相同未发布原请求。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("original candidate");
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .disable_activation_predecessor(&candidate, &drain)
                .await
                .is_err()
        );
        fixture.events.release();
        fixture
            .manager
            .restore_activation_predecessor(&candidate, &drain)
            .await
            .expect("actual old restore");
    });
    // Sealed authority retains the actual old pins while dropping temporary readiness before abort.
    // 密封权威保留实际旧固定句柄，同时在中止前释放临时就绪状态。
    let mut restoration = fixture
        .manager
        .prepare_activation_predecessor_restoration(&candidate, drain)
        .expect("actual original restoration");
    drop(candidate);
    // Retain exact pre-write catalog identity and original callback receipts for no-replay assertions.
    // 保留精确写入前目录身份及原回调回执，供禁止重放断言使用。
    let closed = fixture.manager.snapshot();
    // Canonical inspection must remain available even while only the helper's immediate readback is lost.
    // 即使仅辅助方法即时回读丢失，规范检查仍必须可用。
    let fault = store.lose_predecessor_abort_observation_once(restoration.request_id());
    // This failure occurs only after the real atomic writer persisted the exact Aborted record.
    // 此失败仅在真实原子写入器持久保存精确 Aborted 记录后发生。
    let error = fixture
        .manager
        .abort_and_stage_activation_predecessor_restoration(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect_err("write and immediate observation lost");
    assert!(
        error.message.contains("immediate Aborted indexed readback"),
        "{}",
        error.message
    );
    assert_eq!(
        transaction.record().phase(),
        crate::VulcanPluginActivationTransactionPhase::Prepared
    );
    assert!(restoration.staged_catalog().is_none());
    assert!(Arc::ptr_eq(&closed, &fixture.manager.snapshot()));
    assert_eq!(
        store
            .activation_transaction("hook-plugin", restoration.request_id())
            .expect("canonical inspection unaffected")
            .expect("real original receipt")
            .phase(),
        crate::VulcanPluginActivationTransactionPhase::Aborted
    );
    drop(fault);
    // Add a real unrelated package and refresh after the unknown result; later staging must use this current catalog.
    // 未知结果后添加真实无关插件包并刷新；稍后暂存必须使用此当前目录。
    let other_package = fixture._root.path().join("bundled/other-plugin");
    crate::runtime::tests::write_user_plugin_package(&other_package, "other-plugin");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("new unrelated catalog namespace");
    // Confirm the unrelated namespace actually exists before the same original owner resumes its abort observation.
    // 在相同原所有者继续其中止观测之前，确认无关命名空间实际存在。
    let latest = fixture.manager.snapshot();
    assert!(latest.plugins.contains_key("other-plugin"));
    assert_eq!(
        latest.plugins["hook-plugin"].runtime_state,
        VulcanPluginRuntimeState::Draining
    );
    // The same transaction and drain confirm only exact indexed Aborted; no second write or callback is needed.
    // 相同事务及排空句柄仅确认精确已索引 Aborted；无需第二次写入或回调。
    let staged = fixture
        .manager
        .abort_and_stage_activation_predecessor_restoration(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("later same-owner exact Aborted confirmation");
    assert_eq!(
        transaction.record().phase(),
        crate::VulcanPluginActivationTransactionPhase::Aborted
    );
    assert!(staged.plugins.contains_key("other-plugin"));
    assert_eq!(
        staged.plugins["other-plugin"].content_fingerprint,
        latest.plugins["other-plugin"].content_fingerprint
    );
    assert_eq!(
        staged.plugins["other-plugin"].generation,
        latest.plugins["other-plugin"].generation
    );
    assert_eq!(
        staged.plugins["hook-plugin"].runtime_state,
        VulcanPluginRuntimeState::Enabling
    );
    assert!(Arc::ptr_eq(&staged, &fixture.manager.snapshot()));
    assert_eq!(
        fixture
            .events
            .values
            .lock()
            .expect("actual restore entries")
            .iter()
            .filter(|event| event.topic == "enable")
            .count(),
        1
    );
    assert_eq!(
        store
            .activation("hook-plugin")
            .expect("unchanged selected old")
            .as_ref(),
        transaction.record().predecessor()
    );
}
