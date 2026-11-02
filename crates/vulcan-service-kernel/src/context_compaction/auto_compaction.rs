//! Context compaction auto compaction helpers.
//! 上下文压缩 auto compaction 辅助逻辑。

use super::*;
use crate::{
    applied_output_style_snapshot, vmm_session_controller::open_registered_session_vmm_controller,
    webui_vmm_runtime_readiness,
};

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
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    active_stream_task_id: Uuid,
    request: &ExecutionPreparationRequest,
    preparation: &ExecutionPreparation,
) -> Result<Option<ExecutionPreparationRequest>, String> {
    maybe_auto_compact_webui_session_with_mode(
        app,
        state,
        controller_runtime,
        agent_runtime,
        active_stream_task_id,
        request,
        preparation,
        false,
    )
    .await
}

/// Forces one context compaction after a provider/tool boundary reported a verified budget overflow.
/// 在 Provider/工具边界报告已验证预算溢出后强制执行一次上下文压缩。
pub(crate) async fn force_auto_compact_webui_session_after_provider_boundary(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    active_stream_task_id: Uuid,
    request: &ExecutionPreparationRequest,
    preparation: &ExecutionPreparation,
) -> Result<Option<ExecutionPreparationRequest>, String> {
    maybe_auto_compact_webui_session_with_mode(
        app,
        state,
        controller_runtime,
        agent_runtime,
        active_stream_task_id,
        request,
        preparation,
        true,
    )
    .await
}

