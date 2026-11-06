//! Goal and Plan mode transitions, accounting, and recovery regression tests.
//! 目标与计划模式转换、记账及恢复回归测试。

use super::*;

/// Goal and Plan public transitions reject invalid states without changing any document field.
/// Goal 与 Plan 公开转换在无效状态下会拒绝，且不修改任何文档字段。
#[test]
fn goal_and_plan_transition_errors_are_failure_atomic() {
    let mut document = new_document();
    assert!(
        document
            .set_plan_mode(
                PlanModeTransitionInput::try_new(true, "enter plan").unwrap(),
                T0,
            )
            .expect("enable plan")
    );
    let before_create = document.clone();
    assert!(matches!(
        document.create_goal("goal", Some(10), "goal context", T0 + 1),
        Err(DomainError::ModeConflict { .. })
    ));
    assert_eq!(document, before_create);

    assert!(
        document
            .set_plan_mode(
                PlanModeTransitionInput::try_new(false, "leave plan").unwrap(),
                T0 + 2,
            )
            .expect("disable plan")
    );
    for transition in ["update", "clear", "pause", "resume"] {
        let before = document.clone();
        let result = match transition {
            "update" => document
                .update_goal_definition("goal", Some(10), "goal context", T0 + 3)
                .map(|_| ()),
            "clear" => document.clear_goal("goal removed", T0 + 3).map(|_| ()),
            "pause" => document.pause_goal("goal paused", T0 + 3).map(|_| ()),
            "resume" => document
                .resume_goal("goal", Some(10), "goal context", T0 + 3)
                .map(|_| ()),
            _ => unreachable!("transition table is closed"),
        };
        assert!(matches!(
            result,
            Err(DomainError::GoalEnablementMismatch { .. })
        ));
        assert_eq!(document, before);
    }
}

/// Goal create, update, pause, resume, and clear publish complete atomic fact groups.
/// Goal 的创建、更新、暂停、恢复与清除都发布完整的原子事实组。
#[test]
fn goal_lifecycle_transitions_publish_complete_fact_groups() -> Result<(), DomainError> {
    let mut document = new_document();
    let baseline_revision = document.memory_revision();
    let created = document.create_goal("goal one", Some(100), "goal one context", T0)?;
    assert!(created.continuation_turn_id.is_some());
    assert_eq!(document.metadata.mode.goal.state, GoalState::Active);
    assert_eq!(document.events().len(), 2);
    assert_eq!(document.memory_revision(), baseline_revision + 1);
    // Lifetime baselines survive definition and pause/resume transitions.
    // 生命周期基线跨目标编辑与暂停恢复保持不变。
    let mut usage_baseline = crate::v1::lifetime_usage::GoalUsageSnapshot::default();
    usage_baseline.main.tool_call_count = 7;
    document.metadata.mode.goal.usage_baseline = Some(usage_baseline.clone());
    assert_eq!(
        document.dirty_files().collect::<BTreeSet<_>>(),
        BTreeSet::from([DirtyFile::Metadata, DirtyFile::ActiveTail])
    );

    let unchanged_revision = document.memory_revision();
    assert!(
        document
            .update_goal_definition("goal one", Some(100), "ignored no-op context", T0 + 1)?
            .is_none()
    );
    assert_eq!(document.memory_revision(), unchanged_revision);

    let updated = document
        .update_goal_definition("goal two", Some(200), "goal two context", T0 + 2)?
        .expect("changed goal definition");
    assert!(updated.context_seq.is_some());
    assert_eq!(document.metadata.mode.goal.objective, "goal two");

    let before_empty_pause = document.clone();
    assert!(document.pause_goal("", T0 + 3).is_err());
    assert_eq!(document, before_empty_pause);
    document.pause_goal("goal paused", T0 + 3)?;
    assert_eq!(document.metadata.mode.goal.state, GoalState::Paused);
    document.resume_goal("goal two", Some(200), "goal resumed", T0 + 4)?;
    assert_eq!(document.metadata.mode.goal.state, GoalState::Active);
    assert_eq!(
        document.metadata.mode.goal.usage_baseline,
        Some(usage_baseline)
    );
    document.clear_goal("goal removed", T0 + 5)?;
    assert_eq!(document.metadata.mode.goal, GoalConfig::default());
    Ok(())
}

