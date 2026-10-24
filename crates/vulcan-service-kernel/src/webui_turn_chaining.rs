//! WebUI chained-turn dispatch after a parent turn reaches an idle point.
//! WebUI 父级回合到达空闲点后的链式回合分发逻辑。

use vulcan_service_api::ServiceEventEmitter;
use vulcan_session_core::v1::document::{
    ApiGroupCompletionReceipt, SubagentReturnReceipt, TurnCompletionTransition, TurnSuccessor,
};
use vulcan_workbench_model::{PendingSessionPromptEntry, PendingSessionPromptSource, SessionId};
use vulcan_workspace_store::data_plane::HostRuntimeHandle;
use vulcan_workspace_store::workspace_store::{
    load_project_resource_catalog, locate_session_v1_storage,
};

use super::conversation_submission::exact_v1_dispatch_runtime_options;
use super::pending_prompt_services::rebuild_pending_prompt_panel_from_v1;
use super::pending_prompts::emit_pending_prompts_changed_delta;
use super::session_goal_lifecycle::{
    conversation_attachments_from_resource_ids, session_goal_dispatch_authority_from_v1_handle,
    session_goal_summary_from_v1_handle, session_v1_handle_from_state,
};
use super::session_runtime_registry_v1::SessionMutationReceipt;
use super::webui_prompt_lifecycle::{
    SessionQueuedDispatchAuthority, maybe_prepare_webui_goal_continuation,
    maybe_prepare_webui_pending_in_turn_continuation, maybe_prepare_webui_queued_successor,
    maybe_prepare_webui_subagent_return_continuation, maybe_prepare_webui_user_root,
    pending_in_turn_dispatch_authority_from_v1_handle, queued_dispatch_authority_from_v1_handle,
    subagent_return_dispatch_authority_from_v1_handle, user_root_dispatch_authority_from_v1_handle,
};
use super::{
    AgentRuntimeBridge, SharedServiceBridgeState, cache_session_goal_summary,
    current_unix_millis_text, current_unix_millis_value, webui_session_has_active_run,
    webui_session_kernel_has_active_turn, webui_trace, webui_trace_if_slow,
};

/// Starts one same-turn continuation selected from durable collaboration state after terminal admission opens.
/// 终态准入开放后，从可靠协作状态选定并启动一次同回合续跑。
/// Service arguments retain their original owners; `session_id` is the finished physical execution owner.
/// 服务参数保留原所有者；`session_id` 是刚结束物理执行的所有者。
/// Returns false when no exact business continuation remains, without creating a replacement turn.
/// 不再有精确业务续跑时返回 false，不创建替代回合。
pub(crate) async fn start_exact_v1_collaboration_continuation(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
) -> Result<bool, String> {
    let handle = session_v1_handle_from_state(state, session_id).await?;
    let authority = handle
        .read(|document| {
            if document.active_api().is_some() {
                return None;
            }
            document.pending_collaboration_continuation()
        })
        .map_err(|error| error.to_string())?;
    let Some(authority) = authority else {
        return Ok(false);
    };
    crate::webui_prompt_lifecycle::maybe_prepare_webui_collaboration_continuation(
        app,
        state,
        controller_runtime,
        agent_runtime,
        session_id,
        authority,
    )
    .await
}

