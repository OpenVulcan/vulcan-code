//! Context compaction auto compaction helpers.
//! 上下文压缩 auto compaction 辅助逻辑。

use super::*;

/// Runs budget-triggered context compaction before one exact active Provider stream continues.
/// 在一个精确活动 Provider 流继续前执行预算触发的上下文压缩。
///
/// # Parameters
/// 参数
/// `app` emits committed deltas, `state` owns retained service state, and `controller_runtime` persists terminal snapshots.
/// `app` 发送已提交增量，`state` 持有保留服务状态，`controller_runtime` 持久化终态快照。
/// `agent_runtime` executes compaction, `active_stream_task_id` proves exact stream ownership, and `request` with `preparation` supplies the pending Provider request and budget facts.
/// `agent_runtime` 执行压缩，`active_stream_task_id` 证明精确流所有权，`request` 与 `preparation` 提供待执行 Provider 请求及预算事实。
///
/// # Returns
/// 返回值
/// Returns a refreshed request after committed compaction, `None` when compaction is unnecessary, or an explicit terminal failure.
/// 压缩提交后返回刷新请求；无需压缩时返回 `None`；失败时返回显式终态错误。
pub(crate) async fn maybe_auto_compact_webui_session_before_provider_request(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    active_stream_task_id: Uuid,
    request: &ExecutionPreparationRequest,
    preparation: &ExecutionPreparation,
) -> Result<Option<ExecutionPreparationRequest>, String> {
    if request.prompt_purpose != ExecutionPromptPurpose::UserTurn {
        return Ok(None);
    }
    let Some(context_budget) = preparation.context_budget.as_ref() else {
        return Ok(None);
    };
    // vmm_controller_registry is cloned without retaining the shared service lock.
    // vmm_controller_registry 在不保留共享服务锁的情况下完成克隆。
    let vmm_controller_registry = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .vmm_controller_registry
        .clone();
    // vmm_controller is the sole owner used for pressure and refreshed injection projections.
    // vmm_controller 是压力与刷新注入投影使用的唯一所有者。
    let vmm_controller = vmm_controller_registry.get_or_open(
        &controller_runtime.data_root,
        request.session_id,
        &current_unix_millis_text(),
    )?;
    // vmm_cache_pressure measures immutable text that can only be reclaimed at a compaction boundary.
    // vmm_cache_pressure 衡量只能在压缩边界回收的不可变文本。
    let vmm_cache_pressure = vmm_controller.snapshot()?.cache_pressure();
    // vmm_pressure_requires_compaction raises priority only after an absolute or proportional threshold.
    // vmm_pressure_requires_compaction 仅在达到绝对或比例阈值后提高压缩优先级。
    let vmm_pressure_requires_compaction =
        vmm_cache_pressure_requires_compaction(context_budget, vmm_cache_pressure);
    if !context_budget.should_compact() && !vmm_pressure_requires_compaction {
        return Ok(None);
    }
    let session_id = request.session_id;
    let budget_note = format!(
        "{}；VMM 可回收锚点约 {} tokens（{} bytes）",
        context_budget.summary_note(),
        vmm_cache_pressure.reclaimable_anchor_estimated_tokens,
        vmm_cache_pressure.reclaimable_anchor_bytes,
    );
    webui_trace(
        "stream_auto_compaction_needed",
        format!(
            "session={session_id} severity={:?} usage={} vmm_reclaimable_tokens={} vmm_pressure_trigger={vmm_pressure_requires_compaction}",
            context_budget.severity,
            context_budget.usage_label(),
            vmm_cache_pressure.reclaimable_anchor_estimated_tokens,
        ),
    );

    let full_context_state = request.conversation_state.clone();
    let context_start_message_id = current_workspace_snapshot_from_shared_state(state)
        .ok()
        .flatten()
        .and_then(|workspace| {
            workspace
                .session_runtime_state
                .get(&session_id)
                .and_then(|runtime_state| runtime_state.context_start_message_id.clone())
        });
    // preparation_commit carries the previous phase, status id, and aligned delta from one exact-owner mutation.
    // preparation_commit 携带由一次精确所有者变更生成的前序阶段、状态标识与对齐增量。
    let (previous_phase, status_message_id, phase_event) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if !active_stream_task_matches_id(&guard, session_id, active_stream_task_id) {
            return Err(format!(
                "context compaction ownership was superseded before preparation: session={session_id} task={active_stream_task_id}"
            ));
        }
        // preparation_state is created while exact stream ownership remains protected by the state guard.
        // preparation_state 在精确流所有权仍受状态 guard 保护时创建。
        let (previous_phase, status_message_id) = {
            let workspace = guard
                .workspace_snapshot
                .as_mut()
                .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
            let previous_phase = workspace
                .session_run_phases
                .get(&session_id)
                .copied()
                .unwrap_or(SessionRunPhase::Preparing);
            let status_message_id = append_webui_context_compaction_status_message_to_workspace(
                workspace,
                session_id,
                &budget_note,
                AUTO_CONTEXT_COMPACTION_TRIGGER,
            )?;
            workspace
                .session_run_phases
                .insert(session_id, SessionRunPhase::Preparing);
            if let Some(session) = workspace
                .sessions
                .iter_mut()
                .find(|session| session.id == session_id)
            {
                session.status = SessionState::Running;
                session.last_updated_label = String::from("刚刚");
                session.updated_at = current_unix_millis_text();
            }
            (previous_phase, status_message_id)
        };
        // phase_event is generated before releasing ownership so replacement work cannot be overwritten.
        // phase_event 在释放所有权前生成，避免替代任务被覆盖。
        let phase_event =
            apply_session_phase_to_state(&mut guard, session_id, SessionRunPhase::Preparing)?;
        (previous_phase, status_message_id, phase_event)
    };
    emit_context_compaction_message_window_delta(app, state, session_id)?;
    emit_service_event_with_trace(
        app,
        "context_compaction_preparing_phase_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(phase_event)),
    );

    if webui_auto_context_compaction_failure_count(state, session_id)
        >= MAX_CONSECUTIVE_AUTO_COMPACTION_FAILURES
    {
        let error = format!(
            "自动上下文压缩已连续失败 {MAX_CONSECUTIVE_AUTO_COMPACTION_FAILURES} 次，本次请求已停止，避免覆盖或反复改写当前会话状态。"
        );
        persist_failed_webui_context_compaction(
            app,
            state,
            controller_runtime,
            active_stream_task_id,
            session_id,
            previous_phase,
            &status_message_id,
            &budget_note,
            AUTO_CONTEXT_COMPACTION_TRIGGER,
            &error,
        )?;
        return Err(error);
    }

    let plan = match webui_context_compaction_plan_from_conversation_state(
        session_id,
        &full_context_state,
        context_start_message_id.as_deref(),
        "自动上下文压缩未找到可压缩的历史上下文，已停止本次请求以避免预算溢出。",
    ) {
        Ok(plan) => plan,
        Err(error) => {
            persist_failed_webui_context_compaction(
                app,
                state,
                controller_runtime,
                active_stream_task_id,
                session_id,
                previous_phase,
                &status_message_id,
                &budget_note,
                AUTO_CONTEXT_COMPACTION_TRIGGER,
                &error,
            )?;
            return Err(error);
        }
    };
    let job_request = match build_webui_context_compaction_job_request(
        state,
        agent_runtime,
        session_id,
        plan,
        full_context_state,
    ) {
        Ok(request) => request,
        Err(error) => {
            persist_failed_webui_context_compaction(
                app,
                state,
                controller_runtime,
                active_stream_task_id,
                session_id,
                previous_phase,
                &status_message_id,
                &budget_note,
                AUTO_CONTEXT_COMPACTION_TRIGGER,
                &error,
            )?;
            return Err(error);
        }
    };
    let terminal_result = stream_webui_context_compaction_execution(
        app,
        state,
        job_request,
        session_id,
        &status_message_id,
        &budget_note,
        AUTO_CONTEXT_COMPACTION_TRIGGER,
    )
    .await;
    match terminal_result {
        Ok(execution) => {
            let compaction_usage_segment = context_compaction_usage_segment_from_execution(
                AUTO_CONTEXT_COMPACTION_TRIGGER,
                &execution,
            );
            // finish_outcome distinguishes a committed compaction from a persisted rejection.
            // finish_outcome 区分已提交压缩与已持久化拒绝。
            let finish_outcome = finish_webui_context_compaction_execution(
                app,
                state,
                agent_runtime,
                session_id,
                previous_phase,
                &status_message_id,
                &budget_note,
                AUTO_CONTEXT_COMPACTION_TRIGGER,
                ContextCompactionCommitMode::ContinueActiveStream {
                    task_id: active_stream_task_id,
                },
                execution,
            )?;
            // rejection_error must survive outcome consumption so automatic continuation can stop.
            // rejection_error 必须跨越终态结果消费，以便停止自动续接。
            let rejection_error = finish_outcome.rejection_error().map(str::to_owned);
            // snapshot always requires persistence, including a committed rejection record.
            // snapshot 始终需要持久化，包括已提交的拒绝记录。
            let snapshot = finish_outcome.into_workspace_snapshot();
            // persist_reason preserves the authoritative terminal classification for diagnostics.
            // persist_reason 为诊断保留权威终态分类。
            let persist_reason = if rejection_error.is_some() {
                "auto_context_compaction_rejected"
            } else {
                "auto_context_compaction"
            };
            spawn_context_compaction_persist_task(
                state.clone(),
                controller_runtime.clone(),
                snapshot,
                session_id,
                status_message_id.clone(),
                Some(String::from(persist_reason)),
            );
            if let Some(error) = rejection_error {
                return Err(error);
            }
            // refreshed_vmm_state observes the cache epoch committed by the compaction controller command.
            // refreshed_vmm_state 观察由压缩控制器命令提交的缓存纪元。
            let refreshed_vmm_state = vmm_controller.snapshot()?;
            let mut refreshed = refreshed_execution_request_after_context_compaction(
                state,
                request,
                &refreshed_vmm_state,
            )?;
            refreshed
                .embedded_turn_usage_segments
                .push(compaction_usage_segment);
            Ok(Some(refreshed))
        }
        Err(error) => {
            persist_failed_webui_context_compaction(
                app,
                state,
                controller_runtime,
                active_stream_task_id,
                session_id,
                previous_phase,
                &status_message_id,
                &budget_note,
                AUTO_CONTEXT_COMPACTION_TRIGGER,
                &error,
            )?;
            Err(error)
        }
    }
}