/// Verifies blocked goals remain stopped until explicit resume opens a continuation without another user prompt.
/// 验证受阻目标保持停止，直到显式恢复开启续跑，无需另一条用户提示词。
/// Parameters: none. Returns: success when the blocked, resume, and next-API boundaries satisfy the domain contract.
/// 参数：无。返回值：受阻、恢复与下一次 API 边界满足领域契约时成功。
#[test]
fn blocked_goal_resumes_from_play_into_a_new_continuation() -> Result<(), DomainError> {
    // document models a goal that blocks after its third step while preserving the completed history.
    // document 模拟第三步后受阻的目标，同时保留已完成历史。
    let mut document = new_document();
    document.create_goal("Finish five steps", Some(20_000), "Finish five steps", T0)?;
    // blocked_turn_id identifies the run that must not automatically continue after blocking.
    // blocked_turn_id 标识受阻后不得自动续跑的回合。
    let blocked_turn_id = document.active_turn().expect("created goal turn").turn_id;
    // api belongs to the physical request that reports the blocking condition.
    // api 属于报告阻碍条件的物理请求。
    let api = document.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    document.append_assistant_text(api, "Step 3/5", AssistantSemanticStatus::Completed, T0 + 2)?;
    document.block_goal("goal_blocked", T0 + 3)?;
    complete_ok(&mut document, api, None, T0 + 4)?;
    assert_eq!(
        document
            .complete_turn_and_select_successor(blocked_turn_id, T0 + 5)?
            .successor,
        TurnSuccessor::None,
    );
    assert_eq!(document.metadata.mode.goal.state, GoalState::Blocked);
    assert!(document.metadata.open_turn_start_seq.is_none());

    // blocked_snapshot proves an incorrect pause command remains rejected without erasing the blocked reason.
    // blocked_snapshot 证明错误暂停命令仍被拒绝，并且不会抹掉受阻原因。
    let blocked_snapshot = document.clone();
    assert!(document.pause_goal("pause again", T0 + 6).is_err());
    assert_eq!(document, blocked_snapshot);
    // resumed is the same transition selected by the WebUI's explicit active status request.
    // resumed 是 WebUI 显式请求 active 状态时选择的同一个转换。
    let resumed = document.resume_goal(
        "Finish five steps",
        Some(20_000),
        "Finish five steps",
        T0 + 7,
    )?;
    // continuation is a new goal-owned turn, never a replay of the completed user prompt.
    // continuation 是新的目标所属回合，绝不是重放已经完成的用户提示词。
    let continuation = document.active_turn().expect("resumed goal turn");
    assert_eq!(Some(continuation.turn_id), resumed.continuation_turn_id);
    assert_ne!(continuation.turn_id, blocked_turn_id);
    assert_eq!(continuation.origin, TurnOrigin::GoalContinuation);
    assert!(!continuation.closed);
    assert_eq!(document.metadata.mode.goal.state, GoalState::Active);
    assert_eq!(document.metadata.mode.goal.objective, "Finish five steps");
    assert_eq!(document.metadata.mode.goal.budget_tokens, Some(20_000));
    assert!(
        !document
            .events()
            .iter()
            .any(|event| matches!(event.event, EventBody::UserInput(_)))
    );
    assert!(
        document
            .start_api(
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                T0 + 8,
            )
            .is_ok()
    );
    Ok(())
}