/// Starts the next WebUI chained turn after one stream finishes.
/// 在单个执行流结束后启动下一条 WebUI 链式回合。
///
/// # Parameters
/// 参数
/// `app` emits workbench updates caused by chained turn startup.
/// `app` 发出链式回合启动产生的工作台更新。
/// `state` owns retained service state and runtime kernels.
/// `state` 持有保留服务状态与运行时内核。
/// `controller_runtime` loads workspace and goal state when retained state is insufficient.
/// `controller_runtime` 在保留状态不足时加载工作区与目标状态。
/// `agent_runtime` owns prompt-turn hook and provider configuration handles.
/// `agent_runtime` 持有提示词回合 hook 与 Provider 配置句柄。
/// `finished_session_id` identifies the session whose previous turn has finished.
/// `finished_session_id` 标识刚结束上一回合的会话。
///
/// # Returns
/// 返回值
/// `true` when the exact durable queued or Goal successor starts its physical dispatch.
/// 精确可靠的排队或目标后继启动物理分发时返回 `true`。
pub(super) async fn maybe_start_webui_turn_chain_after_finished(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    finished_session_id: SessionId,
    receipt: SessionMutationReceipt<TurnCompletionTransition>,
) -> bool {
    let receipt_revision = receipt.revision;
    let started =
        match receipt.value.successor {
            TurnSuccessor::None => {
                crate::agent_collaboration::wake_session_tasks_if_idle(
                    app,
                    state,
                    controller_runtime,
                    finished_session_id,
                )
                .await
            }
            TurnSuccessor::GoalContinuation {
                turn_id,
                context_seq,
            } => async {
                let handle = session_v1_handle_from_state(state, finished_session_id).await?;
                let authority = session_goal_dispatch_authority_from_v1_handle(
                    handle.as_ref(),
                    finished_session_id,
                )?
                .ok_or_else(|| {
                    String::from(
                        "The completed Turn receipt names a Goal successor that is no longer open.",
                    )
                })?;
                if authority.turn_id != turn_id || authority.context_seq != context_seq {
                    return Err(String::from(
                        "The open Goal successor differs from the durable Turn completion receipt.",
                    ));
                }
                maybe_prepare_webui_goal_continuation(
                    app,
                    state,
                    controller_runtime,
                    agent_runtime,
                    finished_session_id,
                    authority,
                )
                .await
            }
            .await,
            TurnSuccessor::QueuedUser {
                turn_id,
                input_id,
                client_action_id,
                input_seq,
            } => {
                async {
                    let handle = session_v1_handle_from_state(state, finished_session_id).await?;
                    let authority = queued_dispatch_authority_from_v1_handle(
                        handle.as_ref(),
                        turn_id,
                        input_id,
                        &client_action_id,
                        input_seq,
                    )?;
                    start_exact_v1_queued_turn(
                        app,
                        state,
                        controller_runtime,
                        agent_runtime,
                        finished_session_id,
                        authority,
                    )
                    .await
                }
                .await
            }
        };
    let started = match started {
        Ok(started) => started,
        Err(error) => {
            tracing::warn!(
                "[webui-service] dispatch exact V1 Turn successor failed: session={finished_session_id} revision={receipt_revision} error={error}"
            );
            false
        }
    };
    webui_trace(
        "stream_finished_v1_successor",
        format!("session={finished_session_id} revision={receipt_revision} started={started}"),
    );
    started
}

/// Rebuilds the visible queue from V1 and emits it only when it changed.
/// 从 V1 重建可见队列，仅在变化时发出。
///
/// Inserted rows stay until their inputs reach the conversation, which happens at API terminals
/// inside the stream; the end of each stream is where the panel catches up.
/// 已插入行保留到其输入进入会话为止，这发生在流内的 API 终态；每次流结束时面板随之更新。
pub(crate) async fn refresh_v1_pending_prompt_panel(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    let handle = session_v1_handle_from_state(state, session_id).await?;
    let locator = locate_session_v1_storage(controller_runtime, session_id.to_string().as_str())
        .await
        .map_err(|error| {
            format!("Failed to locate the pending queue project of `{session_id}`: {error}")
        })?;
    let project_directory = locator.project_directory();
    let catalog = load_project_resource_catalog(&project_directory).map_err(|error| {
        format!("Failed to load the pending queue resource catalog of `{session_id}`: {error}")
    })?;
    if rebuild_pending_prompt_panel_from_v1(
        state,
        session_id,
        handle.as_ref(),
        &project_directory,
        &catalog,
    )? {
        emit_pending_prompts_changed_delta(app, state, session_id)?;
    }
    Ok(())
}

