use super::*;
use crate::conversation_submission::{
    ExistingSubmissionQueueDecision, emit_queued_prompt_run_phase_delta,
    evaluate_existing_submission_queue, mutate_latest_workspace_for_existing_session,
};

/// Verifies an idle existing session continues immediately instead of entering the visible queue.
/// 验证空闲已有会话会立即继续，而不是进入可见队列。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if idle retained state is treated as blocking work.
/// 此测试无返回值；若空闲保留状态被视为阻塞工作则失败。
#[test]
fn idle_existing_session_submission_starts_immediately() {
    // session_id identifies the idle retained existing conversation.
    // session_id 标识空闲的保留已有会话。
    let session_id = SessionId::new_v4();
    // service_state owns the idle kernel registry and retained workspace.
    // service_state 持有空闲内核注册表与保留工作区。
    let service_state = test_service_state_with_parent_session(session_id);
    // workspace_snapshot is the exact hydrated base observed by the queue decision.
    // workspace_snapshot 是排队决策观察的精确已水合基础。
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot")
        .clone();
    // state exposes the same retained revision used to build the workspace input.
    // state 提供构建工作区输入时使用的同一保留版本。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // provider_configuration supplies provider-specific pending-lane behavior.
    assert_eq!(
        evaluate_existing_submission_queue(&state, &workspace_snapshot, Some(session_id),)
            .expect("idle queue decision"),
        ExistingSubmissionQueueDecision::StartImmediately
    );
}

/// Verifies an active kernel turn routes a new submission into the exact existing-session queue.
/// 验证活动内核回合会将新提交路由到精确已有会话队列。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if active kernel ownership is ignored.
/// 此测试无返回值；若活动内核所有权被忽略则失败。
#[test]
fn active_kernel_turn_routes_submission_to_existing_session_queue() {
    // session_id identifies the retained conversation with active system work.
    // session_id 标识存在活动系统工作的保留会话。
    let session_id = SessionId::new_v4();
    // service_state owns the active kernel turn and retained workspace.
    // service_state 持有活动内核回合与保留工作区。
    let mut service_state = test_service_state_with_parent_session(session_id);
    start_webui_session_kernel_user_turn(&mut service_state, session_id, "turn-a")
        .expect("kernel turn should start");
    // workspace_snapshot is the hydrated base consumed by shared pending-lane rules.
    // workspace_snapshot 是共享待发送通道规则消费的已水合基础。
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot")
        .clone();
    // state exposes the active kernel revision to the decision.
    // state 向决策提供活动内核版本。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // provider_configuration supplies provider-specific pending-lane behavior.
    assert_eq!(
        evaluate_existing_submission_queue(&state, &workspace_snapshot, Some(session_id),)
            .expect("active queue decision"),
        ExistingSubmissionQueueDecision::Queue { session_id }
    );
}

/// Verifies a queued commit preserves workspace state that advanced after submission preflight.
/// 验证排队提交会保留提交预检后继续推进的工作区状态。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if a stale snapshot replaces the latest retained revision.
/// 此测试无返回值；若过期快照替换最新保留版本则失败。
#[test]
fn queued_workspace_commit_preserves_post_preflight_state_and_mutates_once() {
    // session_id identifies the existing session shared by preflight and live revisions.
    // session_id 标识预检版本与实时版本共享的已有会话。
    let session_id = SessionId::new_v4();
    // service_state owns the retained revision that advances after the stale clone is captured.
    // service_state 持有在过期副本捕获后继续推进的保留版本。
    let mut service_state = test_service_state_with_parent_session(session_id);
    // stale_workspace simulates the hydrated snapshot captured by submission preflight.
    // stale_workspace 模拟提交预检捕获的已水合快照。
    let stale_workspace = service_state
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot")
        .clone();
    // initial_queued_steps records the queue count before the single committed mutation.
    // initial_queued_steps 记录单次已提交变更前的排队计数。
    let initial_queued_steps = stale_workspace
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .expect("target session")
        .queued_steps;
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("live workspace snapshot")
        .session_run_phases
        .insert(session_id, SessionRunPhase::Thinking);
    // state exposes the newer retained revision to the atomic queue commit helper.
    // state 向原子排队提交辅助函数提供较新的保留版本。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // mutation_count proves the queue mutation closure is not replayed during reconciliation.
    // mutation_count 证明队列变更闭包不会在协调期间被重复执行。
    let mut mutation_count = 0_u8;

    let observed_phase = mutate_latest_workspace_for_existing_session(
        &state,
        stale_workspace,
        session_id,
        |latest_workspace| {
            mutation_count = mutation_count.saturating_add(1);
            let session = latest_workspace
                .sessions
                .iter_mut()
                .find(|session| session.id == session_id)
                .expect("target session");
            session.queued_steps = session.queued_steps.saturating_add(1);
            Ok(latest_workspace
                .session_run_phases
                .get(&session_id)
                .copied())
        },
    )
    .expect("queued workspace commit");

    assert_eq!(mutation_count, 1);
    assert_eq!(observed_phase, Some(SessionRunPhase::Thinking));
    // committed_workspace verifies both the newer phase and the single queue mutation survived together.
    // committed_workspace 验证较新阶段与单次队列变更共同保留下来。
    let committed_workspace = state
        .lock()
        .expect("service state should remain lockable")
        .workspace_snapshot
        .clone()
        .expect("committed workspace snapshot");
    assert_eq!(
        committed_workspace.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Thinking)
    );
    assert_eq!(
        committed_workspace
            .sessions
            .iter()
            .find(|session| session.id == session_id)
            .expect("target session")
            .queued_steps,
        initial_queued_steps.saturating_add(1)
    );
}

/// Verifies queued acknowledgement projection rereads the phase after the queue transaction releases its lock.
/// 验证排队确认投影会在排队事务释放锁后重新读取阶段。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if a stale captured phase is projected.
/// 此测试无返回值；若投影了过期捕获阶段则失败。
#[test]
fn queued_projection_uses_latest_phase() {
    // session_id identifies the conversation whose queued prompt was already dispatched.
    // session_id 标识排队提示词已经开始分发的会话。
    let session_id = SessionId::new_v4();
    // service_state advances to Preparing after the original queue commit released its lock.
    // service_state 模拟原始排队提交释放锁后推进到 Preparing。
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot")
        .session_run_phases
        .insert(session_id, SessionRunPhase::Preparing);
    // state exposes the advanced authoritative phase.
    // state 暴露已推进的权威阶段。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // app discards the emitted delta because retained state is the assertion target.
    // app 丢弃已发送增量，因为断言目标是保留状态。
    let app = ServiceEventEmitter::new(Vec::new());

    let (_revision, run_phase) = emit_queued_prompt_run_phase_delta(&app, &state, session_id)
        .expect("latest queued projection should commit");

    assert_eq!(run_phase, SessionRunPhase::Preparing);
}
