//! WebUI chained-turn dispatch after a parent turn reaches an idle point.
//! WebUI 父级回合到达空闲点后的链式回合分发逻辑。

use vulcan_service_api::ServiceEventEmitter;
use vulcan_session_core::v1::QueuedInputIdentity;
use vulcan_session_core::v1::collaboration::CollaborationRunControl;
use vulcan_session_core::v1::document::{
    ApiGroupCompletionReceipt, SessionDocument, SubagentReturnReceipt, TurnCompletionTransition,
    TurnOrigin, TurnSuccessor,
};
use vulcan_session_core::v1::event::{ApiCallStatus, ApiPurposeKind, EventBody, UserInputSource};
use vulcan_workbench_model::{PendingSessionPromptEntry, PendingSessionPromptSource, SessionId};
use vulcan_workspace_store::data_plane::HostRuntimeHandle;
use vulcan_workspace_store::workspace_store::{
    load_project_resource_catalog, locate_session_v1_storage,
};

use super::conversation_submission::exact_v1_dispatch_runtime_options;
use super::pending_prompt_services::rebuild_pending_prompt_panel_from_v1;
use super::pending_prompts::emit_pending_prompts_changed_delta;
use super::project_conversation_services::ExistingSubmissionAdmissionAuthorization;
use super::session_goal_lifecycle::{
    conversation_attachments_from_resource_ids, session_goal_dispatch_authority_from_v1_handle,
    session_goal_summary_from_v1_handle, session_v1_handle_from_state,
};
use super::session_runtime_registry_v1::{
    SessionFlushReason, SessionHandle, SessionMutationAdmission, SessionMutationChange,
    SessionMutationReceipt,
};
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
use std::sync::Arc;

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

