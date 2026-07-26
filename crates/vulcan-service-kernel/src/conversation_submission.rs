//! WebUI conversation prompt submission orchestration.
//! WebUI 会话提示词提交编排逻辑。

use std::collections::BTreeMap;

use uuid::Uuid;
use vulcan_agent_runtime::workbench_prompt_turn::{
    SharedPromptTurnBeginInput, begin_shared_prompt_turn,
};
use vulcan_service_api::{
    ComposerAttachmentSummary, ServiceEvent, ServiceEventEmitter, SubmitConversationPromptAck,
    UpdateConversationRunPhaseDelta, WorkbenchDelta, WorkbenchDeltaEnvelope,
};
use vulcan_workbench_model::{
    PendingSessionPromptSource, SessionId, SessionRunPhase, SessionState,
};

use super::{
    SharedServiceBridgeState, UserPromptEditSubmitRequest, active_kernel_session_ids,
    apply_webui_prompt_begin_outer_steps_deferred_context_persist,
    build_conversation_messages_patched_delta, build_vmm_hook_config_snapshot,
    capture_preserved_workspace_run_state, composer_attachment_summary_to_core,
    conversation_messages_from_session, current_unix_millis_text, current_unix_millis_value,
    current_workspace_snapshot_from_shared_state, emit_materialized_session_message_delta,
    emit_pending_prompts_changed_delta, emit_service_event_with_trace,
    enqueue_webui_pending_prompt_from_submission, hydrate_retained_submission_session_if_needed,
    load_workspace_for_submission, mark_webui_prompt_snapshot_preparing,
    materialize_prompt_session, owning_project_id_for_session, parse_prompt_session_target,
    parse_session_id, preserve_active_session_run_state,
    replace_conversation_messages_in_workbench_window,
    retained_submission_snapshot_has_session_context, retained_submission_snapshot_is_usable,
    rollback_persisted_session_before_user_prompt, runtime_parts_from_state,
    session_has_blocking_system_prompt_work, snapshot_webui_context_before_user_prompt,
    spawn_service_task, spawn_workspace_session_persist_task,
    start_reserved_webui_session_kernel_user_turn, start_webui_precheck_or_continue,
    start_webui_session_kernel_user_turn, store_workspace_snapshot_without_workbench_rebuild,
    sync_hydrated_session_dialogue_ready_marker, validate_composer_attachment_summary,
    webui_session_allows_user_prompt_editing, webui_session_kernel_has_active_turn,
    webui_shared_prompt_precheck_plan, webui_submit_trace, webui_trace_if_slow,
    workbench_with_live_pending_question, workspace_path_for_session,
    workspace_user_prompt_message_allows_destructive_edit,
};
mod activation;
mod edit;
mod preflight;
mod queue;
mod runtime_options;

pub(crate) use activation::*;
pub(super) use edit::*;
pub(crate) use preflight::*;
pub(crate) use queue::*;
pub(crate) use runtime_options::*;

/// Defines how an existing session obtains dialogue context before prompt submission.
/// 定义已有会话在提交提示词前如何取得对话上下文。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExistingSessionHydrationPolicy {
    /// Loads persisted dialogue when the retained runtime context is empty.
    /// 当保留运行态上下文为空时加载持久化对话。
    HydrateIfEmpty,
    /// Trusts the retained snapshot even when a destructive edit intentionally made it empty.
    /// 即使破坏性编辑有意将其清空，也信任保留快照。
    TrustRetainedSnapshot,
}

/// Determines whether an existing submission must hydrate missing dialogue from persistence.
/// 判断已有会话提交是否必须从持久化层水合缺失对话。
///
/// # Parameters
/// 参数
/// `session_id` is present only for submissions targeting an existing session.
/// `session_id` 仅在提交目标为已有会话时存在。
/// `policy` describes whether the retained snapshot is authoritative when empty.
/// `policy` 描述保留快照为空时是否仍具权威性。
///
/// # Returns
/// 返回值
/// Returns true only when an existing ordinary submission requires hydration.
/// 仅当已有会话的普通提交需要水合时返回 true。
fn existing_submission_requires_hydration(
    session_id: Option<SessionId>,
    policy: ExistingSessionHydrationPolicy,
) -> bool {
    session_id.is_some() && policy == ExistingSessionHydrationPolicy::HydrateIfEmpty
}