/// Replays goal wall time across edits, pauses, blocked intervals, and a process restart without counting offline time.
/// 跨编辑、暂停、受阻及进程重启重放目标墙钟时长，不计入离线时间。
#[test]
fn goal_work_clock_survives_pause_block_and_restart() -> Result<(), DomainError> {
    use crate::v1::goal::goal_work_time;
    let mut document = new_document();
    document.create_goal("work", None, "work", T0)?;
    let origin = document.metadata.mode.goal.origin_seq;
    document.pause_goal("paused", T0 + 2_650)?;
    document.update_goal_definition("edited", None, "edited", T0 + 50_000)?;
    assert_eq!(goal_work_time(&document).unwrap().elapsed_ms, 2_650);
    document.resume_goal("edited", None, "edited", T0 + 100_000)?;
    document.block_goal("goal_blocked", T0 + 103_250)?;
    assert_eq!(goal_work_time(&document).unwrap().elapsed_ms, 5_900);
    document.resume_goal("edited", None, "edited", T0 + 200_000)?;
    assert_eq!(document.metadata.mode.goal.origin_seq, origin);
    // A real API-start fact supplies the final durable checkpoint before the crash.
    // 真实 API 开始事实提供闪退前最后一个持久化检查点。
    document.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 203_100,
    )?;
    document.project_persisted_interruptions()?;
    assert_eq!(document.metadata.mode.goal.state, GoalState::Paused);
    assert!(document.active_turn().is_none_or(|turn| turn.closed));
    assert_eq!(goal_work_time(&document).unwrap().elapsed_ms, 9_000);
    assert_eq!(document.project_persisted_interruptions()?, 0);
    // Loading much later still replays the stopped timestamp, not the time of reopening.
    // 很久之后加载仍重放停止时间戳，而不是重新打开的时间。
    let mut restored = SessionDocument::from_parts(
        document.metadata.clone(),
        initialized_runtime_lock(),
        Vec::new(),
        Vec::new(),
        document.events().to_vec(),
    )?;
    assert_eq!(goal_work_time(&restored).unwrap().elapsed_ms, 9_000);
    let resumed = restored.resume_goal("edited", None, "edited", T0 + 1_000_000)?;
    assert!(resumed.continuation_turn_id.is_some());
    restored.complete_goal(T0 + 1_001_000)?;
    assert_eq!(goal_work_time(&restored).unwrap().elapsed_ms, 10_000);
    Ok(())
}

/// Recovering between requests pauses a goal too, and closed-turn updates remain reloadable.
/// 请求间隙恢复也会暂停目标，已闭合回合后的状态更新仍可重载。
#[test]
fn goal_restart_between_requests_pauses_and_remains_reloadable() -> Result<(), DomainError> {
    let mut document = new_document();
    document.create_goal("work", None, "work", T0)?;
    assert_eq!(document.project_persisted_interruptions()?, 4);
    assert_eq!(document.metadata.mode.goal.state, GoalState::Paused);
    document.update_goal_definition("edited", None, "edited", T0 + 5)?;
    document.complete_goal(T0 + 10)?;
    document.clear_goal("clear", T0 + 15)?;
    SessionDocument::from_parts(
        document.metadata.clone(),
        initialized_runtime_lock(),
        Vec::new(),
        Vec::new(),
        document.events().to_vec(),
    )?;
    Ok(())
}

/// Restart pauses automatic Goal work without discarding its already-admitted user successor.
/// 重启暂停自动 Goal 工作，但不丢弃已经准入的用户后继。
/// Covers durable replay, exact original input ownership and repeated interruption projection.
/// 覆盖可靠重放、精确原输入归属及重复中断投影。
#[test]
fn goal_restart_preserves_admitted_user_successor() -> Result<(), DomainError> {
    // The real enqueue transaction closes only the old automatic continuation.
    // 真实排队事务只关闭旧自动续跑。
    let mut document = new_document();
    document.create_goal("Goal", None, "Continue", T0)?;
    document.enqueue_input(
        ClientInputSubmission::new(
            ClientActionId::try_new("restart-user")?,
            "Original user work",
            vec![],
        ),
        T0 + 1,
    )?;
    // Preserve source ownership across the startup projection and a complete reload.
    // 在启动投影及完整重载前后保留来源归属。
    let original = document.active_turn().unwrap().clone();
    assert_eq!(document.project_persisted_interruptions()?, 2);
    assert_eq!(document.metadata.mode.goal.state, GoalState::Paused);
    assert_eq!(document.active_turn(), Some(&original));
    assert_eq!(document.project_persisted_interruptions()?, 0);
    let restored = SessionDocument::from_parts(
        document.metadata.clone(),
        initialized_runtime_lock(),
        vec![],
        vec![],
        document.events().to_vec(),
    )?;
    assert_eq!(restored.active_turn(), Some(&original));
    Ok(())
}

