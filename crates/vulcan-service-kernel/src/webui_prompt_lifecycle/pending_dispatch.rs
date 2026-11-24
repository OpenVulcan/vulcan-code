//! WebUI prompt lifecycle pending dispatch helpers.
//! WebUI 提示词生命周期 pending dispatch 辅助逻辑。

use super::*;
use crate::conversation_submission::queued_dispatch::dispatch_accepted_queued_turn;
use crate::notify_webui_wait_agent_parent_interruption_if_waiting;
use crate::pending_prompt_services::{
    pending_prompt_action_id, pending_prompt_attachment_workspace_path,
    pending_prompt_queue_identity, rebuild_pending_prompt_panel_from_v1,
    reconcile_pending_prompt_attachment_leases,
};
use crate::session_goal_lifecycle::{current_v1_goal_unix_millis, session_v1_handle_from_state};
use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionHandle, SessionMutationAdmission, SessionMutationChange,
    SessionRuntimeError,
};
use crate::terminal_settlement_service::ensure_session_terminal_settlement_settled;
use vulcan_session_core::v1::{
    ClientActionId, DomainError, IdempotentMutation, QueuedInputIdentity, TurnId,
};
use vulcan_workspace_store::workspace_store::load_project_resource_catalog;

// STOPPED_INSERTION_ERROR explains why the exact queue entry remains available.
// STOPPED_INSERTION_ERROR 说明精确队列条目为何保持可用。
const STOPPED_INSERTION_ERROR: &str = "The conversation is stopped. Continue it before inserting a message; the message remains queued.";

/// Describes the disposition of the exact queue row selected by the user.
/// 描述用户所选精确队列行的处置。
enum PendingPromptInsertion {
    /// The action owns neither a queued item nor a scheduled root.
    /// 该动作既无队列项，也无已调度根回合。
    Missing,
    /// The input was accepted into an open turn.
    /// 输入已接纳到开放回合。
    InTurn(QueuedInputIdentity),
    /// A fresh or already admitted root needs dispatch.
    /// 新建或已接纳的根回合需要派发。
    NewTurn(QueuedPromptRoot),
}

/// Identifies the admission state of a queued prompt that opens a user turn.
/// 标识开启用户回合的排队消息准入状态。
enum QueuedPromptRoot {
    /// The original queue identity must still be admitted as a fresh turn.
    /// 原队列身份仍需准入为新回合。
    Queued(QueuedInputIdentity),
    /// The original turn is already recorded in the journal.
    /// 原回合已记录在日志中。
    Scheduled(TurnId),
}

/// Guides the current business turn or sends the selected queue row in a fresh turn when idle.
/// 引导当前业务回合，或在空闲时用所选队列行开启新回合。
/// A manual summary retains the queue entry until successful settlement authorizes dispatch.
/// 手动摘要期间保留排队条目，直到成功结算授权分发。
///
/// # Parameters
/// 参数
/// `app` emits pending-prompt and dispatch deltas.
/// `app` 发出待发送队列与分发增量。
/// `state` owns retained service state and runtime handles.
/// `state` 持有保留的服务状态与运行时句柄。
/// `entry_id` selects the visible queue row to insert.
/// `entry_id` 选择要插入的可见队列行。
///
/// # Returns
/// 返回值
/// `true` when the prompt was accepted for insertion or dispatch.
/// 提示词已被接受插入或派发时返回 `true`。
pub(crate) async fn service_insert_pending_prompt(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    entry_id: String,
) -> Result<bool, String> {
    // session_id freezes the selection used by the existing queue-row action.
    // session_id 固定现有队列行操作使用的会话选择。
    let session_id = selected_session_id_from_state(&state)?;
    insert_pending_prompt_for_session(app, state, session_id, entry_id).await
}