/// Submits a WebUI user prompt into a new or existing conversation.
/// 将 WebUI 用户提示词提交到新会话或已有会话。
///
/// # Parameters
/// 参数
/// `app` emits workbench updates during submission preparation and precheck dispatch.
/// `app` 在提交准备与 precheck 分发期间发送工作台更新。
/// `state` owns service runtime handles and the retained workspace snapshot.
/// `state` 持有服务运行时句柄与保留工作区快照。
/// `project_id` selects the target project when creating a new conversation.
/// `project_id` 在创建新会话时选择目标项目。
/// `conversation_id` selects an existing conversation when present.
/// `conversation_id` 在存在时选择已有会话。
/// `prompt` is the trimmed user prompt body.
/// `prompt` 是裁剪后的用户提示词正文。
/// `hidden_model_guidance` optionally adds hidden runtime guidance.
/// `hidden_model_guidance` 可选追加隐藏运行时引导。
/// `execution_mode` chooses build or plan execution mode.
/// `execution_mode` 选择 build 或 plan 执行模式。
/// `reasoning_effort` optionally overrides model reasoning effort for this turn.
/// `reasoning_effort` 可选覆盖本回合模型推理强度。
/// `attachments` contains composer attachment summaries to stage into the turn.
/// `attachments` 包含要加入本回合的输入区附件摘要。
/// `disabled_ai_capability_kinds` disables selected AI capability kinds for this turn.
/// `disabled_ai_capability_kinds` 禁用本回合选定 AI 能力类型。
/// `disabled_ai_capability_target_selectors` disables selected AI capability targets for this turn.
/// `disabled_ai_capability_target_selectors` 禁用本回合选定 AI 能力目标。
/// `ai_capability_target_bindings` overrides selected AI capability target ordering.
/// `ai_capability_target_bindings` 覆盖选定 AI 能力目标排序。
/// `existing_session_hydration_policy` controls whether an empty retained session is rehydrated.
/// `existing_session_hydration_policy` 控制是否重新水合空的保留会话。
///
/// # Returns
/// 返回值
/// Returns a lightweight acknowledgement after queuing or starting the submitted prompt.
/// 返回排队或启动提交提示词后的轻量确认信息。
pub(super) async fn service_submit_conversation_prompt(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    project_id: Option<String>,
    conversation_id: Option<String>,
    prompt: String,
    hidden_model_guidance: Option<String>,
    execution_mode: Option<String>,
    reasoning_effort: Option<String>,
    attachments: Option<Vec<ComposerAttachmentSummary>>,
    disabled_ai_capability_kinds: Option<Vec<String>>,
    disabled_ai_capability_target_selectors: Option<Vec<String>>,
    ai_capability_target_bindings: Option<BTreeMap<String, String>>,
    existing_session_hydration_policy: ExistingSessionHydrationPolicy,
) -> Result<SubmitConversationPromptAck, String> {
    // Measures the whole submit RPC body from service entry to final response.
    // 衡量提交 RPC 主体从服务入口到最终返回的整体耗时。
    let submit_total_started_at = current_unix_millis_value();
    let prompt = prompt.trim().to_string();
    webui_submit_trace(
        "submit_prompt_received",
        format!(
            "conversation_id={} project_id={} prompt_chars={} attachments={}",
            conversation_id.as_deref().unwrap_or("<draft>"),
            project_id.as_deref().unwrap_or("<none>"),
            prompt.chars().count(),
            attachments.as_ref().map(Vec::len).unwrap_or(0)
        ),
    );
    let attachments = attachments
        .unwrap_or_default()
        .into_iter()
        .map(composer_attachment_summary_to_core)
        .collect::<Result<Vec<_>, _>>()?;
    if prompt.is_empty() && attachments.is_empty() {
        return Err(String::from("请输入任务指令。"));
    }

    // Measures runtime handle acquisition before workspace hydration begins.
    // 衡量工作区水合开始前获取运行时句柄的耗时。
    let runtime_parts_started_at = current_unix_millis_value();
    let (
        controller_runtime,
        agent_runtime,
        _luaskills_catalog,
        _token_budget_runtime,
        _mcp_catalog,
        _logs,
    ) = runtime_parts_from_state(&state)?;
    webui_submit_trace(
        "submit_prompt_runtime_parts_ready",
        format!(
            "conversation_id={} elapsed_ms={}",
            conversation_id.as_deref().unwrap_or("<draft>"),
            current_unix_millis_value().saturating_sub(runtime_parts_started_at)
        ),
    );
    // submission_target fixes create-or-continue intent before admission, loading, or hydration starts.
    // submission_target 在准入、加载或水合开始前固定新建或继续意图。
    let submission_target = parse_prompt_session_target(conversation_id.as_deref())?;
    // submission_session_id is the existing identity used by retained-state and hydration lookups.
    // submission_session_id 是保留状态与水合查找使用的已有会话身份。
    let submission_session_id = submission_target.existing_session_id();
    // Measures retained workspace reuse without cloning the full workbench snapshot on the hot path.
    // 衡量保留工作区复用，并避免在热路径克隆完整工作台快照。
    let retained_snapshot_started_at = current_unix_millis_value();
    // RetainedSubmissionPreflight keeps admission ownership aligned with the captured workspace revision.
    // RetainedSubmissionPreflight 让准入所有权与捕获的工作区版本保持一致。
    let RetainedSubmissionPreflight {
        turn_admission,
        project_admission,
        preserved_workspace_run_state,
        retained_dialogue_context_available,
        retained_workspace_snapshot,
    } = prepare_retained_submission_preflight(&state, project_id.as_deref(), submission_target)?;
    webui_trace_if_slow(
        "submit_prompt_retained_snapshot_slow",
        retained_snapshot_started_at,
        format!(
            "conversation_id={} retained_dialogue_context_available={retained_dialogue_context_available} preserved_sessions={}",
            conversation_id.as_deref().unwrap_or("<draft>"),
            preserved_workspace_run_state
                .as_ref()
                .map(|state| state.session_count())
                .unwrap_or_default()
        ),
    );
    let workspace_load_started_at = current_unix_millis_value();
    let (mut workspace_snapshot, workspace_source) =
        if let Some(workspace_snapshot) = retained_workspace_snapshot {
            (workspace_snapshot, "retained")
        } else {
            (
                load_workspace_for_submission(controller_runtime.clone(), submission_session_id)
                    .await?,
                "disk",
            )
        };
    if existing_submission_requires_hydration(
        submission_session_id,
        existing_session_hydration_policy,
    ) && let Some(session_id) = submission_session_id
    {
        hydrate_retained_submission_session_if_needed(
            controller_runtime.clone(),
            &mut workspace_snapshot,
            session_id,
        )
        .await?;
    }
    webui_submit_trace(
        "submit_prompt_workspace_ready",
        format!(
            "conversation_id={} retained_dialogue_context_available={retained_dialogue_context_available} source={workspace_source} elapsed_ms={}",
            conversation_id.as_deref().unwrap_or("<draft>"),
            current_unix_millis_value().saturating_sub(workspace_load_started_at)
        ),
    );
    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
        &mut workspace_snapshot,
    );
    if let ExistingSubmissionQueueDecision::Queue { session_id } =
        evaluate_existing_submission_queue(
            &state,
            &workspace_snapshot,
            &agent_runtime.provider_configuration,
            submission_session_id,
        )?
    {
        return queue_existing_submission(
            &app,
            &state,
            &controller_runtime,
            &agent_runtime,
            ExistingSubmissionQueueInput {
                workspace_snapshot,
                session_id,
                prompt,
                hidden_model_guidance,
                attachments,
                reasoning_effort,
                disabled_ai_capability_kinds,
                submit_total_started_at,
            },
        );
    }
    let materialize_started_at = current_unix_millis_value();
    // materialization_project_id pins a new session to the exact project selected under preflight ownership.
    // materialization_project_id 将新会话固定到预检所有权下选择的精确项目。
    let materialization_project_id = if submission_session_id.is_none() {
        project_admission
            .exact_project_id()
            .or(project_id.as_deref())
    } else {
        project_id.as_deref()
    };
    let materialized_session = materialize_prompt_session(
        &mut workspace_snapshot,
        materialization_project_id,
        submission_target,
        if prompt.is_empty() {
            "附件会话"
        } else {
            &prompt
        },
    )?;
    project_admission.validate_materialized_session(&materialized_session)?;
    webui_submit_trace(
        "submit_prompt_materialize_ready",
        format!(
            "session={} title_chars={} workspace_path_chars={} elapsed_ms={}",
            materialized_session.session_id,
            materialized_session.session_title.chars().count(),
            materialized_session.workspace_path.chars().count(),
            current_unix_millis_value().saturating_sub(materialize_started_at)
        ),
    );
    // kernel_reservation_id binds an existing materialization to the exact admission owner captured before loading.
    // kernel_reservation_id 将已有会话物化结果绑定到加载前捕获的精确准入所有者。
    let kernel_reservation_id = turn_admission.kernel_reservation_id(&materialized_session)?;
    prepare_materialized_submission_session(
        &mut workspace_snapshot,
        &materialized_session,
        &agent_runtime.ai_tool_configuration,
    )?;
    // session_id is copied from the authoritative materialization result after activation consumes its classification.
    // session_id 在激活阶段消费物化分类后，从权威物化结果中复制。
    let session_id = materialized_session.session_id;
    // session_title borrows the authoritative title through the final activation commit.
    // session_title 借用权威标题直到最终激活提交完成。
    let session_title = materialized_session.session_title.as_str();
    // workspace_path borrows the owning project path through precheck dispatch preparation.
    // workspace_path 借用所属项目路径直到 precheck 分发准备完成。
    let workspace_path = materialized_session.workspace_path.as_str();
    webui_submit_trace(
        "submit_prompt_starting_run",
        format!(
            "session={session_id} title_chars={}",
            session_title.chars().count()
        ),
    );
    let submit_run_prepare_started_at = current_unix_millis_value();
    let runtime_options = prepare_submission_runtime_options(
        &controller_runtime,
        &mut workspace_snapshot,
        session_id,
        &agent_runtime.ai_tool_configuration,
        SubmissionRuntimeOptionsInput {
            execution_mode,
            reasoning_effort,
            disabled_ai_capability_kinds,
            disabled_ai_capability_target_selectors,
            ai_capability_target_bindings,
        },
    )
    .await?;
    webui_submit_trace(
        "submit_prompt_runtime_options_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(submit_run_prepare_started_at)
        ),
    );
    let context_snapshot_started_at = current_unix_millis_value();
    snapshot_webui_context_before_user_prompt(
        &controller_runtime,
        &workspace_snapshot,
        session_id,
        PendingSessionPromptSource::UserFollowup,
    )?;
    webui_submit_trace(
        "submit_prompt_context_snapshot_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(context_snapshot_started_at)
        ),
    );
    // Resolves every fallible precheck-plan input before hook runtime opens a logical turn.
    // 在 hook runtime 打开逻辑回合前解析全部可失败的 precheck 计划输入。
    let precheck_plan_started_at = current_unix_millis_value();
    let precheck_plan = webui_shared_prompt_precheck_plan(
        &agent_runtime,
        &workspace_snapshot,
        session_id,
        workspace_path,
    )?;
    webui_submit_trace(
        "submit_prompt_precheck_plan_ready",
        format!(
            "session={session_id} has_hooks={} skip={} labels={} elapsed_ms={}",
            precheck_plan.has_precheck_hooks,
            precheck_plan.should_skip_precheck,
            precheck_plan.progress_labels.len(),
            current_unix_millis_value().saturating_sub(precheck_plan_started_at)
        ),
    );
    let turn_begin_started_at = current_unix_millis_value();
    let precheck_start = {
        let mut hook_runtime = agent_runtime
            .conversation_hook_runtime
            .lock()
            .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?;
        begin_shared_prompt_turn(
            &mut workspace_snapshot,
            &mut hook_runtime,
            SharedPromptTurnBeginInput {
                session_id,
                session_title,
                workspace_path,
                prompt_entry_id: format!(
                    "webui-prompt-{session_id}-{}",
                    current_unix_millis_text()
                ),
                prompt: &prompt,
                hidden_model_guidance: hidden_model_guidance.as_deref(),
                provider_visible_agent_messages: Vec::new(),
                attachments,
                append_canonical_user_message: true,
                append_visible_user_prompt_record: true,
                source: PendingSessionPromptSource::UserFollowup,
                execution_mode: runtime_options.execution_mode,
                reasoning_effort: runtime_options.reasoning_effort,
                disabled_ai_capability_kinds: runtime_options.disabled_ai_capability_kinds,
                disabled_ai_capability_target_selectors: runtime_options
                    .disabled_ai_capability_target_selectors,
                ai_capability_target_bindings: runtime_options.ai_capability_target_bindings,
            },
        )
    }?;
    let accepted_message_id = precheck_start.visible_user_prompt_record_id.clone();
    let turn_id = Some(precheck_start.preparation_turn_id.clone());
    webui_submit_trace(
        "submit_prompt_turn_begin_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(turn_begin_started_at)
        ),
    );
    // Keeps skipped precheck turns in the preparatory phase until provider execution starts.
    // 在 precheck 跳过时保持准备阶段，直到模型执行真正开始。
    let submit_visible_run_phase =
        if precheck_plan.should_skip_precheck || !precheck_plan.has_precheck_hooks {
            SessionRunPhase::Preparing
        } else {
            SessionRunPhase::MemoryRecall
        };
    {
        let kernel_turn_started_at = current_unix_millis_value();
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if let Some(kernel_reservation_id) = kernel_reservation_id {
            start_reserved_webui_session_kernel_user_turn(
                &mut guard,
                session_id,
                &precheck_start.preparation_turn_id,
                kernel_reservation_id,
            )?;
        } else {
            start_webui_session_kernel_user_turn(
                &mut guard,
                session_id,
                &precheck_start.preparation_turn_id,
            )?;
        }
        webui_submit_trace(
            "submit_prompt_kernel_turn_started",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(kernel_turn_started_at)
            ),
        );
    }
    mark_webui_prompt_snapshot_preparing(&mut workspace_snapshot, session_id);
    let begin_commit_started_at = current_unix_millis_value();
    store_workspace_snapshot_without_workbench_rebuild(&state, workspace_snapshot.clone())?;
    webui_submit_trace(
        "submit_prompt_begin_workspace_committed",
        format!(
            "session={session_id} elapsed_ms={} elapsed_ms_since_received={}",
            current_unix_millis_value().saturating_sub(begin_commit_started_at),
            current_unix_millis_value().saturating_sub(submit_total_started_at)
        ),
    );
    commit_materialized_submission_activation(
        &app,
        &state,
        &workspace_snapshot,
        &materialized_session,
    )?;
    if let Some(message_id) = precheck_start.visible_user_prompt_record_id.as_deref()
        && let Some(message) = workspace_snapshot
            .session_messages
            .get(&session_id)
            .and_then(|messages| messages.iter().find(|message| message.id == message_id))
    {
        // delta_started_at measures only the post-commit visible projection and event emission.
        // delta_started_at 仅衡量提交后的可见投影与事件发送。
        let delta_started_at = current_unix_millis_value();
        emit_materialized_session_message_delta(
            &app,
            &state,
            session_id,
            message,
            submit_visible_run_phase,
        )?;
        webui_submit_trace(
            "submit_prompt_user_message_delta_ready",
            format!(
                "session={session_id} message_id={message_id} elapsed_ms={} elapsed_ms_since_received={}",
                current_unix_millis_value().saturating_sub(delta_started_at),
                current_unix_millis_value().saturating_sub(submit_total_started_at)
            ),
        );
        tokio::task::yield_now().await;
        webui_submit_trace(
            "submit_prompt_user_message_delta_yielded",
            format!(
                "session={session_id} message_id={message_id} elapsed_ms_since_received={}",
                current_unix_millis_value().saturating_sub(submit_total_started_at)
            ),
        );
    }
    webui_submit_trace(
        "submit_prompt_visible_user_message_emitted",
        format!(
            "session={session_id} elapsed_ms_since_received={}",
            current_unix_millis_value().saturating_sub(submit_total_started_at)
        ),
    );
    let outer_steps_started_at = current_unix_millis_value();
    apply_webui_prompt_begin_outer_steps_deferred_context_persist(
        &controller_runtime,
        &mut workspace_snapshot,
        session_id,
        session_title,
    );
    webui_submit_trace(
        "submit_prompt_outer_steps_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(outer_steps_started_at)
        ),
    );
    let hook_snapshot_started_at = current_unix_millis_value();
    agent_runtime
        .vmm_hook_runtime
        .replace_snapshot(build_vmm_hook_config_snapshot(
            &agent_runtime.provider_configuration,
            &workspace_snapshot,
        ))?;
    webui_submit_trace(
        "submit_prompt_hook_snapshot_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(hook_snapshot_started_at)
        ),
    );
    let precheck_workspace_commit_started_at = current_unix_millis_value();
    store_workspace_snapshot_without_workbench_rebuild(&state, workspace_snapshot)?;
    webui_submit_trace(
        "submit_prompt_precheck_workspace_committed",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(precheck_workspace_commit_started_at)
        ),
    );
    let state_for_persist = state.clone();
    let controller_runtime_for_persist = controller_runtime.clone();
    spawn_service_task(async move {
        let snapshot = match current_workspace_snapshot_from_shared_state(&state_for_persist) {
            Ok(Some(snapshot)) => snapshot,
            Ok(None) => return,
            Err(error) => {
                eprintln!("[webui-service] submit begin persist snapshot failed: {error}");
                return;
            }
        };
        spawn_workspace_session_persist_task(
            state_for_persist,
            controller_runtime_for_persist,
            snapshot,
            session_id,
            Some(String::from("submit_begin")),
        );
    });
    let service_state = state.clone();
    let precheck_dispatch_started_at = current_unix_millis_value();
    let precheck_started = start_webui_precheck_or_continue(
        &app,
        &service_state,
        &controller_runtime,
        &agent_runtime,
        precheck_start,
        precheck_plan,
    )?;
    let revision = current_workbench_revision_from_state(&state)?;
    webui_submit_trace(
        "submit_prompt_precheck_dispatch_returned",
        format!(
            "session={session_id} started={precheck_started} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(precheck_dispatch_started_at)
        ),
    );
    webui_submit_trace(
        "submit_prompt_returning",
        format!(
            "session={session_id} elapsed_ms_since_received={}",
            current_unix_millis_value().saturating_sub(submit_total_started_at)
        ),
    );
    Ok(SubmitConversationPromptAck {
        conversation_id: session_id.to_string(),
        accepted_message_id,
        turn_id,
        run_phase: submit_visible_run_phase.to_string(),
        queued: false,
        revision,
    })
}

