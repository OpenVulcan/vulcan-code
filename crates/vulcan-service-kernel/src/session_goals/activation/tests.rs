//! User Goal activation, no-op preservation, and native-tool authority regressions.
//! 用户 Goal 激活、无操作保留与原生工具权威回归。

use super::*;
use crate::session_goal_lifecycle::session_goal_dispatch_authority_from_v1_handle;
use crate::session_runtime_registry_v1::SessionPersistenceBackend;
use vulcan_session_core::v1::document::FlushCandidate;
use vulcan_workspace_store::workspace_store::{SessionV1FlushResult, SessionV1StoreError};

/// Captures each complete publication so tests can inspect atomic mode/control visibility.
/// 捕获每次完整发布，使测试可检查模式与控制的原子可见性。
#[derive(Default)]
struct ActivationBackend {
    /// Published frontiers, recorded in the same order as the actual durable writer.
    /// 按实际可靠写入器顺序记录的已发布前沿。
    candidates: std::sync::Mutex<Vec<FlushCandidate>>,
}

impl SessionPersistenceBackend for ActivationBackend {
    /// Accepts the exact candidate and returns its metadata as the publication receipt.
    /// 接纳精确候选，并将其元数据作为发布回执返回。
    fn flush(
        &self,
        candidate: &FlushCandidate,
    ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
        self.candidates.lock().unwrap().push(candidate.clone());
        Ok(SessionV1FlushResult {
            written_file_count: candidate.dirty.len(),
            published_metadata: Some(candidate.metadata.clone()),
        })
    }
}

/// Builds a truly idle canonical owner and its observable writer without a synthetic user root.
/// 构建真正空闲的规范所有者及可观察写入器，不合成用户根。
fn idle_goal_handle() -> (Arc<SessionHandle>, Arc<ActivationBackend>) {
    // The real fresh default is the activation boundary under test.
    // 真实全新默认状态是受测激活边界。
    let session_id = SessionId::new_v4();
    let document = SessionDocument::new(session_id.to_string(), "project-a", "Goal activation", 1);
    let backend = Arc::new(ActivationBackend::default());
    let handle = SessionHandle::with_backend(session_id, document, backend.clone());
    (handle, backend)
}

/// Persists an explicit control edge and returns its exact resulting revision.
/// 持久保存明确控制边，并返回其精确结果修订。
fn set_control(handle: &SessionHandle, state: CollaborationRunControl, time: i64) -> u64 {
    handle
        .set_collaboration_run_control_durable(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new(format!("goal-test-control-{time}"))
                .unwrap(),
            state,
            changed_at_ms: time,
        })
        .unwrap()
        .revision
}