/// Legacy paused goals whose origin was rewritten must not gain working time from unrelated metadata updates.
/// 旧版暂停期间改写过起点的目标，不得因无关元数据更新增加工作时长。
#[test]
fn legacy_paused_goal_clock_never_extrapolates() -> Result<(), DomainError> {
    let mut document = new_document();
    document.create_goal("work", None, "work", T0)?;
    document.pause_goal("paused", T0 + 1_000)?;
    let edited = document
        .update_goal_definition("edited", None, "edited", T0 + 100_000)?
        .unwrap();
    // Older resume/edit implementations moved origin_seq to the saved context even while paused.
    // 旧版恢复、编辑实现即使暂停也会将 origin_seq 移至保存上下文。
    document.metadata.mode.goal.origin_seq = edited.context_seq;
    document.metadata.updated_at_ms = T0 + 10_000_000;
    assert_eq!(
        crate::v1::goal::goal_work_time(&document)
            .unwrap()
            .elapsed_ms,
        0
    );
    Ok(())
}

/// Saving over a completed Goal creates a fresh active identity and resets its accounting origin.
/// 在已完成 Goal 上保存会创建新的活动身份，并重置其记账起点。
#[test]
fn save_goal_replaces_completed_generation_atomically() -> Result<(), DomainError> {
    let mut document = new_document();
    let first = document
        .save_goal("goal one", Some(100), "goal one context", T0)?
        .expect("first save must create a Goal");
    let first_origin_seq = document
        .metadata
        .mode
        .goal
        .origin_seq
        .expect("created Goal must own an origin seq");
    let first_turn_id = document
        .metadata
        .mode
        .goal
        .origin_turn_id
        .expect("created Goal must own an origin turn");
    assert_eq!(first.context_seq, Some(first_origin_seq));

    document.metadata.mode.goal.state = GoalState::Completed;
    document.metadata.mode.goal.budget_cache = Some(crate::v1::config::GoalBudgetCache {
        covered_until_seq: first.notice_seq,
        direct_total_tokens: 40,
        subagent_total_tokens: 2,
        total_tokens: 42,
        updated_at_ms: T0 + 1,
    });
    document
        .active_turn
        .as_mut()
        .expect("created Goal must own an active turn")
        .closed = true;
    document.metadata.open_turn_start_seq = None;

    let baseline_event_count = document.events().len();
    let baseline_revision = document.memory_revision();
    let replacement = document
        .save_goal("goal two", Some(200), "goal two context", T0 + 2)?
        .expect("completed Goal must be replaced even when save is otherwise valid");
    let replacement_origin_seq = document
        .metadata
        .mode
        .goal
        .origin_seq
        .expect("replacement Goal must own a new origin seq");
    let replacement_turn_id = document
        .metadata
        .mode
        .goal
        .origin_turn_id
        .expect("replacement Goal must own a new origin turn");

    assert_ne!(replacement_origin_seq, first_origin_seq);
    assert_ne!(replacement_turn_id, first_turn_id);
    assert_eq!(replacement.context_seq, Some(replacement_origin_seq));
    assert_eq!(replacement.continuation_turn_id, Some(replacement_turn_id));
    assert_eq!(document.metadata.mode.goal.objective, "goal two");
    assert_eq!(document.metadata.mode.goal.budget_tokens, Some(200));
    assert_eq!(document.metadata.mode.goal.state, GoalState::Active);
    assert!(document.metadata.mode.goal.budget_cache.is_none());
    assert_eq!(document.events().len(), baseline_event_count + 2);
    assert_eq!(document.memory_revision(), baseline_revision + 1);
    assert!(matches!(
        &document.events()[baseline_event_count].event,
        EventBody::SystemContext(SystemContextEvent {
            operation: SystemContextOperation::Upsert,
            content,
            ..
        }) if content == "goal two context"
    ));
    assert!(matches!(
        &document.events()[baseline_event_count + 1].event,
        EventBody::DisplayNote(DisplayNoteEvent { reason_code, .. }) if reason_code == "goal_saved"
    ));
    Ok(())
}