/// Reads the latest accepted workbench revision from shared service state.
/// 从共享服务状态读取最新已接受的工作台版本号。
fn current_workbench_revision_from_state(state: &SharedServiceBridgeState) -> Result<u64, String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))
        .map(|guard| guard.workbench_generation)
}

/// Emits the latest queued-prompt projection as an ordered workbench delta.
/// 将最新排队提示词投影作为有序工作台增量发出。
///
/// # Parameters
/// # 参数
/// - `app`: Event emitter that publishes the ordered projection.
/// - `app`：发布有序投影的事件发送器。
/// - `state`: Shared service state containing the authoritative workspace and revision.
/// - `state`：包含权威工作区与版本号的共享服务状态。
/// - `session_id`: Conversation whose current run phase must be projected.
/// - `session_id`：需要投影当前运行阶段的会话。
///
/// # Returns
/// # 返回值
/// The committed workbench revision and authoritative current run phase, or an exact state error.
/// 已提交的工作台版本与权威当前运行阶段，或准确的状态错误。
pub(crate) fn emit_queued_prompt_run_phase_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: Uuid,
) -> Result<(u64, SessionRunPhase), String> {
    let (event, run_phase) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        // workspace is reread under the generation lock so later turn advancement always wins.
        // workspace 在版本锁内重新读取，使之后发生的回合推进始终胜出。
        let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("cannot project queued prompt without a retained workspace snapshot")
        })?;
        // run_phase is the current authoritative phase, never the value captured by queue commit.
        // run_phase 是当前权威阶段，绝不使用排队提交时捕获的旧值。
        let run_phase = workspace
            .session_run_phases
            .get(&session_id)
            .copied()
            .unwrap_or(SessionRunPhase::Idle);
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let revision = guard.workbench_generation;
        guard.workbench.revision = revision;
        let conversation_id = session_id.to_string();
        let run_phase_text = run_phase.to_string();
        let mut status = None;
        let mut last_updated_label = None;
        if let Some(conversation) = guard
            .workbench
            .conversations
            .iter_mut()
            .find(|candidate| candidate.id == conversation_id)
        {
            conversation.run_phase = run_phase_text.clone();
            status = Some(conversation.status.clone());
            last_updated_label = Some(conversation.last_updated_label.clone());
        }
        (
            WorkbenchDeltaEnvelope {
                generation: revision,
                delta: WorkbenchDelta::UpdateConversationRunPhase(
                    UpdateConversationRunPhaseDelta {
                        conversation_id,
                        run_phase: run_phase_text,
                        status,
                        last_updated_label,
                    },
                ),
            },
            run_phase,
        )
    };
    let generation = event.generation;
    emit_service_event_with_trace(
        app,
        "workbench_queued_prompt_run_phase_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok((generation, run_phase))
}

#[cfg(test)]
mod submission_hydration_policy_tests {
    use super::*;

    /// Verifies a first-prompt edit treats its intentionally empty retained history as authoritative.
    /// 验证编辑第一条提示词时会将有意清空的保留历史视为权威状态。
    #[test]
    fn first_prompt_edit_does_not_rehydrate_the_removed_original_prompt() {
        let session_id = SessionId::from_u128(901);

        assert!(!existing_submission_requires_hydration(
            Some(session_id),
            ExistingSessionHydrationPolicy::TrustRetainedSnapshot,
        ));
        assert!(existing_submission_requires_hydration(
            Some(session_id),
            ExistingSessionHydrationPolicy::HydrateIfEmpty,
        ));
    }

    /// Verifies new-session submissions never enter the existing-session hydration path.
    /// 验证新会话提交永远不会进入已有会话水合路径。
    #[test]
    fn new_session_submission_never_requires_existing_session_hydration() {
        assert!(!existing_submission_requires_hydration(
            None,
            ExistingSessionHydrationPolicy::HydrateIfEmpty,
        ));
    }
}
