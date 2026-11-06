//! Shell-job wake admission tests against real session storage and the global job runtime.
//! 基于真实会话存储与全局任务运行时的 shell 任务唤醒准入测试。

use super::*;
use crate::agent_stream::provider_v1_sink::ProviderV1StreamSink;
use crate::tests::test_support::test_service_state_with_parent_session;
use vulcan_host_tools::shell_job::{ShellJobFinal, ShellJobNotice, ShellJobStatus};
use vulcan_session_core::v1::event::{EventBody, UserInputSource};
use vulcan_session_core::v1::shell_job_notice::ShellJobNoticeFacts;

/// Creates a stored idle session whose handle is not shared with any other test.
/// 创建一个已存储的空闲会话，其句柄不与其他测试共享。
fn idle_session(directory: &std::path::Path, session_id: SessionId) -> Arc<SessionHandle> {
    let locator = vulcan_workspace_store::workspace_store::create_session_v1_storage_fixture(
        directory,
        &session_id.to_string(),
        1,
    )
    .unwrap();
    let document = vulcan_workspace_store::workspace_store::load_session_v1_full(&locator)
        .unwrap()
        .document;
    SessionHandle::new(session_id, locator, document)
}

/// Queues one finished background job notice for `session_id` in the global job runtime.
/// 在全局任务运行时中为 `session_id` 排入一条后台任务结束通知。
fn queue_notice(session_id: SessionId, job_id: &str) {
    ShellJobRuntime::global().requeue_notices(
        &session_id.to_string(),
        vec![ShellJobNotice {
            job_id: job_id.to_string(),
            purpose: String::from("Run the test suite"),
            command: String::from("cargo test"),
            outcome: ShellJobFinal {
                status: ShellJobStatus::Completed,
                exit_code: Some(0),
                duration_ms: 10,
                finished_at_ms: 1,
                failure_reason: None,
            },
        }],
    );
}

/// Job ids of the finish notices recorded as agent-message inputs, which is how notices reach the
/// model and how clients recognise them.
/// 记录为代理消息输入的结束通知的任务标识；通知以此送达模型，客户端也以此识别它们。
fn recorded_notice_jobs(handle: &SessionHandle) -> Vec<String> {
    handle
        .read(|document| {
            document
                .events()
                .iter()
                .filter_map(|event| match &event.event {
                    EventBody::UserInput(input)
                        if input.input_source == UserInputSource::AgentMessage =>
                    {
                        ShellJobNoticeFacts::parse(&input.text).map(|facts| facts.job_id)
                    }
                    _ => None,
                })
                .collect()
        })
        .unwrap()
}

/// An idle session with pending notices opens exactly one wake turn whose first request delivers
/// every notice once.
/// 带有待送达通知的空闲会话只开启一个唤醒回合，其首次请求将每条通知只送达一次。
#[tokio::test]
async fn idle_session_opens_one_wake_turn_and_delivers_notices_once() {
    let directory = tempfile::tempdir().unwrap();
    let session_id = SessionId::new_v4();
    let service = test_service_state_with_parent_session(session_id);
    let catalog = service.project_resource_catalog_registry.clone();
    let state = Arc::new(Mutex::new(service));
    let handle = idle_session(directory.path(), session_id);
    queue_notice(session_id, "job-a");
    queue_notice(session_id, "job-b");

    let start = open_shell_job_continuation_turn(&state, &handle)
        .await
        .unwrap()
        .expect("an idle session with pending notices must open a wake turn");
    assert!(
        open_shell_job_continuation_turn(&state, &handle)
            .await
            .unwrap()
            .is_none(),
        "a second trigger must not open another turn while the first is open"
    );
    let authority = shell_job_dispatch_authority_from_v1_handle(handle.as_ref())
        .unwrap()
        .expect("the open wake turn must yield dispatch authority");
    assert_eq!(authority.turn_id, start.turn_id);
    assert_eq!(authority.context_seq, start.context_seq);

    let sink = ProviderV1StreamSink::new(handle.clone(), catalog);
    assert_eq!(sink.consume_shell_job_notices().unwrap(), 2);
    assert_eq!(sink.consume_shell_job_notices().unwrap(), 0);
    let mut jobs = recorded_notice_jobs(handle.as_ref());
    jobs.sort();
    assert_eq!(jobs, ["job-a", "job-b"]);
    assert_eq!(pending_notice_count(session_id), 0);
}