/// Starts one exact open queued UserRoot Turn without consulting the legacy pending queue.
/// 不查询旧待发送队列，启动一个精确开放的排队 UserRoot 回合。
///
/// # Parameters
/// 参数
/// `authority` is copied only from the durable V1 UserInput event selected by its mother receipt.
/// `authority` 仅从母回执选定的可靠 V1 UserInput 事件复制。
///
/// # Returns
/// 返回值
/// Returns true when this call starts business or a later business API already covers the queued input.
/// 当前调用启动业务，或更晚的业务 API 已覆盖排队输入时返回 true。
pub(crate) async fn start_exact_v1_queued_turn(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    authority: SessionQueuedDispatchAuthority,
) -> Result<bool, String> {
    let handle = session_v1_handle_from_state(state, session_id).await?;
    let locator = locate_session_v1_storage(controller_runtime, session_id.to_string().as_str())
        .await
        .map_err(|error| format!("Failed to locate the project resource catalog of queued successor `{session_id}`: {error}"))?;
    let project_directory = locator.project_directory();
    let catalog = load_project_resource_catalog(&project_directory)
        .map_err(|error| format!("Failed to load the project resource catalog of queued successor `{session_id}`: {error}"))?;
    if rebuild_pending_prompt_panel_from_v1(
        state,
        session_id,
        handle.as_ref(),
        &project_directory,
        &catalog,
    )? {
        emit_pending_prompts_changed_delta(app, state, session_id)?;
    }
    // A queued successor can be acknowledged only by business after its exact input.
    // 只有精确输入之后的业务请求才能确认排队后继已经分发。
    let (active_turn, already_dispatched) = handle
        .read(|document| {
            (
                document.active_api().map(|api| api.turn_id),
                document.business_input_dispatch_started(authority.turn_id, authority.input_seq),
            )
        })
        .map_err(|error| format!("Failed to read the V1 queued dispatch active API: {error}"))?;
    if let Some(active_turn_id) = active_turn {
        if active_turn_id == authority.turn_id {
            return Ok(already_dispatched);
        }
        return Err(String::from(
            "Another V1 Turn already owns the active physical API.",
        ));
    }
    if already_dispatched {
        return Ok(true);
    }
    let attachments = conversation_attachments_from_resource_ids(
        &project_directory,
        &catalog,
        &authority.resource_ids,
    );
    let entry = PendingSessionPromptEntry {
        id: format!(
            "webui-v1-queued-successor-turn-{}",
            authority.turn_id.value()
        ),
        prompt: authority.text.clone(),
        hidden_model_guidance: None,
        preview: authority.text.clone(),
        attachments,
        model_reference: None,
        runtime_options: exact_v1_dispatch_runtime_options(handle.as_ref())?,
        source: PendingSessionPromptSource::UserFollowup,
        task_id: None,
        panel_entry_id: None,
        created_at: current_unix_millis_text(),
    };
    if maybe_prepare_webui_queued_successor(
        app,
        state,
        controller_runtime,
        agent_runtime,
        session_id,
        authority.clone(),
        entry,
    )
    .await?
    {
        return Ok(true);
    }
    let dispatched_after_attempt = handle
        .read(|document| {
            document.business_input_dispatch_started(authority.turn_id, authority.input_seq)
        })
        .map_err(|error| format!("Failed to recheck the V1 queued dispatch active API: {error}"))?;
    Ok(dispatched_after_attempt)
}