/// Executes the shared compaction transaction with an explicit trigger mode.
/// 使用显式触发模式执行共享上下文压缩事务。
// Compaction keeps durable identity, UI presentation and borrowed execution owners separate.
// 压缩流程分别保留持久化身份、界面展示及借用的执行所有者。
#[expect(
    clippy::too_many_arguments,
    reason = "Compaction keeps durable identity, UI presentation and borrowed execution owners separate."
)]
async fn maybe_auto_compact_webui_session_with_mode(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    active_stream_task_id: Uuid,
    request: &ExecutionPreparationRequest,
    preparation: &ExecutionPreparation,
    force_compaction: bool,
) -> Result<Option<ExecutionPreparationRequest>, String> {
    if !request.prompt_purpose.supports_host_auto_compaction() {
        return Ok(None);
    }
    // Dedicated sessions keep their locked memory verbatim and are never compacted.
    // 专职会话按原样保留锁定记忆，从不压缩。
    if request.dedicated_session.is_some() {
        return if force_compaction {
            Err(String::from(
                "Dedicated assistant sessions do not support context compaction; the current context exceeds the model window, so choose a model with a larger capacity or start a new session.",
            ))
        } else {
            Ok(None)
        };
    }
    let Some(context_budget) = preparation.context_budget.as_ref() else {
        return if force_compaction {
            Err(String::from(
                "context compaction was required but the execution preparation has no budget snapshot",
            ))
        } else {
            Ok(None)
        };
    };
    // A model whose real request window is already exceeded cannot use a session-node
    // compaction attempt to make an incompatible model switch appear admissible.
    // 真实请求窗口已超出的模型不能借会话节点压缩尝试掩盖不兼容的模型切换。
    if context_budget
        .provider_request_hard_limit
        .is_some_and(|limit| context_budget.active_context_tokens >= limit)
    {
        return Err(format!(
            "The current context of {} tokens has reached the real request limit of the selected model; choose a model with enough capacity or shorten the context first.",
            context_budget.active_context_tokens
        ));
    }
    // VMM pressure is eligible only when the request and the current configured service are both ready.
    // 仅当请求准入事实与当前已配置服务都就绪时，VMM 压力才可参与压缩决策。
    let vmm_runtime_ready =
        request.vmm_runtime_ready && webui_vmm_runtime_readiness(state)?.service_ready;
    let vmm_controller = if vmm_runtime_ready {
        Some(open_registered_session_vmm_controller(
            state,
            request.session_id,
            &current_unix_millis_text(),
        )?)
    } else {
        None
    };
    let vmm_cache_pressure = if let Some(controller) = vmm_controller.as_ref() {
        controller.snapshot()?.cache_pressure()
    } else {
        vulcan_vmm::VmmCachePressure::default()
    };
    // vmm_pressure_requires_compaction raises priority only after an absolute or proportional threshold.
    // vmm_pressure_requires_compaction 仅在达到绝对或比例阈值后提高压缩优先级。
    let vmm_pressure_requires_compaction =
        vmm_cache_pressure_requires_compaction(context_budget, vmm_cache_pressure);
    // Completed usage can already exceed the line even when the first character estimate is low.
    // 即使首轮字符估算偏低，已完成请求的实测用量也可能已经超过压缩线。
    let observed_budget = if !force_compaction && !context_budget.should_compact() {
        observed_compaction_budget_before_provider_request(
            state,
            request.session_id,
            &preparation.model_blueprint,
            context_budget,
        )?
    } else {
        None
    };
    // This historical signal only triggers maintenance; final request admission retains its exact evidence checks.
    // 历史信号仅触发维护；最终请求准入仍保留精确证据校验。
    let observed_usage_requires_compaction = observed_budget
        .as_ref()
        .is_some_and(ContextBudgetSnapshot::should_compact);
    if !force_compaction
        && !context_budget.should_compact()
        && !vmm_pressure_requires_compaction
        && !observed_usage_requires_compaction
    {
        return Ok(None);
    }
    let session_id = request.session_id;
    // Label the previous observation explicitly rather than presenting it as this request's exact size.
    // 显式标注此前观测，避免将其呈现为本次请求的精确大小。
    let observed_note = observed_budget
        .as_ref()
        .filter(|_| observed_usage_requires_compaction)
        .map(|budget| {
            format!(
                "; prior uncompacted business context: {}",
                budget.summary_note()
            )
        })
        .unwrap_or_default();
    let budget_note = format!(
        "{}{}; VMM reclaimable anchors about {} tokens ({} bytes)",
        context_budget.summary_note(),
        observed_note,
        vmm_cache_pressure.reclaimable_anchor_estimated_tokens,
        vmm_cache_pressure.reclaimable_anchor_bytes,
    );
    webui_trace(
        "stream_auto_compaction_needed",
        format!(
            "session={session_id} severity={:?} usage={} vmm_reclaimable_tokens={} vmm_pressure_trigger={vmm_pressure_requires_compaction} observed_usage_trigger={observed_usage_requires_compaction}",
            context_budget.severity,
            context_budget.usage_label(),
            vmm_cache_pressure.reclaimable_anchor_estimated_tokens,
        ),
    );

    // A blocked attempt does not rewrite status or count as another failed Provider request.
    // 被拦截的尝试不改写状态，也不计为另一次供应商请求失败。
    if webui_auto_context_compaction_failure_count(state, session_id, active_stream_task_id)
        >= MAX_CONSECUTIVE_AUTO_COMPACTION_FAILURES
    {
        return Err(format!(
            "Automatic context compaction failed {MAX_CONSECUTIVE_AUTO_COMPACTION_FAILURES} times in this execution; retry explicitly to start a new attempt."
        ));
    }

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
                session.last_updated_label = String::from("just now");
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

    // modelTarget freezes the exact route, declared window, and tail policy before planning.
    // modelTarget 在规划前冻结精确路由、声明窗口与尾部策略。
    let model_target = resolve_webui_context_compaction_model_target(
        state,
        agent_runtime,
        session_id,
        Some((request, preparation)),
    )?;
    // preparedPolicy must match the independently lease-validated target without default fallback.
    // preparedPolicy 必须与经租约独立校验的目标一致，不允许回退默认值。
    let prepared_policy = preparation.context_compaction_policy.ok_or_else(|| {
        String::from("Automatic context compaction has no frozen compaction policy.")
    })?;
    if prepared_policy != model_target.context_compaction_policy {
        return Err(String::from(
            "Automatic context compaction target policy drifted after execution preparation.",
        ));
    }
    if context_budget.context_limit != model_target.context_limit {
        return Err(String::from(
            "Automatic context compaction context window does not match the frozen model target.",
        ));
    }
    let compaction_plan = match webui_context_compaction_plan_from_v1(
        state,
        session_id,
        prepared_policy,
        "Automatic context compaction found no history context to compact; this request was stopped to avoid a budget overflow.",
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
            )
            .await?;
            return Err(error);
        }
    };
    let job_request = match build_webui_context_compaction_job_request(
        state,
        agent_runtime,
        session_id,
        compaction_plan.plan,
        compaction_plan.full_context_state,
        compaction_plan.media_projection,
        Some((request, preparation)),
        model_target,
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
            )
            .await?;
            return Err(error);
        }
    };
    let terminal_result = stream_webui_context_compaction_execution(
        app,
        state,
        controller_runtime,
        job_request,
        session_id,
        compaction_plan.retained_from_seq,
        &status_message_id,
        &budget_note,
        AUTO_CONTEXT_COMPACTION_TRIGGER,
    )
    .await;
    match terminal_result {
        Ok(execution) => {
            let compaction_usage_segment = context_compaction_usage_segment_from_execution(
                AUTO_CONTEXT_COMPACTION_TRIGGER,
                &execution.execution,
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
            )
            .await?;
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
            persist_context_compaction_snapshot(
                state.clone(),
                controller_runtime.clone(),
                snapshot,
                session_id,
                status_message_id.clone(),
                Some(String::from(persist_reason)),
            )
            .await?;
            if let Some(error) = rejection_error {
                return Err(error);
            }
            // This reads the current durable VMM cache; the V1 compaction receipt alone does not advance its epoch.
            // 此处读取当前持久 VMM 缓存；V1 压缩回执本身不会推进其纪元。
            let refreshed_vmm_state = match vmm_controller.as_ref() {
                Some(controller) => controller.snapshot()?,
                None => vulcan_vmm::VmmSessionMemoryState::default(),
            };
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
            )
            .await?;
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

/// Returns consecutive failures for `task_id` only; a different stream starts with a fresh retry budget.
/// 只返回 `task_id` 的连续失败次数；不同流拥有新的重试额度。
pub(crate) fn webui_auto_context_compaction_failure_count(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    task_id: Uuid,
) -> usize {
    state
        .lock()
        .ok()
        .and_then(|guard| {
            guard
                .workspace_snapshot
                .as_ref()
                .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
                .filter(|runtime_state| {
                    runtime_state.auto_compaction_failure_task_id == Some(task_id)
                })
                .map(|runtime_state| runtime_state.auto_compaction_consecutive_failures)
        })
        .unwrap_or_default()
}

/// Carries one compaction plan derived exclusively from a durable V1 Provider projection.
/// 承载一份仅从可靠 V1 Provider 投影派生的上下文压缩计划。
#[derive(Debug, Clone)]
pub(crate) struct WebuiContextCompactionPlan {
    /// Exact source proof that permits only composer draft changes before standalone admission.
    /// 精确来源证明，仅允许独立准入前发生输入框草稿变化。
    pub(crate) source: vulcan_session_core::v1::context_maintenance::ManualContextMaintenanceSource,
    /// Exact frozen prompt-media source owner from the same snapshot as the transcript plan.
    /// 与转录计划来自相同快照的精确冻结提示词媒体来源所有者。
    pub(crate) media_projection:
        Arc<dyn vulcan_agent_runtime::prompt_media_projection::PromptMediaProjectionRuntime>,
    /// Frozen compaction source and retained-tail policy.
    /// 冻结的压缩来源与保留尾部策略。
    pub(crate) plan: CompactionPlan,
    /// Provider-visible V1 dialogue projected into the compaction runtime input shape.
    /// 投影到压缩运行时输入形态的 Provider 可见 V1 对话。
    pub(crate) full_context_state: SessionConversationState,
    /// Inclusive durable event boundary retained by the next context generation.
    /// 下一上下文代际保留的可靠事件包含式边界。
    pub(crate) retained_from_seq: Seq,
}

/// Plans context compaction exclusively from the durable V1 Provider projection.
/// 仅从可靠 V1 Provider 投影规划上下文压缩。
pub(crate) fn webui_context_compaction_plan_from_v1(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    policy: ContextCompactionPolicy,
    empty_error: &str,
) -> Result<WebuiContextCompactionPlan, String> {
    let handle =
        crate::session_goal_lifecycle::registered_session_v1_handle_from_state(state, session_id)?;
    let (snapshot, source) = handle
        .capture_context_compaction_planning_snapshot_with_source()
        .map_err(|error| format!("failed to capture V1 compaction projection: {error}"))?;
    let projected_messages = snapshot
        .sourced_items
        .iter()
        .enumerate()
        .filter_map(|(index, projected)| {
            compaction_message_from_v1_projection(projected)
                .map(|message| message.map(|(message, seq)| (message, seq, index)))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let full_session_messages = projected_messages
        .iter()
        .map(|(message, _, _)| message.clone())
        .collect::<Vec<_>>();
    if full_session_messages.is_empty() {
        return Err(empty_error.to_string());
    }
    let plan = plan_context_compaction(CompactionPlanInput {
        session_label: &session_id.to_string(),
        messages: &full_session_messages,
        all_messages: &full_session_messages,
        context_start_message_id: None,
        source_until_message_id: None,
        policy,
    })
    .ok_or_else(|| empty_error.to_string())?;
    let retained_message_id = plan
        .tail_selection
        .retained_message_ids
        .first()
        .ok_or_else(|| String::from("V1 compaction plan has no retained tail boundary"))?;
    let retained_from_seq = projected_messages
        .iter()
        .find(|(message, _, _)| message.id == *retained_message_id)
        .map(|(_, source_seq, _)| *source_seq)
        .ok_or_else(|| {
            format!(
                "V1 compaction retained message `{retained_message_id}` has no durable source seq"
            )
        })?;
    let full_context_state = vulcan_context::conversation_state_from_session_messages(
        &full_session_messages,
    )
    .map_err(|error| format!("failed to build V1 compaction conversation state: {error}"))?;
    // Capture the existing catalog owner without retaining the service mutex during resource preparation.
    // 捕获既有目录所有者，不在资源准备期间持有服务互斥锁。
    let resource_registry = Arc::clone(
        &state
            .lock()
            .map_err(|error| error.to_string())?
            .project_resource_catalog_registry,
    );
    // Exact source indexes come from this same projection traversal, never parsed from message names.
    // 精确来源索引来自相同投影遍历，绝不从消息名称解析。
    let media_projection = crate::agent_stream::frozen_prompt_media_projection(
        snapshot,
        session_id.to_string(),
        projected_messages
            .iter()
            .map(|(message, _, index)| (message.id.clone(), *index))
            .collect(),
        resource_registry,
    )?;
    Ok(WebuiContextCompactionPlan {
        source,
        media_projection,
        plan,
        full_context_state,
        retained_from_seq,
    })
}

/// Converts one sourced V1 Provider item into the planner's transcript-only shape.
/// 把一条带来源的 V1 Provider 投影项转换为规划器的仅转录形态。
pub(crate) fn compaction_message_from_v1_projection(
    projected: &SourcedProjectedItem,
) -> Option<Result<(SessionMessageEntry, Seq), String>> {
    let created_at = String::new();
    match &projected.item {
        ProjectedItem::SystemText { .. } => None,
        ProjectedItem::UserInput { text, resource_ids } => {
            let mut body = text.clone();
            if !resource_ids.is_empty() {
                let resources = resource_ids
                    .iter()
                    .map(|resource_id| format!("- resource_id={}", resource_id.value()))
                    .collect::<Vec<_>>()
                    .join("\n");
                if !body.is_empty() {
                    body.push_str("\n\n");
                }
                body.push_str("[Project resources]\n");
                body.push_str(&resources);
            }
            Some(Ok((
                SessionMessageEntry {
                    id: format!("v1-user-{}", projected.source_from_seq.value()),
                    kind: SessionMessageKind::User,
                    record_kind: SessionMessageRecordKind::Transcript,
                    visible: true,
                    title: String::from("USER"),
                    body,
                    metadata: None,
                    created_at,
                },
                projected.source_from_seq,
            )))
        }
        ProjectedItem::AssistantText {
            text,
            reasoning,
            tool_calls,
            ..
        } => Some(
            compaction_assistant_message_from_v1_projection(
                projected,
                text,
                reasoning.as_deref(),
                tool_calls,
                created_at,
            )
            .map(|message| (message, projected.source_from_seq)),
        ),
        ProjectedItem::ToolResult {
            tool_call_id,
            status,
            output,
            ..
        } => Some(
            serde_json::to_string(output)
                .map_err(|error| format!("failed to serialize V1 tool result for compaction: {error}"))
                .map(|body| {
                    let metadata = json!({
                        "tool_call_id": tool_call_id.value().to_string(),
                        "is_error": *status != vulcan_session_core::v1::event::ToolCallStatus::Completed,
                    });
                    (
                        SessionMessageEntry {
                            id: format!("v1-tool-result-{}", tool_call_id.value()),
                            kind: SessionMessageKind::Assistant,
                            record_kind: SessionMessageRecordKind::ToolResultOutput,
                            visible: true,
                            title: String::from("TOOL RESULT"),
                            body,
                            metadata: Some(metadata),
                            created_at,
                        },
                        projected.source_from_seq,
                    )
                }),
        ),
        ProjectedItem::SubagentReturnText { text } => Some(Ok((
            SessionMessageEntry {
                id: format!("v1-subagent-return-{}", projected.source_from_seq.value()),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::from("SUBAGENT RETURN"),
                body: text.clone(),
                metadata: None,
                created_at,
            },
            projected.source_from_seq,
        ))),
    }
}

/// Builds one assistant compaction message without losing local tool identity.
/// 构建一条不丢失本地工具身份的 assistant 压缩消息。
fn compaction_assistant_message_from_v1_projection(
    projected: &SourcedProjectedItem,
    text: &str,
    reasoning: Option<&str>,
    tool_calls: &[vulcan_session_core::v1::projection::ProjectedToolCall],
    created_at: String,
) -> Result<SessionMessageEntry, String> {
    let mut body_parts = Vec::new();
    if !text.is_empty() {
        body_parts.push(text.to_string());
    }
    if let Some(reasoning) = reasoning.filter(|value| !value.is_empty()) {
        body_parts.push(format!("reasoning: {reasoning}"));
    }
    for tool_call in tool_calls {
        let arguments = serde_json::to_string(&tool_call.input).map_err(|error| {
            format!("failed to serialize V1 tool arguments for compaction: {error}")
        })?;
        let mut header = format!(
            "tool: {} (local_tool_call_id={})",
            tool_call.tool_name,
            tool_call.tool_call_id.value()
        );
        if let Some(provider_id) = tool_call.provider_tool_call_id.as_deref() {
            header.push_str(&format!(", provider_tool_call_id={provider_id}"));
        }
        body_parts.push(format!("{header}\narguments: {arguments}"));
    }
    Ok(SessionMessageEntry {
        id: format!(
            "v1-assistant-{}-{}",
            projected.source_from_seq.value(),
            projected.source_to_seq.value()
        ),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::AssistantOutput,
        visible: true,
        title: String::from("AI"),
        body: body_parts.join("\n\n"),
        metadata: Some(json!({
            "provider_message_id": format!(
                "v1-api-range-{}-{}",
                projected.source_from_seq.value(),
                projected.source_to_seq.value()
            ),
        })),
        created_at,
    })
}

/// Resolves one exact WebUI compaction target before planning or job construction.
/// 在规划或构建任务之前解析一份精确 WebUI 压缩目标。
///
/// # Parameters
/// # 参数
/// `state` owns persisted session selections, `agent_runtime` owns the public provider runtime,
/// `session_id` selects the session, and `source_execution` optionally supplies an already frozen ordinary turn.
/// `state` 持有持久化会话选择，`agent_runtime` 持有公共 Provider 运行时，`session_id` 选择会话，
/// `source_execution` 可选提供已冻结的普通回合。
///
/// # Returns
/// # 返回值
/// The exact immutable target, or an explicit error when no dispatchable target exists.
/// 返回精确不可变目标；不存在可分派目标时返回显式错误。
pub(crate) fn resolve_webui_context_compaction_model_target(
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    source_execution: Option<(&ExecutionPreparationRequest, &ExecutionPreparation)>,
) -> Result<PromptCompactionModelTarget, String> {
    let (
        retained_route_lease_identity,
        active_provider_id,
        active_connection_id,
        active_model_id,
        session_runtime_registry,
    ) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let runtime_state = guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&session_id));
        (
            runtime_state.and_then(|state| state.provider_route_lease.clone()),
            runtime_state.and_then(|state| state.active_provider_id.clone()),
            runtime_state.and_then(|state| state.active_connection_id.clone()),
            runtime_state.and_then(|state| state.active_model_id.clone()),
            Arc::clone(&guard.session_runtime_registry),
        )
    };
    let context_compaction_window_node_id = session_runtime_registry
        .get(session_id)
        .map_err(|error| format!("Context compaction has no V1 session owner: {error}"))?
        .read(|document| document.metadata.config.context_compaction_window_node_id)
        .map_err(|error| format!("Context compaction cannot read V1 metadata: {error}"))?;
    let route_lease_identity = source_execution
        .and_then(|(_, preparation)| {
            preparation
                .model_blueprint
                .session_provider_route_lease_identity()
        })
        .or(retained_route_lease_identity);
    let compaction_session_id = session_id.to_string();
    let target = match source_execution {
        Some((_, preparation)) => resolve_prompt_compaction_model_target(
            agent_runtime.public_ai_runtime.as_ref(),
            compaction_session_id.as_str(),
            route_lease_identity.as_ref(),
            Some(preparation.model_blueprint.provider_id.as_str()),
            Some(preparation.model_blueprint.connection_id.as_str()),
            Some(preparation.model_blueprint.model_id.as_str()),
            source_execution.and_then(|(request, _)| request.context_compaction_window_node_id),
            preparation.context_compaction_policy,
        )?,
        None => resolve_prompt_compaction_model_target(
            agent_runtime.public_ai_runtime.as_ref(),
            compaction_session_id.as_str(),
            route_lease_identity.as_ref(),
            active_provider_id.as_deref(),
            active_connection_id.as_deref(),
            active_model_id.as_deref(),
            context_compaction_window_node_id,
            None,
        )?,
    };
    target.ok_or_else(|| {
        String::from("Context compaction requires an exact provider, connection, and model target.")
    })
}

