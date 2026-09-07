//! Exercise the real conversation-stop and index-cancel services while a nonempty VCE write is suspended.
//! 在非空 VCE 写入暂停期间验证真实的会话停止与索引取消服务。

use super::*;
use futures_util::FutureExt;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use vulcan_workbench_model::ProjectVceSettings;
use vulcan_workspace_store::vce_data_plane::VceJobState;

/// Register a real session-owned task whose acknowledgements prove it remains scheduled, not merely cached.
/// 注册真实会话所属任务，通过应答证明仍被调度，而非仅保留缓存条目。
/// Inputs select the exact service/session and a fresh turn; returns the task identity and liveness sender.
/// 输入选择精确服务、会话及新回合；返回任务身份与存活验证发送端。
fn start_live_conversation_probe(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    turn_id: &str,
) -> (Uuid, mpsc::Sender<oneshot::Sender<()>>) {
    // The real kernel accepts the turn before the real stream registry takes ownership.
    // 真实内核先准入回合，再由真实流注册表取得所有权。
    start_webui_session_kernel_user_turn(
        &mut state.lock().expect("session state"),
        session_id,
        turn_id,
    )
    .expect("admit conversation while indexing");
    // Each acknowledgement requires the task future to run after the caller's operation.
    // 每次应答都要求任务协程在调用方操作之后实际运行。
    let (sender, mut receiver) = mpsc::channel::<oneshot::Sender<()>>(1);
    // This fresh owner cannot accidentally identify the project index job.
    // 此全新所有者不会意外标识项目索引作业。
    let task_id = Uuid::new_v4();
    spawn_registered_active_stream_task_with_mode(
        state,
        session_id,
        task_id,
        None,
        ActiveStreamTaskRegistrationMode::RejectIfOccupied,
        async move {
            while let Some(acknowledgement) = receiver.recv().await {
                acknowledgement.send(()).expect("live probe receiver");
            }
        },
    )
    .expect("register independent conversation task");
    (task_id, sender)
}

/// Assert a registered conversation remains live after an index operation, with a bounded actual task exchange.
/// 在索引操作后通过有界真实任务交互断言已注册会话仍然存活。
/// Inputs identify the current task and its sender; returns after the exact live owner acknowledges.
/// 输入标识当前任务及发送端；在精确存活所有者应答后返回。
async fn assert_conversation_probe_live(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    task_id: Uuid,
    sender: &mpsc::Sender<oneshot::Sender<()>>,
) {
    // A mailbox entry alone is insufficient evidence, so require execution of an acknowledgement.
    // 仅有邮箱条目不足以证明存活，因此要求真正执行一次应答。
    let (acknowledge, received) = oneshot::channel();
    sender
        .send(acknowledge)
        .await
        .expect("conversation task accepts work");
    tokio::time::timeout(Duration::from_secs(5), received)
        .await
        .expect("conversation must not wait for indexing")
        .expect("live conversation acknowledgement");
    // The authoritative registry and kernel must still agree on a live session.
    // 权威注册表和内核仍必须共同表明会话存活。
    let guard = state.lock().expect("live session state");
    assert_eq!(
        guard
            .active_stream_tasks
            .get(&session_id)
            .map(ActiveStreamTask::task_id),
        Some(task_id)
    );
    assert!(webui_session_has_active_run(&guard, session_id));
}

