//! Locked-session context budget invalidation regressions.
//! 已锁定会话上下文预算失效回归。

use super::*;
use std::collections::BTreeMap;
use vulcan_context::ContextCompactionWindowNodeId;
use vulcan_workbench_model::SessionRuntimeStateEntry;

/// Verifies changed global node values invalidate only locked active and archived budgets.
/// 验证全局节点数值变化只使已锁定活动与归档会话的预算失效。
#[test]
fn changed_node_values_invalidate_locked_session_budgets_only() {
    // activeLockedId identifies a visible session whose node resolves through global values.
    // activeLockedId 标识节点通过全局数值解析的可见会话。
    let active_locked_id = crate::SessionId::from_u128(0x6201);
    // activeUnlockedId identifies a visible default-window session that must remain untouched.
    // activeUnlockedId 标识必须保持不变的可见默认窗口会话。
    let active_unlocked_id = crate::SessionId::from_u128(0x6202);
    // archivedLockedId proves archived session state follows the same invalidation contract.
    // archivedLockedId 证明归档会话状态遵循相同失效契约。
    let archived_locked_id = crate::SessionId::from_u128(0x6203);
    // lockedState carries both budget and assembly fingerprints to prove only budget identity changes.
    // lockedState 同时携带预算与组装指纹，用于证明只有预算身份发生变化。
    let locked_state = SessionRuntimeStateEntry {
        context_window_selection: Some(vulcan_context::ContextWindowSelection::Node(
            ContextCompactionWindowNodeId::CompressionWindow2,
        )),
        live_context_budget_fingerprint: Some(String::from("live-budget")),
        live_context_assembly_fingerprint: Some(String::from("live-assembly")),
        anchor_context_budget_fingerprint: Some(String::from("anchor-budget")),
        anchor_context_assembly_fingerprint: Some(String::from("anchor-assembly")),
        ..SessionRuntimeStateEntry::default()
    };
    // unlockedState carries stale-looking values that are independent of global node configuration.
    // unlockedState 携带看似过期但与全局节点配置无关的值。
    let unlocked_state = SessionRuntimeStateEntry {
        live_context_budget_fingerprint: Some(String::from("unlocked-live-budget")),
        anchor_context_budget_fingerprint: Some(String::from("unlocked-anchor-budget")),
        ..SessionRuntimeStateEntry::default()
    };
    // workspace contains both active and archived runtime-state stores used by production publication.
    // workspace 包含生产发布使用的活动与归档运行态存储。
    let mut workspace = vulcan_workspace_store::workspace_store::WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_runtime_state: BTreeMap::from([
            (active_locked_id, locked_state.clone()),
            (active_unlocked_id, unlocked_state),
        ]),
        archived_session_runtime_state: BTreeMap::from([(archived_locked_id, locked_state)]),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: active_locked_id,
        hydrated_session_ids: BTreeSet::from([
            active_locked_id,
            active_unlocked_id,
            archived_locked_id,
        ]),
    };

    // invalidatedSessionIds must include every locked state and no default-window state.
    // invalidatedSessionIds 必须包含全部锁定状态，且不包含默认窗口状态。
    let invalidated_session_ids =
        invalidate_context_compaction_window_budget_fingerprints(&mut workspace);

    assert_eq!(
        invalidated_session_ids,
        BTreeSet::from([active_locked_id, archived_locked_id])
    );
    for session_id in [active_locked_id, archived_locked_id] {
        let runtime_state = workspace
            .session_runtime_state
            .get(&session_id)
            .or_else(|| workspace.archived_session_runtime_state.get(&session_id))
            .expect("locked runtime state should remain present");
        assert_eq!(runtime_state.live_context_budget_fingerprint, None);
        assert_eq!(runtime_state.anchor_context_budget_fingerprint, None);
        assert_eq!(
            runtime_state.live_context_assembly_fingerprint.as_deref(),
            Some("live-assembly")
        );
        assert_eq!(
            runtime_state.anchor_context_assembly_fingerprint.as_deref(),
            Some("anchor-assembly")
        );
    }
    let unlocked_runtime_state = workspace
        .session_runtime_state
        .get(&active_unlocked_id)
        .expect("unlocked runtime state should remain present");
    assert_eq!(
        unlocked_runtime_state
            .live_context_budget_fingerprint
            .as_deref(),
        Some("unlocked-live-budget")
    );
    assert_eq!(
        unlocked_runtime_state
            .anchor_context_budget_fingerprint
            .as_deref(),
        Some("unlocked-anchor-budget")
    );
    let reconciliation = PublicAiWorkspaceReconciliation::context_budget_invalidations(
        invalidated_session_ids.clone(),
    );
    assert!(reconciliation.changed_session_ids.is_empty());
    assert!(reconciliation.revoked_session_ids.is_empty());
    assert_eq!(
        reconciliation.invalidated_context_budget_session_ids,
        invalidated_session_ids
    );
}
