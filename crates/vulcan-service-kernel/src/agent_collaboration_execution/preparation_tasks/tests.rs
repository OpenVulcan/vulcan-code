//! Real preparation registration and cancellation through the child Stop service.
//! 通过子停止服务验证真实准备登记与取消。

use super::*;
use crate::agent_collaboration::child_execution_ownership::{
    retain_child_question_execution_owner, tests::child_ownership_fixture,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use vulcan_agent_runtime::agent_runtime::task_runtime::TaskRuntime;

/// Registers the fixture's real QA kernel; returns its generation after releasing submission admission.
/// 登记夹具的真实答疑内核；释放提交准入后返回其代际。
fn register_kernel(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    execution_id: &CollaborationExecutionId,
) -> u64 {
    let admission =
        crate::project_conversation_services::try_begin_pending_prompt_dispatch_exclusion(
            state, session_id,
        )
        .unwrap()
        .unwrap();
    let reservation = admission
        .existing_submission_authorization(session_id)
        .unwrap()
        .reservation_id;
    crate::start_reserved_webui_session_kernel_turn(
        &mut state.lock().unwrap(),
        session_id,
        "preparation",
        ActiveTurnKind::QuestionExecution {
            execution_id: execution_id.clone(),
        },
        reservation,
    )
    .unwrap();
    crate::webui_session_kernel_generation(&state.lock().unwrap(), session_id).unwrap()
}

/// Stop must join registered preparation before completing its reliable auxiliary boundary.
/// 停止必须等待已登记准备退出，然后完成可靠辅助边界。
#[tokio::test]
async fn question_preparation_stop_joins_live_future_and_rejects_late_restart() {
    let (state, handle, execution) = child_ownership_fixture();
    let runtime = Arc::new(TaskRuntime::new());
    // Actual registry edges make the running question a nested descendant of its ordinary main session.
    // 实际注册表边使运行中问题成为普通主会话的嵌套后代。
    let root = Uuid::parse_str(&execution.participant.root_session_id).unwrap();
    // The intermediate child is completed and therefore absent from a running-business-only selection.
    // 中间子任务已完成，因此不出现在仅运行中业务选择中。
    let intermediate = Uuid::new_v4();
    for (task_id, parent_session_id, public_task_id) in [
        (intermediate, root, "completed-parent"),
        (handle.session_id(), intermediate, "question-child"),
    ] {
        runtime
            .seed_test_subagent_registry_entry(
                vulcan_agent_runtime::agent_runtime::task_runtime::TestSubagentRegistryEntrySeed {
                    root_session_id: root,
                    parent_session_id,
                    task_id,
                    public_task_id: public_task_id.to_owned(),
                    display_title: public_task_id.to_owned(),
                    description: None,
                    subagent: None,
                    status: vulcan_task_session::TaskSessionStatus::Completed,
                    result_summary: None,
                },
            )
            .unwrap();
    }
    let admission = runtime
        .try_reserve_child_question_execution(handle.session_id())
        .unwrap()
        .unwrap();
    retain_child_question_execution_owner(&state, &execution, admission).unwrap();
    let generation = register_kernel(&state, handle.session_id(), &execution.execution_id);
    // One acknowledgement proves the future really started; the other channel proves its destruction.
    // 一个确认信号证明 Future 确实启动；另一个通道证明其已销毁。
    let (started, started_receiver) = tokio::sync::oneshot::channel();
    let (exit, mut exit_receiver) = tokio::sync::oneshot::channel::<()>();
    spawn_question_preparation_task(
        &state,
        handle.session_id(),
        generation,
        "preparation",
        &execution.execution_id,
        async move {
            started.send(()).unwrap();
            std::future::pending::<()>().await;
            drop(exit);
        },
    )
    .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), started_receiver)
        .await
        .unwrap()
        .unwrap();
    assert!(state.lock().unwrap().active_stream_tasks.is_empty());
    // Lifecycle selection must traverse completed parents to discover this actual running question.
    // 生命周期选择必须经过已完成父任务，发现本实际运行中的答疑。
    let mut bridge = crate::tests::test_support::test_agent_runtime_bridge();
    bridge.task_runtime = runtime.clone();
    state.lock().unwrap().agent_runtime = Some(bridge);
    assert!(
        crate::project_conversation_services::begin_session_lifecycle_transition(
            &state,
            [root],
            "archive",
        )
        .is_err()
    );
    crate::conversation_run_services::stop_owned_child_auxiliary_tree(
        &crate::ServiceEventEmitter::new(Vec::new()),
        &state,
        &runtime,
        root,
    )
    .await
    .unwrap();
    assert_eq!(
        exit_receiver.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Closed)
    );
    assert!(state.lock().unwrap().question_preparation_tasks.is_empty());
    drop(
        crate::project_conversation_services::begin_session_lifecycle_transition(
            &state,
            [root],
            "archive",
        )
        .unwrap(),
    );
    assert!(
        state
            .lock()
            .unwrap()
            .child_question_execution_owners
            .is_empty()
    );
    assert!(
        runtime
            .try_reserve_child_question_execution(handle.session_id())
            .unwrap()
            .is_some()
    );
    // A completed Stop must reject the old generation without polling any new side effect.
    // 完成停止后必须拒绝旧代际，不能轮询任何新副作用。
    let polled = Arc::new(AtomicBool::new(false));
    let polled_in_task = polled.clone();
    assert!(
        spawn_question_preparation_task(
            &state,
            handle.session_id(),
            generation,
            "preparation",
            &execution.execution_id,
            async move {
                polled_in_task.store(true, Ordering::SeqCst);
            }
        )
        .is_err()
    );
    assert!(!polled.load(Ordering::SeqCst));
}

