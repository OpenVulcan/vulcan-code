//! Context compaction completion helpers.
//! 上下文压缩 completion 辅助逻辑。

use super::*;
use crate::require_segmented_session_directory_for_retained_session;

/// Exact active-stream ownership behavior required by one context-compaction commit.
/// 单次上下文压缩提交所需的精确活动流所有权行为。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContextCompactionCommitMode {
    /// Validate the parent Provider stream owner and keep it active after compaction.
    /// 校验父 Provider 流所有者，并在压缩后保持其活动状态。
    ContinueActiveStream {
        /// Exact Provider stream task identity that owns the auto-compaction commit.
        /// 拥有自动压缩提交的精确 Provider 流任务身份。
        task_id: Uuid,
    },
    /// Validate and atomically terminate the standalone manual-compaction stream.
    /// 校验并原子终结独立手动压缩流。
    FinishActiveStream {
        /// Exact manual-compaction task identity that owns the terminal commit.
        /// 拥有终态提交的精确手动压缩任务身份。
        task_id: Uuid,
    },
}

impl ContextCompactionCommitMode {
    /// Returns the exact active-stream task identity required by this commit.
    /// 返回当前提交要求的精确活动流任务身份。
    ///
    /// # Returns
    /// 返回值
    /// The immutable task id captured when the owning stream was registered.
    /// 返回所有者流注册时捕获的不可变 task id。
    const fn task_id(self) -> Uuid {
        match self {
            Self::ContinueActiveStream { task_id } | Self::FinishActiveStream { task_id } => {
                task_id
            }
        }
    }

    /// Returns whether this commit must remove the active task and close its kernel turn.
    /// 返回当前提交是否必须移除活动任务并关闭其 kernel 回合。
    ///
    /// # Returns
    /// 返回值
    /// `true` only for standalone manual-compaction terminal commits.
    /// 仅独立手动压缩终态提交返回 `true`。
    const fn finishes_active_stream(self) -> bool {
        matches!(self, Self::FinishActiveStream { .. })
    }
}

/// Rejects a context-compaction commit unless the exact stream task still owns the session slot.
/// 除非精确流任务仍拥有会话槽位，否则拒绝上下文压缩提交。
///
/// # Parameters
/// 参数
/// `state` contains the authoritative active-stream registry.
/// `state` 包含权威活动流注册表。
/// `session_id` selects the context-compaction session.
/// `session_id` 选择上下文压缩会话。
/// `commit_mode` carries the exact expected task identity.
/// `commit_mode` 携带精确的预期任务身份。
///
/// # Returns
/// 返回值
/// Success for the exact owner, or an explicit stale-owner error without mutation.
/// 精确所有者返回成功；过期所有者返回显式错误且不修改状态。
fn ensure_context_compaction_commit_owner(
    state: &ServiceBridgeState,
    session_id: SessionId,
    commit_mode: ContextCompactionCommitMode,
) -> Result<(), String> {
    active_stream_task_matches_id(state, session_id, commit_mode.task_id())
        .then_some(())
        .ok_or_else(|| {
            format!(
                "context compaction stream ownership was superseded for session {session_id} task {}",
                commit_mode.task_id()
            )
        })
}

/// Atomically removes and closes a standalone manual-compaction stream after domain commit.
/// 在领域提交后原子移除并关闭独立手动压缩流。
///
/// # Parameters
/// 参数
/// `state` owns the active task registry and session runtime kernels.
/// `state` 持有活动任务注册表与会话运行时 kernel。
/// `session_id` selects the committed compaction session.
/// `session_id` 选择已提交的压缩会话。
/// `commit_mode` decides whether the parent stream continues or the manual stream finishes.
/// `commit_mode` 决定父流继续运行或手动流结束。
/// `terminal_status` is the authoritative kernel outcome for a finishing manual stream.
/// `terminal_status` 是结束手动流时使用的权威 kernel 结果。
///
/// # Returns
/// 返回值
/// Success after retaining or exactly removing the owner; an error if ownership changed unexpectedly.
/// 保留或精确移除所有者后返回成功；所有权意外变化时返回错误。
fn finish_context_compaction_stream_owner_if_needed(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    commit_mode: ContextCompactionCommitMode,
    terminal_status: TurnTerminalStatus,
) -> Result<(), String> {
    if !commit_mode.finishes_active_stream() {
        return Ok(());
    }
    remove_active_stream_task_if_id(state, session_id, commit_mode.task_id()).ok_or_else(|| {
        format!(
            "context compaction stream ownership changed during terminal commit for session {session_id} task {}",
            commit_mode.task_id()
        )
    })?;
    finish_webui_session_kernel_turn(state, session_id, terminal_status);
    Ok(())
}