/// Every completed-Goal replacement error leaves the complete document unchanged.
/// 已完成 Goal 替换的任何错误都会保持完整文档不变。
#[test]
fn save_goal_completed_replacement_errors_are_failure_atomic() -> Result<(), DomainError> {
    let mut document = new_document();
    document
        .save_goal("goal one", Some(100), "goal one context", T0)?
        .expect("first save must create a Goal");
    document.metadata.mode.goal.state = GoalState::Completed;
    document
        .active_turn
        .as_mut()
        .expect("created Goal must own an active turn")
        .closed = true;
    document.metadata.open_turn_start_seq = None;

    let before_empty = document.clone();
    assert!(
        document
            .save_goal("   ", Some(200), "goal two context", T0 + 1)
            .is_err()
    );
    assert_eq!(document, before_empty);

    let (_, next_seq) = document.id_allocator.frontier();
    document.id_allocator = IdAllocator::from_frontier(JS_SAFE_INTEGER_MAX + 1, next_seq);
    let before_allocator_failure = document.clone();
    assert!(
        document
            .save_goal("goal two", Some(200), "goal two context", T0 + 2)
            .is_err()
    );
    assert_eq!(document, before_allocator_failure);
    Ok(())
}

/// Plan mode atomically couples metadata with its system semantic and remains idempotent.
/// Plan 模式原子绑定 metadata 与系统语义，同时保持幂等。
#[test]
fn set_plan_mode_is_atomic_idempotent_and_goal_exclusive() -> Result<(), DomainError> {
    let mut document = new_document();
    let baseline_revision = document.memory_revision();
    let before_empty = document.clone();
    assert!(PlanModeTransitionInput::try_new(true, "   ").is_err());
    assert_eq!(document, before_empty);
    assert!(document.set_plan_mode(PlanModeTransitionInput::try_new(true, "enter plan")?, T0)?);
    assert!(matches!(
        &document.events()[0].event,
        EventBody::SystemContext(SystemContextEvent {
            context_key,
            operation: SystemContextOperation::Upsert,
            scope: SystemContextScope::Session,
            provider_role_hint: Some(SystemRoleHint::Developer),
            content,
            ..
        }) if context_key == "plan_mode" && content == "enter plan"
    ));
    let entered_projection = crate::v1::projection::project_document(&document, None, &[])?;
    assert!(matches!(
        entered_projection.first(),
        Some(crate::v1::projection::ProjectedItem::UserInput { text, .. })
            if text.contains("<vulcan_context_update")
                && text.ends_with(">enter plan</vulcan_context_update>")
    ));
    assert_eq!(document.memory_revision(), baseline_revision + 1);
    assert_eq!(
        document.dirty_files().collect::<BTreeSet<_>>(),
        BTreeSet::from([DirtyFile::Metadata, DirtyFile::ActiveTail])
    );
    let unchanged_revision = document.memory_revision();
    assert!(!document.set_plan_mode(
        PlanModeTransitionInput::try_new(true, "duplicate enter")?,
        T0 + 1,
    )?);
    assert_eq!(document.memory_revision(), unchanged_revision);

    assert!(document.set_plan_mode(
        PlanModeTransitionInput::try_new(false, "leave plan")?,
        T0 + 2,
    )?);
    assert!(matches!(
        &document.events()[1].event,
        EventBody::SystemContext(SystemContextEvent {
            context_key,
            operation: SystemContextOperation::Remove,
            content,
            ..
        }) if context_key == "plan_mode" && content == "leave plan"
    ));
    let exited_projection = crate::v1::projection::project_document(&document, None, &[])?;
    assert!(exited_projection.iter().any(|item| {
        matches!(
            item,
            crate::v1::projection::ProjectedItem::UserInput { text, .. }
                if text.contains("<vulcan_context_expired")
                    && text.ends_with(">leave plan</vulcan_context_expired>")
        )
    }));
    document.create_goal("goal", None, "goal context", T0 + 3)?;
    let before_conflict = document.clone();
    assert!(matches!(
        document.set_plan_mode(
            PlanModeTransitionInput::try_new(true, "enter plan").unwrap(),
            T0 + 4,
        ),
        Err(DomainError::ModeConflict { .. })
    ));
    assert_eq!(document, before_conflict);

    let mut exhausted = new_document();
    exhausted.id_allocator = IdAllocator::from_frontier(1, JS_SAFE_INTEGER_MAX + 1);
    let before_capacity_error = exhausted.clone();
    assert!(
        exhausted
            .set_plan_mode(
                PlanModeTransitionInput::try_new(true, "enter plan").unwrap(),
                T0,
            )
            .is_err()
    );
    assert_eq!(exhausted, before_capacity_error);
    Ok(())
}

