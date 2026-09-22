//! Persistent rejection cleanup retains the exact admitted child turn.
//! 持久拒绝清理保留精确已准入子回合。

use super::*;
use vulcan_session_core::v1::{ClientActionId, ClientInputSubmission, TurnOutcome};

/// Startup rejection cleanup refuses an active Provider rather than closing live work.
/// 启动拒绝清理拒绝活动 Provider，不关闭正在执行的工作。
/// Parameters: none. Returns: none; verifies the API and revision remain unchanged.
/// 参数：无。返回值：无；验证 API 和修订保持不变。
#[tokio::test]
async fn rejected_startup_cannot_close_an_active_api() {
    use vulcan_session_core::v1::{
        ApiPurposeKind,
        context::LockedBaselineProjection,
        document::PrepareAndStartApiInput,
        event::{ApiRequestSnapshot, ProviderProjectionProof},
        ids::Seq,
        projection::provider_visible_fingerprint,
    };
    // Preserve the frozen owner while starting a real request through the production boundary.
    // 保留冻结所有者，同时通过生产边界启动真实请求。
    let (_directory, handle, owner) = fixture();
    // The projection proof is calculated from the actual current resident history.
    // 投影证明根据实际当前驻留历史计算。
    let input = handle
        .read(|document| {
            // Freeze the exact last source fact for request admission.
            // 为请求准入冻结精确最后来源事实。
            let through_seq = document.events().last().unwrap().seq;
            PrepareAndStartApiInput {
                purpose: ApiPurposeKind::MainGeneration,
                request: ApiRequestSnapshot::new("openai", "model"),
                request_budget_evidence:
                    crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
                projection: ProviderProjectionProof {
                    segment_generation: document.segment_generation(),
                    document_revision: document.memory_revision(),
                    retained_from_seq: Seq::try_new(1).unwrap(),
                    through_seq,
                    fingerprint_sha256: provider_visible_fingerprint(
                        document.events(),
                        through_seq,
                    )
                    .unwrap(),
                },
                locked_baseline: LockedBaselineProjection::empty(),
                continuation_action_id: None,
                now_ms: 3,
            }
        })
        .unwrap();
    // Start through the canonical durable API wrapper, without dispatching a network request.
    // 通过规范可靠 API 包装器启动，不分发网络请求。
    let started = handle.prepare_and_start_api_durable(input).unwrap();
    assert!(close_rejected_child(&handle, owner, "invalid late rejection").is_err());
    handle
        .read(|document| {
            assert_eq!(document.memory_revision(), started.revision);
            assert!(document.active_api().is_some());
            assert!(!document.active_turn().unwrap().closed);
        })
        .unwrap();
}

/// Builds a real isolated child storage owner and its captured startup identity.
/// 构造真实隔离子存储所有者及其捕获的启动身份。
/// Returns the storage lifetime, canonical handle and original ownership snapshot.
/// 返回存储生命周期、规范句柄及原始归属快照。
fn fixture() -> (tempfile::TempDir, Arc<SessionHandle>, DetachedStartupOwner) {
    // Keep the real storage tree alive until assertions and disk reload finish.
    // 保留真实存储树，直到断言及磁盘重载结束。
    let directory = tempfile::tempdir().unwrap();
    // The physical child is independent of its execution/root-input identifier.
    // 物理子会话独立于其执行及根输入标识。
    let task_id = SessionId::new_v4();
    // Use the established storage fixture, without installing any model runtime.
    // 使用既有存储夹具，不安装任何模型运行时。
    let locator = vulcan_workspace_store::workspace_store::create_session_v1_storage_fixture(
        directory.path(),
        &task_id.to_string(),
        1,
    )
    .unwrap();
    // Load the exact persisted document before taking canonical ownership.
    // 在取得规范归属之前加载精确持久文档。
    let document = vulcan_workspace_store::workspace_store::load_session_v1_full(&locator)
        .unwrap()
        .document;
    // The production persistence owner performs every following mutation.
    // 生产持久化所有者执行之后的所有修改。
    let handle = SessionHandle::new(task_id, locator, document);
    handle
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("rejected-execution").unwrap(),
                "child work",
                vec![],
            ),
            2,
        )
        .unwrap();
    // Capture only after validating the real root action against the runtime execution.
    // 仅在根据运行时执行校验真实根动作之后捕获归属。
    let owner = capture_owner(&handle, "rejected-execution").unwrap();
    (directory, handle, owner)
}

/// A rejected pre-Provider admission closes its child and persists the actual reason.
/// Provider 之前被拒的准入关闭其子回合，并持久保存实际原因。
/// Parameters: none. Returns: none; verifies durable reload and source identity rejection.
/// 参数：无。返回值：无；验证可靠重载及来源身份拒绝。
#[tokio::test]
async fn rejected_startup_closes_exact_child_durably() {
    // Hold the original storage owner throughout cleanup and reload.
    // 在清理及重载期间保留原始存储所有者。
    let (_directory, handle, owner) = fixture();
    // Cold hydration retains the terminal fact, not an in-memory closed ActiveTurn placeholder.
    // 冷水合保留终态事实，不保留内存中已关闭的 ActiveTurn 占位。
    let original_turn = owner.turn_id;
    assert!(capture_owner(&handle, "unrelated-execution").is_err());
    close_rejected_child(&handle, owner, "subscription unavailable").unwrap();
    // Reload from disk rather than treating an in-memory terminal state as durable proof.
    // 从磁盘重载，不把内存终态当成可靠证据。
    let restored = vulcan_workspace_store::workspace_store::load_session_v1_full(
        &handle.storage_locator().unwrap(),
    )
    .unwrap()
    .document;
    assert!(restored.active_turn().is_none());
    assert!(restored.turn_has_terminal_fact(original_turn));
    assert!(restored.events().iter().any(|event| event.turn_id == Some(original_turn)
        && matches!(&event.event, EventBody::DisplayNote(note) if note.reason_code == "detached_start_admission_failed")));
}

/// Late admission cleanup cannot close a newer turn in the reused physical child.
/// 迟到准入清理不能关闭复用物理子会话中的较新回合。
/// Parameters: none. Returns: none; compares revision and open-turn identity before and after.
/// 参数：无。返回值：无；比较清理前后的修订及开放回合身份。
#[tokio::test]
async fn rejected_startup_preserves_replacement_turn() {
    // The captured owner remains tied to the rejected first root.
    // 捕获的所有者持续绑定被拒的第一个根输入。
    let (_directory, handle, owner) = fixture();
    handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::DisplayBoundary,
            |document| document.close_turn(TurnOutcome::Stopped, 1, 3),
        )
        .unwrap();
    // Reuse the same physical child with a distinct explicit root identity.
    // 使用不同的显式根身份复用同一物理子会话。
    let replacement = handle
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("replacement-execution").unwrap(),
                "new work",
                vec![],
            ),
            4,
        )
        .unwrap();
    close_rejected_child(&handle, owner, "late subscription rejection").unwrap();
    handle
        .read(|document| {
            assert_eq!(document.memory_revision(), replacement.revision);
            assert_eq!(document.active_turn().unwrap().turn_id, replacement.value);
            assert!(!document.active_turn().unwrap().closed);
        })
        .unwrap();
}