/// Authoritative terminal outcome after a context-compaction workspace commit.
/// 上下文压缩工作区提交后的权威终态结果。
pub(crate) enum ContextCompactionFinishOutcome {
    /// The compacted conversation state was committed and may continue execution.
    /// 压缩后的会话状态已提交，可以继续执行。
    Committed {
        /// Workspace snapshot containing the committed compaction checkpoint.
        /// 包含已提交压缩检查点的工作区快照。
        workspace_snapshot: WorkspaceSnapshot,
    },
    /// The attempt was rejected and its failure record was committed for persistence.
    /// 本次尝试已被拒绝，其失败记录已提交并等待持久化。
    Rejected {
        /// Workspace snapshot containing the committed failure record.
        /// 包含已提交失败记录的工作区快照。
        workspace_snapshot: WorkspaceSnapshot,
        /// Exact failure that must stop automatic continuation.
        /// 必须阻止自动续接的精确失败原因。
        error: String,
    },
}

impl ContextCompactionFinishOutcome {
    /// Returns the rejection diagnostic when execution continuation must stop.
    /// 当必须停止执行续接时返回拒绝诊断。
    ///
    /// # Returns
    /// 返回值
    /// The exact rejection error, or `None` for a committed compaction.
    /// 返回精确拒绝错误；压缩已提交时返回 `None`。
    pub(crate) fn rejection_error(&self) -> Option<&str> {
        match self {
            Self::Committed { .. } => None,
            Self::Rejected { error, .. } => Some(error),
        }
    }

    /// Consumes the outcome and returns the snapshot that must be persisted.
    /// 消费终态结果并返回必须持久化的快照。
    ///
    /// # Returns
    /// 返回值
    /// The committed success or failure workspace snapshot.
    /// 返回已提交的成功或失败工作区快照。
    pub(crate) fn into_workspace_snapshot(self) -> WorkspaceSnapshot {
        match self {
            Self::Committed { workspace_snapshot }
            | Self::Rejected {
                workspace_snapshot, ..
            } => workspace_snapshot,
        }
    }
}

/// Projects committed context-compaction state without reclassifying it as a pre-commit failure.
/// 投影已提交的上下文压缩状态，且不会把它重新归类为提交前失败。
///
/// # Parameters
/// 参数
/// `app` emits focused deltas, `state` owns retained views, `session_id` selects the session, and `phase_event` is the already-committed phase delta.
/// `app` 发送聚焦增量，`state` 持有保留视图，`session_id` 选择会话，`phase_event` 是已提交的阶段增量。
pub(crate) fn project_committed_context_compaction_surfaces(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    phase_event: WorkbenchDeltaEnvelope,
) {
    if let Err(error) = emit_context_compaction_message_window_delta(app, state, session_id) {
        webui_trace(
            "context_compaction_message_projection_failed_after_commit",
            format!("session={session_id} error={error}"),
        );
    }
    emit_service_event_with_trace(
        app,
        "context_compaction_phase_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(phase_event)),
    );
}

/// Dispatches the non-authoritative compacted notification with explicit lock diagnostics.
/// 使用显式锁诊断分发非权威的压缩完成通知。
///
/// # Parameters
/// 参数
/// `agent_runtime` owns the hook dispatcher and the remaining parameters form the committed payload.
/// `agent_runtime` 持有 Hook 分发器，其余参数组成已提交载荷。
///
/// # Returns
/// 返回值
/// Success after scheduling the notification, or an explicit poisoned-lock diagnostic.
/// 通知排队后返回成功；锁中毒时返回显式诊断。
fn dispatch_context_compaction_committed_hook(
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    project_id: Option<String>,
    trigger: &str,
    provider_request_count: usize,
    summary_body: &str,
) -> Result<(), String> {
    // hook_runtime only dispatches an asynchronous notification and does not own compaction state.
    // hook_runtime 仅分发异步通知，不持有上下文压缩状态。
    let mut hook_runtime = agent_runtime
        .conversation_hook_runtime
        .lock()
        .map_err(|error| format!("failed to lock conversation hook runtime: {error}"))?;
    hook_runtime.record_compacted(
        session_id,
        project_id,
        trigger,
        false,
        provider_request_count,
        summary_body,
    );
    Ok(())
}