/// Registration rejects a stale generation and a second owner without replacing existing preparation.
/// 登记拒绝过期代际及第二个所有者，不替换已有准备。
#[tokio::test]
async fn question_preparation_registration_preserves_exact_owner() {
    let (state, handle, execution) = child_ownership_fixture();
    let generation = register_kernel(&state, handle.session_id(), &execution.execution_id);
    assert!(
        spawn_question_preparation_task(
            &state,
            handle.session_id(),
            generation + 1,
            "preparation",
            &execution.execution_id,
            async {}
        )
        .is_err()
    );
    assert!(state.lock().unwrap().question_preparation_tasks.is_empty());
    spawn_question_preparation_task(
        &state,
        handle.session_id(),
        generation,
        "preparation",
        &execution.execution_id,
        std::future::pending(),
    )
    .unwrap();
    let first_id = state.lock().unwrap().question_preparation_tasks[&handle.session_id()]
        .task
        .task_id();
    assert!(
        spawn_question_preparation_task(
            &state,
            handle.session_id(),
            generation,
            "preparation",
            &execution.execution_id,
            async {}
        )
        .is_err()
    );
    assert_eq!(
        state.lock().unwrap().question_preparation_tasks[&handle.session_id()]
            .task
            .task_id(),
        first_id
    );
    let task = state
        .lock()
        .unwrap()
        .question_preparation_tasks
        .remove(&handle.session_id())
        .unwrap();
    task.task.abort_and_wait().await.unwrap();
}

/// Completed preparation removes its own task while preserving the registered successor stream.
/// 完成准备只移除自己的任务，并保留已登记的后继流。
#[tokio::test]
async fn question_preparation_handoff_preserves_provider_stream() {
    // Use the same real child ownership and kernel contracts as the Stop regression.
    // 使用与停止回归相同的真实子所有权及内核契约。
    let (state, handle, execution) = child_ownership_fixture();
    let runtime = Arc::new(TaskRuntime::new());
    let admission = runtime
        .try_reserve_child_question_execution(handle.session_id())
        .unwrap()
        .unwrap();
    retain_child_question_execution_owner(&state, &execution, admission).unwrap();
    let generation = register_kernel(&state, handle.session_id(), &execution.execution_id);
    let session_id = handle.session_id();
    let stream_id = Uuid::new_v4();
    let state_for_preparation = state.clone();
    // This signal confirms that the actual successor registration has succeeded.
    // 此信号确认实际后继登记已经成功。
    let (registered, registered_receiver) = tokio::sync::oneshot::channel();
    spawn_question_preparation_task(
        &state,
        session_id,
        generation,
        "preparation",
        &execution.execution_id,
        async move {
            crate::state::spawn_registered_kernel_stream_task_with_mode(
                &state_for_preparation,
                session_id,
                stream_id,
                None,
                crate::state::ActiveStreamTaskRegistrationMode::RejectIfOccupied,
                generation,
                "preparation",
                std::future::pending(),
            )
            .unwrap();
            registered.send(()).unwrap();
        },
    )
    .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), registered_receiver)
        .await
        .unwrap()
        .unwrap();
    // Wait for the wrapper's own completion boundary, not an assumed scheduler delay.
    // 等待包装器自己的完成边界，不假定调度延迟。
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if !state
                .lock()
                .unwrap()
                .question_preparation_tasks
                .contains_key(&session_id)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        state.lock().unwrap().active_stream_tasks[&session_id].task_id(),
        stream_id
    );
    assert!(
        runtime
            .try_reserve_child_question_execution(session_id)
            .unwrap()
            .is_none()
    );
    crate::conversation_run_services::stop_owned_child_question_execution(
        &crate::ServiceEventEmitter::new(Vec::new()),
        &state,
        session_id,
    )
    .await
    .unwrap();
    assert!(state.lock().unwrap().active_stream_tasks.is_empty());
    assert!(state.lock().unwrap().question_preparation_tasks.is_empty());
}