/// Starts the next physical API for Provider-visible inputs already committed inside the same logical Turn.
/// 为已经提交到同一逻辑回合内的 Provider 可见输入启动下一次物理 API。
///
/// # Parameters
/// 参数
/// `receipt` is the atomic API-terminal receipt that selected and appended every input seq.
/// `receipt` 是选定并追加全部输入 seq 的原子 API 终态回执。
///
/// # Returns
/// 返回值
/// Returns true when the exact continuation starts or durable history proves it already started.
/// 当精确续跑已启动，或可靠历史证明此前已启动时返回 true。
pub(crate) async fn start_exact_v1_pending_in_turn(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    receipt: ApiGroupCompletionReceipt,
) -> Result<bool, String> {
    let last_input_seq = receipt
        .promoted_input_seqs
        .last()
        .copied()
        .ok_or_else(|| String::from("The in-turn terminal receipt contains no input seq."))?;
    let handle = session_v1_handle_from_state(state, session_id).await?;
    let (active_api_turn, already_dispatched) = handle
        .read(|document| {
            let active_api_turn = document.active_api().map(|api| api.turn_id);
            let already_dispatched =
                document.business_input_dispatch_started(receipt.turn_id, last_input_seq);
            (active_api_turn, already_dispatched)
        })
        .map_err(|error| {
            format!("Failed to read the V1 in-turn continuation dispatch state: {error}")
        })?;
    if let Some(active_turn_id) = active_api_turn {
        if active_turn_id == receipt.turn_id {
            // Auxiliary occupancy cannot acknowledge the promoted business input.
            // 辅助占用不能确认已提升的业务输入。
            return Ok(already_dispatched);
        }
        return Err(String::from(
            "Another V1 Turn already owns the active physical API.",
        ));
    }
    if already_dispatched {
        return Ok(true);
    }

    let authority = pending_in_turn_dispatch_authority_from_v1_handle(handle.as_ref(), &receipt)?;
    let locator = locate_session_v1_storage(controller_runtime, session_id.to_string().as_str())
        .await
        .map_err(|error| format!("Failed to locate the project resource catalog of in-turn continuation `{session_id}`: {error}"))?;
    let project_directory = locator.project_directory();
    let catalog = load_project_resource_catalog(&project_directory)
        .map_err(|error| format!("Failed to load the project resource catalog of in-turn continuation `{session_id}`: {error}"))?;
    let resource_ids = authority
        .inputs
        .iter()
        .flat_map(|input| input.resource_ids.iter().copied())
        .collect::<Vec<_>>();
    let attachments =
        conversation_attachments_from_resource_ids(&project_directory, &catalog, &resource_ids);
    let prompt = authority
        .inputs
        .iter()
        .map(|input| input.text.as_str())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let entry = PendingSessionPromptEntry {
        id: format!(
            "webui-v1-pending-in-turn-{}-{}",
            authority.turn_id.value(),
            authority.stats_seq.value()
        ),
        prompt: prompt.clone(),
        hidden_model_guidance: None,
        preview: prompt,
        attachments,
        model_reference: None,
        runtime_options: exact_v1_dispatch_runtime_options(handle.as_ref())?,
        source: PendingSessionPromptSource::UserInterrupt,
        task_id: None,
        panel_entry_id: None,
        created_at: current_unix_millis_text(),
    };
    if maybe_prepare_webui_pending_in_turn_continuation(
        app,
        state,
        controller_runtime,
        agent_runtime,
        session_id,
        authority,
        entry,
    )
    .await?
    {
        return Ok(true);
    }
    let dispatched_after_attempt = handle
        .read(|document| document.business_input_dispatch_started(receipt.turn_id, last_input_seq))
        .map_err(|error| {
            format!("Failed to recheck the V1 in-turn continuation dispatch state: {error}")
        })?;
    Ok(dispatched_after_attempt)
}