/// Applies the terminal compaction result, writes its checkpoint, and closes the visible status section.
/// 应用终态压缩结果、写入检查点，并关闭可见状态分段。
///
/// # Parameters
/// 参数
/// `app` emits focused deltas, `state` owns retained state, `agent_runtime` dispatches notifications, and `session_id` identifies the session.
/// `app` 发送聚焦增量，`state` 持有保留状态，`agent_runtime` 分发通知，`session_id` 标识会话。
/// `previous_phase`, `section_id`, `budget_note`, and `trigger` restore and describe the visible compaction section; `execution` is the Provider result.
/// `previous_phase`、`section_id`、`budget_note` 与 `trigger` 恢复并描述可见压缩分段；`execution` 是 Provider 结果。
/// `commit_mode` binds the domain commit to one exact active-stream task and terminal behavior.
/// `commit_mode` 将领域提交绑定到一个精确活动流任务及终态行为。
///
/// # Returns
/// 返回值
/// Returns a committed or rejected durable outcome, or an error only when no terminal workspace state could be committed.
/// 返回已提交或已拒绝的可持久化终态；仅在无法提交任何工作区终态时返回错误。
pub(crate) fn finish_webui_context_compaction_execution(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    previous_phase: SessionRunPhase,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    commit_mode: ContextCompactionCommitMode,
    execution: PromptCompactionExecution,
) -> Result<ContextCompactionFinishOutcome, String> {
    if execution.fallback_used {
        let retry_hint = if trigger == AUTO_CONTEXT_COMPACTION_TRIGGER {
            "自动压缩未完成，已停止本次请求以避免预算溢出。"
        } else {
            "请重新触发 /compact。"
        };
        // rejection_error is retained with the failure snapshot and stops automatic continuation.
        // rejection_error 与失败快照共同保留，并阻止自动续接。
        let rejection_error = format!(
            "API 未返回可用压缩结果，本次上下文压缩已失败。Provider 请求：{}。{retry_hint}",
            execution.provider_request_count,
        );
        // workspace_snapshot contains the committed failed-attempt record that still requires persistence.
        // workspace_snapshot 包含仍需持久化的已提交失败尝试记录。
        let workspace_snapshot = fail_webui_context_compaction_execution(
            app,
            state,
            session_id,
            previous_phase,
            section_id,
            budget_note,
            trigger,
            commit_mode,
            &rejection_error,
        )?;
        return Ok(ContextCompactionFinishOutcome::Rejected {
            workspace_snapshot,
            error: rejection_error,
        });
    }
    let summary_body = execution.compaction_result.summary_record.body.clone();
    let provider_request_count = execution.provider_request_count;
    let runtime_backup_controller = state
        .lock()
        .ok()
        .and_then(|guard| guard.controller_runtime.clone());
    // controller_runtime is required because compaction must durably advance the VMM cache epoch.
    // controller_runtime 是必需的，因为压缩必须持久推进 VMM 缓存纪元。
    let controller_runtime = runtime_backup_controller.as_ref().ok_or_else(|| {
        String::from("Core VMM controller runtime is unavailable during context compaction.")
    })?;
    // vmm_controller_registry is cloned together with the exact session locator.
    // vmm_controller_registry 与精确会话定位一起克隆。
    let (vmm_controller_registry, session_directory) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let session_directory = require_segmented_session_directory_for_retained_session(
            &guard,
            &controller_runtime.data_root,
            session_id,
        )?;
        (guard.vmm_controller_registry.clone(), session_directory)
    };
    match vulcan_workspace_store::workspace_store::backup_session_runtime_log_before_compaction(
        &session_directory,
        session_id,
        &execution.compaction_result.compaction_id,
        &execution.compaction_result.created_at,
    ) {
        Ok(copied_files) if !copied_files.is_empty() => {
            webui_trace(
                "context_compaction_runtime_backup",
                format!("session={session_id} files={}", copied_files.len()),
            );
        }
        Ok(_) => {}
        Err(error) => {
            webui_trace(
                "context_compaction_runtime_backup_failed",
                format!("session={session_id} error={error}"),
            );
        }
    }
    // vmm_controller is the sole state owner for this session's cache epoch.
    // vmm_controller 是该会话缓存纪元的唯一状态所有者。
    let vmm_controller = vmm_controller_registry.get_or_open(
        &session_directory,
        session_id,
        &execution.compaction_result.created_at,
    )?;
    let mut injection_merge_event = None;
    let mut injection_merge_timestamp = String::new();
    let mut ttl_prune_report = InjectionPruneReport::default();
    // vmm_lifecycle_events remain empty unless the compaction draft commits atomically.
    // vmm_lifecycle_events 仅在压缩草稿原子提交后才会包含事件。
    let mut vmm_lifecycle_events = Vec::new();
    let mut draft_error: Option<String> = None;
    // invariant_error distinguishes cross-domain corruption from a rejectable compaction draft.
    // invariant_error 区分跨领域损坏与可拒绝的压缩候选。
    let mut invariant_error: Option<String> = None;
    // committed_phase_event preserves the delta generated by the atomic retained-state mutation.
    // committed_phase_event 保留由原子保留状态变更生成的增量。
    let mut committed_phase_event: Option<WorkbenchDeltaEnvelope> = None;
    let workspace = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        ensure_context_compaction_commit_owner(&guard, session_id, commit_mode)?;
        {
            // workspace is scoped to the domain mutation so phase and kernel state can commit afterward under the same guard.
            // workspace 的作用域仅覆盖领域变更，以便阶段与内核状态随后在同一 guard 下提交。
            let workspace = guard
                .workspace_snapshot
                .as_mut()
                .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
            // checkpoint_already_committed proves the durable compaction identity is present in ordinary retained state.
            // checkpoint_already_committed 证明普通保留状态中已经存在该持久压缩身份。
            let checkpoint_already_committed = workspace
                .session_compactions
                .get(&session_id)
                .is_some_and(|checkpoints| {
                    checkpoints.iter().any(|checkpoint| {
                        checkpoint.id == execution.compaction_result.compaction_id
                    })
                });
            let now = current_unix_millis_text();
            let runtime_state = workspace
                .session_runtime_state
                .entry(session_id)
                .or_insert_with(|| SessionRuntimeStateEntry {
                    updated_at: now.clone(),
                    ..SessionRuntimeStateEntry::default()
                });
            // vmm_command_id makes one exact compaction epoch transition durably idempotent.
            // vmm_command_id 使一个精确压缩纪元转换具备持久幂等性。
            let vmm_command_id = format!(
                "context_compaction:{}",
                execution.compaction_result.compaction_id
            );
            // runtime_already_committed must agree with the checkpoint collection before duplicate recovery is allowed.
            // runtime_already_committed 在允许重复恢复前必须与检查点集合保持一致。
            let runtime_already_committed = runtime_state.last_compaction_id.as_deref()
                == Some(execution.compaction_result.compaction_id.as_str());
            if checkpoint_already_committed != runtime_already_committed {
                // error preserves the exact partial ordinary-domain identity for fail-stop diagnostics.
                // error 为故障停止诊断保留精确的普通领域部分提交身份。
                let error = format!(
                    "Context compaction `{}` has a partial ordinary-state commit; automatic compatibility recovery is disabled.",
                    execution.compaction_result.compaction_id
                );
                invariant_error = Some(error.clone());
                draft_error = Some(error);
            }
            // ordinary_state_already_committed identifies a fully installed idempotent retry.
            // ordinary_state_already_committed 标识普通状态已经完整安装的幂等重试。
            let ordinary_state_already_committed =
                checkpoint_already_committed && runtime_already_committed;
            if ordinary_state_already_committed {
                // committed_vmm_state must prove the matching controller receipt and cache epoch.
                // committed_vmm_state 必须证明匹配的控制器回执与缓存纪元。
                let committed_vmm_state = vmm_controller.snapshot()?;
                if !committed_vmm_state.has_committed_controller_command(&vmm_command_id)
                    || committed_vmm_state
                        .cache_epoch
                        .covered_compaction_id
                        .as_deref()
                        != Some(execution.compaction_result.compaction_id.as_str())
                {
                    // error identifies a current-format cross-domain invariant violation without fallback.
                    // error 标识当前格式的跨领域约束违规，且不提供回退。
                    let error = format!(
                        "Context compaction `{}` ordinary state is committed without the matching Core VMM command and cache epoch.",
                        execution.compaction_result.compaction_id
                    );
                    invariant_error = Some(error.clone());
                    draft_error = Some(error);
                }
            }
            if draft_error.is_none() && !ordinary_state_already_committed {
                // built_draft crosses the controller closure without exposing mutable VMM ownership.
                // built_draft 跨越控制器闭包传递，且不暴露可变 VMM 所有权。
                let mut built_draft = None;
                // command_result returns either the first committed draft or a reconstruction from the same committed state.
                // command_result 返回首次提交候选，或从同一已提交状态重建的候选。
                let command_result = vmm_controller
                    .execute_command(
                        &vmm_command_id,
                        &execution.compaction_result.created_at,
                        |candidate| {
                            // draft is built against the exact serialized controller candidate.
                            // draft 基于精确的串行控制器候选状态构建。
                            let draft = build_webui_context_compaction_draft(
                                runtime_state,
                                candidate,
                                session_id,
                                section_id,
                                budget_note,
                                trigger,
                                provider_request_count,
                                &summary_body,
                                &execution,
                                agent_runtime.provider_configuration.prompt.output_style,
                            )?;
                            if !draft.baseline.matches(runtime_state) {
                                return Err(String::from(
                                    "上下文压缩候选状态基线失配，已保留当前会话状态并取消提交。",
                                ));
                            }
                            built_draft = Some(draft);
                            Ok(())
                        },
                    )
                    .and_then(|vmm_commit| {
                        // lifecycle_events exist only for the first domain transition, never for a duplicate reconstruction.
                        // lifecycle_events 仅由首次领域转换生成，重复重建绝不重复生成。
                        let lifecycle_events = if vmm_commit.applied {
                            crate::vmm_runtime_audit::build_vmm_lifecycle_events_for_state_diff(
                                &vmm_commit.previous_state,
                                &vmm_commit.state,
                                None,
                                crate::vmm_runtime_audit::VmmRemovedLeaseAuditAction::Expired,
                            )
                        } else {
                            Vec::new()
                        };
                        // draft is either the closure result or a deterministic ordinary-state repair.
                        // draft 为闭包结果或确定性的普通状态修复候选。
                        let draft = if vmm_commit.applied {
                            built_draft.take().ok_or_else(|| {
                                String::from(
                                    "VMM context compaction committed without a runtime draft.",
                                )
                            })?
                        } else {
                            // replay_candidate proves rebuilding ordinary state does not mutate the committed VMM revision.
                            // replay_candidate 证明重建普通状态不会修改已提交的 VMM 修订。
                            let mut replay_candidate = vmm_commit.state.clone();
                            // replay_draft deterministically reconstructs only the missing ordinary projection.
                            // replay_draft 仅确定性重建缺失的普通投影。
                            let replay_draft = build_webui_context_compaction_draft(
                                runtime_state,
                                &mut replay_candidate,
                                session_id,
                                section_id,
                                budget_note,
                                trigger,
                                provider_request_count,
                                &summary_body,
                                &execution,
                                agent_runtime.provider_configuration.prompt.output_style,
                            )?;
                            if replay_candidate != vmm_commit.state {
                                return Err(format!(
                                    "Duplicate context compaction `{}` attempted to mutate its committed Core VMM state.",
                                    execution.compaction_result.compaction_id
                                ));
                            }
                            replay_draft
                        };
                        Ok((draft, lifecycle_events))
                    });
                match command_result {
                    Ok((draft, lifecycle_events)) => {
                        injection_merge_event = draft.injection_merge_event.clone();
                        injection_merge_timestamp = draft.injection_merge_timestamp.clone();
                        ttl_prune_report = draft.ttl_prune_report;
                        vmm_lifecycle_events = lifecycle_events;
                        // checkpoint is the single ordinary-state identity installed for this compaction.
                        // checkpoint 是为本次压缩安装的唯一普通状态身份。
                        let checkpoint = draft.checkpoint;
                        *runtime_state = draft.runtime_state;
                        workspace
                            .session_compactions
                            .entry(session_id)
                            .or_default()
                            .push(checkpoint);
                        // runtime_state_snapshot feeds display projection from the just-installed state.
                        // runtime_state_snapshot 从刚安装的状态生成显示投影。
                        let runtime_state_snapshot = runtime_state.clone();
                        merge_runtime_state_records_into_display_messages(
                            workspace,
                            session_id,
                            &runtime_state_snapshot,
                        );
                        workspace
                            .session_run_phases
                            .insert(session_id, previous_phase);
                        if let Some(session) = workspace
                            .sessions
                            .iter_mut()
                            .find(|session| session.id == session_id)
                        {
                            session.status = SessionState::Completed;
                            session.last_updated_label = String::from("刚刚");
                            session.updated_at = current_unix_millis_text();
                        }
                        sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
                    }
                    Err(error) => {
                        draft_error = Some(error);
                    }
                }
            }
        }
        if draft_error.is_none() {
            // phase_event is generated while exact task ownership is still protected by the transaction guard.
            // phase_event 在精确任务所有权仍受事务 guard 保护时生成。
            let phase_event = apply_session_phase_to_state(&mut guard, session_id, previous_phase)?;
            finish_context_compaction_stream_owner_if_needed(
                &mut guard,
                session_id,
                commit_mode,
                TurnTerminalStatus::Completed,
            )?;
            committed_phase_event = Some(phase_event);
        }
        // workspace_snapshot includes kernel terminalization synchronized by the same retained-state transaction.
        // workspace_snapshot 包含由同一保留状态事务同步的内核终结结果。
        guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?
            .clone()
    };
    if let Some(error) = invariant_error {
        return Err(error);
    }
    if let Some(error) = draft_error {
        // workspace_snapshot retains the rejected draft diagnostic for persistence and display.
        // workspace_snapshot 为持久化与展示保留被拒绝草稿的诊断。
        let workspace_snapshot = fail_webui_context_compaction_execution(
            app,
            state,
            session_id,
            previous_phase,
            section_id,
            budget_note,
            trigger,
            commit_mode,
            &error,
        )?;
        return Ok(ContextCompactionFinishOutcome::Rejected {
            workspace_snapshot,
            error,
        });
    }
    // phase_event must exist for every accepted draft because it is created in the same success branch.
    // 每个已接受草稿都必须存在 phase_event，因为它与成功分支在同一事务中创建。
    let phase_event = committed_phase_event.ok_or_else(|| {
        String::from("context compaction committed without an aligned phase delta")
    })?;
    project_committed_context_compaction_surfaces(app, state, session_id, phase_event);
    // VMM Hook state is a derived projection and cannot invalidate an already-committed compaction epoch.
    // VMM Hook 状态是派生投影，不得使已提交的压缩纪元失效。
    if let Err(error) = refresh_exact_vmm_hook_session_projection(
        &agent_runtime.vmm_hook_runtime,
        &workspace,
        session_id,
    ) {
        webui_trace(
            "context_compaction_vmm_snapshot_refresh_failed_after_commit",
            format!("session={session_id} error={error}"),
        );
    }
    if ttl_prune_report.removed_item_count > 0 || ttl_prune_report.removed_group_count > 0 {
        webui_trace(
            "context_compaction_pruned_expired_injections",
            format!(
                "session={session_id} items={} groups={}",
                ttl_prune_report.removed_item_count, ttl_prune_report.removed_group_count
            ),
        );
    }
    if let Some(event) = injection_merge_event
        && let Err(error) = record_webui_runtime_injection_merged(
            &session_directory,
            session_id,
            event,
            &injection_merge_timestamp,
        )
    {
        webui_trace(
            "runtime_injection_merged_audit_failed",
            format!("session={session_id} error={error}"),
        );
    }
    if !vmm_lifecycle_events.is_empty()
        && let Err(error) = crate::vmm_runtime_audit::record_vmm_lifecycle_events(
            &session_directory,
            session_id,
            &execution.compaction_result.created_at,
            vmm_lifecycle_events,
        )
    {
        webui_trace(
            "runtime_vmm_compaction_audit_failed",
            format!("session={session_id} error={error}"),
        );
    }
    if let Err(error) = dispatch_context_compaction_committed_hook(
        agent_runtime,
        session_id,
        owning_project_id_for_session(&workspace, session_id),
        trigger,
        provider_request_count,
        &summary_body,
    ) {
        webui_trace(
            "context_compaction_hook_dispatch_failed_after_commit",
            format!("session={session_id} error={error}"),
        );
    }
    Ok(ContextCompactionFinishOutcome::Committed {
        workspace_snapshot: workspace,
    })
}