/// Goal completion rejects an in-flight API before appending any closure fact.
/// Goal 完成在追加任何闭合事实前拒绝仍在途的 API。
#[test]
fn goal_completion_with_active_api_is_non_mutating() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.metadata.mode.goal.enabled = true;
    doc.metadata.mode.goal.state = GoalState::Active;
    doc.metadata.mode.goal.objective = "objective".to_string();
    let turn_id = doc.submit_root_input_fixture("work", Vec::new(), T0)?;
    doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    let before = doc.clone();

    assert!(matches!(
        doc.complete_turn_and_select_successor(turn_id, T0 + 1),
        Err(DomainError::TurnCompletionWhileApiActive {
            turn_id: actual_turn_id,
            ..
        }) if actual_turn_id == turn_id.value()
    ));
    assert_eq!(doc, before);
    Ok(())
}

/// Goal creation requires its exact running tool when an API is active, preserving other mode gates.
/// API 活动期间，Goal 创建要求对应工具正在运行，同时保留其他模式闸门。
/// Parameters: none. Returns: domain errors; checks rejection atomicity and successful round-trip.
/// 参数：无。返回值：领域错误；检查拒绝原子性以及成功往返。
#[test]
fn create_goal_requires_running_native_tool_during_api() -> Result<(), DomainError> {
    // doc starts with a physical API but no native Goal tool authority.
    // doc 初始持有物理 API，但没有原生 Goal 工具权威。
    let mut doc = new_document();
    doc.submit_root_input_fixture("create a goal", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let before = doc.clone();
    assert!(matches!(
        doc.create_goal("goal", None, "goal", T0 + 2),
        Err(DomainError::ModeTransitionWhileApiActive { .. })
    ));
    assert_eq!(doc, before);
    let tool = doc.prepare_tool(
        api,
        RequestStreamId::try_new("goal-stream")?,
        "create_goal",
        ToolPayload::try_new(serde_json::json!({"objective": "goal"}))?,
        Some(String::from("goal-tool")),
        T0 + 3,
    )?;
    assert!(matches!(
        doc.create_goal("goal", None, "goal", T0 + 4),
        Err(DomainError::ModeTransitionWhileApiActive { .. })
    ));
    doc.start_prepared_tool(api, tool, T0 + 5)?;
    let transition = doc.create_goal("goal", Some(100), "goal", T0 + 6)?;
    assert!(transition.continuation_turn_id.is_none());
    assert_eq!(doc.active_api().unwrap().api_call_id, api);
    assert!(matches!(
        doc.pause_goal("paused", T0 + 7),
        Err(DomainError::ModeTransitionWhileApiActive { .. })
    ));
    doc.finish_tool(
        api,
        tool,
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"ok": true}))?),
            ToolCallStatus::Completed,
            T0 + 8,
        ),
    )?;
    advance_fixture_durable(&mut doc)?;
    let restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        doc.runtime_lock.clone(),
        doc.compactions.clone(),
        doc.segment_catalog.clone(),
        doc.events.to_vec(),
    )?;
    assert_eq!(restored.metadata.mode.goal, doc.metadata.mode.goal);
    Ok(())
}

