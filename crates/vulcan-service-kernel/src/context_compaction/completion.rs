//! Context compaction completion helpers.
//! 上下文压缩 completion 辅助逻辑。

use super::*;

/// Verifies `execution` against exact physical `durability` and builds the commit for `retained_from_seq`.
/// 根据精确物理`durability`验证`execution`，并为`retained_from_seq`构建提交。
/// Returns a mutually exclusive local-summary or native-checkpoint input; display text never proves native output.
/// 返回互斥的本地摘要或原生检查点输入；展示文字不能证明原生输出。
pub(crate) fn verified_compaction_commit_input(
    execution: &PromptCompactionExecution,
    durability: &crate::agent_stream::ProviderV1FinishedDurability,
    retained_from_seq: Seq,
) -> Result<vulcan_session_core::v1::document::CompactionCommitInput, String> {
    use vulcan_agent_runtime::agent_runtime::prompt_compaction::PromptCompactionExecutionProfile;
    use vulcan_session_core::v1::{
        context::RemoteResponsesCheckpoint, document::CompactionCommitInput,
    };
    if execution.fallback_used
        || execution.provider_request_count == 0
        || durability.api_terminal_failure_persisted
    {
        return Err(String::from(
            "Compaction commit requires a successful physical execution.",
        ));
    }
    // The exact terminal and admitted baseline belong to the final physical attempt, not an earlier retry.
    // 精确终态与准入基线属于最终物理尝试，而不是先前重试。
    let terminal = durability
        .api_terminal
        .as_ref()
        .ok_or_else(|| String::from("Compaction has no durable API terminal receipt."))?;
    // baseline is the exact admitted static context, never rebuilt from current settings at commit.
    // baseline 是精确准入的静态上下文，提交时绝不从当前设置重建。
    let baseline = durability
        .locked_baseline
        .as_ref()
        .ok_or_else(|| String::from("Compaction has no admitted locked baseline."))?;
    // result is an independent runtime report that must match the sink receipt below.
    // result 是独立运行时报告，必须与下方接收器回执相等。
    let result = &execution.compaction_result;
    match execution.execution_profile {
        PromptCompactionExecutionProfile::RemoteResponses => {
            if durability.completed_assistant_text.is_some() {
                return Err(String::from(
                    "Native compaction cannot commit assistant summary semantics.",
                ));
            }
            // The sink supplies native bytes only after checking the real admitted route and successful terminal.
            // 接收器仅在核对实际准入路由及成功终态后提供原生字节。
            let remote = durability
                .completed_remote_compaction
                .as_ref()
                .ok_or_else(|| {
                    String::from("Remote compaction has no verified native terminal receipt.")
                })?;
            // route belongs to the reported native bytes and cannot be inferred from model display labels.
            // route 属于报告的原生字节，不能从模型显示名称推导。
            let route = result.provider_context_route.as_ref().ok_or_else(|| {
                String::from("Remote compaction result is missing its native route identity.")
            })?;
            // checkpoint converts the declared result without aliases, fallback fields, or diagnostic text.
            // checkpoint 转换已声明结果，不使用别名、候选字段或诊断文字。
            let checkpoint = RemoteResponsesCheckpoint {
                provider_id: route.provider_id.clone(),
                connection_id: route.connection_id.clone(),
                route_fingerprint: route.route_fingerprint.clone(),
                protocol_profile_id: route.protocol_profile_id.clone(),
                model_id: route.model_id.clone(),
                output_items: result.provider_context_items.clone(),
                response_id: result.provider_context_response_id.clone(),
            };
            if remote.api_call_id != terminal.value.api_call_id
                || remote.checkpoint != checkpoint
                || execution.provider_id.as_deref() != Some(checkpoint.provider_id.as_str())
                || execution.model_id.as_deref() != Some(checkpoint.model_id.as_str())
            {
                return Err(String::from(
                    "Remote compaction result differs from its exact admitted physical receipt.",
                ));
            }
            Ok(CompactionCommitInput::remote_responses(
                remote.api_call_id,
                terminal.value.stats_seq,
                retained_from_seq,
                checkpoint,
                baseline.clone(),
            ))
        }
        PromptCompactionExecutionProfile::CachePreservingFullFidelity
        | PromptCompactionExecutionProfile::PrunedMinimal => {
            if durability.completed_remote_compaction.is_some()
                || !result.provider_context_items.is_empty()
                || result.provider_context_route.is_some()
                || result.provider_context_usage.is_some()
                || result.provider_context_response_id.is_some()
            {
                return Err(String::from(
                    "Local compaction cannot commit native remote output.",
                ));
            }
            // Local summary identity includes the physical API as well as ordering before its stats event.
            // 本地摘要身份同时包含物理 API 及先于其统计事件的顺序。
            let assistant = durability
                .completed_assistant_text
                .as_ref()
                .ok_or_else(|| {
                    String::from("Local compaction has no durable assistant receipt.")
                })?;
            if assistant.api_call_id != terminal.value.api_call_id
                || assistant.seq.value() >= terminal.value.stats_seq.value()
            {
                return Err(String::from(
                    "Compaction assistant and API terminal receipts do not match.",
                ));
            }
            // summary retains exact persisted model text after removing only the known display wrapper.
            // summary 仅移除已知展示包装后保留精确持久模型正文。
            let summary =
                verified_compaction_summary_body(&result.summary_record.body, &assistant.text)?;
            Ok(CompactionCommitInput::new(
                assistant.api_call_id,
                assistant.seq,
                retained_from_seq,
                summary,
                baseline.clone(),
            ))
        }
    }
}