/// Records a failed manual context-compaction attempt without moving the conversation anchor.
/// 记录失败的手动上下文压缩尝试，并且不移动会话上下文锚点。
///
/// # Parameters
/// 参数
/// `app` emits focused deltas, `state` owns retained state, and `session_id` identifies the failed session.
/// `app` 发送聚焦增量，`state` 持有保留状态，`session_id` 标识失败会话。
/// `previous_phase`, `section_id`, `budget_note`, and `trigger` identify the visible attempt; `error` is its exact rejection diagnostic.
/// `previous_phase`、`section_id`、`budget_note` 与 `trigger` 标识可见尝试；`error` 是其精确拒绝诊断。
/// `commit_mode` binds failure mutation and optional stream termination to one exact owner.
/// `commit_mode` 将失败修改及可选流终结绑定到一个精确所有者。
///
/// # Returns
/// 返回值
/// Returns the workspace snapshot containing the committed failure record, or a pre-commit state error.
/// 返回包含已提交失败记录的工作区快照，或提交前状态错误。
pub(crate) fn fail_webui_context_compaction_execution(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    previous_phase: SessionRunPhase,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    commit_mode: ContextCompactionCommitMode,
    error: &str,
) -> Result<WorkspaceSnapshot, String> {
    // committed_state carries the post-terminal workspace and its aligned phase delta.
    // committed_state 携带终结后的工作区及其对齐阶段增量。
    let (workspace, phase_event) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        ensure_context_compaction_commit_owner(&guard, session_id, commit_mode)?;
        {
            // workspace is scoped to the failure mutation before the transaction updates shared phase state.
            // workspace 的作用域仅覆盖失败变更，随后事务再更新共享阶段状态。
            let workspace = guard
                .workspace_snapshot
                .as_mut()
                .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
            apply_webui_context_compaction_failure_to_workspace(
                workspace,
                session_id,
                previous_phase,
                section_id,
                budget_note,
                trigger,
                error,
            )?;
        }
        // phase_event is derived from the same failure commit while exact ownership remains locked.
        // phase_event 在精确所有权仍被锁定时由同一失败提交生成。
        let phase_event = apply_session_phase_to_state(&mut guard, session_id, previous_phase)?;
        finish_context_compaction_stream_owner_if_needed(
            &mut guard,
            session_id,
            commit_mode,
            TurnTerminalStatus::Failed,
        )?;
        // workspace_snapshot includes the kernel state synchronized by terminalization.
        // workspace_snapshot 包含终结时同步的内核状态。
        let workspace_snapshot = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?
            .clone();
        (workspace_snapshot, phase_event)
    };
    project_committed_context_compaction_surfaces(app, state, session_id, phase_event);
    Ok(workspace)
}