/// Guides or sends the exact queued entry in its owning session, independent of selection.
/// 在精确队列项的所属会话中引导或发送，不依赖当前选中会话。
/// `app` emits deltas, `state` holds the runtime, and `session_id` and `entry_id` identify the owner and action.
/// `app` 发出增量，`state` 持有运行时，`session_id` 和 `entry_id` 标识所属会话和操作。
/// Returns true on acceptance, including deferred insertion during manual compaction.
/// 接受插队时返回 true，包括手动压缩期间延后插入的情况。
pub(crate) async fn insert_pending_prompt_for_session(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    session_id: SessionId,
    entry_id: String,
) -> Result<bool, String> {
    let client_action_id = pending_prompt_action_id(&entry_id)?;
    // Retry a retained terminal settlement before accepting another user action for this session.
    // 接纳该会话的另一个用户动作前，先重试遗留的终态结算。
    ensure_session_terminal_settlement_settled(&state, session_id).await?;
    let handle = session_v1_handle_from_state(&state, session_id).await?;
    {
        // Hold the same service lock as compaction settlement so an accepted request cannot miss the handoff.
        // 持有与压缩结算相同的服务锁，使已接收请求不会错过交接。
        let mut guard = state.lock().map_err(|error| error.to_string())?;
        // A failed phase can reach the UI before the old turn's terminal is durable.
        // 失败阶段可能在旧回合终态落盘前到达界面。
        if guard
            .session_terminal_persistence_owners
            .contains_key(&session_id)
        {
            return Err(String::from(
                "The previous turn is finishing. The message remains queued.",
            ));
        }
        if guard
            .session_runtime_kernels
            .get(&session_id)
            .and_then(|kernel| kernel.active_turn())
            .is_some_and(|turn| turn.kind == crate::ActiveTurnKind::ContextCompaction)
            && let Some(task) = guard.active_stream_tasks.get_mut(&session_id)
        {
            // Resolve the exact item while compaction still owns the handoff.
            // 在压缩仍持有交接归属时解析精确条目。
            let identity = pending_prompt_queue_identity(handle.as_ref(), &client_action_id)?;
            // Check current queue membership rather than an action identity recovered from terminal history.
            // 校验当前队列成员资格，而不是仅依赖从终态历史恢复的动作身份。
            let (queued, stopped) = handle
                .read(|document| {
                    (document
                        .metadata
                        .input_state
                        .queued_inputs
                        .iter()
                        .any(|item| {
                            item.input_id == identity.input_id
                                && item.client_action_id == identity.client_action_id
                        }), !matches!(document.collaboration_run_control(),
                            vulcan_session_core::v1::collaboration::CollaborationRunControl::Running
                            | vulcan_session_core::v1::collaboration::CollaborationRunControl::NaturallyIdle))
                })
                .map_err(|error| error.to_string())?;
            if stopped {
                return Err(STOPPED_INSERTION_ERROR.to_owned());
            }
            if !queued {
                return Err(String::from(
                    "The message is no longer queued for insertion.",
                ));
            }
            if !task.deferred_compaction_insertions.contains(&identity) {
                task.deferred_compaction_insertions.push(identity);
            }
            return Ok(true);
        }
    }
    let attachment_workspace_path = pending_prompt_attachment_workspace_path(&state, session_id)?;
    // Only an in-turn insertion uses the interruption notification below.
    // 只有回合内插队才使用下方的中断通知。
    let identity = match accept_pending_prompt_insertion(
        &handle,
        &client_action_id,
        current_v1_goal_unix_millis()?,
    )
    .map_err(pending_prompt_insertion_error)?
    {
        PendingPromptInsertion::Missing => {
            return Err(format!("Pending message `{entry_id}` not found."));
        }
        PendingPromptInsertion::InTurn(identity) => identity,
        PendingPromptInsertion::NewTurn(root) => {
            return dispatch_pending_prompt_root(
                &app,
                &state,
                &handle,
                session_id,
                client_action_id,
                root,
                &attachment_workspace_path,
            )
            .await;
        }
    };
    // Once the input is in the transcript, a retry must not interrupt the new request processing it.
    // 输入已进入会话记录后，重试不能打断正在处理它的新请求。
    let awaits_insertion = handle
        .read(|document| {
            document
                .metadata
                .input_state
                .pending_in_turn_inputs
                .iter()
                .any(|pending| {
                    pending.input_id == identity.input_id
                        && pending.client_action_id == identity.client_action_id
                })
        })
        .map_err(|error| format!("Failed to read the accepted V1 insertion: {error}"))?;
    if awaits_insertion {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        notify_webui_wait_agent_parent_interruption_if_waiting(
            &guard,
            session_id,
            "user_interrupt_prompt",
        )?;
        drop(guard);
        // An unstreamed explicit insertion releases a foreground command so the model can answer.
        // 尚未入流的显式插队会释放前台命令，使模型能够回复。
        vulcan_host_tools::shell_job::ShellJobRuntime::global().notify_interjection(
            &vulcan_agent_runtime::tooling::native::canonical_shell_job_session_id(
                &session_id.to_string(),
            ),
        );
    }
    // The V1 promotion is already durable; later panel failures cannot retain its queue lease.
    // V1 提升已可靠提交；后续面板故障不能保留该队列租约。
    reconcile_pending_prompt_attachment_leases(
        handle.as_ref(),
        session_id,
        &attachment_workspace_path,
    )
    .map_err(|error| format!("The pending message was promoted into the V1 current turn, but attachment cleanup is pending a retry: {error}"))?;
    let locator = handle
        .storage_locator()
        .map_err(|error| format!("Failed to read the V1 session resource location: {error}"))?;
    let project_directory = locator.project_directory();
    let catalog = load_project_resource_catalog(&project_directory)
        .map_err(|error| format!("Failed to load the V1 project resource catalog: {error}"))?;
    rebuild_pending_prompt_panel_from_v1(
        &state,
        session_id,
        handle.as_ref(),
        &project_directory,
        &catalog,
    )?;
    emit_pending_prompts_changed_delta(&app, &state, session_id)?;
    // A parent may already be between requests with only its children running.
    // 父会话可能已处于两次请求之间，只剩子代理在运行。
    if let Err(error) =
        crate::webui_turn_chaining::schedule_waiting_business_continuation(&app, &state, session_id)
    {
        tracing::warn!(%session_id, %error, "Accepted insertion remains pending");
    }
    Ok(true)
}

