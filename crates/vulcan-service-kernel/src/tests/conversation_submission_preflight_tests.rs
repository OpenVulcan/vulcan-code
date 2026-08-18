use super::*;
use crate::conversation_submission::{
    attach_client_submission_id_to_visible_user_record, commit_created_submission_workspace_slice,
    prepare_materialized_submission_session, prepare_retained_submission_preflight,
};

/// Builds an initialized retained workspace that owns `project-a` for new-session admission tests.
/// 构造拥有 `project-a` 的已初始化保留工作区，用于新会话准入测试。
///
/// # Parameters
/// 参数
/// `session_id` identifies the existing idle session retained by the test workspace.
/// `session_id` 标识测试工作区保留的现有空闲会话。
///
/// # Returns
/// 返回值
/// Returns a service state whose retained workspace explicitly contains `project-a`.
/// 返回保留工作区显式包含 `project-a` 的服务状态。
fn initialized_project_a_service_state(session_id: SessionId) -> ServiceBridgeState {
    let mut service_state = test_service_state_with_parent_session(session_id);
    let retained_workspace = service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace should exist");
    retained_workspace
        .task_projects
        .push(TaskProjectSummary::new(
            "project-a",
            "Project A",
            "D:/project-a",
            false,
            0,
            None,
            None,
            Vec::new(),
        ));
    service_state
}

/// Verifies an existing session resolves its retained project owner before lifecycle admission.
/// 验证已有会话会在生命周期准入前解析其保留项目所有者。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if an omitted request project bypasses the owner lifecycle gate.
/// 此测试无返回值；若省略请求项目能够绕过所有者生命周期闸门则失败。
#[test]
fn retained_session_owner_project_blocks_submission_during_lifecycle_transition() {
    // session_id identifies the retained existing conversation owned by project-a.
    // session_id 标识由 project-a 拥有的保留已有会话。
    let session_id = SessionId::new_v4();
    // service_state owns the retained session and the authoritative project lifecycle reservation.
    // service_state 持有保留会话与权威项目生命周期预留。
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state.project_admission_reservations.insert(
        ProjectAdmissionScope::Exact(String::from("project-a")),
        ProjectAdmissionReservation {
            reservation_id: Uuid::new_v4(),
            kind: ProjectAdmissionReservationKind::LifecycleTransition,
        },
    );
    // state exposes one coherent revision to the retained submission preflight.
    // state 向保留提交预检提供一个一致版本。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));

    // error is the exact lifecycle rejection derived from the retained session owner.
    // error 是从保留会话所有者推导出的精确生命周期拒绝。
    let error = prepare_retained_submission_preflight(
        &state,
        None,
        PromptSessionTarget::Existing(session_id),
        None,
    )
    .err()
    .expect("project lifecycle reservation should reject submission");

    assert!(error.contains("project-a"));
    assert!(error.contains("archive or removal lifecycle transition"));
}

/// Verifies a retained archived conversation is rejected before any submission reservation is inserted.
/// 验证保留的归档会话会在插入任何提交预留前被拒绝。
///
/// # Parameters
/// 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails if an archived target acquires session or project ownership.
/// 此测试无返回值；若归档目标取得会话或项目所有权则失败。
#[test]
fn retained_archived_session_is_rejected_before_admission() {
    // session_id identifies the retained row moved from active to archived storage.
    // session_id 标识从活动存储移动到归档存储的保留行。
    let session_id = SessionId::new_v4();
    // service_state supplies the authoritative retained workspace under test.
    // service_state 提供测试中的权威保留工作区。
    let mut service_state = test_service_state_with_parent_session(session_id);
    {
        // workspace_snapshot moves the exact summary to the archived list without changing identity.
        // workspace_snapshot 将精确摘要移动到归档列表且不改变身份。
        let workspace_snapshot = service_state
            .workspace_snapshot
            .as_mut()
            .expect("test workspace should exist");
        // archived_session is the exact regular-session summary removed from the active list.
        // archived_session 是从活动列表移除的精确普通会话摘要。
        let archived_session = workspace_snapshot.sessions.remove(0);
        workspace_snapshot.archived_sessions.push(archived_session);
    }
    // state exposes archived membership and empty admission tables under one lock.
    // state 在同一个锁下提供归档成员关系与空准入表。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));

    // error is produced by target lifecycle validation before reservation creation.
    // error 由目标生命周期校验在创建预留前产生。
    let error = prepare_retained_submission_preflight(
        &state,
        None,
        PromptSessionTarget::Existing(session_id),
        None,
    )
    .err()
    .expect("archived target should be rejected");
    assert!(error.contains("archived"));
    let guard = state.lock().expect("service state lock");
    assert!(guard.session_turn_admission_reservations.is_empty());
    assert!(guard.project_admission_reservations.is_empty());
}