/// Schedules the original business turn after a user message or a live result is accepted.
/// 用户消息或实时结果接纳后，调度原业务回合。
/// `app` publishes the new execution and `session_id` is the exact recipient. Work remains durable if busy.
/// `app` 发布新执行，`session_id` 为精确接收方。占用时工作仍可靠保留。
pub(crate) fn schedule_waiting_business_continuation(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<(), String> {
    // Count the wakeup from acceptance through admission so collaboration waits keep the recipient live.
    // 从接纳到准入持续登记唤醒，使协作等待保持对接收方的观察。
    let owner =
        crate::agent_collaboration::dispatch_owner::DispatchOwner::acquire(state, session_id)?;
    let app = app.clone();
    let state = state.clone();
    crate::spawn_service_task(async move {
        let _owner = owner;
        if let Err(error) = start_waiting_business_continuation(&app, &state, session_id).await {
            tracing::warn!(%session_id, %error, "Accepted input or task result remains pending");
        }
    });
    Ok(())
}

/// Returns whether the recipient began a same-turn continuation without displacing an active request.
/// 返回接收方是否开始同回合续跑，同时避免替换活动请求。
/// The service owner handles concurrent wakeups and the V1 ledger selects the actual pending work.
/// 服务所有者处理并发唤醒，V1 账本选择实际待处理工作。
pub(crate) async fn start_waiting_business_continuation(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<bool, String> {
    // The current process must own both dispatch runtimes and have released this session's physical execution.
    // 当前进程必须拥有两个分发运行时，且已释放本会话的物理执行。
    let (controller, agent) = {
        let guard = state.lock().map_err(|error| error.to_string())?;
        if guard.active_stream_tasks.contains_key(&session_id)
            || webui_session_has_active_run(&guard, session_id)
            || guard.workspace_snapshot.as_ref().is_some_and(|snapshot| {
                crate::runtime_workspace_helpers::session_has_pending_plan_artifact(
                    snapshot, session_id,
                )
            })
        {
            return Ok(false);
        }
        let Some(runtimes) = guard
            .controller_runtime
            .clone()
            .zip(guard.agent_runtime.clone())
        else {
            return Ok(false);
        };
        runtimes
    };
    // Protect queue publication from a concurrent submission's attachment and acknowledgement handoff.
    // 保护队列发布，避免与并发提交的附件和确认交接冲突。
    let Some(admission) =
        crate::project_conversation_services::try_begin_pending_prompt_dispatch_exclusion(
            state, session_id,
        )?
    else {
        return Ok(false);
    };
    // Read and promote through the canonical owner while the service reservation is held.
    // 持有服务预留期间，通过规范所有者读取并提升输入。
    let handle = session_v1_handle_from_state(state, session_id).await?;
    let receipt = publish_waiting_user_inputs(
        Arc::clone(&handle),
        crate::session_goal_lifecycle::current_v1_goal_unix_millis()?,
    )
    .await?;
    if let Some(receipt) = receipt {
        if let Err(error) =
            crate::pending_prompt_services::pending_prompt_attachment_workspace_path(
                state, session_id,
            )
            .and_then(|workspace_path| {
                crate::pending_prompt_services::reconcile_pending_prompt_attachment_leases(
                    &handle,
                    session_id,
                    &workspace_path,
                )
            })
        {
            tracing::warn!(%session_id, %error, "Dispatched queue attachment release remains pending");
        }
        // An unavailable projection must not strand canonical inputs that are ready to dispatch.
        // 投影不可用时，也不能搁置已可派发的规范输入。
        if let Err(error) =
            refresh_v1_pending_prompt_panel(app, state, &controller, session_id).await
        {
            tracing::warn!(%session_id, %error, "Pending prompt panel refresh failed");
        }
        // Request preparation takes its own exclusion and revalidates the exact receipt.
        // 请求准备取得自身排他预留，并重新校验精确回执。
        drop(admission);
        return start_exact_v1_pending_in_turn(
            app,
            state,
            &controller,
            &agent,
            session_id,
            receipt,
        )
        .await;
    }
    drop(admission);
    start_exact_v1_collaboration_continuation(app, state, &controller, &agent, session_id).await
}

/// Publishes inserted inputs or one eligible queue head under `handle` at `now_ms`.
/// 在 `handle` 中以 `now_ms` 发布插队输入或一条符合条件的队首消息。
/// A second wakeup reads the same durable events until a business API takes ownership.
/// 第二次唤醒读取相同的可靠事件，直到业务 API 接管。
pub(crate) async fn publish_waiting_user_inputs(
    handle: Arc<SessionHandle>,
    now_ms: i64,
) -> Result<Option<ApiGroupCompletionReceipt>, String> {
    let receipt = handle
        .mutate_durable_if_changed_async(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::InputAccepted,
            move |document| {
                let Some(mut receipt) = waiting_insertion_boundary(document) else {
                    return Ok(SessionMutationChange::Unchanged(None));
                };
                // Preserve the original input ids and ordered attachments through the existing transition.
                // 经既有转换保留原始输入标识与有序附件。
                let has_insertions = !document
                    .metadata
                    .input_state
                    .pending_in_turn_inputs
                    .is_empty();
                // Reuse the previous selection on a second wakeup. Ordinary inputs are consumed one at a time.
                // 重复唤醒复用先前的选择；普通排队每次只消费一条。
                let queue_head = if has_insertions || !receipt.promoted_input_seqs.is_empty() {
                    None
                } else {
                    waiting_queue_head(document, &receipt)
                };
                let changed = has_insertions || queue_head.is_some();
                if changed {
                    receipt
                        .promoted_input_seqs
                        .extend(document.apply_atomic_transition(|document| {
                            if let Some(identity) = queue_head {
                                document.promote_queued_input_to_pending_turn(identity, now_ms)?;
                            }
                            document.promote_pending_in_turn_inputs_after_api(now_ms)
                        })?);
                }
                let receipt = (!receipt.promoted_input_seqs.is_empty()).then_some(receipt);
                Ok(if changed {
                    SessionMutationChange::Changed(receipt)
                } else {
                    SessionMutationChange::Unchanged(receipt)
                })
            },
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(receipt.value)
}

/// Returns the first ordinary queue entry when only a child obligation keeps the business open.
/// 业务仅因子任务义务保持开放时，返回普通队列的第一条输入。
/// `receipt` identifies the successful API whose tool-free completion authorizes FIFO admission.
/// `receipt` 标识成功 API；该 API 无工具调用的终态授权按队列接纳。
fn waiting_queue_head(
    document: &SessionDocument,
    receipt: &ApiGroupCompletionReceipt,
) -> Option<QueuedInputIdentity> {
    // Read only the winning entry and its API group. A tool-loop boundary cannot consume ordinary input.
    // 只读取选中的队首及所属 API 组；工具循环边界不能消费普通输入。
    let first = document.metadata.input_state.queued_inputs.first()?;
    if document.completion_obligations(receipt.turn_id).open_child_task_ids.is_empty()
        || document.events().iter().rev()
            .skip_while(|event| event.seq >= receipt.stats_seq)
            .take_while(|event| !matches!(&event.event, EventBody::ApiStarted(api) if api.api_call_id == receipt.api_call_id))
            .any(|event| matches!(&event.event, EventBody::ToolCall(tool) if tool.api_call_id == receipt.api_call_id))
    {
        return None;
    }
    Some(QueuedInputIdentity {
        input_id: first.input_id,
        client_action_id: first.client_action_id.clone(),
    })
}

/// Reads the last successful business API and any unsent insertions in its still-open turn.
/// 读取仍开放回合的最后一次成功业务 API 及尚未发送的插队输入。
/// Returns no boundary during an active API, an auxiliary turn, or after explicit stop.
/// 活动 API、辅助回合或显式停止时不返回边界。
fn waiting_insertion_boundary(document: &SessionDocument) -> Option<ApiGroupCompletionReceipt> {
    // Only a running business turn that has reached a physical request boundary can accept this wakeup.
    // 只有已到达物理请求边界且仍运行的业务回合才能接纳此唤醒。
    let turn = document.active_turn().filter(|turn| {
        !turn.closed
            && !matches!(
                turn.origin,
                TurnOrigin::QuestionExecution | TurnOrigin::ContextMaintenance
            )
    })?;
    if document.active_api().is_some()
        || document.collaboration_run_control() != CollaborationRunControl::Running
    {
        return None;
    }
    // Use the last business terminal itself so an older success cannot mask a later failure.
    // 使用最后一次业务终态本身，防止更早的成功掩盖较新的失败。
    let (stats_seq, stats) = document.events().iter().rev().find_map(|event| {
        let EventBody::ApiStats(stats) = &event.event else {
            return None;
        };
        (event.turn_id == Some(turn.turn_id)
            && !matches!(
                stats.purpose,
                ApiPurposeKind::QuestionExecution | ApiPurposeKind::Compaction
            ))
        .then_some((event.seq, stats))
    })?;
    if stats.status != ApiCallStatus::Completed {
        return None;
    }
    // A previous wakeup can publish an input before admission. Its exact event still authorizes retry.
    // 前一次唤醒可能先发布输入再准入，其精确事件仍可授权重试。
    let mut promoted_input_seqs: Vec<_> = document
        .events()
        .iter()
        .rev()
        .take_while(|event| event.seq > stats_seq)
        .filter(|event| {
            event.turn_id == Some(turn.turn_id)
                && matches!(&event.event, EventBody::UserInput(input) if input.input_source == UserInputSource::PendingInTurn)
                && !document.business_input_dispatch_started(turn.turn_id, event.seq)
        })
        .map(|event| event.seq)
        .collect();
    promoted_input_seqs.reverse();
    Some(ApiGroupCompletionReceipt {
        api_call_id: stats.api_call_id,
        turn_id: turn.turn_id,
        stats_seq,
        promoted_input_seqs,
    })
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
    let started = match receipt.value.successor {
        TurnSuccessor::None => {
            match crate::agent_collaboration::wake_session_tasks_if_idle(
                app,
                state,
                controller_runtime,
                finished_session_id,
            )
            .await
            {
                Ok(true) => Ok(true),
                // Background jobs that finished during this turn wake the session only after
                // every other successor declined it.
                // 本回合期间结束的后台任务，仅在其他后继都未接手时才唤醒会话。
                task_wake => {
                    if let Err(error) = task_wake {
                        tracing::warn!(
                            "[webui-service] wake agent tasks failed: session={finished_session_id} error={error}"
                        );
                    }
                    crate::shell_job_continuation::maybe_start_shell_job_continuation_if_idle(
                        app,
                        state,
                        controller_runtime,
                        agent_runtime,
                        finished_session_id,
                    )
                    .await
                }
            }
        }
        TurnSuccessor::GoalContinuation {
            turn_id,
            context_seq,
        } => {
            async {
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
            .await
        }
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
    start_v1_queued_turn_with_admission(
        app,
        state,
        controller_runtime,
        agent_runtime,
        session_id,
        authority,
        None,
    )
    .await
}

/// Starts the exact queued turn using the caller's retained authorization when provided.
/// 如调用方仍持有授权，则使用该授权启动精确排程回合。
/// Service arguments identify the session and durable input. Returns whether preparation was accepted.
/// 服务参数标识会话和已保存输入。返回准备是否已接手。
pub(super) async fn start_v1_queued_turn_with_admission(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    authority: SessionQueuedDispatchAuthority,
    authorization: Option<ExistingSubmissionAdmissionAuthorization>,
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
        (authority.clone(), authorization),
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