/// A session without pending notices, or one whose turn admission is owned elsewhere, stays idle.
/// 没有待送达通知或回合准入被其他所有者持有的会话保持空闲。
#[tokio::test]
async fn busy_or_quiet_session_does_not_open_a_wake_turn() {
    let directory = tempfile::tempdir().unwrap();
    let session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        session_id,
    )));
    let handle = idle_session(directory.path(), session_id);
    assert!(
        open_shell_job_continuation_turn(&state, &handle)
            .await
            .unwrap()
            .is_none(),
        "no pending notice means nothing to wake for"
    );

    queue_notice(session_id, "job-busy");
    let running =
        crate::project_conversation_services::try_begin_pending_prompt_dispatch_exclusion(
            &state, session_id,
        )
        .unwrap()
        .expect("the test owns the only admission");
    assert!(
        open_shell_job_continuation_turn(&state, &handle)
            .await
            .unwrap()
            .is_none(),
        "a session whose turn admission is owned elsewhere must not be woken"
    );
    assert!(
        shell_job_dispatch_authority_from_v1_handle(handle.as_ref())
            .unwrap()
            .is_none()
    );
    assert_eq!(pending_notice_count(session_id), 1);
    drop(running);
    ShellJobRuntime::global().take_notices(&session_id.to_string());
}

/// A wake turn that could not dispatch is stopped, and its notices stay pending for later delivery.
/// 无法分发的唤醒回合被停止，其通知保持待送达以便稍后送达。
#[tokio::test]
async fn undispatched_wake_turn_is_stopped_and_keeps_notices_pending() {
    let directory = tempfile::tempdir().unwrap();
    let session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        session_id,
    )));
    let handle = idle_session(directory.path(), session_id);
    queue_notice(session_id, "job-stop");
    let start = open_shell_job_continuation_turn(&state, &handle)
        .await
        .unwrap()
        .unwrap();
    stop_undispatched_shell_job_continuation_turn(&handle, start.turn_id)
        .await
        .unwrap();
    stop_undispatched_shell_job_continuation_turn(&handle, start.turn_id)
        .await
        .unwrap();
    assert!(
        shell_job_dispatch_authority_from_v1_handle(handle.as_ref())
            .unwrap()
            .is_none()
    );
    assert!(
        handle
            .read(|document| document.active_turn().is_none_or(|turn| turn.closed))
            .unwrap()
    );
    assert_eq!(pending_notice_count(session_id), 1);
    ShellJobRuntime::global().take_notices(&session_id.to_string());
}

/// Only listed root sessions without an active stream or pending plan decision may wake.
/// 只有已列出、没有活动流且没有待决计划的根会话才能唤醒。
#[test]
fn only_listed_root_sessions_can_wake() {
    let root = SessionId::new_v4();
    let state = test_service_state_with_parent_session(root);
    assert!(root_session_idle_in(&state, root));
    assert!(
        !root_session_idle_in(&state, SessionId::new_v4()),
        "child and unknown sessions are not listed and must not wake"
    );
}

/// Concurrent triggers for one session collapse into a single attempt series with one rerun.
/// 同一会话的并发触发合并为单个尝试序列并记录一次重试。
#[test]
fn concurrent_triggers_collapse_into_one_attempt_series() {
    let session_id = SessionId::new_v4();
    let slot = WakeSlot::claim(session_id).expect("the first trigger owns the slot");
    assert!(WakeSlot::claim(session_id).is_none());
    assert!(WakeSlot::claim(session_id).is_none());
    assert!(slot.take_rerun());
    assert!(!slot.take_rerun());
    drop(slot);
    assert!(WakeSlot::claim(session_id).is_some());
}