/// Resolves and accepts one exact queue action at the current durable turn boundary.
/// 在当前可靠回合边界解析并接纳一个精确队列动作。
/// Uses the session `handle` and timestamp `now_ms`; an idle disposition requires separate new-turn admission.
/// 使用会话句柄 `handle` 和时间戳 `now_ms`；空闲处置仍需单独取得新回合准入。
fn accept_pending_prompt_insertion(
    handle: &SessionHandle,
    client_action_id: &ClientActionId,
    now_ms: i64,
) -> Result<PendingPromptInsertion, SessionRuntimeError> {
    handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::InputAccepted,
            |document| {
                // A repeat click or the FIFO scheduler may have already admitted this exact action.
                // 重复点击或队列调度可能已经接纳同一个动作。
                if let Some(turn_id) =
                    document.scheduled_queued_turn_for_action(client_action_id)?
                {
                    return Ok(SessionMutationChange::Unchanged(
                        PendingPromptInsertion::NewTurn(QueuedPromptRoot::Scheduled(turn_id)),
                    ));
                }
                // Resolve queue provenance under the same mutation as admission.
                // 在接纳动作的同一修改中解析队列归属。
                let Some(identity) = document.queued_input_identity_for_action(client_action_id)?
                else {
                    return Ok(SessionMutationChange::Unchanged(
                        PendingPromptInsertion::Missing,
                    ));
                };
                // An absent open turn requires separate root admission before dequeuing.
                // 没有开放回合时，需在出队前单独取得根回合准入。
                let insertion = match document
                    .promote_queued_input_to_pending_turn(identity.clone(), now_ms)
                {
                    Ok(IdempotentMutation::Committed(receipt)) => SessionMutationChange::Changed(
                        PendingPromptInsertion::InTurn(receipt.identity),
                    ),
                    Ok(IdempotentMutation::Idempotent(receipt)) => {
                        SessionMutationChange::Unchanged(PendingPromptInsertion::InTurn(
                            receipt.identity,
                        ))
                    }
                    Err(DomainError::NoActiveTurn) => SessionMutationChange::Unchanged(
                        PendingPromptInsertion::NewTurn(QueuedPromptRoot::Queued(identity)),
                    ),
                    Err(error) => return Err(error),
                };
                Ok(insertion)
            },
        )
        .map(|receipt| receipt.value)
}

