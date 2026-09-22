//! Real durable main-Hook recovery and stale-frontier rejection without a provider.
//! 无供应商调用的真实可靠主 Hook 恢复及过期前沿拒绝。

use super::*;
use vulcan_session_core::v1::document::ClientInputSubmission;
use vulcan_session_core::v1::ids::ClientActionId;
use vulcan_workspace_store::workspace_store::{
    create_session_v1_storage_fixture, load_session_v1_full, load_session_v1_tail,
};

/// Cold recovery preserves original identity and canonical history, and rejects stale admissions atomically.
/// 冷恢复保留原身份及规范历史，并原子拒绝过期准入。
/// Uses actual files and two independent Hook runtimes; returns no value.
/// 使用真实文件及两个独立 Hook 运行时；无返回值。
#[test]
fn main_business_recovery_restores_original_hook_and_rejects_stale_frontier() {
    let temporary = tempfile::tempdir().unwrap();
    let session_id = SessionId::from_u128(0x62d2);
    let locator =
        create_session_v1_storage_fixture(temporary.path(), &session_id.to_string(), 1).unwrap();
    let handle = SessionHandle::new(
        session_id,
        locator.clone(),
        load_session_v1_tail(&locator).unwrap().document,
    );
    handle
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("original-root").unwrap(),
                "Original task",
                vec![],
            ),
            2,
        )
        .unwrap();
    let runtime = crate::tests::test_support::test_agent_runtime_bridge();
    let prepared = runtime
        .conversation_hook_runtime
        .lock()
        .unwrap()
        .prepare_user_prompt_hook_requests(
            session_id,
            "original-root",
            None,
            "D:/main-recovery-test".into(),
            PendingSessionPromptSource::UserFollowup,
            vulcan_hooks::SystemPromptHookPhase::FirstSession,
            "Sanitized original task",
            &Default::default(),
            false,
        );
    crate::session_business_hook::capture_business_hook_before_dispatch(
        &handle,
        &runtime,
        session_id,
        &prepared.turn_id,
    )
    .unwrap();
    let cold = SessionHandle::new(
        session_id,
        locator.clone(),
        load_session_v1_full(&locator).unwrap().document,
    );
    let authority = main_business_recovery_authority(&cold).unwrap().unwrap();
    let before = load_session_v1_full(&locator).unwrap().document;
    let cold_runtime = crate::tests::test_support::test_agent_runtime_bridge();
    restore_main_business_recovery_hook(&cold, &cold_runtime, &authority).unwrap();
    restore_main_business_recovery_hook(&cold, &cold_runtime, &authority).unwrap();
    assert_eq!(
        cold_runtime
            .conversation_hook_runtime
            .lock()
            .unwrap()
            .active_turn_id(session_id),
        Some(prepared.turn_id.as_str())
    );
    // Compare public durable components; SessionDocument's whole-value equality is Core-test-only.
    // 比较公开可靠组件；SessionDocument 的整体相等实现仅限 Core 自身测试。
    let after = load_session_v1_full(&locator).unwrap().document;
    assert_eq!(after.metadata, before.metadata);
    assert_eq!(after.events(), before.events());
    assert_eq!(after.runtime_lock, before.runtime_lock);
    assert_eq!(after.compactions, before.compactions);
    // Later canonical input invalidates the earlier frontier before any new runtime owner is installed.
    // 后续规范输入在安装任何新运行时所有者之前使先前前沿失效。
    cold.enqueue_input(
        ClientInputSubmission::new(
            ClientActionId::try_new("later-input").unwrap(),
            "Later direction",
            vec![],
        ),
        3,
    )
    .unwrap();
    let untouched_runtime = crate::tests::test_support::test_agent_runtime_bridge();
    assert!(restore_main_business_recovery_hook(&cold, &untouched_runtime, &authority).is_err());
    assert!(
        !untouched_runtime
            .conversation_hook_runtime
            .lock()
            .unwrap()
            .has_active_turn(session_id)
    );
    let fresh = main_business_recovery_authority(&cold).unwrap().unwrap();
    assert_eq!(fresh.hook.observed_inputs.len(), 1);
    restore_main_business_recovery_hook(&cold, &untouched_runtime, &fresh).unwrap();
    assert_eq!(
        untouched_runtime
            .conversation_hook_runtime
            .lock()
            .unwrap()
            .active_turn_id(session_id),
        Some(prepared.turn_id.as_str())
    );
}