/// Verifies a session admission reservation rejects submission before any snapshot clone is returned.
/// 验证会话准入预留会在返回任何快照克隆前拒绝提交。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if the reservation reason is not preserved.
/// 此测试无返回值；若未保留预留原因则失败。
#[test]
fn session_admission_reservation_rejects_retained_submission_preflight() {
    // session_id identifies the exact session protected by Goal mutation.
    // session_id 标识由目标变更保护的精确会话。
    let session_id = SessionId::new_v4();
    // service_state owns the authoritative admission reservation.
    // service_state 持有权威准入预留。
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state.session_turn_admission_reservations.insert(
        session_id,
        SessionTurnAdmissionReservation {
            reservation_id: Uuid::new_v4(),
            kind: SessionTurnAdmissionReservationKind::GoalMutation,
        },
    );
    // state exposes the reserved revision to preflight.
    // state 向预检提供已预留版本。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));

    // error is the exact reservation rejection produced before snapshot selection.
    // error 是在快照选择前产生的精确预留拒绝。
    let error = prepare_retained_submission_preflight(
        &state,
        None,
        PromptSessionTarget::Existing(session_id),
        None,
    )
    .err()
    .expect("session reservation should reject submission");

    assert!(error.contains("Goal authority mutation"));
}

/// Verifies one existing-session preflight owns admission until its exact kernel turn starts.
/// 验证一次已有会话预检会持有准入权，直到其精确 kernel 回合启动。
///
/// # Parameters
/// 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails when a competitor can enter or the owner cannot start.
/// 此测试无返回值；若竞争者能够进入或所有者无法启动则失败。
#[test]
fn existing_submission_admission_blocks_competitor_and_authorizes_owner() {
    // session_id identifies the exact existing session protected across submission preparation.
    // session_id 标识在提交准备期间受到保护的精确已有会话。
    let session_id = SessionId::new_v4();
    // service_state supplies one idle retained parent session.
    // service_state 提供一个空闲的保留父会话。
    let service_state = test_service_state_with_parent_session(session_id);
    // state exposes reservation and kernel ownership under one shared lock.
    // state 在同一个共享锁下提供预留与 kernel 所有权。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // preflight owns the PromptSubmission reservation until explicitly dropped.
    // preflight 持有 PromptSubmission 预留，直到被显式释放。
    let preflight = prepare_retained_submission_preflight(
        &state,
        None,
        PromptSessionTarget::Existing(session_id),
        None,
    )
    .expect("first existing submission should reserve admission");

    // competitor_error proves the reservation closes the second-submit race.
    // competitor_error 证明该预留关闭第二次提交竞态。
    let competitor_error = prepare_retained_submission_preflight(
        &state,
        None,
        PromptSessionTarget::Existing(session_id),
        None,
    )
    .err()
    .expect("competing submission should be rejected");
    assert!(competitor_error.contains("user prompt submission"));
    // materialized_session represents the authoritative existing-session result later in the same request.
    // materialized_session 表示同一请求后续产生的权威已有会话结果。
    let materialized_session = MaterializedPromptSession {
        session_id,
        project_id: Some(String::from("project-a")),
        session_title: String::from("父会话"),
        workspace_path: String::new(),
        kind: PromptSessionMaterializationKind::Existing,
    };
    // reservation_id is the exact owner token accepted by kernel registration.
    // reservation_id 是 kernel 登记接受的精确所有者令牌。
    let reservation_id = preflight
        .turn_admission
        .kernel_reservation_id(&materialized_session)
        .expect("materialized session should match admission")
        .expect("existing session should have a reservation");
    {
        // guard commits the kernel turn while the exact reservation remains current.
        // guard 在精确预留仍为当前所有者时提交 kernel 回合。
        let mut guard = state.lock().expect("service state lock");
        start_reserved_webui_session_kernel_user_turn(
            &mut guard,
            session_id,
            "turn-owner",
            reservation_id,
        )
        .expect("reservation owner should start kernel turn");
    }

    drop(preflight);

    // guard verifies dropping the preflight releases only admission, not the committed kernel turn.
    // guard 验证释放预检只会移除准入预留，不会移除已提交的 kernel 回合。
    let guard = state.lock().expect("service state lock");
    assert!(
        !guard
            .session_turn_admission_reservations
            .contains_key(&session_id)
    );
    assert!(webui_session_kernel_has_active_turn(&guard, session_id));
}