/// Verify both cancellation directions using production service entrypoints, real indexed session persistence,
/// 使用生产服务入口和真实索引式会话持久化，验证两个取消方向的独立性，
/// and the caller's already-running nonempty VCE job; returns with that job cancelling and no live chat task.
/// 输入为调用方已运行的非空 VCE 作业；返回时该作业处于取消中且没有存活聊天任务。
/// This does not substitute the probe mailbox for a real provider text stream acceptance test.
/// 此测试不把探针邮箱冒充真实供应商文本流验收。
pub(crate) async fn verify_stop_and_index_cancellation(
    manager: vce::VceRuntimeManager,
    project_root: PathBuf,
    settings: ProjectVceSettings,
    job_id: String,
) {
    // The fixture owns its controller and session files, independently of all user data and the VCE storage.
    // 夹具持有独立控制器与会话文件，与全部用户数据和 VCE 存储分离。
    let fixture = TestControllerRuntimeRoot::new(SessionId::new_v4());
    let (client, controller) = fixture.attached_controller_runtime_handle().await;
    let session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        session_id,
    )));
    // Always release the managed controller and any registered task, including assertion-unwind paths.
    // 始终释放托管控制器及任何已注册任务，包括断言展开路径。
    let outcome = std::panic::AssertUnwindSafe(async {
        // Publish a fresh real session whose project identity exactly matches the running index.
        // 发布项目身份与运行中索引完全一致的全新真实会话。
        let session_id_text = session_id.to_string();
        let metadata = vulcan_session_core::v1::config::MetadataDocument::new(
            &session_id_text,
            "nonempty-project",
            "VCE cancellation isolation",
            1,
        );
        let runtime_lock = vulcan_session_core::v1::context::RuntimeLock {
            initialization_state:
                vulcan_session_core::v1::context::RuntimeInitializationState::FreshUninitialized,
            current_compaction_id: None,
            locked_sources: Vec::new(),
        };
        vulcan_workspace_store::workspace_store::create_indexed_session_v1(
            &controller,
            &vulcan_workspace_store::workspace_store::CreateSessionV1Request {
                project_id: "nonempty-project",
                storage_partition:
                    vulcan_workspace_store::workspace_store::SESSION_V1_STORAGE_PARTITION,
                date_bucket: "1970-01-01",
                metadata: &metadata,
                runtime_lock: &runtime_lock,
                compactions: &[],
                events: &[],
            },
        )
        .await
        .expect("create exact indexed conversation");
        {
            let mut guard = state.lock().expect("fixture state");
            guard.controller_runtime = Some(controller.clone());
            guard.agent_runtime = Some(test_agent_runtime_bridge());
            guard.vce_runtime = Some(manager.clone());
            let workspace = guard.workspace_snapshot.as_mut().expect("workspace");
            workspace.sessions[0].project_id = Some(String::from("nonempty-project"));
            workspace.task_projects.push(
                TaskProjectSummary::new(
                    "nonempty-project",
                    "VCE isolation",
                    project_root.display().to_string(),
                    false,
                    0,
                    None,
                    None,
                    vec![session_id],
                )
                .with_vce_settings(settings),
            );
        }
        // Stop must remove the actual task while leaving the suspended project writer untouched.
        // 停止操作必须移除真实会话任务，同时保持暂停的项目写入任务不变。
        let (first_task, first_sender) =
            start_live_conversation_probe(&state, session_id, "vce-stop-first");
        assert_conversation_probe_live(&state, session_id, first_task, &first_sender).await;
        let stopped = tokio::time::timeout(
            Duration::from_secs(15),
            service_stop_conversation_run(
                ServiceEventEmitter::new(Vec::new()),
                state.clone(),
                session_id_text.clone(),
                Some(StopConversationRunMode::MainAndSubagents),
            ),
        )
        .await
        .expect("stop must not await project indexing")
        .expect("real stop service");
        assert!(matches!(
            stopped,
            StopConversationRunResponse::Stopped { .. }
        ));
        assert!(
            first_sender.is_closed(),
            "the original conversation task must actually stop"
        );
        assert!(!webui_session_has_active_run(
            &state.lock().expect("stopped state"),
            session_id
        ));
        assert_eq!(
            manager
                .get_job("nonempty-project", Some(&job_id))
                .await
                .expect("read writer after conversation stop")
                .expect("writer receipt")
                .state,
            VceJobState::Running,
            "conversation stop must not cancel indexing"
        );

        // A replacement conversation must be admitted while the same index update is still suspended.
        // 同一索引更新仍暂停时，必须能够准入替代会话回合。
        let (next_task, next_sender) =
            start_live_conversation_probe(&state, session_id, "vce-stop-next");
        assert_conversation_probe_live(&state, session_id, next_task, &next_sender).await;
        vce::service_cancel_vce_job(
            state.clone(),
            vulcan_service_api::VceJobCancelRequest {
                project_id: String::from("nonempty-project"),
                job_id: job_id.clone(),
            },
        )
        .await
        .expect("real index cancel service");
        assert_eq!(
            manager
                .get_job("nonempty-project", Some(&job_id))
                .await
                .expect("read cancelling writer")
                .expect("writer receipt")
                .state,
            VceJobState::Cancelling
        );
        assert_conversation_probe_live(&state, session_id, next_task, &next_sender).await;
        service_stop_conversation_run(
            ServiceEventEmitter::new(Vec::new()),
            state.clone(),
            session_id_text,
            Some(StopConversationRunMode::MainOnly),
        )
        .await
        .expect("retire probe session");
        assert!(next_sender.is_closed());
    })
    .catch_unwind()
    .await;
    // Extract the exact remaining task without holding a synchronous lock across asynchronous teardown.
    // 提取精确残留任务，不在异步清理期间持有同步锁。
    let remaining = state
        .lock()
        .expect("cleanup fixture state")
        .active_stream_tasks
        .remove(&session_id);
    if let Some(task) = remaining {
        let _ = task.abort_and_wait().await;
    }
    drop(state);
    client
        .shutdown()
        .await
        .expect("release isolated conversation controller");
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}
