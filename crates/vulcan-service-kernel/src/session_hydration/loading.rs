//! Session hydration loading helpers.
//! 会话水合 loading 辅助逻辑。

use super::*;
use crate::segmented_session_directory_for_workspace_session;

pub(crate) async fn load_workspace_for_submission(
    controller_runtime: ControllerRuntimeHandle,
    session_id: Option<SessionId>,
) -> Result<WorkspaceSnapshot, String> {
    // Measures lightweight workspace index loading for prompt submission.
    // 衡量提示词提交时轻量工作区索引载入耗时。
    let workspace_load_started_at = current_unix_millis_value();
    let mut workspace_snapshot =
        vulcan_workspace_store::workspace_store::load_workspace_index_snapshot(
            controller_runtime.clone(),
        )
        .await?;
    webui_trace_if_slow(
        "submit_prompt_workspace_snapshot_load_slow",
        workspace_load_started_at,
        format!(
            "conversation_id={}",
            session_id
                .map(|session_id| session_id.to_string())
                .as_deref()
                .unwrap_or("<draft>")
        ),
    );
    if let Some(session_id) = session_id {
        // Measures selected session hydration when retained context cannot be reused.
        // 衡量无法复用保留上下文时的选中会话水合耗时。
        let hydration_started_at = current_unix_millis_value();
        let hydration = if let Some((project_id, created_at)) =
            session_project_created_at_for_hydration(&workspace_snapshot, session_id)
        {
            vulcan_workspace_store::workspace_store::load_session_hydration_snapshot_for_project_created_at(
                    controller_runtime.clone(),
                    &project_id,
                    &created_at,
                    session_id,
                    SELECTED_SESSION_TURN_LOAD_LIMIT,
                )
                .await
        } else {
            Ok(empty_session_hydration_snapshot(session_id))
        };
        match hydration {
            Ok(hydration) => {
                apply_submission_session_hydration(
                    &controller_runtime,
                    &mut workspace_snapshot,
                    session_id,
                    hydration,
                )?;
            }
            Err(error) if is_missing_segmented_session_file_error(&error) => {}
            Err(error) => return Err(error),
        }
        webui_trace_if_slow(
            "submit_prompt_session_hydration_slow",
            hydration_started_at,
            format!("session={session_id}"),
        );
    }
    Ok(workspace_snapshot)
}