/// Verifies concurrent queued-prompt preparation receives one admission owner and can retry after release.
/// 验证并发排队提示词准备只会获得一个准入所有者，并可在释放后重新领取。
#[test]
fn pending_prompt_dispatch_admission_is_single_owner_and_reusable() {
    let session_id = SessionId::new_v4();
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(
        test_service_state_with_parent_session(session_id),
    ));

    let first = try_begin_pending_prompt_dispatch_exclusion(&state, session_id)
        .expect("first pending dispatch admission should be readable")
        .expect("first pending dispatch should own admission");
    assert!(
        try_begin_pending_prompt_dispatch_exclusion(&state, session_id)
            .expect("competing pending dispatch admission should be readable")
            .is_none()
    );

    drop(first);

    assert!(
        try_begin_pending_prompt_dispatch_exclusion(&state, session_id)
            .expect("released pending dispatch admission should be readable")
            .is_some()
    );
}

/// Verifies an outer destructive transaction can authorize only its nested prompt replay.
/// 验证外层破坏性事务只能授权其自身的嵌套提示词重放。
///
/// # Parameters
/// 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails if preauthorization leaks, rejects its owner, or admits a competitor.
/// 此测试无返回值；若预授权泄漏、拒绝所有者或放行竞争者则失败。
#[test]
fn preauthorized_existing_submission_preserves_outer_admission_ownership() {
    // session_id identifies the exact idle session owned across destructive rewrite and replay.
    // session_id 标识跨破坏性重写与重放持有的精确空闲会话。
    let session_id = SessionId::new_v4();
    // state owns retained workspace, admission rows, and the simulated runtime kernel.
    // state 持有保留工作区、准入行与模拟运行时内核。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(
        test_service_state_with_parent_session(session_id),
    ));
    // outer_guard retains the PromptSubmission row for the complete edit transaction.
    // outer_guard 在完整编辑事务期间保留 PromptSubmission 行。
    let outer_guard = begin_existing_prompt_submission_exclusion(&state, session_id, "编辑提示词")
        .expect("outer edit should reserve idle session");
    // authorization is a non-owning exact token whose lifetime remains bounded by outer_guard.
    // authorization 是一个无所有权的精确令牌，其生命周期仍受 outer_guard 约束。
    let authorization = outer_guard
        .existing_submission_authorization(session_id)
        .expect("outer guard should expose exact authorization");
    // preflight must accept only the already-current exact owner instead of inserting a second row.
    // preflight 必须只接受已经生效的精确所有者，而不是插入第二行。
    let preflight = prepare_retained_submission_preflight(
        &state,
        None,
        PromptSessionTarget::Existing(session_id),
        Some(authorization),
    )
    .expect("nested replay should accept outer authorization");

    // competitor_error proves an ordinary submission cannot reuse the outer transaction's reservation.
    // competitor_error 证明普通提交不能复用外层事务的预留。
    let competitor_error = prepare_retained_submission_preflight(
        &state,
        None,
        PromptSessionTarget::Existing(session_id),
        None,
    )
    .err()
    .expect("ordinary competitor must remain blocked");
    assert!(competitor_error.contains("user prompt submission"));
    // materialized_session represents the exact existing target authorized by the edit transaction.
    // materialized_session 表示编辑事务授权的精确已有目标。
    let materialized_session = MaterializedPromptSession {
        session_id,
        project_id: Some(String::from("project-a")),
        session_title: String::from("父会话"),
        workspace_path: String::new(),
        kind: PromptSessionMaterializationKind::Existing,
    };
    assert_eq!(
        preflight
            .turn_admission
            .kernel_reservation_id(&materialized_session)
            .expect("nested replay should match exact target"),
        Some(authorization.reservation_id)
    );

    drop(preflight);
    assert_eq!(
        state
            .lock()
            .expect("service state lock")
            .session_turn_admission_reservations
            .get(&session_id)
            .map(|reservation| reservation.reservation_id),
        Some(authorization.reservation_id)
    );
    drop(outer_guard);
    assert!(
        !state
            .lock()
            .expect("service state lock after outer release")
            .session_turn_admission_reservations
            .contains_key(&session_id)
    );
}