/// Converts domain rejections to the existing queue-action feedback.
/// 将领域拒绝转为既有的队列操作反馈。
/// Returns a readable diagnostic for `error`; the queue retains any unaccepted input.
/// 返回 `error` 的可读提示，未接纳的输入仍保留在队列中。
fn pending_prompt_insertion_error(error: SessionRuntimeError) -> String {
    match error {
        SessionRuntimeError::Domain(cause)
            if matches!(
                cause.as_ref(),
                DomainError::CollaborationStateViolation {
                    field: "pending_input.stopped_turn"
                }
            ) =>
        {
            STOPPED_INSERTION_ERROR.to_owned()
        }
        SessionRuntimeError::Domain(cause)
            if matches!(
                cause.as_ref(),
                DomainError::CollaborationStateViolation {
                    field: "pending_input.active_turn"
                }
            ) =>
        {
            String::from("Another turn has started. Try guiding the queued message again.")
        }
        error => format!("Failed to accept the queued message: {error}"),
    }
}

/// Starts one selected queue row for an idle session or resumes its already admitted dispatch.
/// 为闲置会话发送所选队列行，或恢复该行已经接纳的派发。
/// The session and action identify the exact owner. Returns true after durable admission, even when dispatch must wait.
/// 会话与动作标识精确所有者。可靠准入后返回 true，即使仍须等待派发。
async fn dispatch_pending_prompt_root(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    handle: &SessionHandle,
    session_id: SessionId,
    client_action_id: ClientActionId,
    root: QueuedPromptRoot,
    attachment_workspace_path: &str,
) -> Result<bool, String> {
    // Keep any fresh turn reservation alive through dispatch handoff.
    // 让新回合预留持续到派发交接结束。
    let admission;
    // Both fresh admission and retry use the same ordinary queue dispatcher.
    // 新准入和重试均使用同一个普通队列派发器。
    let (turn_id, authorization) = match root {
        QueuedPromptRoot::Scheduled(turn_id) => (turn_id, None),
        QueuedPromptRoot::Queued(identity) => {
            // Keep the reservation through durable admission and adapter startup so Stop and other submissions cannot take it over.
            // 保持预留直至可靠准入与适配器启动，避免停止和其他提交接管。
            admission =
                crate::project_conversation_services::try_begin_pending_prompt_dispatch_exclusion(
                    state, session_id,
                )?
                .ok_or_else(|| {
                    String::from(
                        "The session is busy. Try sending the queued message again shortly.",
                    )
                })?;
            // Resolve the selected session's project from its authoritative storage locator.
            // 从权威存储定位器解析所选会话的项目。
            let locator = handle
                .storage_locator()
                .map_err(|error| error.to_string())?;
            // The live binding ensures a deleted or unmounted workspace leaves the row queued.
            // 实时绑定确保目录删除或卷卸载时消息继续留在队列。
            let project = crate::project_directories::active_project_directory_owner(
                state,
                locator.project_id(),
            )?;
            vulcan_host_tools::project_directory::require_bound_project_directory(
                std::path::Path::new(&project.path),
                project.root_volume_id.as_deref(),
            )?;
            // Timestamp the durable admission after the directory preflight.
            // 在目录预检后为可靠准入记录时间。
            let now_ms = current_v1_goal_unix_millis()?;
            // Pass the same reservation to the ordinary queue dispatcher.
            // 将同一个预留传给既有队列派发器。
            let authorization = admission.existing_submission_authorization(session_id)?;
            // Core rechecks the idle boundary under its document lock before dequeuing.
            // Core 在出队前持有文档锁复核空闲边界。
            let receipt = handle
                .mutate_durable_if_changed(
                    SessionMutationAdmission::RootTurn,
                    SessionFlushReason::InputAccepted,
                    |document| {
                        document
                            .schedule_idle_queued_input(identity, now_ms)
                            .map(SessionMutationChange::Changed)
                    },
                )
                .map_err(pending_prompt_insertion_error)?;
            (receipt.value, Some(authorization))
        }
    };

    // Cleanup or projection failures cannot strand the newly accepted message.
    // 清理或投影失败不能搁置刚接纳的消息。
    if let Err(error) =
        reconcile_pending_prompt_attachment_leases(handle, session_id, attachment_workspace_path)
    {
        tracing::warn!(%session_id, %error, "Dispatched queue attachment release remains pending");
    }
    if let Ok(controller) =
        crate::project_conversation_services::controller_runtime_from_state(state)
        && let Err(error) = crate::webui_turn_chaining::refresh_v1_pending_prompt_panel(
            app,
            state,
            &controller,
            session_id,
        )
        .await
    {
        tracing::warn!(%session_id, %error, "Pending prompt panel refresh failed");
    }
    dispatch_accepted_queued_turn(
        app,
        state,
        session_id,
        turn_id,
        client_action_id,
        authorization,
    )
    .await;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::test_support::{
        TestHostRuntimeRoot, run_with_service_worker_stack, test_service_state_with_parent_session,
    };
    use std::sync::{Arc, Mutex};
    use vulcan_session_core::v1::{
        ClientActionId, ClientInputSubmission, CollaborationOperationId, CollaborationRunControl,
        SetRunControlInput,
    };

    /// Verifies a real queue RPC sends the selected input after a durable preparation failure.
    /// 验证准备失败落盘后，真实队列 RPC 会发送所选输入。
    /// Takes no arguments. Checks wire request order, repeat clicks, and reloaded turn ownership.
    /// 不接收参数。检查线协议请求顺序、重复点击和重载后的回合归属。
    #[test]
    fn failed_turn_queue_row_sends_original_input_once() {
        run_with_service_worker_stack(verify_failed_turn_queue_dispatch);
    }

    /// Exercises the exact selected queue item on a local Provider before and after reloading.
    /// 使用本地供应商检验所选队列项在重载前后的精确派发。
    /// Takes no arguments and finishes after the wire and durable state assertions.
    /// 不接收参数，完成线协议和持久状态断言后返回。
    async fn verify_failed_turn_queue_dispatch() {
        use crate::agent_collaboration::compaction_test_https as https;
        use vulcan_provider_adapters::{CredentialConnection, CredentialMode, PublicAiRuntime};
        use vulcan_session_core::v1::config::ActiveRequestConfig;
        use vulcan_session_core::v1::event::{EventBody, UserInputSource};

        // Use a unique session identity for the production dispatch path.
        // 为生产派发链路使用唯一的会话身份。
        let session_id = SessionId::new_v4();
        // Retain fixture storage for the cold-load assertions.
        // 保留夹具存储供冷加载断言使用。
        let root = TestHostRuntimeRoot::new(session_id);
        // Keep the fixture workspace separate from the repository and customer data.
        // 使夹具工作区与仓库及客户数据隔离。
        let workspace_directory = tempfile::tempdir().unwrap();
        // Observe actual wire requests on a local listener.
        // 在本地监听器观察实际线协议请求。
        let server = https::Server::start("Fixture queued response.").await;
        // Connect the normal service runtime to the pinned fixture transport.
        // 将常规服务运行时连接到固定的夹具传输层。
        let mut bridge = crate::tests::test_support::test_agent_runtime_bridge();
        bridge.public_ai_runtime = Arc::new(
            PublicAiRuntime::first_batch_with_http_client_for_testing(
                Arc::new(https::Secrets),
                server.http.clone(),
            )
            .unwrap(),
        );
        // Install a synthetic provider account whose hostname is pinned to the local test listener.
        // 安装合成供应商账号，主机名固定指向本地测试监听器。
        let mut config = bridge.public_ai_runtime.configuration_snapshot().unwrap();
        config
            .connection_graph
            .add_connection(
                &bridge.public_ai_runtime.providers().unwrap(),
                CredentialConnection {
                    id: "queued-recovery-account".into(),
                    provider_id: "opencode-go".into(),
                    service_entry_id: "go-api".into(),
                    credential_profile_id: "api-key".into(),
                    credential_mode: CredentialMode::Token,
                    credential_id: "queued-recovery-credential".into(),
                    display_name: "Fixture account".into(),
                    secret_reference: "compaction-test-secret".into(),
                    management_secret_reference: None,
                    service_origin_override: None,
                },
            )
            .unwrap();
        config.set_model_enabled("opencode-go", "deepseek-v4.1-flash", true);
        bridge
            .public_ai_runtime
            .replace_configuration(config)
            .unwrap();
        // Keep the selected workspace and the durable session bound to the same project.
        // 使选中的工作区与可靠会话绑定到同一个项目。
        let mut service = test_service_state_with_parent_session(session_id);
        service.controller_runtime = Some(root.attached_host_runtime_handle().await);
        service.agent_runtime = Some(bridge);
        root.bind_workspace_project(&mut service, workspace_directory.path(), vec![session_id]);
        service.workbench.selected_conversation_id = Some(session_id.to_string());
        // Share the durable session registry and the pending queue projection.
        // 共享可靠会话注册表与待发送队列投影。
        let state = Arc::new(Mutex::new(service));
        // Use the indexed session owner that the browser RPC will resolve.
        // 使用浏览器 RPC 将解析的已索引会话所有者。
        let handle = session_v1_handle_from_state(&state, session_id)
            .await
            .unwrap();
        // Apply production projections without external event subscribers.
        // 应用生产投影且无需外部事件订阅方。
        let app = ServiceEventEmitter::new(Vec::new());
        handle
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("failed-original").unwrap(),
                    "Original request before the failure",
                    Vec::new(),
                )
                .with_bound_request_config(ActiveRequestConfig {
                    provider: Some("opencode-go".into()),
                    connection_id: Some("queued-recovery-account".into()),
                    model: Some("deepseek-v4.1-flash".into()),
                    thinking_level: Some("low".into()),
                    temperature: None,
                }),
                2,
            )
            .unwrap();
        // The clicked row is behind another message. Only that exact row should dispatch first.
        // 被点击项位于另一条消息之后。只有该精确队列项应率先派发。
        let earlier_action = ClientActionId::try_new("older-queue-action").unwrap();
        let chosen_action = ClientActionId::try_new("chosen-queue-action").unwrap();
        let inserted_action = ClientActionId::try_new("already-inserted-action").unwrap();
        let earlier_prompt = "The older queued request";
        let chosen_prompt = "The chosen recovery request";
        let inserted_prompt = "The supplement inserted before the failure";
        for (action, prompt, now_ms) in [
            (&earlier_action, earlier_prompt, 3),
            (&chosen_action, chosen_prompt, 4),
            (&inserted_action, inserted_prompt, 5),
        ] {
            handle
                .enqueue_input(
                    ClientInputSubmission::new(action.clone(), prompt, Vec::new()),
                    now_ms,
                )
                .unwrap();
        }
        // A previously accepted supplement must keep its original turn and leave the pending panel.
        // 先前接纳的补充消息必须保留原回合归属并离开待发送面板。
        let inserted_identity = pending_prompt_queue_identity(&handle, &inserted_action).unwrap();
        let inserted = handle
            .promote_queued_input_to_pending_turn(inserted_identity, 6)
            .unwrap()
            .value;
        // Reproduce the interval in which the service has claimed failure and Core still has an open turn.
        // 复现服务已接管失败结算、Core 仍有开放回合的时间窗口。
        let terminal_owner =
            crate::state::SessionTerminalPersistenceOwner::StreamTask(uuid::Uuid::new_v4());
        crate::state::register_session_terminal_persistence_owner(
            &mut state.lock().unwrap(),
            session_id,
            terminal_owner.clone(),
        )
        .unwrap();
        assert!(
            service_insert_pending_prompt(
                app.clone(),
                state.clone(),
                chosen_action.as_str().to_owned(),
            )
            .await
            .is_err()
        );
        assert!(
            handle
                .read(|document| document
                    .metadata
                    .input_state
                    .queued_inputs
                    .iter()
                    .any(|item| item.client_action_id == chosen_action))
                .unwrap()
        );
        // This is the durable failure boundary used after request preparation exhausts re-planning.
        // 请求准备耗尽重规划次数后使用同一个可靠失败边界。
        handle
            .mutate_durable(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::DisplayBoundary,
                |document| {
                    assert!(document.fail_execution_boundary_and_close_turn(
                        "provider_request_preparation_unstable",
                        None,
                        7,
                    )?);
                    Ok(())
                },
            )
            .unwrap();
        crate::state::SessionTerminalPersistenceOwnerGuard::adopt_registered(
            state.clone(),
            session_id,
            terminal_owner,
        )
        .unwrap()
        .release()
        .unwrap();
        assert!(server.requests.lock().unwrap().is_empty());

        // Execute the same RPC used by the queue button, twice for the same durable action.
        // 对同一个可靠动作执行两次队列按钮使用的 RPC。
        for _ in 0..2 {
            // Both requests must acknowledge the same queued action.
            // 两次请求都必须确认同一个排队动作。
            let response = crate::rpc_dispatch::dispatch_service_rpc(
                app.clone(),
                state.clone(),
                "service_insert_pending_prompt".into(),
                Some(serde_json::json!({
                    "entryId": chosen_action.as_str(),
                })),
            )
            .await
            .unwrap();
            assert_eq!(response, serde_json::Value::Bool(true));
        }
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                if server.requests.lock().unwrap().len() >= 2
                    && !state
                        .lock()
                        .unwrap()
                        .active_stream_tasks
                        .contains_key(&session_id)
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("both queued requests must reach the loopback model and settle");
        {
            // Check the authored input order independently of host context messages with a user role.
            // 独立检验用户撰写的输入顺序，排除使用 user 角色承载的宿主上下文消息。
            let requests = server.requests.lock().unwrap();
            assert_eq!(requests.len(), 2);
            for (request, expected) in requests.iter().zip([
                vec![inserted_prompt, chosen_prompt],
                vec![inserted_prompt, chosen_prompt, earlier_prompt],
            ]) {
                // Extract only the distinct fixture prompts in provider order.
                // 按供应商请求中的顺序提取不同的夹具提示词。
                let authored = request["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|message| message["role"] == "user")
                    .filter_map(|message| {
                        // Match fixture markers in either text or multipart content.
                        // 在纯文本或多段内容中匹配夹具标记。
                        let content = message["content"].to_string();
                        [inserted_prompt, chosen_prompt, earlier_prompt]
                            .into_iter()
                            .find(|prompt| content.contains(prompt))
                    })
                    .collect::<Vec<_>>();
                assert_eq!(authored, expected);
            }
        }
        // Cold loading also proves that the failed original turn and each new input survive.
        // 冷加载同时证明失败的原回合与各条新输入均可靠保留。
        let loaded = vulcan_workspace_store::workspace_store::load_session_v1_for_runtime(
            &root.v1_storage_locator(),
        )
        .unwrap();
        // Read persisted state instead of the in-process queue mirror.
        // 读取持久态，而不依赖进程内队列镜像。
        let document = loaded.document;
        // Each queued root must still have exactly one event in the reloaded journal.
        // 每个排队根输入在重载日志中仍必须只有一条事件。
        let scheduled = document
            .events()
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::UserInput(input)
                    if input.input_source == UserInputSource::QueuedScheduled =>
                {
                    Some(&input.client_action_id)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(scheduled, vec![&chosen_action, &earlier_action]);
        assert!(document.metadata.input_state.queued_inputs.is_empty());
        assert!(
            document
                .metadata
                .input_state
                .pending_in_turn_inputs
                .is_empty()
        );
        assert!(document.events().iter().any(|event| {
            event.turn_id == Some(inserted.turn_id)
                && matches!(&event.event, EventBody::UserInput(input)
                    if input.input_source == UserInputSource::PendingInTurn
                        && input.client_action_id == inserted_action)
        }));
        assert!(document.events().iter().any(|event| {
            matches!(&event.event, EventBody::DisplayNote(note)
                if note.reason_code == "provider_request_preparation_unstable")
        }));
        server.shutdown().await;
    }

    /// Verifies that a stopped turn retains its queued input and that a resumed turn accepts its exact RPC target.
    /// 验证停止回合保留排队消息，恢复后通过 RPC 精确目标接收插队。
    /// Takes no parameters and returns after both durable outcomes are verified.
    /// 无参数，验证两个可靠结果后返回。
    #[test]
    fn stopped_pending_insertion_preserves_queued_input() {
        run_with_service_worker_stack(verify_stopped_pending_insertion);
    }

    /// Exercises the insertion RPC while stopped and after resuming a conversation.
    /// 在会话停止及恢复后分别验证插队 RPC。
    /// Takes no arguments and finishes after checking the durable queue and shortcut results.
    /// 不接收参数，检查持久队列和快捷操作结果后返回。
    async fn verify_stopped_pending_insertion() {
        let session_id = SessionId::new_v4();
        let root = TestHostRuntimeRoot::new(session_id);
        let mut service = test_service_state_with_parent_session(session_id);
        service.controller_runtime = Some(root.attached_host_runtime_handle().await);
        service.workbench.selected_conversation_id = Some(session_id.to_string());
        service
            .workspace_snapshot
            .as_mut()
            .unwrap()
            .task_projects
            .push(vulcan_workbench_model::TaskProjectSummary::new(
                "project-a",
                "Fixture",
                ".",
                false,
                0,
                None,
                None,
                Vec::new(),
            ));
        let state = Arc::new(Mutex::new(service));
        let handle = session_v1_handle_from_state(&state, session_id)
            .await
            .unwrap();
        handle
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("stopped-insert-root").unwrap(),
                    "Original business",
                    Vec::new(),
                ),
                2,
            )
            .unwrap();
        handle
            .set_collaboration_run_control_durable(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new("stopped-insert-control").unwrap(),
                state: CollaborationRunControl::MainAndSubagents,
                changed_at_ms: 3,
            })
            .unwrap();
        let action = ClientActionId::try_new("stopped-insert-message").unwrap();
        handle
            .enqueue_input(
                ClientInputSubmission::new(action.clone(), "Keep queued", Vec::new()),
                4,
            )
            .unwrap();
        pending_prompt_attachment_workspace_path(&state, session_id).unwrap();
        let before = handle
            .read_full_durable(|document| {
                serde_json::to_value((&document.metadata, document.events())).unwrap()
            })
            .unwrap();
        let result = service_insert_pending_prompt(
            ServiceEventEmitter::new(Vec::new()),
            state.clone(),
            action.as_str().to_owned(),
        )
        .await;
        assert!(result.is_err());
        assert_eq!(
            handle
                .read_full_durable(|document| {
                    serde_json::to_value((&document.metadata, document.events())).unwrap()
                })
                .unwrap(),
            before
        );
        assert_eq!(state.lock().unwrap().workbench_generation, 0);
        assert!(state.lock().unwrap().active_stream_tasks.is_empty());

        handle
            .set_collaboration_run_control_durable(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new("resume-before-shortcut-insert")
                    .unwrap(),
                state: CollaborationRunControl::Running,
                changed_at_ms: 5,
            })
            .unwrap();
        // shortcut_action is the newly submitted item; the older queued item must retain its place.
        // shortcut_action 是新提交的消息，较早的排队消息必须保留原有位置。
        let shortcut_action = ClientActionId::try_new("shortcut-insert-message").unwrap();
        handle
            .enqueue_input(
                ClientInputSubmission::new(shortcut_action.clone(), "Insert this", Vec::new()),
                6,
            )
            .unwrap();
        state.lock().unwrap().workbench.selected_conversation_id = None;
        // inserted records the explicit RPC response after the user has left the target conversation.
        // inserted 记录用户离开目标会话后，显式 RPC 返回的结果。
        let inserted = crate::rpc_dispatch::dispatch_service_rpc(
            ServiceEventEmitter::new(Vec::new()),
            state,
            String::from("service_insert_pending_prompt"),
            Some(serde_json::json!({
                "conversationId": session_id,
                "entryId": shortcut_action.as_str(),
            })),
        )
        .await
        .unwrap();
        assert_eq!(inserted, serde_json::Value::Bool(true));
        handle
            .read_full_durable(|document| {
                // input_state verifies the exact new message was promoted and the older one retained.
                // input_state 用于验证精确的新消息已插入，较早的消息仍留在队列中。
                let input_state = &document.metadata.input_state;
                assert_eq!(input_state.queued_inputs.len(), 1);
                assert!(
                    input_state
                        .queued_inputs
                        .iter()
                        .any(|input| input.client_action_id == action)
                );
                assert_eq!(input_state.pending_in_turn_inputs.len(), 1);
                assert!(
                    input_state
                        .pending_in_turn_inputs
                        .iter()
                        .any(|input| input.client_action_id == shortcut_action)
                );
            })
            .unwrap();
    }
}
