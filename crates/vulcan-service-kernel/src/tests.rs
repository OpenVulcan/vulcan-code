use super::*;
use crate::git::{
    WebuiWorkspaceDiffMode, run_webui_git_command, webui_diff_hunk_scoped_unified_diff,
    webui_unified_diff_file_summaries, webui_unified_diff_line_count, webui_unified_diff_stats,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use vulcan_agent_runtime::conversation_protocol::{
    SessionConversationMessage, SessionConversationPayload,
    conversation_state_to_compaction_source_messages,
};
use vulcan_workbench_model::SessionTurnUsageSegmentKind;

pub(crate) mod test_support;

use test_support::*;

mod active_session_state_tests;
mod capability_metadata_tests;
mod mcp_catalog_projection_tests;
mod question_execution_finish_tests;
mod terminal_settlement_tests;

/// Verifies that status reads retain the Core generation used by tray discovery checks.
/// 验证状态读取保留托盘发现校验使用的同一个 Core 代次。
#[test]
fn service_status_started_at_is_stable_across_reads() {
    let state = create_service_bridge_state(test_service_shell_host());
    let first = service_status_snapshot_from_state(&state);
    std::thread::sleep(std::time::Duration::from_millis(3));
    let second = service_status_snapshot_from_state(&state);

    assert_eq!(first.pid, second.pid);
    assert_eq!(first.started_at, second.started_at);
}

/// Verifies application shutdown drains tasks before releasing MCP.
/// 验证应用关闭会先排空任务，再释放 MCP。
#[test]
fn runtime_shutdown_order_is_tasks_then_mcp() {
    let events = Arc::new(Mutex::new(Vec::<&'static str>::new()));
    let task_events = Arc::clone(&events);
    let mcp_events = Arc::clone(&events);
    let stopped_id = Uuid::new_v4();

    let stopped = ordered_runtime_shutdown(
        move || {
            task_events.lock().expect("task events").push("tasks");
            Ok(vec![stopped_id])
        },
        move || {
            mcp_events.lock().expect("mcp events").push("mcp");
            Ok(())
        },
    )
    .expect("ordered shutdown");

    assert_eq!(stopped, vec![stopped_id]);
    assert_eq!(*events.lock().expect("final events"), vec!["tasks", "mcp"]);
}

/// A failed drain after process-owned teardown must not reopen a registry with dead MCP handles.
/// 进程运行时拆卸后的排空失败不得重新开放持有失效 MCP 句柄的注册表。
#[test]
fn irreversible_shutdown_failure_keeps_admission_closed() {
    let state = create_service_bridge_state(test_service_shell_host());
    let registry = {
        let mut guard = state.lock().expect("service state");
        guard.service_lifecycle_state.begin_shutdown_drain();
        guard.service_lifecycle_state.block_shutdown_recovery();
        Arc::clone(&guard.session_runtime_registry)
    };
    registry.begin_shutdown().expect("begin session drain");

    let error = service_resume_after_failed_app_shutdown(&state)
        .expect_err("irreversible teardown must not report recovery");
    assert!(error.contains("cannot safely recover"));
    assert!(registry.shutdown_admission_is_closed());
    let status = service_status_snapshot_from_state(&state);
    assert_eq!(status.lifecycle, ServiceLifecycle::RecoveryFailed);
    assert_eq!(
        status.last_failure.expect("retained failure").code,
        "shutdown_recovery_failed"
    );
}

/// Rejects every test persistence publication without touching user session storage.
/// 拒绝测试持久化发布，不触碰用户会话存储。
struct FailingShutdownPersistenceBackend;

impl crate::session_runtime_registry_v1::SessionPersistenceBackend
    for FailingShutdownPersistenceBackend
{
    /// Returns a deterministic storage failure for the exact shutdown candidate.
    /// 为精确的关闭候选返回确定的存储故障。
    fn flush(
        &self,
        _candidate: &vulcan_session_core::v1::document::FlushCandidate,
    ) -> Result<
        vulcan_workspace_store::workspace_store::SessionV1FlushResult,
        vulcan_workspace_store::workspace_store::SessionV1StoreError,
    > {
        use vulcan_workspace_store::workspace_store::{SessionV1StorageError, SessionV1StoreError};
        Err(SessionV1StoreError::Storage(SessionV1StorageError::io(
            std::path::PathBuf::from("injected-shutdown-session.json"),
            "injected shutdown persistence failure",
        )))
    }
}

/// A real dirty session flush failure must keep the Core alive but refuse false admission recovery.
/// 真实脏会话刷新失败时 Core 应保持存活，却不得虚假恢复准入。
#[test]
fn failed_shutdown_persistence_reports_unrecoverable_admission() {
    use crate::session_runtime_registry_v1::SessionHandle;
    use vulcan_session_core::v1::{
        document::{ClientInputSubmission, SessionDocument},
        ids::ClientActionId,
    };

    let state = create_service_bridge_state(test_service_shell_host());
    let session_id = SessionId::new_v4();
    let mut document = SessionDocument::new(session_id.to_string(), "project-1", "title", 1);
    document
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("shutdown-flush-fault").expect("valid action id"),
                "must persist",
                Vec::new(),
            ),
            2,
        )
        .expect("dirty root input");
    assert!(document.memory_revision() > document.durable_revision());
    let registry = Arc::clone(
        &state
            .lock()
            .expect("service state")
            .session_runtime_registry,
    );
    registry
        .register(SessionHandle::with_backend(
            session_id,
            document,
            Arc::new(FailingShutdownPersistenceBackend),
        ))
        .expect("register dirty session");

    let error = service_prepare_for_app_shutdown(&state)
        .expect_err("failed durable flush must reject shutdown");
    assert!(
        error.contains("injected shutdown persistence failure"),
        "{error}"
    );
    let recovery = service_resume_after_failed_app_shutdown(&state)
        .expect_err("stopped process-owned runtime cannot safely recover");
    assert!(recovery.contains("cannot safely recover"), "{recovery}");
    assert!(registry.shutdown_admission_is_closed());
    assert_eq!(
        service_status_snapshot_from_state(&state).lifecycle,
        ServiceLifecycle::RecoveryFailed
    );
}

/// Fails the first complete session flush and captures the exact candidate on retry.
/// 首次完整会话刷盘失败，重试时捕获精确候选。
struct FailOnceShutdownPersistenceBackend {
    /// Whether the first physical publication remains unconsumed.
    /// 首次物理发布故障是否仍未消耗。
    fail_first: AtomicBool,
    /// Candidate accepted by the successful retry.
    /// 成功重试时接纳的候选。
    published: Mutex<Option<vulcan_session_core::v1::document::FlushCandidate>>,
}

impl crate::session_runtime_registry_v1::SessionPersistenceBackend
    for FailOnceShutdownPersistenceBackend
{
    /// Rejects one flush, then acknowledges the complete latest candidate.
    /// 拒绝一次刷新，然后确认最新的完整候选。
    fn flush(
        &self,
        candidate: &vulcan_session_core::v1::document::FlushCandidate,
    ) -> Result<
        vulcan_workspace_store::workspace_store::SessionV1FlushResult,
        vulcan_workspace_store::workspace_store::SessionV1StoreError,
    > {
        use vulcan_workspace_store::workspace_store::{
            SessionV1FlushResult, SessionV1StorageError, SessionV1StoreError,
        };
        if self.fail_first.swap(false, Ordering::AcqRel) {
            return Err(SessionV1StoreError::Storage(SessionV1StorageError::io(
                std::path::PathBuf::from("injected-retry-session.json"),
                "injected first shutdown persistence failure",
            )));
        }
        *self.published.lock().expect("published session candidate") = Some(candidate.clone());
        Ok(SessionV1FlushResult {
            written_file_count: candidate.dirty.len(),
            published_metadata: Some(candidate.metadata.clone()),
        })
    }
}

/// A second Quit attempt persists the same dirty session after a one-time storage failure.
/// 一次性存储故障后，第二次退出尝试必须持久化同一脏会话。
#[test]
fn failed_shutdown_persistence_can_be_retried_without_discarding_dirty_session() {
    use crate::session_runtime_registry_v1::SessionHandle;
    use vulcan_session_core::v1::{
        document::{ClientInputSubmission, SessionDocument},
        ids::ClientActionId,
    };

    let state = create_service_bridge_state(test_service_shell_host());
    let session_id = SessionId::new_v4();
    let mut document = SessionDocument::new(session_id.to_string(), "project-1", "title", 1);
    document
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("shutdown-flush-retry").expect("valid action id"),
                "persist after retry",
                Vec::new(),
            ),
            2,
        )
        .expect("dirty root input");
    let backend = Arc::new(FailOnceShutdownPersistenceBackend {
        fail_first: AtomicBool::new(true),
        published: Mutex::new(None),
    });
    let registry = Arc::clone(
        &state
            .lock()
            .expect("service state")
            .session_runtime_registry,
    );
    registry
        .register(SessionHandle::with_backend(
            session_id,
            document,
            backend.clone(),
        ))
        .expect("register dirty session");
    assert!(service_prepare_for_app_shutdown(&state).is_err());
    assert!(service_resume_after_failed_app_shutdown(&state).is_err());
    assert!(registry.shutdown_admission_is_closed());
    assert!(
        backend
            .published
            .lock()
            .expect("first flush receipt")
            .is_none()
    );

    service_prepare_for_app_shutdown(&state).expect("second shutdown flush succeeds");
    let published = backend
        .published
        .lock()
        .expect("retry flush receipt")
        .clone()
        .expect("complete session candidate was persisted");
    assert_eq!(published.metadata.session_id, session_id.to_string());
    assert!(
        published
            .tail_events
            .iter()
            .any(|event| format!("{event:?}").contains("persist after retry"))
    );
}
mod conversation_selection_tests;
mod conversation_submission_activation_tests;
mod conversation_submission_preflight_tests;
mod conversation_submission_queue_tests;
mod detached_subagent_tests;
mod goal_recovery_tests;
mod goal_todolist_tests;
mod mcp_settings_tests;
mod pending_question_projection_tests;
mod prompt_runtime_state_tests;
pub(crate) mod provider_configuration_persistence_scope;
pub(crate) mod public_ai_persistence_scope;
mod rpc_diff_tests;
mod session_kernel_tests;
mod settings_projection_tests;
mod stop_task_mirror_tests;
mod subagent_display_metadata_tests;
mod subagent_fixed_model_tests;
mod subagent_navigation_tests;
pub(crate) mod vce_configuration_readiness_tests;
pub(crate) mod vce_conversation_isolation_tests;
mod vce_embedding_save_tests;
mod vce_live_model_acceptance_tests;
mod vce_project_settings_tests;
mod vce_request_preparation_tests;
mod vce_stream_fixture;
mod vce_stream_interleaving_tests;