/// Verifies a stale submission owner cannot bypass or delete a replacement admission row.
/// 验证过期提交所有者无法绕过或删除替代准入行。
///
/// # Parameters
/// 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails when stale ownership can start or erase successor work.
/// 此测试无返回值；若过期所有权能够启动或清除后继工作则失败。
#[test]
fn stale_submission_admission_cannot_start_or_release_replacement() {
    // session_id identifies the exact existing session whose owner is replaced.
    // session_id 标识所有者被替换的精确已有会话。
    let session_id = SessionId::new_v4();
    // state owns both the stale guard and the replacement reservation row.
    // state 同时持有过期守卫与替代预留行。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(
        test_service_state_with_parent_session(session_id),
    ));
    // preflight captures the first reservation owner that will become stale.
    // preflight 捕获即将过期的首个预留所有者。
    let preflight = prepare_retained_submission_preflight(
        &state,
        None,
        PromptSessionTarget::Existing(session_id),
        None,
    )
    .expect("first submission should reserve admission");
    // materialized_session is the exact existing target matched by the stale guard.
    // materialized_session 是过期守卫匹配的精确已有目标。
    let materialized_session = MaterializedPromptSession {
        session_id,
        project_id: Some(String::from("project-a")),
        session_title: String::from("父会话"),
        workspace_path: String::new(),
        kind: PromptSessionMaterializationKind::Existing,
    };
    // stale_reservation_id is the original owner token retained outside shared state.
    // stale_reservation_id 是保留在共享状态之外的原始所有者令牌。
    let stale_reservation_id = preflight
        .turn_admission
        .kernel_reservation_id(&materialized_session)
        .expect("materialization should match stale admission")
        .expect("existing session should have admission");
    // replacement_reservation_id uniquely owns the successor row installed after the stale capture.
    // replacement_reservation_id 唯一拥有在过期捕获后安装的后继行。
    let replacement_reservation_id = Uuid::new_v4();
    {
        // guard replaces only the authoritative row, leaving the stale RAII owner alive for the test.
        // guard 只替换权威行，同时让过期 RAII 所有者继续存活以供测试。
        let mut guard = state.lock().expect("service state lock");
        guard.session_turn_admission_reservations.insert(
            session_id,
            SessionTurnAdmissionReservation {
                reservation_id: replacement_reservation_id,
                kind: SessionTurnAdmissionReservationKind::PromptSubmission,
            },
        );
        // error is the ownership rejection produced for the stale token.
        // error 是过期令牌产生的所有权拒绝。
        let error = start_reserved_webui_session_kernel_user_turn(
            &mut guard,
            session_id,
            "turn-stale",
            stale_reservation_id,
        )
        .expect_err("stale admission must not start a kernel turn");
        assert!(error.contains("user prompt submission"));
    }

    drop(preflight);

    // guard proves stale Drop cleanup retained the replacement row by exact owner identity.
    // guard 证明过期 Drop 清理依据精确所有者身份保留了替代行。
    let guard = state.lock().expect("service state lock");
    assert_eq!(
        guard
            .session_turn_admission_reservations
            .get(&session_id)
            .map(|reservation| reservation.reservation_id),
        Some(replacement_reservation_id)
    );
    assert!(!webui_session_kernel_has_active_turn(&guard, session_id));
}