/// Verifies the model summary inside a display record against the exact durable assistant text.
/// 校验显示记录中的模型摘要是否与精确持久化助手正文一致。
/// Accepts the Agent summary record and V1 receipt text; returns canonical text or an explicit mismatch.
/// 接受 Agent 摘要记录与 V1 回执正文，返回规范正文或明确的不一致错误。
pub(crate) fn verified_compaction_summary_body(
    summary_record_body: &str,
    durable_assistant_text: &str,
) -> Result<String, String> {
    // The Agent adds continuation instructions for display; V1 persists only the model summary.
    // Agent 为显示添加续接指令；V1 仅持久化模型摘要。
    let summary = vulcan_context_compaction::extract_compaction_model_summary(summary_record_body)
        .ok_or_else(|| String::from("Context compaction result has no valid canonical summary."))?;
    if summary != durable_assistant_text {
        return Err(String::from(
            "Context compaction summary differs from the exact durable assistant semantic.",
        ));
    }
    Ok(summary)
}

/// Checks production summary wrapping without weakening exact durable-text equality.
/// 检查生产摘要包装，同时保持精确持久正文相等要求。
#[cfg(test)]
#[test]
fn compaction_summary_verification_preserves_canonical_identity() {
    // The standard constructor is the same continuation wrapper used by CompactionResult.
    // 标准构造器与 CompactionResult 使用相同的续接包装。
    let summary = "Preserve the PostgreSQL decision.";
    let wrapped = vulcan_context_compaction::build_compaction_continuation_prompt(summary);
    assert_ne!(wrapped, summary);
    assert_eq!(
        verified_compaction_summary_body(&wrapped, summary).expect("verified continuation wrapper"),
        summary
    );
    assert_eq!(
        verified_compaction_summary_body(summary, summary).expect("verified plain summary"),
        summary
    );
    assert!(verified_compaction_summary_body(&wrapped, "Different decision.").is_err());
    assert!(verified_compaction_summary_body(&wrapped, &format!("{summary} ")).is_err());
    assert!(verified_compaction_summary_body("", "").is_err());
}

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
    vmm_runtime_ready: bool,
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
        vmm_runtime_ready,
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
pub(crate) async fn finish_webui_context_compaction_execution(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    previous_phase: SessionRunPhase,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    commit_mode: ContextCompactionCommitMode,
    execution: WebuiContextCompactionExecution,
) -> Result<ContextCompactionFinishOutcome, String> {
    let WebuiContextCompactionExecution {
        execution,
        retained_from_seq,
        durability,
    } = execution;
    if execution.fallback_used {
        let retry_hint = if trigger == AUTO_CONTEXT_COMPACTION_TRIGGER {
            "自动压缩未完成，已停止本次请求以避免预算溢出。"
        } else {
            "请重新触发 /compact。"
        };
        // rejectionError is retained with the failure snapshot and stops automatic continuation.
        // rejectionError 与失败快照共同保留，并阻止自动续接。
        let rejection_error = format!(
            "API 未返回可用压缩结果，本次上下文压缩已失败。Provider 请求：{}。{retry_hint}",
            execution.provider_request_count,
        );
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

    // Owner validation happens before the durable context publication so a stale
    // stream can never advance the authoritative context generation.
    // 在可靠上下文发布前验证 owner，避免过期流推进权威上下文代次。
    {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        ensure_context_compaction_commit_owner(&guard, session_id, commit_mode)?;
    }

    let provider_request_count = execution.provider_request_count;
    // Root and child commits share the exact local/native physical receipt validation.
    // 主子提交共用精确本地/原生物理回执校验。
    let commit_input =
        verified_compaction_commit_input(&execution, &durability, retained_from_seq)?;
    // Native diagnostics are display-only; their text never enters the durable checkpoint node.
    // 原生诊断仅供展示，其文字绝不进入可靠检查点节点。
    let summary_body = match &commit_input.content {
        vulcan_session_core::v1::context::CompactionContent::Summary { text } => text.clone(),
        vulcan_session_core::v1::context::CompactionContent::RemoteResponses { .. } => {
            execution.compaction_result.summary_record.body.clone()
        }
    };
    let now_ms = i64::try_from(current_unix_millis_value()).map_err(|_| {
        String::from("Current Unix millisecond timestamp exceeds the V1 i64 contract.")
    })?;
    let handle =
        crate::session_goal_lifecycle::registered_session_v1_handle_from_state(state, session_id)?;
    let compaction_receipt = match handle.record_compaction(commit_input, now_ms) {
        Ok(receipt) => receipt,
        Err(error) => {
            let rejection_error =
                format!("Context compaction V1 context publication failed: {error}");
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
    };

    // VMM is derived from the committed V1 ID; failures stop continuation without pretending to roll back that commit.
    // VMM 派生自已提交 V1 ID；失败会停止续接，但不会伪称回滚该提交。
    // Read generation and compaction together from the same authoritative owner after commit.
    // 提交后从同一权威归属方同时读取代次与压缩身份。
    let (generation, current_compaction_id) = handle
        .read(|document| {
            (
                document.segment_generation(),
                document.runtime_lock.current_compaction_id,
            )
        })
        .map_err(|error| format!("Failed to read committed V1 context epoch: {error}"))?;
    if current_compaction_id != Some(compaction_receipt.value) {
        return Err("V1 context changed before committed VMM compaction publication.".to_owned());
    }
    // Keep the optional compaction encoding identical to ordinary Provider admission.
    // 保持可选压缩编码与普通 Provider 准入相同。
    let context = vulcan_vmm::VmmContextEpoch::new(
        generation,
        Some(compaction_receipt.value.value().to_string()),
    )?;
    super::vmm_projection::publish_vmm_compaction_epoch(state, agent_runtime, session_id, &context)
        .await
        .map_err(|error| {
            format!("V1 compaction committed, but VMM epoch publication failed: {error}")
        })?;

    // The retained workspace is now display-only projection. It must not retain
    // another compaction checkpoint or runtime anchor.
    // 此后的保留工作区仅是显示投影，不得再保存另一份压缩检查点或 runtime 锚点。
    let (workspace, phase_event) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        ensure_context_compaction_commit_owner(&guard, session_id, commit_mode)?;
        {
            let workspace = guard
                .workspace_snapshot
                .as_mut()
                .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
            workspace
                .session_run_phases
                .insert(session_id, previous_phase);
            // This is only the Hook's derived identity hint; the V1 receipt remains the sole compaction authority.
            // 此值仅是 Hook 的派生身份提示；V1 回执仍是唯一压缩权威。
            workspace
                .session_runtime_state
                .get_mut(&session_id)
                .ok_or_else(|| {
                    "Committed V1 compaction has no active session projection.".to_owned()
                })?
                .last_compaction_id = Some(compaction_receipt.value.value().to_string());
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
        let phase_event = apply_session_phase_to_state(&mut guard, session_id, previous_phase)?;
        finish_context_compaction_stream_owner_if_needed(
            &mut guard,
            session_id,
            commit_mode,
            TurnTerminalStatus::Completed,
        )?;
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?
            .clone();
        (workspace, phase_event)
    };
    let vmm_runtime_ready = webui_vmm_runtime_readiness(state)?.service_ready;

    project_committed_context_compaction_surfaces(app, state, session_id, phase_event);
    // VMM Hook state is a derived projection and cannot invalidate the already
    // durable V1 context generation.
    // VMM Hook 状态是派生投影，不得使已经可靠发布的 V1 上下文代次失效。
    if vmm_runtime_ready
        && let Err(error) = refresh_exact_vmm_hook_session_projection(
            &agent_runtime.vmm_hook_runtime,
            &workspace,
            session_id,
        )
    {
        webui_trace(
            "context_compaction_vmm_snapshot_refresh_failed_after_commit",
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
        vmm_runtime_ready,
    ) {
        webui_trace(
            "context_compaction_hook_dispatch_failed_after_commit",
            format!("session={session_id} error={error}"),
        );
    }
    webui_trace(
        "context_compaction_v1_committed",
        format!(
            "session={session_id} compaction_id={} revision={} changed={}",
            compaction_receipt.value.value(),
            compaction_receipt.revision,
            compaction_receipt.changed
        ),
    );
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
    workspace
        .session_runtime_state
        .insert(session_id, runtime_state);
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