/// Returns whether reclaimable immutable VMM anchors justify an earlier automatic compaction.
/// 返回可回收不可变 VMM 锚点是否足以提前触发自动压缩。
///
/// # Parameters
/// 参数
/// `context_budget` supplies the automatic-compaction policy and effective input limit.
/// `context_budget` 提供自动压缩策略与生效输入上限。
/// `pressure` contains body-free aggregate VMM capacity estimates.
/// `pressure` 包含不含正文的 VMM 聚合容量估算。
///
/// # Returns
/// 返回值
/// `true` only when automatic compaction is allowed and either pressure threshold is met.
/// 仅当允许自动压缩且达到任一压力阈值时返回 `true`。
fn vmm_cache_pressure_requires_compaction(
    context_budget: &vulcan_context::ContextBudgetSnapshot,
    pressure: vulcan_vmm::VmmCachePressure,
) -> bool {
    if !context_budget.compaction_auto_enabled || !context_budget.baseline_matches_active_runtime {
        return false;
    }
    if pressure.reclaimable_anchor_estimated_tokens >= VMM_RECLAIMABLE_ANCHOR_COMPACTION_TOKENS {
        return true;
    }
    context_budget.effective_input_limit.is_some_and(|limit| {
        limit > 0
            && pressure.reclaimable_anchor_estimated_tokens as f32 / limit as f32
                >= VMM_RECLAIMABLE_ANCHOR_COMPACTION_RATIO
    })
}