/// Verifies an exact new-session project reservation blocks lifecycle transition until submission exit.
/// 验证精确的新会话项目预留会阻止生命周期转换，直到提交退出。
///
/// # Parameters
/// 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails if lifecycle work overtakes an owned project submission.
/// 此测试无返回值；若生命周期操作越过已有所有权的项目提交则失败。
#[test]
fn new_session_project_admission_blocks_lifecycle_transition_until_release() {
    // session_id supplies one idle child used by the lifecycle transition attempt.
    // session_id 提供一个供生命周期转换尝试使用的空闲子会话。
    let session_id = SessionId::new_v4();
    // state owns project-a and both project/session reservation tables.
    // state 持有 project-a 以及项目/会话预留表。
    let state: SharedServiceBridgeState =
        Arc::new(Mutex::new(initialized_project_a_service_state(session_id)));
    // preflight owns project-a across the simulated asynchronous submission window.
    // preflight 在模拟的异步提交窗口内拥有 project-a。
    let preflight = prepare_retained_submission_preflight(
        &state,
        Some("project-a"),
        PromptSessionTarget::Create,
        None,
    )
    .expect("new session should reserve its explicit project");

    // conflict proves project removal cannot cross the in-flight submission.
    // conflict 证明项目移除不能穿越进行中的提交。
    let conflict = begin_project_lifecycle_transition(&state, "project-a", [session_id], "移除")
        .err()
        .expect("lifecycle transition must respect prompt project ownership");
    assert!(conflict.contains("user prompt submission"));

    drop(preflight);

    // lifecycle_guard proves exact ownership release reopens the project transition.
    // lifecycle_guard 证明精确所有权释放后项目转换重新开放。
    let lifecycle_guard =
        begin_project_lifecycle_transition(&state, "project-a", [session_id], "移除")
            .expect("released project admission should allow lifecycle transition");
    drop(lifecycle_guard);
}

/// Verifies a new-session submission rejects an uninitialized retained workspace without disk fallback.
/// 验证新会话提交会拒绝未初始化的保留工作区，且不会回退磁盘加载。
///
/// # Parameters
/// 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails if new-session preflight reserves an unresolved project or accepts disk fallback.
/// 此测试无返回值；若新会话预检预留未解析项目或接受磁盘回退则失败。
#[test]
fn new_session_rejects_uninitialized_workspace_without_reservation_leak() {
    // state intentionally has no retained workspace, forcing project selection to remain unresolved.
    // state 刻意不含保留工作区，迫使项目选择保持未解析。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(
        initialized_project_a_service_state(SessionId::new_v4()),
    ));
    {
        // guard removes the startup workspace so preflight cannot infer a project identity.
        // guard 移除启动工作区，使预检无法推导项目身份。
        let mut guard = state.lock().expect("service state lock");
        guard.workspace_snapshot = None;
    }
    // error proves submission fails fast instead of invoking the historical full-workspace loader.
    // error 证明提交会快速失败，而不是调用历史全工作区加载器。
    let error =
        prepare_retained_submission_preflight(&state, None, PromptSessionTarget::Create, None)
            .err()
            .expect("draft submission should reject an uninitialized workspace");
    assert!(error.contains("工作区仍在初始化"));

    // lifecycle_guard proves rejected admission left no unresolved reservation behind.
    // lifecycle_guard 证明被拒绝的准入没有遗留未解析预留。
    let lifecycle_guard = begin_project_lifecycle_transition(&state, "project-any", [], "归档")
        .expect("rejected submission must not retain project admission");
    drop(lifecycle_guard);
}