/// Starts one exact open root-input Turn without consulting legacy messages or prompt queues.
/// 不查询旧消息或提示词队列，启动一个精确开放的根输入回合。
///
/// # Parameters
/// 参数
/// `input_seq` is the root user-input boundary published by the edit or fork mother transaction.
/// `input_seq` 是编辑或分叉母事务发布的根用户输入边界。
///
/// # Returns
/// 返回值
/// Returns true when the exact Turn has started, or when its durable history proves it already started.
/// 当精确回合已启动，或其可靠历史证明此前已经启动时返回 true。
pub(crate) async fn start_exact_v1_user_root_turn(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    input_seq: vulcan_session_core::v1::ids::Seq,
) -> Result<bool, String> {
    let handle = session_v1_handle_from_state(state, session_id).await?;
    let (turn_id, active_api_turn, already_dispatched) = handle
        .read(|document| {
            let turn = document
                .active_turn()
                .ok_or_else(|| String::from("The root input has no authoritative open V1 Turn."))?;
            if turn.closed
                || turn.origin != vulcan_session_core::v1::document::TurnOrigin::UserRoot
                || turn.start_seq != input_seq
            {
                return Err(String::from(
                    "The root input no longer owns the authoritative open V1 Turn.",
                ));
            }
            let already_dispatched =
                document.business_input_dispatch_started(turn.turn_id, input_seq);
            Ok((
                turn.turn_id,
                document.active_api().map(|api| api.turn_id),
                already_dispatched,
            ))
        })
        .map_err(|error| format!("Failed to read the V1 root input dispatch state: {error}"))??;
    if let Some(active_turn_id) = active_api_turn {
        if active_turn_id == turn_id {
            // An active auxiliary API occupies the slot without acknowledging the original task.
            // 活动辅助 API 占用执行位置，但不确认原任务已经分发。
            return Ok(already_dispatched);
        }
        return Err(String::from(
            "Another V1 Turn already owns the active physical API.",
        ));
    }
    if already_dispatched {
        return Ok(true);
    }

    let authority = user_root_dispatch_authority_from_v1_handle(handle.as_ref(), input_seq)?;
    let locator = locate_session_v1_storage(controller_runtime, session_id.to_string().as_str())
        .await
        .map_err(|error| format!("Failed to locate the project resource catalog of root input `{session_id}`: {error}"))?;
    let project_directory = locator.project_directory();
    let catalog = load_project_resource_catalog(&project_directory).map_err(|error| {
        format!("Failed to load the project resource catalog of root input `{session_id}`: {error}")
    })?;
    let attachments = conversation_attachments_from_resource_ids(
        &project_directory,
        &catalog,
        &authority.resource_ids,
    );
    let entry = PendingSessionPromptEntry {
        id: format!("webui-v1-user-root-turn-{}", authority.turn_id.value()),
        prompt: authority.text.clone(),
        hidden_model_guidance: None,
        preview: authority.text.clone(),
        attachments,
        model_reference: None,
        runtime_options: exact_v1_dispatch_runtime_options(handle.as_ref())?,
        source: PendingSessionPromptSource::UserFollowup,
        task_id: None,
        panel_entry_id: None,
        created_at: current_unix_millis_text(),
    };
    if maybe_prepare_webui_user_root(
        app,
        state,
        controller_runtime,
        agent_runtime,
        session_id,
        authority,
        entry,
    )
    .await?
    {
        return Ok(true);
    }
    let dispatched_after_attempt = handle
        .read(|document| document.business_input_dispatch_started(turn_id, input_seq))
        .map_err(|error| format!("Failed to recheck the V1 root input dispatch state: {error}"))?;
    Ok(dispatched_after_attempt)
}