/// Goal completion is atomic inside a tool API and suppresses the next continuation turn.
/// Goal 完成可在工具 API 内原子发生，并阻止创建下一续跑回合。
#[test]
fn complete_goal_inside_active_api_preserves_the_turn_and_tool_lifecycle() -> Result<(), DomainError>
{
    let mut doc = new_document();
    let turn_id = doc.submit_root_input_fixture("finish", Vec::new(), T0)?;
    doc.create_goal(
        "finish",
        None,
        "Continue until the goal is complete.",
        T0 + 1,
    )?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 2,
    )?;
    let tool = doc.prepare_tool(
        api,
        RequestStreamId::try_new("goal-complete-stream")?,
        "update_goal",
        ToolPayload::try_new(serde_json::json!({"status": "complete"}))?,
        Some(String::from("provider-goal-complete")),
        T0 + 3,
    )?;
    doc.start_prepared_tool(api, tool, T0 + 4)?;

    let transition = doc.complete_goal(T0 + 5)?.expect("active Goal changes");
    assert_eq!(doc.metadata.mode.goal.state, GoalState::Completed);
    assert_eq!(
        doc.active_api()
            .expect("tool API remains active")
            .api_call_id,
        api
    );
    assert_eq!(
        doc.active_turn().expect("turn remains open").turn_id,
        turn_id
    );
    assert!(!doc.active_turn().expect("turn remains open").closed);
    assert!(matches!(
        doc.events().iter().find(|event| event.seq() == transition.context_seq.unwrap()),
        Some(ConversationEvent {
            event: EventBody::SystemContext(SystemContextEvent {
                context_key,
                operation: SystemContextOperation::Remove,
                ..
            }),
            ..
        }) if context_key == "goal_state"
    ));

    doc.finish_tool(
        api,
        tool,
        tool_completion(
            Some(ToolPayload::try_new(
                serde_json::json!({"status": "complete"}),
            )?),
            ToolCallStatus::Completed,
            T0 + 6,
        ),
    )?;
    complete_ok(&mut doc, api, None, T0 + 7)?;
    let final_api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 8,
    )?;
    doc.append_assistant_text(
        final_api,
        "Goal completed.",
        AssistantSemanticStatus::Completed,
        T0 + 9,
    )?;
    complete_ok(&mut doc, final_api, None, T0 + 10)?;
    let closure = doc.complete_turn_and_select_successor(turn_id, T0 + 11)?;
    assert_eq!(closure.successor, TurnSuccessor::None);
    assert!(
        doc.active_turn()
            .expect("completed turn remains addressable")
            .closed
    );
    Ok(())
}

/// Completing a Goal keeps an already-dispatched continuation Turn open, but closes an
/// undispatched future continuation and remains idempotent.
/// 完成 Goal 时保留已派发的续跑回合，但关闭尚未派发的未来续跑，并保持幂等。
#[test]
fn complete_goal_distinguishes_running_and_undispatched_continuations() -> Result<(), DomainError> {
    let mut running = new_document();
    let created =
        running.create_goal("finish", None, "Continue until the goal is complete.", T0)?;
    let continuation = created.continuation_turn_id.expect("continuation turn");
    let api = running.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    running.complete_goal(T0 + 2)?.expect("state changes");
    assert_eq!(
        running
            .active_api()
            .expect("API remains active")
            .api_call_id,
        api
    );
    assert_eq!(
        running.active_turn().expect("turn remains open").turn_id,
        continuation
    );
    assert!(!running.active_turn().expect("turn remains open").closed);
    let before_noop = running.clone();
    assert_eq!(running.complete_goal(T0 + 3)?, None);
    assert_eq!(running, before_noop);

    let mut undispatched = new_document();
    undispatched.create_goal("finish", None, "Continue until the goal is complete.", T0)?;
    undispatched.complete_goal(T0 + 1)?.expect("state changes");
    assert!(
        undispatched
            .active_turn()
            .expect("closed turn remains addressable")
            .closed
    );
    assert_eq!(undispatched.metadata.open_turn_start_seq, None);

    let mut exhausted = new_document();
    exhausted.submit_root_input_fixture("finish", Vec::new(), T0)?;
    exhausted.create_goal(
        "finish",
        None,
        "Continue until the goal is complete.",
        T0 + 1,
    )?;
    let (next_local_id, _) = exhausted.id_allocator.frontier();
    exhausted.id_allocator = IdAllocator::from_frontier(next_local_id, JS_SAFE_INTEGER_MAX + 1);
    let before_failure = exhausted.clone();
    assert!(exhausted.complete_goal(T0 + 2).is_err());
    assert_eq!(exhausted, before_failure);
    Ok(())
}

/// Toggling Plan mode after a turn closed records a session-level change instead of failing.
/// 回合关闭后切换 Plan 模式记录会话级变更，而不是失败。
#[test]
fn set_plan_mode_after_closed_turn_is_session_scoped() -> Result<(), DomainError> {
    let mut document = new_document();
    document.submit_root_input_fixture("Original work", vec![], T0)?;
    document.close_turn(TurnOutcome::Completed, 1, T0 + 1)?;
    assert!(document.set_plan_mode(
        PlanModeTransitionInput::try_new(true, "enter plan")?,
        T0 + 2
    )?);
    let last = document.events().last().expect("plan event");
    assert_eq!(last.turn_id, None);
    assert!(document.metadata.mode.plan.enabled);
    Ok(())
}