/// Returns the current consecutive automatic compaction failure count for one session.
/// 返回单个会话当前连续自动压缩失败次数。
pub(crate) fn webui_auto_context_compaction_failure_count(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> usize {
    current_workspace_snapshot_from_shared_state(state)
        .ok()
        .flatten()
        .and_then(|workspace| {
            workspace
                .session_runtime_state
                .get(&session_id)
                .map(|runtime_state| runtime_state.auto_compaction_consecutive_failures)
        })
        .unwrap_or_default()
}

/// Builds a compaction plan from a specific pre-status conversation snapshot.
/// 从指定的状态节点插入前会话快照构建压缩计划。
pub(crate) fn webui_context_compaction_plan_from_conversation_state(
    session_id: SessionId,
    conversation_state: &SessionConversationState,
    context_start_message_id: Option<&str>,
    empty_error: &str,
) -> Result<CompactionPlan, String> {
    let full_session_messages =
        conversation_state_to_compaction_source_messages(conversation_state);
    if full_session_messages.is_empty() {
        return Err(empty_error.to_string());
    }
    plan_context_compaction(CompactionPlanInput {
        session_label: &session_id.to_string(),
        messages: &full_session_messages,
        all_messages: &full_session_messages,
        context_start_message_id,
        source_until_message_id: None,
        policy: Default::default(),
    })
    .ok_or_else(|| empty_error.to_string())
}

/// Rebuilds an execution request after a successful compaction commit.
/// 在压缩提交成功后重建执行请求。
pub(crate) fn refreshed_execution_request_after_context_compaction(
    state: &SharedServiceBridgeState,
    request: &ExecutionPreparationRequest,
    vmm_session_memory: &vulcan_vmm::VmmSessionMemoryState,
) -> Result<ExecutionPreparationRequest, String> {
    let workspace_snapshot =
        current_workspace_snapshot_from_shared_state(state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&request.session_id)
        .ok_or_else(|| String::from("自动上下文压缩完成后未找到会话运行时状态。"))?;
    let mut refreshed = request.clone();
    refreshed.conversation_state = runtime_state.conversation_state.clone();
    let turn_marker = refreshed
        .boundary_events
        .first()
        .and_then(|event| event.turn_id.clone());
    refreshed.context_injection = core_provider_context_injection_from_state_for_turn(
        &runtime_state.context_injection_state,
        vmm_session_memory,
        turn_marker.as_deref(),
    );
    Ok(refreshed)
}

/// Persists and projects one failed automatic or manual compaction attempt.
/// 持久化并投影一次失败的自动或手动压缩尝试。
///
/// # Parameters
/// 参数
/// `app` emits committed deltas, `state` owns retained service state, and `controller_runtime` persists the failed snapshot.
/// `app` 发送已提交增量，`state` 持有保留服务状态，`controller_runtime` 持久化失败快照。
/// `active_stream_task_id` proves the exact continuing stream; the remaining values identify and describe the failed compaction attempt.
/// `active_stream_task_id` 证明精确续接流；其余参数标识并描述失败的压缩尝试。
///
/// # Returns
/// 返回值
/// Returns success after scheduling persistence, or an ownership/state error before persistence.
/// 持久化任务排队后返回成功；持久化前所有权或状态异常时返回错误。
pub(crate) fn persist_failed_webui_context_compaction(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    active_stream_task_id: Uuid,
    session_id: SessionId,
    previous_phase: SessionRunPhase,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    error: &str,
) -> Result<(), String> {
    let snapshot = fail_webui_context_compaction_execution(
        app,
        state,
        session_id,
        previous_phase,
        section_id,
        budget_note,
        trigger,
        ContextCompactionCommitMode::ContinueActiveStream {
            task_id: active_stream_task_id,
        },
        error,
    )?;
    spawn_workspace_session_persist_task(
        state.clone(),
        controller_runtime.clone(),
        snapshot,
        session_id,
        Some(String::from("context_compaction_failed")),
    );
    Ok(())
}

pub(crate) fn build_webui_context_compaction_job_request(
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    plan: CompactionPlan,
    full_context_state: SessionConversationState,
) -> Result<PromptCompactionJobRequest, String> {
    let mcp_runtime_access = agent_runtime.extension_generation.mcp_runtime();
    let workspace_snapshot = current_workspace_snapshot_from_shared_state(state)
        .ok()
        .flatten();
    let workspace_path = workspace_snapshot
        .as_ref()
        .map(|workspace| workspace_path_for_session(workspace, session_id))
        .unwrap_or_else(|| normalized_workspace_path_text_from_path(&workspace_root()));
    let ai_tool_configuration = workspace_snapshot
        .as_ref()
        .map(|workspace| {
            session_ai_tool_configuration_or_default(
                workspace,
                session_id,
                &agent_runtime.ai_tool_configuration,
            )
            .clone()
        })
        .unwrap_or_else(|| agent_runtime.ai_tool_configuration.clone());
    let budget_snapshot = context_compaction_budget_snapshot_from_state(
        state,
        session_id,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
    );
    Ok(PromptCompactionJobRequest {
        session_id,
        session_title: current_session_title_from_state(state, session_id),
        workspace_path,
        plan,
        full_context_state,
        input_budget: context_compaction_input_budget_from_snapshot(budget_snapshot.as_ref()),
        observed_input_tokens: context_compaction_observed_input_tokens_from_snapshot(
            budget_snapshot.as_ref(),
        ),
        estimated_input_tokens: context_compaction_estimated_input_tokens_from_snapshot(
            budget_snapshot.as_ref(),
        ),
        provider_configuration: agent_runtime.provider_configuration.clone(),
        model_label: agent_runtime.provider_configuration.model.clone(),
        reasoning_effort: active_reasoning_selection_effort(&agent_runtime.provider_configuration)
            .unwrap_or(ReasoningEffortLevel::Medium),
        ai_tool_configuration,
        agent_profile_catalog: agent_runtime.agent_profile_catalog.clone(),
        luaskills_runtime: agent_runtime.luaskills_runtime.clone(),
        mcp_runtime_access,
        task_runtime: Arc::clone(&agent_runtime.task_runtime),
        question_runtime: Arc::clone(&agent_runtime.question_runtime),
        command_session_runtime: Arc::clone(&agent_runtime.command_session_runtime),
        token_budget_runtime: Arc::clone(&agent_runtime.token_budget_runtime),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one automatic-compaction budget with an exact effective input limit.
    /// 构建一份具有精确生效输入上限的自动压缩预算。
    fn automatic_budget(effective_input_limit: usize) -> vulcan_context::ContextBudgetSnapshot {
        vulcan_context::ContextBudgetSnapshot {
            effective_input_limit: Some(effective_input_limit),
            compaction_auto_enabled: true,
            baseline_matches_active_runtime: true,
            ..vulcan_context::ContextBudgetSnapshot::default()
        }
    }

    /// Verifies absolute and proportional reclaimable-anchor thresholds raise compaction pressure.
    /// 验证可回收锚点的绝对阈值与比例阈值都会提高压缩压力。
    #[test]
    fn reclaimable_vmm_anchor_pressure_triggers_only_at_explicit_thresholds() {
        let budget = automatic_budget(100_000);
        assert!(vmm_cache_pressure_requires_compaction(
            &budget,
            vulcan_vmm::VmmCachePressure {
                reclaimable_anchor_estimated_tokens: VMM_RECLAIMABLE_ANCHOR_COMPACTION_TOKENS,
                ..vulcan_vmm::VmmCachePressure::default()
            }
        ));
        assert!(vmm_cache_pressure_requires_compaction(
            &budget,
            vulcan_vmm::VmmCachePressure {
                reclaimable_anchor_estimated_tokens: 5_000,
                ..vulcan_vmm::VmmCachePressure::default()
            }
        ));
        assert!(!vmm_cache_pressure_requires_compaction(
            &budget,
            vulcan_vmm::VmmCachePressure {
                reclaimable_anchor_estimated_tokens: 4_999,
                ..vulcan_vmm::VmmCachePressure::default()
            }
        ));
    }

    /// Verifies disabled automatic compaction and stale baselines suppress VMM pressure triggers.
    /// 验证禁用自动压缩与陈旧基线都会抑制 VMM 压力触发。
    #[test]
    fn reclaimable_vmm_anchor_pressure_respects_compaction_guards() {
        let mut disabled_budget = automatic_budget(100_000);
        disabled_budget.compaction_auto_enabled = false;
        let mut stale_budget = automatic_budget(100_000);
        stale_budget.baseline_matches_active_runtime = false;
        let pressure = vulcan_vmm::VmmCachePressure {
            reclaimable_anchor_estimated_tokens: VMM_RECLAIMABLE_ANCHOR_COMPACTION_TOKENS,
            ..vulcan_vmm::VmmCachePressure::default()
        };

        assert!(!vmm_cache_pressure_requires_compaction(
            &disabled_budget,
            pressure
        ));
        assert!(!vmm_cache_pressure_requires_compaction(
            &stale_budget,
            pressure
        ));
    }
}