/// Applies failed context-compaction state inside one retained workspace snapshot.
/// 在单个保留的工作区快照内应用上下文压缩失败状态。
pub(crate) fn apply_webui_context_compaction_failure_to_workspace(
    workspace: &mut WorkspaceSnapshot,
    session_id: SessionId,
    previous_phase: SessionRunPhase,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    error: &str,
) -> Result<(), String> {
    let now = current_unix_millis_text();
    let mut runtime_state = workspace
        .session_runtime_state
        .get(&session_id)
        .cloned()
        .unwrap_or_else(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    set_webui_context_compaction_section_metadata_in_state(
        &mut runtime_state,
        section_id,
        section_id,
        "start",
        "failed",
        Some(budget_note),
        trigger,
    );
    update_webui_context_compaction_body_state(&mut runtime_state, section_id, "failed");
    append_webui_context_compaction_failed_message_to_state(
        &mut runtime_state,
        session_id,
        section_id,
        budget_note,
        trigger,
        error,
    )?;
    if trigger == AUTO_CONTEXT_COMPACTION_TRIGGER {
        runtime_state.auto_compaction_consecutive_failures = runtime_state
            .auto_compaction_consecutive_failures
            .saturating_add(1);
    }
    runtime_state.updated_at = now;
    let runtime_state_snapshot = runtime_state.clone();
    workspace
        .session_runtime_state
        .insert(session_id, runtime_state);
    merge_runtime_state_records_into_display_messages(
        workspace,
        session_id,
        &runtime_state_snapshot,
    );
    workspace
        .session_run_phases
        .insert(session_id, previous_phase);
    if let Some(session) = workspace
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    {
        session.status = SessionState::Completed;
        session.last_updated_label = String::from("刚刚");
        session.updated_at = current_unix_millis_text();
    }
    sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
    Ok(())
}