/// Verifies exact project admission rejects a materialization result from another project.
/// 验证精确项目准入会拒绝来自其他项目的物化结果。
///
/// # Parameters
/// 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails if materialization can escape its reserved project.
/// 此测试无返回值；若物化能够逃逸其预留项目则失败。
#[test]
fn exact_project_admission_rejects_cross_project_materialization() {
    // state contains project-a as the explicit new-session target.
    // state 包含作为显式新会话目标的 project-a。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(
        initialized_project_a_service_state(SessionId::new_v4()),
    ));
    // preflight reserves only project-a.
    // preflight 仅预留 project-a。
    let preflight = prepare_retained_submission_preflight(
        &state,
        Some("project-a"),
        PromptSessionTarget::Create,
        None,
    )
    .expect("explicit project should reserve successfully");
    // materialized_session deliberately claims project-b to exercise ownership validation.
    // materialized_session 刻意声明 project-b，以验证所有权校验。
    let materialized_session = MaterializedPromptSession {
        session_id: SessionId::new_v4(),
        project_id: Some(String::from("project-b")),
        session_title: String::from("跨项目会话"),
        workspace_path: String::from("D:/project-b"),
        kind: PromptSessionMaterializationKind::Created,
    };

    // error is the explicit ownership mismatch rather than a compatibility fallback.
    // error 是显式所有权不匹配错误，而不是兼容性回退。
    let error = preflight
        .project_admission
        .validate_materialized_session(&materialized_session)
        .expect_err("materialization must remain inside the reserved project");
    assert!(error.contains("project-a"));
    assert!(error.contains("project-b"));
}

/// Verifies stale project-admission cleanup preserves a replacement owner for the same scope.
/// 验证过期项目准入清理会保留同一作用域的替代所有者。
///
/// # Parameters
/// 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails if stale cleanup removes a successor reservation.
/// 此测试无返回值；若过期清理删除后继预留则失败。
#[test]
fn stale_project_admission_cannot_release_replacement_owner() {
    // state contains project-a and the submission guard that will become stale.
    // state 包含 project-a 以及即将过期的提交守卫。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(
        initialized_project_a_service_state(SessionId::new_v4()),
    ));
    // preflight owns the first project-a reservation generation.
    // preflight 拥有第一代 project-a 预留。
    let preflight = prepare_retained_submission_preflight(
        &state,
        Some("project-a"),
        PromptSessionTarget::Create,
        None,
    )
    .expect("first project admission should succeed");
    // replacement_id identifies the newer authoritative owner.
    // replacement_id 标识更新的权威所有者。
    let replacement_id = Uuid::new_v4();
    {
        // guard replaces the exact row while the stale RAII owner remains alive.
        // guard 在过期 RAII 所有者仍存活时替换精确行。
        let mut guard = state.lock().expect("service state lock");
        guard.project_admission_reservations.insert(
            ProjectAdmissionScope::Exact(String::from("project-a")),
            ProjectAdmissionReservation {
                reservation_id: replacement_id,
                kind: ProjectAdmissionReservationKind::PromptSubmission,
            },
        );
    }

    drop(preflight);

    // guard proves stale cleanup retained the successor row by exact owner identity.
    // guard 证明过期清理依据精确所有者身份保留后继行。
    let guard = state.lock().expect("service state lock after drop");
    assert_eq!(
        guard
            .project_admission_reservations
            .get(&ProjectAdmissionScope::Exact(String::from("project-a")))
            .map(|reservation| reservation.reservation_id),
        Some(replacement_id)
    );
}