/// Tests real save/resume helpers, one-revision publication, stop preservation, and rejected resume.
/// 验证真实保存与恢复帮助函数、单修订发布、停止保留和恢复拒绝。
#[test]
fn user_goal_activation_is_atomic_and_edits_preserve_stop() {
    // Start with no human input so prior root activation cannot mask the defect.
    // 从无人类输入开始，避免先前根激活掩盖缺陷。
    let (handle, backend) = idle_goal_handle();
    let before = handle.read(|doc| doc.memory_revision()).unwrap();
    let saved = super::super::save_session_goal_v1(&handle, "Goal".into(), None, 2).unwrap();
    assert!(saved.changed && saved.value);
    assert_eq!(saved.revision, before + 1);
    assert_eq!(backend.candidates.lock().unwrap().len(), 1);
    assert_eq!(
        handle.read(|doc| doc.collaboration_run_control()).unwrap(),
        CollaborationRunControl::Running
    );
    assert!(
        session_goal_dispatch_authority_from_v1_handle(&handle, handle.session_id())
            .unwrap()
            .is_some()
    );
    // Exact saves, definition edits and redundant Active requests cannot authorize a stopped session.
    // 精确保存、定义编辑和重复 Active 请求不能授权已停止会话。
    for (control, time) in [
        (CollaborationRunControl::Stopped, 3),
        (CollaborationRunControl::MainOnly, 10),
    ] {
        set_control(&handle, control, time);
        let definition = handle
            .read(|doc| doc.metadata.mode.goal.objective.clone())
            .unwrap();
        assert!(
            !super::super::save_session_goal_v1(&handle, definition, None, time + 1)
                .unwrap()
                .changed
        );
        let edited =
            super::super::save_session_goal_v1(&handle, format!("Edited {time}"), None, time + 2)
                .unwrap();
        assert!(edited.changed && !edited.value);
        assert!(
            !update_session_goal_status_v1(&handle, SessionGoalUserStatusUpdate::Active, time + 3)
                .unwrap()
                .changed
        );
        assert_eq!(
            handle.read(|doc| doc.collaboration_run_control()).unwrap(),
            control
        );
        assert!(
            session_goal_dispatch_authority_from_v1_handle(&handle, handle.session_id())
                .unwrap()
                .is_none()
        );
    }
    // A real paused-to-active transition grants consent and mode together in one publication.
    // 真实暂停到活动转换在同一次发布中授予许可并改变模式。
    update_session_goal_status_v1(&handle, SessionGoalUserStatusUpdate::Paused, 20).unwrap();
    let before_resume = handle.read(|doc| doc.memory_revision()).unwrap();
    let publications = backend.candidates.lock().unwrap().len();
    let resumed = resume_session_goal_v1(&handle, "Resumed".into(), Some(100), 21).unwrap();
    assert_eq!(resumed.revision, before_resume + 1);
    assert_eq!(backend.candidates.lock().unwrap().len(), publications + 1);
    assert_eq!(
        handle.read(|doc| doc.collaboration_run_control()).unwrap(),
        CollaborationRunControl::Running
    );
    // Domain rejection leaves both the definition and its execution permission unchanged.
    // 领域拒绝保持定义及其执行许可均不变。
    let before_rejection = handle
        .read(|doc| {
            serde_json::to_value((&doc.metadata, doc.events(), doc.memory_revision())).unwrap()
        })
        .unwrap();
    assert!(resume_session_goal_v1(&handle, "Invalid repeat".into(), None, 22).is_err());
    assert_eq!(
        handle
            .read(|doc| {
                serde_json::to_value((&doc.metadata, doc.events(), doc.memory_revision())).unwrap()
            })
            .unwrap(),
        before_rejection
    );
    set_control(&handle, CollaborationRunControl::Stopped, 23);
    update_session_goal_status_v1(&handle, SessionGoalUserStatusUpdate::Paused, 24).unwrap();
    let before_status = handle.read(|doc| doc.memory_revision()).unwrap();
    let status =
        update_session_goal_status_v1(&handle, SessionGoalUserStatusUpdate::Active, 25).unwrap();
    assert_eq!(status.revision, before_status + 1);
    assert_eq!(
        handle.read(|doc| doc.collaboration_run_control()).unwrap(),
        CollaborationRunControl::Running
    );
}

/// Native model Goal creation never receives the user's independent permission to resume Stop.
/// 原生模型 Goal 创建绝不获得用户独立的恢复 Stop 权限。
#[tokio::test]
async fn native_goal_creation_preserves_stopped_execution_consent() {
    // Register the actual native adapter around a stopped fresh document.
    // 围绕已停止全新文档登记实际原生适配器。
    let (handle, _backend) = idle_goal_handle();
    set_control(&handle, CollaborationRunControl::Stopped, 2);
    let service =
        crate::tests::test_support::test_service_state_with_parent_session(handle.session_id());
    service
        .session_runtime_registry
        .register(handle.clone())
        .unwrap();
    let state = Arc::new(std::sync::Mutex::new(service));
    let native =
        super::super::session_v1_native_goal_runtime(state, ServiceEventEmitter::new(Vec::new()));
    native
        .create_goal(handle.session_id(), "Native goal".into(), None)
        .await
        .unwrap();
    assert_eq!(
        handle.read(|doc| doc.collaboration_run_control()).unwrap(),
        CollaborationRunControl::Stopped
    );
    assert!(
        session_goal_dispatch_authority_from_v1_handle(&handle, handle.session_id())
            .unwrap()
            .is_none()
    );
}