/// Rebuilds an execution request after a successful compaction commit.
/// 在压缩提交成功后重建执行请求。
pub(crate) fn refreshed_execution_request_after_context_compaction(
    state: &SharedServiceBridgeState,
    request: &ExecutionPreparationRequest,
    vmm_session_memory: &vulcan_vmm::VmmSessionMemoryState,
) -> Result<ExecutionPreparationRequest, String> {
    let current_vmm_service_ready = webui_vmm_runtime_readiness(state)?.service_ready;
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let runtime_state = guard
        .workspace_snapshot
        .as_mut()
        .ok_or_else(|| String::from("service is still starting; workspace snapshot is not ready"))?
        .session_runtime_state
        .get_mut(&request.session_id)
        .ok_or_else(|| String::from("The session runtime state was not found after automatic context compaction finished."))?;
    let mut refreshed = request.clone();
    // vmmRuntimeReady cannot become true during compaction and is revoked immediately after stop.
    // vmmRuntimeReady 在压缩期间不能由 false 变为 true，并会在服务停止后立即撤销。
    let vmm_runtime_ready = request.vmm_runtime_ready && current_vmm_service_ready;
    refreshed.vmm_runtime_ready = vmm_runtime_ready;
    // Keep canonical cached layers intact while projecting only currently admitted memory into this request.
    // 保持规范缓存层不变，仅向本次请求投影当前准入的记忆。
    let mut context_injection_state = runtime_state.context_injection_state.clone();
    if !vmm_runtime_ready || !vulcan_vmm::VmmRuntimeFeaturePolicy::from_env().session_state_enabled
    {
        vulcan_agent_runtime::agent_runtime::vmm_hooks::clear_vmm_profile_injections(
            &mut context_injection_state,
            &mut refreshed.system_prompt_hook_blocks,
        );
    }
    refreshed.conversation_state = runtime_state.conversation_state.clone();
    // The compacted state already contains the internal continuation seed, so replaying it as a
    // new UserPrompt would append the original prompt to the visible transcript a second time.
    // 压缩后的状态已经包含内部续接种子，因此若再按新 UserPrompt 重放，就会把原始提示词第二次追加到可见转录。
    refreshed.turn_start_mode = Some(
        vulcan_agent_runtime::agent_runtime::execution_models::ExecutionTurnStartMode::RestoredCheckpoint,
    );
    refreshed.output_style_snapshot = applied_output_style_snapshot(runtime_state);
    let turn_marker = refreshed
        .boundary_events
        .first()
        .and_then(|event| event.turn_id.clone());
    let had_pending_expiry_events = !context_injection_state.pending_expiry_events.is_empty();
    refreshed.context_injection =
        core_provider_context_injection_from_state_for_turn_draining_expiry_events(
            &mut context_injection_state,
            vmm_session_memory,
            turn_marker.as_deref(),
            vmm_runtime_ready,
        );
    if had_pending_expiry_events {
        runtime_state
            .context_injection_state
            .take_pending_expiry_events();
    }
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
// Compaction keeps durable identity, UI presentation and borrowed execution owners separate.
// 压缩流程分别保留持久化身份、界面展示及借用的执行所有者。
#[expect(
    clippy::too_many_arguments,
    reason = "Compaction keeps durable identity, UI presentation and borrowed execution owners separate."
)]
pub(crate) async fn persist_failed_webui_context_compaction(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    _controller_runtime: &HostRuntimeHandle,
    active_stream_task_id: Uuid,
    session_id: SessionId,
    previous_phase: SessionRunPhase,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    error: &str,
) -> Result<(), String> {
    fail_webui_context_compaction_execution(
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
    crate::persist_v1_execution_failure_boundary(
        state,
        session_id,
        "context_compaction_failed",
        Some(error.to_owned()),
    )
    .await
}

/// Copies the complete inert session tool snapshot used by compaction preparation.
/// 复制压缩准备使用的完整惰性会话工具快照。
///
/// # Parameters
/// 参数
/// `runtime_state` owns the materialized session routes; `context` receives the request-local
/// overrides used when no ordinary source execution can be reused.
/// `runtime_state` 持有已物化的会话路由；`context` 接收在无法复用普通来源执行时使用的请求级覆盖。
///
/// # Returns
/// 返回值
/// Returns success after copying a complete snapshot, or an explicit error when an enabled
/// direct or media capability lacks its required exact route.
/// 完整快照复制成功时返回成功；已启用的直连或媒体能力缺少所需精确路由时返回明确错误。
fn apply_session_provider_tool_cache_snapshot_to_compaction(
    runtime_state: &vulcan_workbench_model::SessionRuntimeStateEntry,
    context: &mut PromptCompactionLaunchContext,
) -> Result<(), String> {
    if !runtime_state.provider_tool_settings_initialized {
        return Ok(());
    }
    for capability in &runtime_state.provider_tool_enabled_capabilities {
        // mediaCapability must retain the exact session route during compaction.
        // mediaCapability 必须在压缩期间保留精确会话路由。
        let media_capability = matches!(
            capability.as_str(),
            "image_understand" | "video_understand" | "audio_understand"
        );
        if media_capability {
            if !runtime_state
                .media_understanding_selections
                .contains_key(capability)
            {
                return Err(format!(
                    "Media-understanding capability `{capability}` is enabled but has no exact session route; confirm the session capability settings before compacting."
                ));
            }
            continue;
        }
        if !runtime_state
            .provider_tool_selections
            .contains_key(capability)
        {
            return Err(format!(
                "Provider tool capability `{capability}` is enabled but has no selected model and account; confirm the session capability settings before compacting."
            ));
        }
    }
    context.provider_tool_selections_override = Some(
        runtime_state
            .provider_tool_selections
            .iter()
            .filter(|(capability, _)| {
                runtime_state
                    .provider_tool_enabled_capabilities
                    .contains(*capability)
            })
            .map(|(capability, selection)| (capability.clone(), selection.clone()))
            .collect(),
    );
    context.provider_tool_enabled_capabilities_override =
        Some(runtime_state.provider_tool_enabled_capabilities.clone());
    context.provider_tool_media_understanding_selections_override = Some(
        runtime_state
            .media_understanding_selections
            .iter()
            .filter(|(capability, _)| {
                runtime_state
                    .provider_tool_enabled_capabilities
                    .contains(*capability)
            })
            .map(|(capability, selection)| (capability.clone(), selection.clone()))
            .collect(),
    );
    context.provider_extension_target_sets_override =
        Some(runtime_state.provider_extension_target_sets.clone());
    context.provider_extension_modes_override =
        Some(runtime_state.provider_extension_modes.clone());
    Ok(())
}

/// Builds one WebUI compaction job from an already frozen plan and exact model target.
/// 根据已冻结规划与精确模型目标构建一项 WebUI 压缩任务。
///
/// # Parameters
/// # 参数
/// `state` and `agent_runtime` provide current runtime services, `session_id` owns the job,
/// `plan` and `full_context_state` freeze the source, `source_execution` optionally reuses the ordinary-turn lease,
/// and `model_target` is the single exact provider/model/policy resolution used for dispatch.
/// `state` 与 `agent_runtime` 提供当前运行时服务，`session_id` 拥有该任务，
/// `plan` 与 `full_context_state` 冻结来源，`source_execution` 可选复用普通回合租约，
/// `model_target` 是用于分派的唯一精确 Provider/Model/策略解析结果。
///
/// # Returns
/// # 返回值
/// A validated immutable launch request, or an explicit mismatch/routing error.
/// 返回校验后的不可变启动请求；策略不匹配或路由失败时返回显式错误。
// Compaction keeps durable identity, UI presentation and borrowed execution owners separate.
// 压缩流程分别保留持久化身份、界面展示及借用的执行所有者。
#[expect(
    clippy::too_many_arguments,
    reason = "Compaction keeps durable identity, UI presentation and borrowed execution owners separate."
)]
pub(crate) fn build_webui_context_compaction_job_request(
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    plan: CompactionPlan,
    full_context_state: SessionConversationState,
    media_projection: Arc<
        dyn vulcan_agent_runtime::prompt_media_projection::PromptMediaProjectionRuntime,
    >,
    source_execution: Option<(&ExecutionPreparationRequest, &ExecutionPreparation)>,
    model_target: PromptCompactionModelTarget,
) -> Result<PromptCompactionJobRequest, String> {
    if plan.policy != model_target.context_compaction_policy {
        return Err(String::from(
            "Context compaction plan policy does not match the frozen model target.",
        ));
    }
    // persistedControls captures only the exact session controls required by manual compaction.
    // persistedControls 仅捕获手动压缩需要的精确会话控制。
    let (
        session_reasoning_effort,
        session_active_agent_id,
        workspace_path,
        mut persisted_launch_context,
    ) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let runtime_state = workspace.session_runtime_state.get(&session_id);
        let mut context = PromptCompactionLaunchContext::default();
        if source_execution.is_none()
            && let Some(runtime_state) = runtime_state
        {
            apply_session_provider_tool_cache_snapshot_to_compaction(runtime_state, &mut context)?;
            // Temperature comes from the active V1 request projection, not a pending picker selection.
            // 温度来自活动 V1 请求投影，而非待生效选择器选项。
            context.temperature = runtime_state.active_temperature;
            context.execution_mode = runtime_state
                .active_work_mode
                .clone()
                .unwrap_or_else(|| String::from("build"));
            context.output_style_snapshot = applied_output_style_snapshot(runtime_state);
            context.session_prompt_skill_overrides = runtime_state.prompt_skill_overrides.clone();
        }
        (
            runtime_state.and_then(|state| state.active_reasoning_effort),
            runtime_state.and_then(|state| state.active_agent_id.clone()),
            workspace_path_for_session(workspace, session_id),
            context,
        )
    };
    // modelLabel retains the resolved provider/model selector for terminal diagnostics.
    // modelLabel 为终态诊断保留解析后的供应商/模型选择器。
    let model_label = Some(model_target.label());
    // reasoningEffort follows the session selection before falling back to the runtime default.
    // reasoningEffort 优先采用会话选择，其次回退到运行时默认值。
    let reasoning_effort = source_execution
        .map(|(request, _)| request.reasoning_effort)
        .or(session_reasoning_effort)
        .unwrap_or_else(|| {
            active_reasoning_selection_effort(&agent_runtime.provider_configuration)
                .unwrap_or(ReasoningEffortLevel::Off)
        });
    // launchContext reuses an already materialized ordinary turn when available; manual compaction derives only persisted session controls.
    // launchContext 优先复用已经物化的普通回合；手动压缩仅从持久化会话控制派生。
    let mut launch_context = match source_execution {
        Some((source_request, _)) => {
            PromptCompactionLaunchContext::from_execution_request(source_request)
        }
        None => {
            // projectConfiguration mirrors ordinary WebUI prompt preparation for project-scoped prompt skills.
            // projectConfiguration 镜像普通 WebUI 提示准备使用的项目级提示词技能配置。
            persisted_launch_context.project_prompt_skills =
                crate::load_project_configuration(&workspace_path).prompt_skills;
            persisted_launch_context
        }
    };
    let budget_snapshot = context_compaction_budget_snapshot_from_state(state, session_id);
    launch_context.media_projection = Some(media_projection);
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
        model_target: Some(model_target),
        execution_agent_id: source_execution
            .map(|(_, preparation)| preparation.agent_id.clone())
            .or(session_active_agent_id),
        launch_context,
        public_ai_runtime: Arc::clone(&agent_runtime.public_ai_runtime),
        model_label,
        reasoning_effort,
        agent_profile_catalog: agent_runtime.agent_profile_catalog.clone(),
        luaskills_runtime: agent_runtime.luaskills_runtime.clone(),
        vulcan_plugin_runtime: agent_runtime.vulcan_plugin_runtime.clone(),
        extension_generation: Arc::clone(&agent_runtime.extension_generation),
        task_runtime: Arc::clone(&agent_runtime.task_runtime),
        question_runtime: Arc::clone(&agent_runtime.question_runtime),
        command_session_runtime: Arc::clone(&agent_runtime.command_session_runtime),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Verifies compaction refresh revokes stopped memory without changing canonical or unrelated input.
    /// 验证压缩刷新撤销已停用记忆，不改变规范输入或无关注入。
    /// Uses the shared prompt reducers and production refresh entry; returns after request and retained-state assertions.
    /// 使用共享提示词归约器及生产刷新入口；请求及保留状态断言完成后返回。
    #[tokio::test]
    async fn compaction_refresh_removes_stopped_memory_profiles_and_readiness() {
        use vulcan_agent_runtime::agent_runtime::conversation_hooks::ConversationHookRuntime;
        use vulcan_agent_runtime::workbench_prompt_turn::{
            SharedPromptTurnContinueInput, SharedPromptTurnRuntimeBeginInput,
            begin_shared_prompt_turn_with_runtime_state,
            continue_shared_prompt_turn_after_preparation_with_runtime_state,
        };
        use vulcan_context_injection::{InjectionInsert, InjectionKind};
        use vulcan_workbench_model::{PendingSessionPromptSource, SessionRuntimeStateEntry};

        // The fixture owns only in-memory service state and never opens user storage or a model connection.
        // 夹具仅持有内存服务状态，不打开用户存储或模型连接。
        let session_id = SessionId::from_u128(831);
        // Generate a real canonical request rather than duplicating the evolving request structure.
        // 生成真实规范请求，而非复制持续变化的请求结构。
        let mut runtime_state = SessionRuntimeStateEntry::default();
        // The empty dispatcher still supplies exact correlation through the shared begin reducer.
        // 空派发器仍通过共享开始归约器提供准确关联。
        let mut hooks = ConversationHookRuntime::new();
        // A normal user turn establishes the same request shape consumed by automatic compaction.
        // 普通用户回合建立自动压缩所消费的同一请求形态。
        let start = begin_shared_prompt_turn_with_runtime_state(
            &mut runtime_state,
            &mut hooks,
            SharedPromptTurnRuntimeBeginInput {
                session_id,
                session_title: "memory compaction projection",
                workspace_path: "D:/isolated-projection",
                prompt_entry_id: "compaction-projection-turn".to_owned(),
                continuation_action_id: None,
                prompt: "continue after compaction",
                hidden_model_guidance: None,
                provider_visible_agent_messages: Vec::new(),
                attachments: Vec::new(),
                provider_attachments: None,
                existing_canonical_user_message_id: None,
                append_canonical_user_message: true,
                append_visible_user_prompt_record: false,
                source: PendingSessionPromptSource::UserFollowup,
                vmm_runtime_ready: false,
                vmm_precheck_ready: false,
                execution_mode: "agent".to_owned(),
                reasoning_effort: None,
                owning_project_id: Some("project-a".to_owned()),
            },
        )
        .expect("shared begin");
        // Empty temporary memory is used only while constructing the initial no-hook request.
        // 仅在构造初始无 Hook 请求时使用空临时记忆。
        let mut memory = vulcan_vmm::VmmSessionMemoryState::default();
        // Preparation runs the empty profile dispatcher and skips disabled precheck with exact turn identity.
        // 准备执行空画像派发器，并以准确回合身份跳过已禁用预检。
        let mut request = continue_shared_prompt_turn_after_preparation_with_runtime_state(
            &mut runtime_state,
            &mut memory,
            SharedPromptTurnContinueInput {
                vmm_context_epoch: Some(
                    vulcan_vmm::VmmContextEpoch::new(1, None).expect("fixture context epoch"),
                ),
                vmm_profile_read_source: None,
                profile_outcome: start.profile_request.clone().run().await,
                precheck_outcome: start.precheck_request.clone().skip(),
                start,
                project_configuration: Default::default(),
                runtime_reasoning_effort: None,
                provider_reasoning_effort: None,
                patch_visible_user_prompt_record: false,
            },
        )
        .expect("shared request")
        .request;
        // Simulate memory admitted before compaction, then observe an actually stopped host at refresh.
        // 模拟压缩前记忆已准入，然后在刷新时观察确实已停止的宿主。
        request.vmm_runtime_ready = true;
        request.system_prompt_hook_blocks = vec![
            vulcan_hooks::SystemPromptHookBlock::new("vmm_profile", "old profile base"),
            vulcan_hooks::SystemPromptHookBlock::new("other_hook", "keep system"),
        ];
        runtime_state.context_injection_state.apply_insert(
            InjectionInsert::single(
                InjectionKind::Persistent,
                "vmm_profile_delta",
                "profile",
                "old profile delta",
                None,
            ),
            "2",
        );
        runtime_state.context_injection_state.apply_insert(
            InjectionInsert::single(
                InjectionKind::Persistent,
                "other_feature",
                "generic",
                "keep generic",
                None,
            ),
            "2",
        );
        memory
            .synchronize_stable_anchor_rendering(true, "2")
            .expect("cached anchor rendering");
        memory
            .apply_recall_batch(
                &session_id.to_string(),
                "historical-user",
                &vulcan_vmm::VmmRecallBatch {
                    ttl_turns: 3,
                    injection_tag: "vmm".to_owned(),
                    injection_description: "memory".to_owned(),
                    queries: Vec::new(),
                    items: vec![vulcan_vmm::ContextItem {
                        memory_id: 11,
                        text: "old recalled body".to_owned(),
                        ..Default::default()
                    }],
                },
                "2",
            )
            .expect("old recall cache");
        assert!(!memory.active_leases.is_empty());
        assert!(!memory.cache_epoch.anchors.is_empty());
        // Retain exact authority separately so request projection cannot silently clear stored state.
        // 单独保留准确权威状态，确保请求投影不能悄悄清空存储状态。
        let before_memory = memory.clone();
        // The helper supplies a stopped service with no registered runtime or external health probe.
        // 辅助夹具提供无已注册运行时或外置健康探测的已停用服务。
        let mut service =
            crate::tests::test_support::test_service_state_with_parent_session(session_id);
        service
            .workspace_snapshot
            .as_mut()
            .expect("workspace")
            .session_runtime_state
            .insert(session_id, runtime_state.clone());
        // Only the fixture owns this service lock.
        // 仅夹具拥有此服务锁。
        let state = std::sync::Arc::new(std::sync::Mutex::new(service));
        // Both original readiness values must remain false after observing the stopped host.
        // 观察到宿主停止后，两种原始就绪值都必须保持为 false。
        for originally_ready in [true, false] {
            request.vmm_runtime_ready = originally_ready;
            // Exercise the exact refresh function used by successful automatic compaction.
            // 执行自动压缩成功后使用的准确刷新函数。
            let refreshed =
                refreshed_execution_request_after_context_compaction(&state, &request, &memory)
                    .expect("refresh");
            assert!(!refreshed.vmm_runtime_ready);
            assert_eq!(
                refreshed.system_prompt_hook_blocks,
                vec![vulcan_hooks::SystemPromptHookBlock::new(
                    "other_hook",
                    "keep system"
                )]
            );
            // The generic feature still produces a nonempty overlay, but no VMM body or profile remains.
            // 通用功能仍产生非空覆盖层，但不再包含记忆正文或画像。
            let overlay = refreshed.context_injection.expect("generic injection");
            assert_eq!(overlay.persistent_blocks.len(), 1);
            assert!(overlay.persistent_blocks[0].contains("keep generic"));
            assert!(overlay.anchored_prefixes.is_empty());
            assert!(overlay.cache_epoch_id.is_none());
            assert_eq!(overlay.active_memory_body_bytes, 0);
            assert_eq!(refreshed.prompt, request.prompt);
            assert_eq!(
                serde_json::to_value(&refreshed.conversation_state).expect("request history"),
                serde_json::to_value(&runtime_state.conversation_state).expect("canonical history")
            );
            assert!(matches!(refreshed.turn_start_mode, Some(vulcan_agent_runtime::agent_runtime::execution_models::ExecutionTurnStartMode::RestoredCheckpoint)));
            assert_eq!(memory, before_memory);
            assert_eq!(
                state
                    .lock()
                    .expect("service")
                    .workspace_snapshot
                    .as_ref()
                    .expect("workspace")
                    .session_runtime_state[&session_id]
                    .context_injection_state,
                runtime_state.context_injection_state
            );
            assert_eq!(request.system_prompt_hook_blocks.len(), 2);
        }
    }

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

    /// Verifies standalone compaction diagnostics retain the exact session provider/model selector.
    /// 验证独立压缩诊断会保留精确的会话供应商/模型选择器。
    ///
    /// # Parameters
    /// 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// 返回值
    /// This test returns no value and fails when the resolved target label is rewritten.
    /// 此测试无返回值；解析后的目标标签被改写时测试失败。
    #[test]
    fn context_compaction_model_label_uses_exact_session_selector() {
        // modelTarget contains the authoritative resolved session selection under test.
        // modelTarget 包含本测试使用的权威已解析会话选择。
        let model_target =
            vulcan_agent_runtime::agent_runtime::prompt_compaction::PromptCompactionModelTarget {
                provider_id: String::from("custom-response"),
                model_id: String::from("response-model"),
                connection_id: String::from("response-connection"),
                provider_route_lease_identity:
                    vulcan_workbench_model::SessionProviderRouteLeaseIdentity {
                        schema_version: 1,
                        provider_id: String::from("custom-response"),
                        connection_id: String::from("response-connection"),
                        model_id: String::from("response-model"),
                        protocol_profile_id: String::from("openai-responses.v1"),
                        manifest_revision: 1,
                        manifest_generation: 1,
                        catalog_revision: String::from("catalog-test"),
                        route_revision: String::from("route-test"),
                    },
                context_limit: None,
                context_compaction_policy: vulcan_context::ContextCompactionPolicy::default(),
            };

        assert_eq!(
            model_target.label().as_str(),
            "custom-response/response-model"
        );
    }

    /// Verifies standalone compaction freezes the exact session media route instead of global defaults.
    /// 验证独立压缩冻结精确会话媒体路由，而不是使用全局默认值。
    ///
    /// # Parameters
    /// 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// 返回值
    /// This test returns no value and fails when media route ownership is omitted or rewritten.
    /// 此测试无返回值；媒体路由归属被遗漏或改写时测试失败。
    #[test]
    fn standalone_cache_preserving_compaction_preserves_exact_session_media_route() {
        let mut runtime_state = vulcan_workbench_model::SessionRuntimeStateEntry {
            provider_tool_settings_initialized: true,
            provider_tool_enabled_capabilities: [String::from("video_understand")]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let exact_route =
            vulcan_workbench_model::SessionMediaUnderstandingSelection::ProviderTool {
                selection: vulcan_workbench_model::SessionProviderToolTargetSelection {
                    connection_id: String::from("session-video-account"),
                    target_id: String::from("session-video-target"),
                },
            };
        runtime_state
            .media_understanding_selections
            .insert(String::from("video_understand"), exact_route.clone());
        let mut context = PromptCompactionLaunchContext::default();

        apply_session_provider_tool_cache_snapshot_to_compaction(&runtime_state, &mut context)
            .expect("exact session media route should be accepted");

        assert_eq!(
            context.provider_tool_media_understanding_selections_override,
            Some(BTreeMap::from([(
                String::from("video_understand"),
                exact_route,
            )]))
        );
        assert_eq!(
            context.provider_tool_enabled_capabilities_override,
            Some([String::from("video_understand")].into_iter().collect())
        );
    }

    /// Verifies standalone compaction rejects an enabled media capability without a session route.
    /// 验证独立压缩拒绝缺少会话路由的已启用媒体能力。
    ///
    /// # Parameters
    /// 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// 返回值
    /// This test returns no value and fails when incomplete session state reaches compaction.
    /// 此测试无返回值；不完整会话状态进入压缩流程时测试失败。
    #[test]
    fn standalone_compaction_rejects_missing_session_media_route() {
        let runtime_state = vulcan_workbench_model::SessionRuntimeStateEntry {
            provider_tool_settings_initialized: true,
            provider_tool_enabled_capabilities: [String::from("audio_understand")]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let mut context = PromptCompactionLaunchContext::default();

        let error =
            apply_session_provider_tool_cache_snapshot_to_compaction(&runtime_state, &mut context)
                .expect_err("missing exact media route must be rejected");

        assert!(error.contains("audio_understand"), "{error}");
        assert!(
            context
                .provider_tool_media_understanding_selections_override
                .is_none()
        );
    }
}