/// Returns deterministic session location fields from one retained workspace snapshot.
/// 从保留的工作区快照返回确定性的会话定位字段。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` contains the retained session summaries.
/// `workspace_snapshot` 包含保留的会话摘要。
/// `session_id` selects the session to locate.
/// `session_id` 选择需要定位的会话。
///
/// # Returns
/// 返回值
/// Owned project id and raw created-at timestamp when both are present on the summary.
/// 当摘要同时具备项目标识与原始创建时间戳时返回它们的自有副本。
fn session_project_created_at_for_hydration(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Option<(String, String)> {
    workspace_session_summary_for_session(workspace_snapshot, session_id)
        .and_then(session_summary_project_and_created_at)
        .map(|(project_id, created_at)| (project_id.to_string(), created_at.to_string()))
}

/// Returns whether a retained WebUI snapshot can be used as the submit base snapshot.
/// 判断保留的 WebUI 快照能否作为本次提交的基础快照。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the in-memory snapshot retained by the service bridge.
/// `workspace_snapshot` 是服务桥保留的内存工作区快照。
/// `session_id` optionally selects an existing submitted conversation.
/// `session_id` 可选选择本次提交的既有会话。
///
/// # Returns
/// 返回值
/// `true` for a draft submission or when the existing session is active in the retained snapshot.
/// 草稿提交或既有会话在保留快照中处于活动列表时返回 `true`。
pub(crate) fn retained_submission_snapshot_is_usable(
    workspace_snapshot: &WorkspaceSnapshot,
    project_id: Option<&str>,
    session_id: Option<SessionId>,
) -> bool {
    match session_id {
        Some(session_id) => {
            workspace_session_summary_for_session(workspace_snapshot, session_id).is_some()
        }
        None => project_id.map_or_else(
            || !workspace_snapshot.task_projects.is_empty(),
            |project_id| workspace_snapshot_contains_active_project(workspace_snapshot, project_id),
        ),
    }
}

/// Loads the selected-session display hydration snapshot with a missing-file fallback.
/// 加载选中会话的显示层水合快照，并在分段文件缺失时回退为空快照。
pub(crate) async fn load_selected_session_display_hydration(
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<SessionHydrationSnapshot, String> {
    if let Some((project_id, created_at)) =
        session_project_created_at_for_hydration(workspace_snapshot, session_id)
    {
        vulcan_workspace_store::workspace_store::load_session_display_hydration_snapshot_for_project_created_at(
                controller_runtime,
                &project_id,
                &created_at,
                session_id,
                SELECTED_SESSION_TURN_LOAD_LIMIT,
            )
            .await
            .or_else(|error| {
                if is_missing_segmented_session_file_error(&error) {
                    Ok(empty_session_hydration_snapshot(session_id))
                } else {
                    Err(error)
                }
            })
    } else {
        Ok(empty_session_hydration_snapshot(session_id))
    }
}

/// Applies display hydration data to the selected workspace session.
/// 将显示层水合数据应用到选中的工作区会话。
pub(crate) fn apply_hydration_to_selected_workspace_snapshot(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    hydration: SessionHydrationSnapshot,
    task_runtime: Option<&Arc<TaskRuntime>>,
    live_listener_task_ids: &BTreeSet<SessionId>,
) -> Result<(), String> {
    let SessionHydrationSnapshot {
        messages,
        tool_detail_messages,
        compactions,
        runtime_state,
        ..
    } = hydration;
    let has_live_runtime_owner = webui_subagent_task_has_live_run_for_phase(
        workspace_snapshot
            .session_run_phases
            .get(&session_id)
            .copied(),
        task_runtime,
        live_listener_task_ids.contains(&session_id),
        session_id,
    );
    let mut messages = hydration_messages_with_tool_details(messages, tool_detail_messages);
    if !has_live_runtime_owner {
        normalize_orphaned_tool_request_messages(&mut messages);
    }
    workspace_snapshot
        .session_messages
        .insert(session_id, messages);
    workspace_snapshot
        .session_compactions
        .insert(session_id, compactions);
    if let Some(mut runtime_state) = runtime_state {
        if !has_live_runtime_owner {
            let now = current_unix_millis_value().to_string();
            normalize_orphaned_tool_request_records(&mut runtime_state, &now);
        }
        workspace_snapshot
            .session_runtime_state
            .insert(session_id, runtime_state);
    }
    // Display hydration may legally observe a newly created live session before provider dialogue context is persisted.
    // 显示层水合可能合法地观察到尚未持久化 Provider 对话上下文的新建实时会话。
    sync_hydrated_session_dialogue_ready_marker(workspace_snapshot, session_id);
    let hydrated_session_run_phases = workspace_snapshot.session_run_phases.clone();
    apply_persisted_task_mirror_statuses_to_workspace(
        controller_runtime,
        workspace_snapshot,
        session_id,
        |task_id| {
            webui_subagent_task_has_live_run_for_phase(
                hydrated_session_run_phases.get(&task_id).copied(),
                task_runtime,
                live_listener_task_ids.contains(&task_id),
                task_id,
            )
        },
    )?;
    Ok(())
}

/// Builds an empty hydration payload for a missing segmented session file.
/// 为缺失的分段会话文件构建空水合载荷。
pub(crate) fn empty_session_hydration_snapshot(session_id: SessionId) -> SessionHydrationSnapshot {
    SessionHydrationSnapshot {
        session_id,
        messages: Vec::new(),
        tool_detail_messages: Vec::new(),
        compactions: Vec::new(),
        runtime_state: None,
        earliest_ordinal: None,
        has_more_history: false,
    }
}

/// Hydrates the submitted retained session only when the retained snapshot lacks concrete context.
/// 仅在保留快照缺少具体上下文时水合本次提交的目标会话。
///
/// # Parameters
/// 参数
/// `state` owns process-local admission and execution registries used to distinguish live claims.
/// `state` 持有用于区分实时领取的进程内准入与执行注册表。
/// `controller_runtime` owns the workspace-store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `workspace_snapshot` is the retained WebUI workspace snapshot being submitted.
/// `workspace_snapshot` 是本次提交正在使用的保留 WebUI 工作区快照。
/// `session_id` selects the submitted existing session.
/// `session_id` 选择本次提交的既有会话。
///
/// # Returns
/// 返回值
/// `Ok(())` after the session either already had context or has been hydrated.
/// 当会话已具备上下文或已完成水合后返回 `Ok(())`。
pub(crate) async fn hydrate_retained_submission_session_if_needed(
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    if retained_submission_snapshot_has_session_context(workspace_snapshot, session_id) {
        return Ok(());
    }
    let hydration_started_at = current_unix_millis_value();
    let hydration_result = if let Some((project_id, created_at)) =
        session_project_created_at_for_hydration(workspace_snapshot, session_id)
    {
        vulcan_workspace_store::workspace_store::load_session_hydration_snapshot_for_project_created_at(
                controller_runtime.clone(),
                &project_id,
                &created_at,
                session_id,
                SELECTED_SESSION_TURN_LOAD_LIMIT,
            )
            .await
    } else {
        Ok(empty_session_hydration_snapshot(session_id))
    };
    match hydration_result {
        Ok(hydration) => {
            apply_submission_session_hydration(
                &controller_runtime,
                workspace_snapshot,
                session_id,
                hydration,
            )?;
        }
        Err(error) if is_missing_segmented_session_file_error(&error) => {}
        Err(error) => return Err(error),
    }
    ensure_hydrated_session_dialogue_context(workspace_snapshot, session_id)?;
    webui_trace_if_slow(
        "submit_prompt_retained_session_hydration_slow",
        hydration_started_at,
        format!("session={session_id}"),
    );
    Ok(())
}

/// Hydrates the selected session runtime context when the retained snapshot has no dialogue memory.
/// 当保留快照缺少对话记忆时，水合选中会话的运行态上下文。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the workspace-store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `workspace_snapshot` is the retained WebUI workspace snapshot that receives runtime memory.
/// `workspace_snapshot` 是接收运行态记忆的保留 WebUI 工作区快照。
/// `session_id` selects the session whose runtime JSONL should be hydrated.
/// `session_id` 选择需要从运行时 JSONL 水合的会话。
///
/// # Returns
/// 返回值
/// `Ok(())` after runtime context exists or a concrete hydration error has been reported.
/// 当运行态上下文已存在或已报告明确水合错误后返回 `Ok(())`。
pub(crate) async fn hydrate_selected_session_runtime_if_needed(
    state: &SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    if retained_submission_snapshot_has_session_context(workspace_snapshot, session_id) {
        return recover_interrupted_execution_retry_snapshot(
            state,
            &controller_runtime,
            workspace_snapshot,
            session_id,
        )
        .await;
    }
    let hydration_result = if let Some((project_id, created_at)) =
        session_project_created_at_for_hydration(workspace_snapshot, session_id)
    {
        vulcan_workspace_store::workspace_store::load_session_hydration_snapshot_for_project_created_at(
                controller_runtime.clone(),
                &project_id,
                &created_at,
                session_id,
                SELECTED_SESSION_TURN_LOAD_LIMIT,
            )
            .await
    } else {
        Ok(empty_session_hydration_snapshot(session_id))
    };
    match hydration_result {
        Ok(hydration) => {
            apply_selected_session_runtime_hydration(workspace_snapshot, session_id, hydration)?;
        }
        Err(error) if is_missing_segmented_session_file_error(&error) => {}
        Err(error) => return Err(error),
    }
    ensure_hydrated_session_dialogue_context(workspace_snapshot, session_id)?;
    recover_interrupted_execution_retry_snapshot(
        state,
        &controller_runtime,
        workspace_snapshot,
        session_id,
    )
    .await
}

/// Invalidates one hydrated retry pointer without making the owning conversation unavailable.
/// 在不使所属会话不可用的前提下失效一个已水合重试指针。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` owns the detached hydrated runtime state being reconciled.
/// `workspace_snapshot` 持有正在收敛的分离水合运行态。
/// `session_id` selects the exact retry owner.
/// `session_id` 选择精确的重试所有者。
/// `blocked_reason` is the stable machine reason projected through WebUI i18n.
/// `blocked_reason` 是通过 WebUI i18n 投影的稳定机器原因。
/// `error_summary` is the controlled non-secret support diagnostic.
/// `error_summary` 是受控的非敏感支持诊断。
///
/// # Returns
/// 返回值
/// Success after the reference and continuation eligibility cache are closed, or when no reference remains.
/// 引用与 Continue 资格缓存关闭后返回成功；引用已经不存在时也返回成功。
fn invalidate_hydrated_execution_retry_reference(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    blocked_reason: &str,
    error_summary: &str,
) -> Result<(), String> {
    // updatedAt is shared by the lightweight pointer and its mirrored runtime entry.
    // updatedAt 由轻量指针与其镜像运行态条目共同使用。
    let updated_at = current_unix_millis_value().to_string();
    // runtimeState is the unique detached owner selected by the authoritative session identity.
    // runtimeState 是由权威会话身份选定的唯一分离所有者。
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .ok_or_else(|| String::from("Recovered retry runtime state disappeared."))?;
    let Some(reference) = runtime_state.execution_retry_snapshot.as_mut() else {
        return Ok(());
    };
    // resolvedBlockedReason preserves an earlier, more specific safety fact when one already exists.
    // resolvedBlockedReason 在已有更具体安全事实时保留该事实。
    let resolved_blocked_reason = reference
        .blocked_reason
        .as_deref()
        .filter(|reason| !reason.trim().is_empty())
        .unwrap_or(blocked_reason)
        .to_string();
    reference.invalidate_for_safety_reason(
        resolved_blocked_reason,
        error_summary.to_string(),
        updated_at.clone(),
    );
    runtime_state.provider_call_ledger.continuation.eligible = false;
    runtime_state.updated_at = updated_at;
    Ok(())
}

/// Persists the exact hydrated runtime state after retry recovery changes its lightweight pointer.
/// 在重试恢复修改轻量指针后持久化精确的已水合运行态。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the segmented-session persistence route.
/// `controller_runtime` 持有分段会话持久化路由。
/// `workspace_snapshot` owns the detached runtime state selected for publication.
/// `workspace_snapshot` 持有准备发布的分离运行态。
/// `session_id` identifies the exact persisted session owner.
/// `session_id` 标识精确的持久化会话所有者。
///
/// # Returns
/// 返回值
/// Success only after the recovered runtime state is durable in the segmented session document.
/// 仅当恢复后的运行态已可靠写入分段会话文档时返回成功。
async fn persist_hydrated_execution_retry_runtime_state(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    // runtimeState is cloned so no workspace borrow crosses the asynchronous persistence boundary.
    // runtimeState 会被克隆，避免工作区借用跨越异步持久化边界。
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .cloned()
        .ok_or_else(|| String::from("Recovered retry runtime state disappeared."))?;
    // session_directory derives from the already-hydrated session summary without querying SQLite or scanning files.
    // session_directory 从已水合会话摘要派生，不查询 SQLite，也不扫描文件。
    let session_directory = segmented_session_directory_for_workspace_session(
        &controller_runtime.data_root,
        workspace_snapshot,
        session_id,
    )
    .ok_or_else(|| {
        format!("Recovered retry session `{session_id}` has no exact hydrated storage locator.")
    })?;
    vulcan_workspace_store::workspace_store::persist_single_session_runtime_state_update(
        session_directory,
        session_id,
        &runtime_state,
    )
    .await
}

/// Invalidates one hydrated retry pointer, then attempts to make that fail-closed state durable.
/// 失效一个已水合重试指针，然后尝试可靠保存该失败关闭状态。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the segmented-session persistence route.
/// `controller_runtime` 持有分段会话持久化路由。
/// `workspace_snapshot` receives the user-visible blocked state even if storage is unavailable.
/// `workspace_snapshot` 即使存储不可用也会接收用户可见的阻断状态。
/// `session_id` identifies the exact retry owner.
/// `session_id` 标识精确的重试所有者。
/// `blocked_reason` and `error_summary` are controlled fail-closed diagnostics.
/// `blocked_reason` 与 `error_summary` 是受控的失败关闭诊断。
///
/// # Returns
/// 返回值
/// Always preserves conversation availability after in-memory invalidation; persistence failure is logged for diagnosis.
/// 内存失效后始终保持会话可用；持久化失败会记录日志供诊断。
async fn invalidate_and_persist_hydrated_execution_retry_reference(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    blocked_reason: &str,
    error_summary: &str,
) -> Result<(), String> {
    invalidate_hydrated_execution_retry_reference(
        workspace_snapshot,
        session_id,
        blocked_reason,
        error_summary,
    )?;
    if let Err(error) = persist_hydrated_execution_retry_runtime_state(
        controller_runtime,
        workspace_snapshot,
        session_id,
    )
    .await
    {
        eprintln!(
            "[webui-service] hydrated execution retry invalidation persistence failed for session {session_id}: {error}"
        );
    }
    Ok(())
}

/// Returns whether the current process still owns admission or execution for one retry boundary.
/// 返回当前进程是否仍持有一个重试边界的准入或执行所有权。
///
/// # Parameters
/// 参数
/// `state` owns the process-local admission table, stream registry, and session kernels.
/// `state` 持有进程内准入表、流注册表与会话内核。
/// `session_id` identifies the exact retry-boundary owner.
/// `session_id` 标识精确的重试边界所有者。
///
/// # Returns
/// 返回值
/// `true` when a live owner requires hydration to defer all recovery mutations.
/// 当实时所有者要求 hydration 延后全部恢复性变更时返回 `true`。
fn execution_retry_has_live_process_owner(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<bool, String> {
    // guard provides one lock-consistent ownership snapshot across every process-local registry.
    // guard 为全部进程内注册表提供一个锁一致的所有权快照。
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    Ok(guard
        .session_turn_admission_reservations
        .contains_key(&session_id)
        || guard
            .session_terminal_persistence_owners
            .contains_key(&session_id)
        || guard.active_stream_tasks.contains_key(&session_id)
        || webui_session_kernel_has_active_turn(&guard, session_id))
}

/// Returns whether a hydrated retry pointer may republish Continue eligibility after refreezing.
/// 返回已水合重试指针在重冻后是否可以重新发布 Continue 资格。
///
/// # Parameters
/// 参数
/// `runtime_state` owns the canonical dialogue tail, terminal ledger, and open Provider calls.
/// `runtime_state` 持有规范对话尾部、终态账本与未闭合 Provider 调用。
/// `replay_safety_proven` reports the authoritative safety result of the newly persisted sidecar reference.
/// `replay_safety_proven` 表示新持久化 sidecar 引用的权威安全判定。
///
/// # Returns
/// 返回值
/// `true` only when the body is replay-safe, the exact presentation tail matches the ledger, the claim is released, and no Provider call remains open.
/// 仅当正文可安全重放、精确展示尾部与账本一致、领取已经释放且不存在未闭合 Provider 调用时返回 `true`。
fn hydrated_retry_continuation_is_eligible(
    runtime_state: &SessionRuntimeStateEntry,
    replay_safety_proven: bool,
) -> bool {
    // canonicalTailKind is derived from the authoritative record stream rather than the cached continuation label.
    // canonicalTailKind 从权威记录流推导，而不是信任 Continue 缓存标签。
    let canonical_tail_kind =
        crate::canonical_conversation_tail_kind(runtime_state).unwrap_or("empty");
    // continuationTailMatches detects presentation drift while replay safety remains sidecar-owned.
    // continuationTailMatches 检测展示漂移，而重放安全仍由 sidecar 决定。
    let continuation_tail_matches =
        crate::is_prompt_free_continuation_tail_kind(canonical_tail_kind)
            && runtime_state
                .provider_call_ledger
                .continuation
                .canonical_tail_kind
                .as_deref()
                == Some(canonical_tail_kind);
    replay_safety_proven
        && continuation_tail_matches
        && runtime_state
            .provider_call_ledger
            .continuation
            .claimed_at_ms
            .is_none()
        && runtime_state
            .provider_call_ledger
            .continuation
            .claimed_execution_id
            .is_none()
        && runtime_state
            .provider_call_ledger
            .continuation
            .expected_ledger_revision
            == runtime_state.provider_call_ledger.revision
        && runtime_state.provider_call_ledger.in_flight_call_count() == 0
}

/// Recovers a crash-interrupted claimed retry as a frozen, explicitly retryable snapshot.
/// 将因崩溃中断的已认领重试恢复为冻结且明确可重试的快照。
///
/// # Parameters
/// 参数
/// `state` proves whether a claimed pointer still has a live owner in the current process.
/// `state` 证明已领取指针在当前进程中是否仍有实时所有者。
/// `controller_runtime` owns sidecar and segmented-session persistence.
/// `controller_runtime` 持有 sidecar 与分段会话持久化能力。
/// `workspace_snapshot` is the detached hydrated candidate being reconciled.
/// `workspace_snapshot` 是正在协调的分离水合候选。
/// `session_id` identifies the exact retry owner.
/// `session_id` 标识精确的重试所有者。
///
/// # Returns
/// 返回值
/// Success after safe recovery, deliberate live-owner deferral, or fail-closed invalidation.
/// 在安全恢复、因实时所有者而延迟或失败关闭失效后返回成功。
async fn recover_interrupted_execution_retry_snapshot(
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    // sessionDirectory is resolved once from the retained main-session summary; recovery never enumerates sibling directories.
    // sessionDirectory 浠庝繚鐣欎富浼氳瘽鎽樿涓竴娆℃€цВ鏋愶紱鎭㈠杩囩▼姘镐笉鏋氫妇鍚岀骇鐩綍銆?
    let session_directory = segmented_session_directory_for_workspace_session(
        &controller_runtime.data_root,
        workspace_snapshot,
        session_id,
    )
    .ok_or_else(|| {
        format!(
            "Session `{session_id}` has no exact storage locator during retry recovery; directory scanning is disabled."
        )
    })?;
    // reference is the exact lightweight pointer loaded from this session's durable runtime state.
    // reference 是从当前会话可靠运行态加载的精确轻量指针。
    let reference = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| runtime_state.execution_retry_snapshot.clone());
    let Some(reference) = reference else {
        return Ok(());
    };
    if !matches!(
        reference.state,
        vulcan_workbench_model::ExecutionRetrySnapshotState::Frozen
            | vulcan_workbench_model::ExecutionRetrySnapshotState::Claimed
    ) {
        return Ok(());
    }
    if execution_retry_has_live_process_owner(state, session_id)? {
        // A live owner may still advance either a Frozen checkpoint or a Claimed retry; hydration must never publish a detached competing transition.
        // 实时所有者仍可能推进 Frozen 检查点或 Claimed 重试；hydration 绝不能发布分离且相互竞争的状态迁移。
        return Ok(());
    }
    if reference.schema_version != vulcan_workbench_model::EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION {
        return invalidate_and_persist_hydrated_execution_retry_reference(
            controller_runtime,
            workspace_snapshot,
            session_id,
            "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_VERSION_UNSUPPORTED",
            "The retained execution retry snapshot uses an unsupported strict schema version; automatic migration or fallback is disabled.",
        )
        .await;
    }
    // snapshot is validated for every executable Frozen or crash-orphaned Claimed pointer before UI projection.
    // snapshot 会在投影到界面前校验每个可执行 Frozen 或崩溃遗留 Claimed 指针。
    let mut snapshot = match vulcan_workspace_store::workspace_store::load_execution_retry_snapshot(
        &session_directory,
        session_id,
        &reference,
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            eprintln!("[webui-service] hydrated execution retry body validation failed: {error}");
            return invalidate_and_persist_hydrated_execution_retry_reference(
                controller_runtime,
                workspace_snapshot,
                session_id,
                crate::conversation_submission::EXECUTION_RETRY_SNAPSHOT_BODY_UNREADABLE_REASON,
                crate::conversation_submission::EXECUTION_RETRY_SNAPSHOT_BODY_UNREADABLE_SUMMARY,
            )
            .await;
        }
    };
    if reference.state == vulcan_workbench_model::ExecutionRetrySnapshotState::Frozen {
        return Ok(());
    }
    // claimedAttemptId is the exact cross-document owner that must match the persisted ledger claim.
    // claimedAttemptId 是必须与持久化账本领取一致的精确跨文档所有者。
    let claimed_attempt_id = match snapshot
        .claimed_attempt_id
        .as_deref()
        .filter(|attempt_id| !attempt_id.trim().is_empty())
    {
        Some(attempt_id) => attempt_id.to_string(),
        None => {
            return invalidate_and_persist_hydrated_execution_retry_reference(
                controller_runtime,
                workspace_snapshot,
                session_id,
                "retry_snapshot_transition_failed",
                "The crash-interrupted execution retry snapshot has no exact claimed attempt identity.",
            )
            .await;
        }
    };
    // currentRecoveryState is the post-sanitize continuation generation and canonical conversation
    // that jointly own restart reconciliation.
    // currentRecoveryState 是共同拥有重启收敛过程的净化后 Continue 代际与规范会话。
    let (current_generation, current_conversation_state, sanitized_claim_matches) =
        workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .map(|runtime_state| {
                // continuation is the fresh-process sanitizer result that supersedes the dead claim owner.
                // continuation 是取代失效领取所有者的新进程净化结果。
                let continuation = &runtime_state.provider_call_ledger.continuation;
                // terminalIdentityMatches accepts the exact orphaned claim identity unless a closed
                // Provider call is the stronger restart-interruption owner.
                // terminalIdentityMatches 接受精确孤立领取身份；若已闭合 Provider 调用是更强的
                // 重启中断所有者，则接受该精确重启分类。
                let terminal_identity_matches = match continuation.terminal_reason.as_deref() {
                    Some("continuation_interrupted_after_restart") => {
                        continuation.terminal_execution_id.as_deref()
                            == Some(claimed_attempt_id.as_str())
                    }
                    Some("interrupted_after_restart") => {
                        continuation.terminal_execution_id.is_some()
                    }
                    _ => false,
                };
                (
                    continuation.generation,
                    runtime_state.conversation_state.clone(),
                    snapshot
                        .generation
                        .checked_add(1)
                        .is_some_and(|generation| generation == continuation.generation)
                        && continuation.claimed_at_ms.is_none()
                        && continuation.claimed_execution_id.is_none()
                        && continuation.terminal_at_ms.is_some()
                        && continuation.invalidated_at_ms.is_none()
                        && continuation.expected_ledger_revision
                            == runtime_state.provider_call_ledger.revision
                        && terminal_identity_matches,
                )
            })
            .ok_or_else(|| String::from("Recovered retry runtime state disappeared."))?;
    if !sanitized_claim_matches {
        return invalidate_and_persist_hydrated_execution_retry_reference(
            controller_runtime,
            workspace_snapshot,
            session_id,
            "retry_snapshot_transition_failed",
            "The crash-interrupted execution retry snapshot does not match the exact fresh-process claim reconciliation boundary.",
        )
        .await;
    }
    // Restart reconciliation rebinds the refrozen body to the exact post-sanitize problem generation.
    // 重启收敛将重新冻结的正文绑定到净化后的精确问题代际。
    snapshot.generation = current_generation;
    // canonicalActionAdvanced consumes any supplement, tool result, or mailbox input committed
    // before the crash and selects the exact next safe action without replaying stale input.
    // canonicalActionAdvanced 消费崩溃前已提交的补充消息、工具结果或 mailbox 输入，并选择
    // 精确的下一安全动作，避免重放过期输入。
    let canonical_action_advanced =
        match crate::agent_stream::reconcile_interrupted_execution_retry_canonical_state(
            &mut snapshot,
            &current_conversation_state,
        ) {
            Ok(advanced) => advanced,
            Err(error) => {
                eprintln!(
                    "[webui-service] hydrated execution retry canonical reconciliation failed: {error}"
                );
                return invalidate_and_persist_hydrated_execution_retry_reference(
                controller_runtime,
                workspace_snapshot,
                session_id,
                "retry_snapshot_transition_failed",
                "The crash-interrupted execution retry snapshot could not reconcile its canonical progress.",
            )
            .await;
            }
        };
    snapshot.state = vulcan_workbench_model::ExecutionRetrySnapshotState::Frozen;
    snapshot.claimed_attempt_id = None;
    snapshot.updated_at = current_unix_millis_value().to_string();
    // Preserve a previously proven non-retryable or side-effect-unknown error; restart is not proof of safety.
    // 保留此前已证明不可重试或副作用未知的错误；重启本身不能证明安全。
    if !canonical_action_advanced
        && should_replace_restart_interruption_error(
            snapshot.error.as_ref(),
            &snapshot.pending_action,
        )
    {
        // dispatchOutcome remains unknown for a Provider action and is proven not-dispatched only for local preparation actions.
        // dispatchOutcome 对 Provider 动作保持未知，仅对本地准备动作证明为未分派。
        let dispatch_outcome = match &snapshot.pending_action {
            vulcan_workbench_model::ExecutionRetryPendingAction::ResumePreparation { .. }
            | vulcan_workbench_model::ExecutionRetryPendingAction::ExecuteToolBatch { .. }
            | vulcan_workbench_model::ExecutionRetryPendingAction::PrepareNextProviderRequest
            | vulcan_workbench_model::ExecutionRetryPendingAction::FinalizeAssistant => {
                vulcan_ai_contract::DispatchOutcome::NotDispatched
            }
            _ => vulcan_ai_contract::DispatchOutcome::Unknown,
        };
        snapshot.error = Some(vulcan_ai_contract::AiExecutionError {
            category: vulcan_ai_contract::AiErrorCategory::Cancelled,
            subcategory: Some(String::from("service_restart_interrupted")),
            stage: String::from("startup_recovery"),
            http_status: None,
            transport: None,
            websocket_close_code: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: snapshot
                .provider_route
                .as_ref()
                .map(|route| route.provider_id.clone()),
            entry_id: None,
            model_id: snapshot
                .provider_route
                .as_ref()
                .map(|route| route.model_id.clone()),
            message: String::from("The service restarted while the retry execution was claimed."),
            dispatch_outcome,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: true,
            retry_after: None,
        });
    }
    // updatedReference is published only after the complete refrozen body is durable.
    // updatedReference 仅在完整重冻正文可靠写入后发布。
    let updated_reference =
        match vulcan_workspace_store::workspace_store::persist_interrupted_execution_retry_snapshot(
            &session_directory,
            session_id,
            &snapshot,
        ) {
            Ok(updated_reference) => updated_reference,
            Err(error) => {
                eprintln!(
                    "[webui-service] hydrated execution retry refreeze persistence failed: {error}"
                );
                return invalidate_and_persist_hydrated_execution_retry_reference(
                    controller_runtime,
                    workspace_snapshot,
                    session_id,
                    "retry_snapshot_transition_failed",
                    "The crash-interrupted execution retry snapshot could not be safely refrozen.",
                )
                .await;
            }
        };
    // replaySafetyProven determines the safety half of the mirrored ledger cache from the authoritative refreshed pointer.
    // replaySafetyProven 根据权威刷新指针决定镜像账本缓存的安全性部分。
    let replay_safety_proven = updated_reference.replay_safety_proven();
    let updated_at = updated_reference.updated_at.clone();
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .ok_or_else(|| String::from("Recovered retry runtime state disappeared."))?;
    runtime_state.transient_execution_retry_snapshot = Some(std::sync::Arc::new(snapshot));
    runtime_state.execution_retry_snapshot = Some(updated_reference);
    // The mirrored cache is reopened only after the new body and every live ledger invariant agree.
    // 只有新正文与全部实时账本不变量一致后，才重新开启镜像缓存。
    runtime_state.provider_call_ledger.continuation.eligible =
        hydrated_retry_continuation_is_eligible(runtime_state, replay_safety_proven);
    runtime_state.updated_at = updated_at;
    if let Err(error) = persist_hydrated_execution_retry_runtime_state(
        controller_runtime,
        workspace_snapshot,
        session_id,
    )
    .await
    {
        eprintln!(
            "[webui-service] hydrated execution retry refreeze reference persistence failed for session {session_id}: {error}"
        );
        return invalidate_and_persist_hydrated_execution_retry_reference(
            controller_runtime,
            workspace_snapshot,
            session_id,
            "retry_snapshot_transition_failed",
            "The crash-interrupted execution retry reference could not be durably published.",
        )
        .await;
    }
    Ok(())
}

/// Returns whether restart may replace the retained error with a retryable interruption marker.
/// 判断重启是否可以用可重试的中断标记替换保留错误。
fn should_replace_restart_interruption_error(
    error: Option<&vulcan_ai_contract::AiExecutionError>,
    pending_action: &vulcan_workbench_model::ExecutionRetryPendingAction,
) -> bool {
    let action_can_prove_no_dispatch = matches!(
        pending_action,
        vulcan_workbench_model::ExecutionRetryPendingAction::ResumePreparation { .. }
            | vulcan_workbench_model::ExecutionRetryPendingAction::ExecuteToolBatch { .. }
            | vulcan_workbench_model::ExecutionRetryPendingAction::PrepareNextProviderRequest
            | vulcan_workbench_model::ExecutionRetryPendingAction::FinalizeAssistant
    );
    error.is_none_or(|error| {
        error.retryable
            && !error.semantic_output_observed
            && !error.side_effect_observed
            && (error.dispatch_outcome != vulcan_ai_contract::DispatchOutcome::Unknown
                || action_can_prove_no_dispatch)
    })
}

#[cfg(test)]
mod tests {
    use super::{
        execution_retry_has_live_process_owner, hydrated_retry_continuation_is_eligible,
        invalidate_hydrated_execution_retry_reference, should_replace_restart_interruption_error,
    };
    use crate::{
        SessionTerminalPersistenceOwner, SessionTurnAdmissionReservation,
        SessionTurnAdmissionReservationKind,
    };
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;
    use vulcan_ai_contract::{AiErrorCategory, AiExecutionError, DispatchOutcome};
    use vulcan_context::SessionConversationRecord;
    use vulcan_workbench_model::{
        ExecutionRetryPendingAction, ExecutionRetrySnapshotRef, ExecutionRetrySnapshotState,
        SessionId, SessionMessageKind, SessionMessageRecordKind, SessionRuntimeStateEntry,
    };

    /// Builds one initially executable lightweight retry pointer for hydration invalidation tests.
    /// 为水合失效测试构建一个初始可执行的轻量重试指针。
    ///
    /// # Returns
    /// 返回值
    /// A frozen current-schema pointer whose mirrored safety summary initially permits replay.
    /// 返回一个镜像安全摘要初始允许重放的当前 Schema 冻结指针。
    fn hydrated_safe_retry_reference() -> ExecutionRetrySnapshotRef {
        ExecutionRetrySnapshotRef {
            schema_version: vulcan_workbench_model::EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id: String::from("00000000-0000-0000-0000-000000000001"),
            generation: 1,
            state: ExecutionRetrySnapshotState::Frozen,
            content_sha256: String::from(
                "0000000000000000000000000000000000000000000000000000000000000000",
            ),
            content_bytes: 1,
            logical_execution_id: String::from("execution-1"),
            pending_action_label: String::from("resume_preparation"),
            provider_route: None,
            preparation_route_selection: Some(
                vulcan_workbench_model::ExecutionRetryPreparationRouteSelection {
                    active_provider_id: None,
                    active_connection_id: None,
                    active_model_id: None,
                },
            ),
            context_generation_id: None,
            canonical_dispatch_sha256: "0".repeat(64),
            retryable: true,
            requires_manual_confirmation: false,
            blocked_reason: None,
            claimed_attempt_id: None,
            error_summary: None,
            updated_at: String::from("1"),
        }
    }

    /// Verifies every process-local admission or terminal owner defers retry hydration recovery.
    /// 验证每个进程内准入或终态所有者都会延后重试水合恢复。
    #[test]
    fn retry_boundary_with_live_process_owner_defers_hydration_recovery() {
        // sessionId identifies the exact retry boundary whose process ownership is inspected.
        // sessionId 标识需要检查进程所有权的精确重试边界。
        let session_id = SessionId::new_v4();
        // state starts without any process-local execution owner for the retained session.
        // state 初始时不为保留会话持有任何进程内执行所有者。
        let state = Arc::new(Mutex::new(
            crate::tests::test_support::test_service_state_with_parent_session(session_id),
        ));

        assert!(
            !execution_retry_has_live_process_owner(&state, session_id)
                .expect("owner inspection should lock the test state")
        );

        // reservationId uniquely owns the admission interval before the live kernel is committed.
        // reservationId 唯一持有实时内核提交之前的准入区间。
        let reservation_id = Uuid::new_v4();
        state
            .lock()
            .expect("test state should remain lockable")
            .session_turn_admission_reservations
            .insert(
                session_id,
                SessionTurnAdmissionReservation {
                    reservation_id,
                    kind: SessionTurnAdmissionReservationKind::PromptSubmission,
                },
            );

        assert!(
            execution_retry_has_live_process_owner(&state, session_id)
                .expect("owner inspection should observe the admission reservation")
        );

        state
            .lock()
            .expect("test state should remain lockable")
            .session_turn_admission_reservations
            .remove(&session_id);
        // terminalOwner covers the post-stream interval where no task or kernel remains but settlement is still authoritative.
        // terminalOwner 覆盖流结束后已无任务或 kernel、但结算仍具权威性的区间。
        let terminal_owner = SessionTerminalPersistenceOwner::RunStop(Uuid::new_v4());
        state
            .lock()
            .expect("test state should remain lockable")
            .session_terminal_persistence_owners
            .insert(session_id, terminal_owner);

        assert!(
            execution_retry_has_live_process_owner(&state, session_id)
                .expect("owner inspection should observe terminal persistence")
        );
    }

    /// Verifies an unreadable hydrated body becomes a localized blocked token without removing the conversation.
    /// 验证不可读取的水合正文会变成可本地化阻断令牌，同时不移除会话。
    #[test]
    fn hydrated_retry_invalidation_keeps_conversation_available() {
        // sessionId identifies the exact retained conversation that must survive retry corruption.
        // sessionId 标识即使重试数据损坏也必须保留的精确会话。
        let session_id = SessionId::new_v4();
        // serviceState supplies a production-shaped workspace shell containing the owning session.
        // serviceState 提供包含所属会话的生产形态工作区外壳。
        let mut service_state =
            crate::tests::test_support::test_service_state_with_parent_session(session_id);
        // workspaceSnapshot is the detached hydration candidate mutated by the recovery helper.
        // workspaceSnapshot 是由恢复辅助函数变更的分离水合候选。
        let mut workspace_snapshot = service_state
            .workspace_snapshot
            .take()
            .expect("test workspace should exist");
        workspace_snapshot.session_runtime_state.insert(
            session_id,
            SessionRuntimeStateEntry {
                execution_retry_snapshot: Some(hydrated_safe_retry_reference()),
                ..SessionRuntimeStateEntry::default()
            },
        );

        invalidate_hydrated_execution_retry_reference(
            &mut workspace_snapshot,
            session_id,
            "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_BODY_UNREADABLE",
            "The retained retry body failed validation.",
        )
        .expect("hydration invalidation should keep the session usable");

        assert!(
            workspace_snapshot
                .sessions
                .iter()
                .any(|session| session.id == session_id)
        );
        let runtime_state = &workspace_snapshot.session_runtime_state[&session_id];
        let reference = runtime_state
            .execution_retry_snapshot
            .as_ref()
            .expect("invalidated diagnostic token should remain projected");
        assert_eq!(reference.state, ExecutionRetrySnapshotState::Invalidated);
        assert_eq!(
            reference.blocked_reason.as_deref(),
            Some("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_BODY_UNREADABLE")
        );
        assert!(!runtime_state.provider_call_ledger.continuation.eligible);
    }

    /// Verifies hydration accepts exact assistant/empty labels but rejects presentation-tail drift or unsafe replay bodies.
    /// 验证 hydration 接受精确的助手/空标签，但拒绝展示尾部漂移或不安全重放正文。
    #[test]
    fn hydrated_retry_eligibility_requires_exact_retryable_canonical_tail() {
        // runtimeState starts with one exact user tail and a matching terminal ledger cache.
        // runtimeState 初始持有一个精确用户尾部以及匹配的终态账本缓存。
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state
            .conversation_state
            .records
            .push(SessionConversationRecord {
                id: String::from("hydrated-user-tail"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::from("用户"),
                body: String::from("继续执行"),
                metadata: None,
                created_at: String::from("100"),
            });
        assert!(
            runtime_state
                .provider_call_ledger
                .mark_continuation_terminal(
                    Some(String::from("hydrated-execution")),
                    String::from("failed"),
                    100,
                    String::from("user"),
                    false,
                )
        );
        assert!(hydrated_retry_continuation_is_eligible(
            &runtime_state,
            true
        ));

        // assistantTail is a visible assistant-output record that supersedes the cached user tail.
        // assistantTail 是覆盖缓存用户尾部的可见助手输出记录。
        runtime_state
            .conversation_state
            .records
            .push(SessionConversationRecord {
                id: String::from("hydrated-assistant-tail"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("AI"),
                body: String::from("已经完成"),
                metadata: None,
                created_at: String::from("101"),
            });
        assert!(!hydrated_retry_continuation_is_eligible(
            &runtime_state,
            true
        ));
        // Matching the exact assistant presentation label restores eligibility without changing replay evidence.
        // 匹配精确的助手展示标签后，无需改变重放证据即可恢复资格。
        runtime_state
            .provider_call_ledger
            .continuation
            .canonical_tail_kind = Some(String::from("assistant"));
        assert!(hydrated_retry_continuation_is_eligible(
            &runtime_state,
            true
        ));

        runtime_state.conversation_state.records.clear();
        assert!(!hydrated_retry_continuation_is_eligible(
            &runtime_state,
            true
        ));
        // Matching the normalized empty label proves absence of a presentation row is not itself a safety failure.
        // 匹配规范化空标签后，证明缺失展示行本身并不是安全失败。
        runtime_state
            .provider_call_ledger
            .continuation
            .canonical_tail_kind = Some(String::from("empty"));
        assert!(hydrated_retry_continuation_is_eligible(
            &runtime_state,
            true
        ));
        assert!(!hydrated_retry_continuation_is_eligible(
            &runtime_state,
            false
        ));
    }

    /// Builds a structured error for restart-safety helper tests.
    /// 为重启安全辅助函数测试构建结构化错误。
    fn test_error(
        retryable: bool,
        dispatch_outcome: DispatchOutcome,
        semantic_output_observed: bool,
        side_effect_observed: bool,
    ) -> AiExecutionError {
        AiExecutionError {
            category: AiErrorCategory::ProviderUnavailable,
            subcategory: None,
            stage: String::from("provider_request"),
            http_status: None,
            transport: None,
            websocket_close_code: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: String::from("test"),
            dispatch_outcome,
            semantic_output_observed,
            side_effect_observed,
            terminal: true,
            retryable,
            retry_after: None,
        }
    }

    /// Verifies restart replacement is allowed only when the retained error proves replay safety.
    /// 验证仅在保留错误能够证明重放安全时才允许重启替换。
    #[test]
    fn restart_interruption_replacement_preserves_unsafe_errors() {
        // nextProviderRequest is a local preparation action whose previous Provider dispatch state is irrelevant.
        // nextProviderRequest 是本地准备动作，其先前 Provider 分派状态与恢复安全性无关。
        let next_provider_request = ExecutionRetryPendingAction::PrepareNextProviderRequest;
        // finalizeAssistant is also local and can safely replace an unknown-dispatch restart marker.
        // finalizeAssistant 同样属于本地动作，可以安全替换分派未知的重启标记。
        let finalize_assistant = ExecutionRetryPendingAction::FinalizeAssistant;
        // providerRequest represents the only action whose unknown transport dispatch must remain unsafe.
        // providerRequest 表示唯一必须把未知传输分派继续视为不安全的动作。
        let provider_request =
            serde_json::from_value::<vulcan_ai_contract::AiExecutionRequest>(serde_json::json!({
                "operation": "conversation_respond",
                "model_id": "restart-test-model"
            }))
            .expect("minimal Provider request fixture should decode");
        // unknownDispatchAction preserves the exact Provider request whose physical dispatch cannot be proven.
        // unknownDispatchAction 保留其物理分派无法证明的精确 Provider 请求。
        let unknown_dispatch_action = ExecutionRetryPendingAction::DispatchProviderRequest {
            request: Box::new(provider_request),
        };
        assert!(should_replace_restart_interruption_error(
            None,
            &next_provider_request
        ));
        assert!(should_replace_restart_interruption_error(
            Some(&test_error(true, DispatchOutcome::Dispatched, false, false)),
            &next_provider_request
        ));
        assert!(!should_replace_restart_interruption_error(
            Some(&test_error(
                false,
                DispatchOutcome::Dispatched,
                false,
                false
            )),
            &next_provider_request
        ));
        assert!(!should_replace_restart_interruption_error(
            Some(&test_error(true, DispatchOutcome::Unknown, false, false)),
            &unknown_dispatch_action
        ));
        assert!(should_replace_restart_interruption_error(
            Some(&test_error(true, DispatchOutcome::Unknown, false, false)),
            &finalize_assistant
        ));
        assert!(!should_replace_restart_interruption_error(
            Some(&test_error(true, DispatchOutcome::Dispatched, true, false)),
            &next_provider_request
        ));
        assert!(!should_replace_restart_interruption_error(
            Some(&test_error(true, DispatchOutcome::Dispatched, false, true)),
            &next_provider_request
        ));
    }
}

/// Applies a selected-session hydration snapshot without replacing an existing display window.
/// 应用选中会话水合快照，同时不替换已有显示窗口。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` receives the hydrated runtime context.
/// `workspace_snapshot` 接收水合后的运行态上下文。
/// `session_id` selects the hydrated session.
/// `session_id` 选择被水合的会话。
/// `hydration` contains runtime JSONL and optional display-window data.
/// `hydration` 包含运行时 JSONL 与可选显示窗口数据。
///
/// # Returns
/// 返回值
/// Applies the validated runtime context and hydration marker, or returns the exact replay identity failure.
/// 应用经过校验的运行态上下文与水合标记，或返回准确的重放身份失败。
pub(crate) fn apply_selected_session_runtime_hydration(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    hydration: SessionHydrationSnapshot,
) -> Result<(), String> {
    let SessionHydrationSnapshot {
        mut messages,
        mut tool_detail_messages,
        mut runtime_state,
        ..
    } = hydration;
    if let Some(runtime_state) = runtime_state.as_mut() {
        // Runtime-only hydration is performed before a new live owner is installed for this session.
        // 仅运行态水合发生在为该会话安装新的实时所有者之前。
        let now = current_unix_millis_value().to_string();
        normalize_orphaned_tool_request_records(runtime_state, &now);
    }
    normalize_orphaned_tool_request_messages(&mut messages);
    normalize_orphaned_tool_request_messages(&mut tool_detail_messages);
    match workspace_snapshot.session_messages.entry(session_id) {
        std::collections::btree_map::Entry::Vacant(entry) => {
            let messages = hydration_messages_with_tool_details(messages, tool_detail_messages);
            entry.insert(filter_legacy_goal_internal_user_messages(messages));
        }
        std::collections::btree_map::Entry::Occupied(mut entry) => {
            normalize_orphaned_tool_request_messages(entry.get_mut());
            append_unique_session_messages(entry.get_mut(), tool_detail_messages);
        }
    }
    if let Some(runtime_state) = runtime_state {
        workspace_snapshot
            .session_runtime_state
            .insert(session_id, runtime_state);
    }
    sync_hydrated_session_dialogue_ready_marker(workspace_snapshot, session_id);
    Ok(())
}

/// Applies one session hydration snapshot to the submit workspace snapshot.
/// 将单个会话水合快照应用到提交工作区快照。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the workspace-store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `workspace_snapshot` receives the hydrated messages and runtime state.
/// `workspace_snapshot` 接收已水合的消息与运行态。
/// `session_id` selects the hydrated session.
/// `session_id` 选择被水合的会话。
/// `hydration` contains the lazy-loaded session data.
/// `hydration` 包含懒加载出的会话数据。
///
/// # Returns
/// 返回值
/// `Ok(())` after hydration data and task mirror statuses have been applied.
/// 应用水合数据与任务镜像状态后返回 `Ok(())`。
pub(crate) fn apply_submission_session_hydration(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    hydration: SessionHydrationSnapshot,
) -> Result<(), String> {
    let SessionHydrationSnapshot {
        messages,
        tool_detail_messages,
        runtime_state,
        ..
    } = hydration;
    let messages = hydration_messages_with_tool_details(messages, tool_detail_messages);
    let mut messages = filter_legacy_goal_internal_user_messages(messages);
    normalize_orphaned_tool_request_messages(&mut messages);
    workspace_snapshot
        .session_messages
        .insert(session_id, messages);
    if let Some(mut runtime_state) = runtime_state {
        // Disk hydration is reached only when no retained live dialogue context owns this session.
        // 仅当没有保留的实时对话上下文拥有该会话时才会进入磁盘水合。
        let now = current_unix_millis_value().to_string();
        normalize_orphaned_tool_request_records(&mut runtime_state, &now);
        workspace_snapshot
            .session_runtime_state
            .insert(session_id, runtime_state);
    }
    ensure_hydrated_session_dialogue_context(workspace_snapshot, session_id)?;
    sync_hydrated_session_dialogue_ready_marker(workspace_snapshot, session_id);
    apply_persisted_task_mirror_statuses_to_workspace(
        controller_runtime,
        workspace_snapshot,
        session_id,
        |_| false,
    )
}

/// Combines a loaded display window with hidden tool-detail rows from the same load.
/// 将已加载显示窗口与同次加载得到的隐藏工具详情行合并。
///
/// # Parameters
/// 参数
/// `messages` are the primary loaded transcript rows for the visible window.
/// `messages` 是可见窗口对应的主要已加载转录行。
/// `tool_detail_messages` are rows required for expanding visible tool nodes.
/// `tool_detail_messages` 是展开可见工具节点所需的行。
///
/// # Returns
/// 返回值
/// A retained in-memory window containing both display rows and their hidden details.
/// 返回同时包含显示行及其隐藏详情的内存保留窗口。
fn hydration_messages_with_tool_details(
    mut messages: Vec<SessionMessageEntry>,
    tool_detail_messages: Vec<SessionMessageEntry>,
) -> Vec<SessionMessageEntry> {
    append_unique_session_messages(&mut messages, tool_detail_messages);
    messages
}

/// Appends session messages by stable identity without duplicating already retained rows.
/// 按稳定身份追加会话消息，避免重复已保留行。
///
/// # Parameters
/// 参数
/// `messages` is the retained in-memory message window to extend.
/// `messages` 是需要扩展的内存保留消息窗口。
/// `additional_messages` are candidate rows loaded for the same visible window.
/// `additional_messages` 是为同一可见窗口加载的候选行。
fn append_unique_session_messages(
    messages: &mut Vec<SessionMessageEntry>,
    additional_messages: Vec<SessionMessageEntry>,
) {
    let mut seen_keys = messages
        .iter()
        .map(SessionMessageEntry::stable_merge_key)
        .collect::<BTreeSet<_>>();
    for message in additional_messages {
        if seen_keys.insert(message.stable_merge_key()) {
            messages.push(message);
        }
    }
}
