//! Tests for bounded lease admission and atomic caller-owned reconciliation.
//! 有界租约准入与调用方持有原子协调的测试。

use super::super::test_support::two_account_runtime;
use super::*;

/// Verifies session-route lease admission is bounded without evicting an established route.
/// 验证会话路由租约准入有界，且不会淘汰已建立路由。
#[test]
fn session_selection_lease_admission_is_bounded_and_idempotent() {
    assert!(
        validate_session_selection_lease_admission(
            "session-existing",
            SESSION_SELECTION_LEASE_MAX_ENTRIES,
            true,
        )
        .is_ok(),
        "an established immutable lease must remain readable at capacity"
    );
    let capacity_error = validate_session_selection_lease_admission(
        "session-new",
        SESSION_SELECTION_LEASE_MAX_ENTRIES,
        false,
    )
    .expect_err("a new lease above capacity must fail closed");
    assert!(capacity_error.contains("4096 entries"), "{capacity_error}");
    let identity_error = validate_session_selection_lease_admission("", 0, false)
        .expect_err("an empty lease identity must fail closed");
    assert!(identity_error.contains("1..=1024"), "{identity_error}");
}

/// Verifies the absence guard excludes lease writers until its caller-owned publication ends.
/// 验证缺失 Guard 会排除租约写入方，直到调用方持有的发布结束。
#[test]
fn session_selection_lease_absence_guard_excludes_writers() {
    // runtime owns the exact lease map whose writer admission is under test.
    // runtime 持有本测试所验证写入准入的精确租约映射。
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    // sessionIds identify the complete reconciliation scope protected by the guard.
    // sessionIds 标识由 Guard 保护的完整协调作用域。
    let session_ids = vec![String::from("session-under-reconciliation")];
    // absenceGuard retains the write lock after proving the scoped session has no lease.
    // absenceGuard 在证明作用域会话没有租约后继续持有写锁。
    let absence_guard = runtime
        .acquire_session_selection_lease_absence_guard(&session_ids)
        .expect("an unleased session should admit reconciliation");
    // blockedWriter proves no concurrent lease installation can cross the publication seam.
    // blockedWriter 证明并发租约安装无法穿越发布接缝。
    let blocked_writer = runtime.session_selection_leases.try_write();
    assert!(
        blocked_writer.is_err(),
        "the absence guard must exclude every lease writer"
    );
    drop(blocked_writer);
    drop(absence_guard);
    // admittedWriter proves writer admission resumes immediately after publication releases the guard.
    // admittedWriter 证明发布释放 Guard 后写入准入会立即恢复。
    let admitted_writer = runtime.session_selection_leases.try_write();
    assert!(
        admitted_writer.is_ok(),
        "lease writers must resume after the absence guard is released"
    );
}

/// Verifies an abandoned explicit revocation restores the exact leased selection.
/// 验证被放弃的显式撤销会恢复精确的已租用选择。
#[test]
fn session_selection_lease_reconciliation_guard_restores_on_drop() {
    // runtime and configuration provide one fully resolved immutable test route.
    // runtime 与 configuration 提供一条完整解析的不可变测试路由。
    let (runtime, configuration) = two_account_runtime();
    // selection is the exact before-image installed for the revoked session.
    // selection 是为被撤销会话安装的精确前镜像。
    let selection = runtime
        .resolve_connection_model_selection(
            &configuration,
            "one",
            "openrouter",
            "openai/gpt-5.6-sol",
        )
        .expect("test selection should resolve");
    runtime
        .lease_session_selection(String::from("session-revoked"), selection)
        .expect("test session should lease its route");
    {
        // reconciliationGuard removes the lease while excluding every concurrent writer.
        // reconciliationGuard 在排除全部并发写入方期间移除租约。
        let reconciliation_guard = runtime
            .acquire_session_selection_lease_reconciliation_guard(
                &[String::from("session-revoked")],
                &[],
            )
            .expect("explicit revocation should acquire its guard");
        assert!(
            reconciliation_guard.guard.get("session-revoked").is_none(),
            "the candidate lease map must remove the revoked owner"
        );
    }
    assert!(
        runtime
            .session_selection_lease("session-revoked")
            .expect("restored lease should remain readable")
            .is_some(),
        "dropping an uncommitted guard must restore the lease"
    );
}

/// Verifies a committed explicit revocation remains absent after writer exclusion ends.
/// 验证已提交的显式撤销在写入排他结束后继续保持缺失。
#[test]
fn session_selection_lease_reconciliation_guard_commits_removal() {
    // runtime and configuration provide one fully resolved immutable test route.
    // runtime 与 configuration 提供一条完整解析的不可变测试路由。
    let (runtime, configuration) = two_account_runtime();
    // selection is the exact route whose ownership is explicitly revoked.
    // selection 是所有权被显式撤销的精确路由。
    let selection = runtime
        .resolve_connection_model_selection(
            &configuration,
            "one",
            "openrouter",
            "openai/gpt-5.6-sol",
        )
        .expect("test selection should resolve");
    runtime
        .lease_session_selection(String::from("session-committed"), selection)
        .expect("test session should lease its route");
    // reconciliationGuard owns the removal until the paired publication commits.
    // reconciliationGuard 持有删除操作，直到配套发布提交。
    let reconciliation_guard = runtime
        .acquire_session_selection_lease_reconciliation_guard(
            &[String::from("session-committed")],
            &[],
        )
        .expect("explicit revocation should acquire its guard");
    reconciliation_guard.commit();
    assert!(
        runtime
            .session_selection_lease("session-committed")
            .expect("lease map should remain readable")
            .is_none(),
        "committing the guard must keep the revoked route absent"
    );
}

/// Verifies absence conflicts reject the whole reconciliation before any revocation occurs.
/// 验证缺失冲突会在任何撤销发生前拒绝整个协调事务。
#[test]
fn session_selection_lease_reconciliation_guard_is_atomic_on_conflict() {
    // runtime and configuration provide one fully resolved immutable test route.
    // runtime 与 configuration 提供一条完整解析的不可变测试路由。
    let (runtime, configuration) = two_account_runtime();
    // selection is cloned because both independent owners lease the same frozen route.
    // selection 会被克隆，因为两个独立所有者租用同一冻结路由。
    let selection = runtime
        .resolve_connection_model_selection(
            &configuration,
            "one",
            "openrouter",
            "openai/gpt-5.6-sol",
        )
        .expect("test selection should resolve");
    runtime
        .lease_session_selection(String::from("session-to-revoke"), selection.clone())
        .expect("revocation owner should lease its route");
    runtime
        .lease_session_selection(String::from("session-must-be-absent"), selection)
        .expect("absence-conflict owner should lease its route");
    let error = runtime
        .acquire_session_selection_lease_reconciliation_guard(
            &[String::from("session-to-revoke")],
            &[String::from("session-must-be-absent")],
        )
        .err()
        .expect("an occupied absence scope must reject reconciliation");
    assert!(error.contains("session-must-be-absent"), "{error}");
    assert!(
        runtime
            .session_selection_lease("session-to-revoke")
            .expect("revocation owner should remain readable")
            .is_some(),
        "absence validation must occur before any removal"
    );
}