/// Verifies an implicit new-session submission pins the retained first project before asynchronous loading.
/// 验证隐式新会话提交会在异步加载前固定保留工作区的首个项目。
///
/// # Parameters
/// 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails if later materialization can reselect a different default project.
/// 此测试无返回值；若后续物化能够重新选择不同默认项目则失败。
#[test]
fn implicit_new_session_admission_pins_retained_default_project() {
    // state contains project-a as the authoritative first retained project.
    // state 包含作为权威首个保留项目的 project-a。
    let service_state = initialized_project_a_service_state(SessionId::new_v4());
    // state exposes the retained default and project reservation table under one lock.
    // state 在同一个锁下提供保留默认项目与项目预留表。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // preflight selects and owns the retained default before any asynchronous load can interleave.
    // preflight 在任何异步加载可能插入前选择并拥有保留默认项目。
    let preflight =
        prepare_retained_submission_preflight(&state, None, PromptSessionTarget::Create, None)
            .expect("implicit submission should reserve retained default project");

    assert_eq!(
        preflight.project_admission.exact_project_id(),
        Some("project-a")
    );
    let submission_slice = preflight
        .retained_workspace_snapshot
        .as_ref()
        .expect("new submission should own one bounded workspace slice");
    assert_eq!(submission_slice.task_projects.len(), 1);
    assert_eq!(submission_slice.task_projects[0].id, "project-a");
    assert!(submission_slice.task_projects[0].session_ids.is_empty());
    assert!(submission_slice.sessions.is_empty());
    assert!(submission_slice.session_messages.is_empty());
    assert!(submission_slice.session_runtime_state.is_empty());
}

/// Verifies committing a created bounded slice preserves every historical retained conversation.
/// 验证提交新建有界切片会保留全部历史保留会话。
///
/// # Parameters
/// 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails if the bounded commit replaces the authoritative workspace.
/// 此测试无返回值；若有界提交替换权威工作区则失败。
#[test]
fn created_submission_slice_commit_preserves_retained_history() {
    // historical_session_id identifies the row that must survive the new-session commit.
    // historical_session_id 标识必须在新会话提交后保留的历史行。
    let historical_session_id = SessionId::new_v4();
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(
        initialized_project_a_service_state(historical_session_id),
    ));
    let preflight = prepare_retained_submission_preflight(
        &state,
        Some("project-a"),
        PromptSessionTarget::Create,
        None,
    )
    .expect("initialized project should admit a new conversation");
    let mut submission_slice = preflight
        .retained_workspace_snapshot
        .clone()
        .expect("new-session preflight should own one bounded slice");
    let materialized_session = materialize_prompt_session(
        &mut submission_slice,
        Some("project-a"),
        PromptSessionTarget::Create,
        "New bounded conversation",
    )
    .expect("bounded slice should materialize one conversation");
    prepare_materialized_submission_session(&mut submission_slice, &materialized_session)
        .expect("created conversation should receive runtime defaults");

    commit_created_submission_workspace_slice(&state, &submission_slice, &materialized_session)
        .expect("bounded slice should merge into the retained workspace");

    let guard = state
        .lock()
        .expect("service state lock after bounded commit");
    let workspace = guard
        .workspace_snapshot
        .as_ref()
        .expect("retained workspace should remain initialized");
    assert!(
        workspace
            .sessions
            .iter()
            .any(|session| session.id == historical_session_id)
    );
    assert!(
        workspace
            .sessions
            .iter()
            .any(|session| session.id == materialized_session.session_id)
    );
    assert_eq!(workspace.sessions.len(), 2);
}