/// Starts the parent physical continuation for one exact durable subagent return.
/// 为一条精确可靠的子代理回流启动父级物理续跑。
pub(crate) async fn start_exact_v1_subagent_return_turn(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    receipt: SubagentReturnReceipt,
) -> Result<bool, String> {
    let handle = session_v1_handle_from_state(state, session_id).await?;
    let (active_api_turn, already_dispatched) = handle
        .read(|document| {
            let active_api_turn = document.active_api().map(|api| api.turn_id);
            let already_dispatched = document.subagent_return_dispatch_started(&receipt);
            (active_api_turn, already_dispatched)
        })
        .map_err(|error| {
            format!("Failed to read the V1 subagent return dispatch state: {error}")
        })?;
    if already_dispatched {
        return Ok(true);
    }
    if let Some(active_turn_id) = active_api_turn {
        if active_turn_id == receipt.parent_turn_id {
            // The stream-finished owner receives this durable return after the physical API terminal boundary.
            // 流终态所有者会在物理 API 终态边界后接收这条可靠回流。
            webui_trace(
                "subagent_return_waits_for_owning_parent_api_terminal",
                format!(
                    "session={session_id} turn={} return_seq={}",
                    receipt.parent_turn_id.value(),
                    receipt.return_seq.value()
                ),
            );
            return Ok(false);
        }
        return Err(String::from(
            "Another V1 Turn already owns the active physical API before subagent return dispatch.",
        ));
    }
    // A duplicate receipt already covered by an API does not require reopening its old turn.
    // 已被 API 覆盖的重复回执不需要重新打开旧回合。
    let authority = subagent_return_dispatch_authority_from_v1_handle(handle.as_ref(), &receipt)?;
    if maybe_prepare_webui_subagent_return_continuation(
        app,
        state,
        controller_runtime,
        agent_runtime,
        session_id,
        authority,
    )
    .await?
    {
        return Ok(true);
    }
    let dispatched_after_attempt = handle
        .read(|document| document.subagent_return_dispatch_started(&receipt))
        .map_err(|error| {
            format!("Failed to recheck the V1 subagent return dispatch state: {error}")
        })?;
    Ok(dispatched_after_attempt)
}

/// Starts one hidden session-goal continuation turn when visible queues stay idle.
/// 当可见队列保持空闲时启动一个隐藏的会话目标续跑回合。
pub(super) async fn maybe_start_session_goal_continuation_if_idle(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
) -> Result<bool, String> {
    let continuation_prepare_started_at = current_unix_millis_value();
    let (has_active_run, has_active_kernel_turn) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        (
            webui_session_has_active_run(&guard, session_id),
            webui_session_kernel_has_active_turn(&guard, session_id),
        )
    };
    webui_trace_if_slow(
        "goal_continuation_prepare_slow",
        continuation_prepare_started_at,
        format!("session={session_id}"),
    );
    webui_trace(
        "goal_continuation_prepare_check",
        format!(
            "session={session_id} has_active_run={has_active_run} has_active_kernel_turn={has_active_kernel_turn}"
        ),
    );
    if has_active_run || has_active_kernel_turn {
        return Ok(false);
    }
    let handle = session_v1_handle_from_state(state, session_id).await?;
    let summary = session_goal_summary_from_v1_handle(handle.as_ref(), session_id)?;
    let persisted_goal_authority =
        session_goal_dispatch_authority_from_v1_handle(handle.as_ref(), session_id)?;
    let Some(summary) = summary else {
        cache_session_goal_summary(state, session_id, None)?;
        webui_trace("goal_continuation_no_goal", format!("session={session_id}"));
        return Ok(false);
    };
    if summary.status != vulcan_service_api::SessionGoalStatus::Active {
        cache_session_goal_summary(state, session_id, Some(summary))?;
        webui_trace(
            "goal_continuation_inactive_goal",
            format!("session={session_id}"),
        );
        return Ok(false);
    }
    let Some(persisted_goal_authority) = persisted_goal_authority else {
        cache_session_goal_summary(state, session_id, Some(summary))?;
        webui_trace(
            "goal_continuation_no_persisted_open_turn",
            format!("session={session_id}"),
        );
        return Ok(false);
    };
    cache_session_goal_summary(state, session_id, Some(summary))?;
    let started = maybe_prepare_webui_goal_continuation(
        app,
        state,
        controller_runtime,
        agent_runtime,
        session_id,
        persisted_goal_authority,
    )
    .await?;
    webui_trace(
        "goal_continuation_admitted",
        format!("session={session_id} started={started}"),
    );
    Ok(started)
}