/// Verifies one browser submission identifier reaches visible and canonical user records together.
/// 验证一个浏览器提交标识会同时到达可见与规范用户记录。
///
/// # Parameters
/// 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails if either retained projection loses correlation metadata.
/// 此测试无返回值；若任一保留投影丢失关联元数据则失败。
#[test]
fn client_submission_identifier_is_attached_to_both_user_record_projections() {
    // session_id and message_id select the exact mirrored user record pair.
    // session_id 与 message_id 选择精确的镜像用户记录对。
    let session_id = SessionId::new_v4();
    let message_id = "visible-user-message";
    let mut service_state = test_service_state_with_parent_session(session_id);
    let workspace = service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace should exist");
    workspace.session_messages.insert(
        session_id,
        vec![
            SessionMessageEntry::new(SessionMessageKind::User, "", "hello")
                .with_runtime_metadata(message_id, "1000"),
        ],
    );
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state
        .conversation_state
        .records
        .push(vulcan_context::SessionConversationRecord {
            id: message_id.to_string(),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::new(),
            body: String::from("hello"),
            metadata: None,
            created_at: String::from("1000"),
        });
    workspace
        .session_runtime_state
        .insert(session_id, runtime_state);

    attach_client_submission_id_to_visible_user_record(
        workspace,
        session_id,
        Some(message_id),
        "submission-123",
        "new",
        1000,
    )
    .expect("both user projections should accept correlation metadata");

    let visible_identifier = workspace.session_messages[&session_id][0]
        .metadata
        .as_ref()
        .and_then(serde_json::Value::as_object)
        .and_then(|metadata| metadata.get("client_submission_id"))
        .and_then(serde_json::Value::as_str);
    let canonical_identifier = workspace.session_runtime_state[&session_id]
        .conversation_state
        .records[0]
        .metadata
        .as_ref()
        .and_then(serde_json::Value::as_object)
        .and_then(|metadata| metadata.get("client_submission_id"))
        .and_then(serde_json::Value::as_str);
    assert_eq!(visible_identifier, Some("submission-123"));
    assert_eq!(canonical_identifier, Some("submission-123"));
    assert_eq!(
        workspace.session_messages[&session_id][0]
            .metadata
            .as_ref()
            .and_then(serde_json::Value::as_object)
            .and_then(|metadata| metadata.get("client_submission_accepted_at_ms"))
            .and_then(serde_json::Value::as_str),
        Some("1000")
    );
}

/// Verifies slow submission diagnostics are sanitized, bounded, and visible to diagnostics surfaces.
/// 验证慢提交诊断经过净化、保持有界并可被诊断界面读取。
///
/// # Parameters
/// 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails if prompt-like delimiters leak or either buffer stays empty.
/// 此测试无返回值；若提示词式分隔符泄漏或任一缓冲区保持为空则失败。
#[test]
fn slow_submission_diagnostic_is_sanitized_and_retained_for_webui() {
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(
        test_service_state_with_parent_session(SessionId::new_v4()),
    ));

    retain_slow_submission_diagnostic(
        &state,
        "submission/with unsafe spaces",
        "new",
        "post ack/preparation",
        201,
    )
    .expect("slow diagnostic should be retained");

    let guard = state.lock().expect("service state lock after diagnostic");
    let startup_line = guard
        .startup
        .recent_logs
        .last()
        .expect("startup diagnostics should contain the slow stage");
    let workbench_line = guard
        .workbench
        .logs
        .last()
        .expect("workbench diagnostics should contain the slow stage");
    assert!(startup_line.contains("submissionwithunsafespaces"));
    assert!(startup_line.contains("stage=postackpreparation"));
    assert!(!startup_line.contains('/'));
    assert_eq!(workbench_line.source, "submission");
    assert_eq!(workbench_line.level, "warning");
}
