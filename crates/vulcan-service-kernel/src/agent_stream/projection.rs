//! Streaming transcript and runtime event projection helpers.
//! 流式转录与运行时事件投影辅助函数。

use super::*;
use vulcan_service_api::{
    ToolRequestProgressChangedDelta, ToolRequestProgressSnapshot, ToolRequestProgressState,
};

use crate::vmm_session_controller::open_registered_session_vmm_controller;

/// Semantic kind of one provider content delta projected into a streaming transcript record.
/// 投影到流式转录记录中的单个 Provider 内容增量语义类型。
#[derive(Clone, Copy)]
pub(crate) enum StreamContentKind {
    /// Provider reasoning content stored in a collapsed thinking record.
    /// 存储在折叠思考记录中的 Provider 推理内容。
    Thinking,
    /// Provider assistant content stored in a visible output record.
    /// 存储在可见输出记录中的 Provider 助手内容。
    Output,
}

impl StreamContentKind {
    /// Returns the stable first-event trace label for this content kind.
    /// 返回当前内容类型的稳定首事件追踪标签。
    ///
    /// # Parameters
    /// # 参数
    /// This method has no parameters beyond the content kind.
    /// 此方法除内容类型外没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// The trace label used by provider-stream latency diagnostics.
    /// 返回 Provider 流延迟诊断使用的追踪标签。
    pub(crate) fn trace_event_label(self) -> &'static str {
        match self {
            Self::Thinking => "thinking_delta",
            Self::Output => "output_delta",
        }
    }

    /// Returns the stable visible-delta kind label for this content kind.
    /// 返回当前内容类型的稳定可见增量类型标签。
    ///
    /// # Parameters
    /// # 参数
    /// This method has no parameters beyond the content kind.
    /// 此方法除内容类型外没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// The label written to first-visible-delta diagnostics.
    /// 返回写入首个可见增量诊断的标签。
    pub(crate) fn visible_delta_label(self) -> &'static str {
        match self {
            Self::Thinking => "thinking",
            Self::Output => "output",
        }
    }
}

/// Emits one transient reasoning status without mutating transcript or retry state.
/// 发送一条瞬态推理状态，且不修改转录或重试状态。
///
/// # Parameters
/// 参数
/// `app` emits the lightweight event, `state` owns generation ordering, `session_id` selects the
/// conversation, and `status` replaces or clears the current line.
/// `app` 发送轻量事件，`state` 持有代际顺序，`session_id` 选择会话，`status` 替换或清除当前行。
///
/// # Returns
/// 返回值
/// Success after the ordered event is emitted, or an explicit lock failure.
/// 有序事件发送后返回成功，否则返回明确的加锁失败。
pub(crate) fn emit_transient_reasoning_status(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    status: Option<RuntimeTransientReasoningStatus>,
) -> Result<(), String> {
    // event is assembled under the generation lock but intentionally leaves snapshot data untouched.
    // event 在代际锁内组装，但刻意不修改快照数据。
    let event = {
        // guard owns only the ordered workbench generation for this ephemeral emission.
        // guard 仅持有当前瞬态发送所需的有序工作台代际。
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        // generation orders the live event against every other Workbench delta.
        // generation 将实时事件与其他全部 Workbench 增量排序。
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::TransientReasoningStatusChanged(
                TransientReasoningStatusChangedDelta {
                    conversation_id: session_id.to_string(),
                    status: status.map(|status| TransientReasoningStatusSnapshot {
                        kind: match status.kind {
                            RuntimeTransientReasoningStatusKind::Summary => {
                                TransientReasoningStatusKind::Summary
                            }
                            RuntimeTransientReasoningStatusKind::Encrypted => {
                                TransientReasoningStatusKind::Encrypted
                            }
                        },
                        summaries: status
                            .summaries
                            .into_iter()
                            .map(|summary| TransientReasoningSummaryItemSnapshot {
                                item_id: summary.item_id,
                                text: summary.text,
                            })
                            .collect(),
                        reasoning_tokens: status.reasoning_tokens,
                    }),
                },
            ),
        }
    };
    emit_service_event_with_trace(
        app,
        "workbench_transient_reasoning_status_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(())
}

/// Emits or clears one ephemeral tool-request progress snapshot under exact runtime authority.
/// 在精确运行时权限下发送或清除单个瞬态工具请求进度快照。
///
/// # Parameters
/// 参数
/// `app` emits the lightweight event, `state` owns generation ordering, `session_id` selects the
/// conversation, `authority` proves event ownership, and `event` carries the exact runtime metadata.
/// `app` 发送轻量事件，`state` 持有代际顺序，`session_id` 选择会话，`authority` 证明事件归属，
/// `event` 携带精确运行时元数据。
///
/// # Returns
/// 返回值
/// Success after an applicable event is emitted, or an explicit metadata, ownership, or lock error.
/// 适用事件发送后返回成功，否则返回明确的元数据、权限或加锁错误。
fn emit_tool_request_progress_changed(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    authority: RuntimeEventProjectionAuthority,
    event: &RuntimeEvent,
) -> Result<(), String> {
    // change is absent for runtime events outside the closed request-formation lifecycle.
    // change 对封闭请求构造生命周期之外的运行时事件为空。
    let Some((request_stream_id, progress)) = tool_request_progress_change_from_event(event)?
    else {
        return Ok(());
    };
    // envelope is assembled under the authority and generation lock without touching transcript state.
    // envelope 在权限与代际锁内组装，且不会修改转录状态。
    let envelope = commit_runtime_event_projection_state(state, session_id, authority, |guard| {
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        // generation orders the leaf-only progress update against durable lifecycle patches.
        // generation 将仅叶子级进度更新与持久生命周期补丁进行排序。
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        Ok(WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::ToolRequestProgressChanged(ToolRequestProgressChangedDelta {
                conversation_id: session_id.to_string(),
                request_stream_id,
                progress,
            }),
        })
    })?;
    emit_service_event_with_trace(
        app,
        "workbench_tool_request_progress_emit",
        ServiceEvent::WorkbenchDelta(Box::new(envelope)),
    );
    Ok(())
}

/// Decodes the exact transient tool-request change represented by one runtime event.
/// 解码单条运行时事件表示的精确瞬态工具请求变更。
///
/// # Parameters
/// 参数
/// `event` is the authoritative runtime event produced by `ToolRequestStreamCoordinator`.
/// `event` 是 `ToolRequestStreamCoordinator` 生成的权威运行时事件。
///
/// # Returns
/// 返回值
/// A replacement or clear operation for applicable lifecycle events, none for unrelated events,
/// or an explicit contract error for malformed applicable metadata.
/// 对适用生命周期事件返回替换或清除操作，对无关事件返回空，适用元数据损坏时返回明确契约错误。
fn tool_request_progress_change_from_event(
    event: &RuntimeEvent,
) -> Result<Option<(String, Option<ToolRequestProgressSnapshot>)>, String> {
    if !matches!(
        event.kind,
        RuntimeEventKind::ToolRequestStarted
            | RuntimeEventKind::ToolRequestProgress
            | RuntimeEventKind::ToolRequestPreviewUpdated
            | RuntimeEventKind::ToolRequestReady
            | RuntimeEventKind::ToolRequestInvalid
            | RuntimeEventKind::ToolRequestAborted
    ) {
        return Ok(None);
    }
    // metadata is mandatory for every request-stream lifecycle event emitted by the coordinator.
    // metadata 对协调器发出的每个请求流生命周期事件都是必需的。
    let metadata = event.metadata.as_ref().ok_or_else(|| {
        format!(
            "runtime event {:?} is missing tool request metadata",
            event.kind
        )
    })?;
    let request_stream_id = tool_request_metadata_string(metadata, "request_stream_id")?;
    if matches!(
        event.kind,
        RuntimeEventKind::ToolRequestInvalid | RuntimeEventKind::ToolRequestAborted
    ) {
        return Ok(Some((request_stream_id, None)));
    }
    // progressState closes the receiving animation as soon as the strict lexer proves one JSON root.
    // progressState 在严格词法器证明一个 JSON 根值后立即结束接收动画。
    let progress_state =
        match tool_request_metadata_string(metadata, "tool_display_state")?.as_str() {
            "receiving" => ToolRequestProgressState::Receiving,
            "arguments_assembled" => ToolRequestProgressState::ArgumentsAssembled,
            "ready" => ToolRequestProgressState::Ready,
            state => {
                return Err(format!(
                    "runtime tool request progress has unsupported display state `{state}`"
                ));
            }
        };
    // progress preserves only coordinator-owned safe fields and the parser-sanitized preview.
    // progress 仅保留协调器持有的安全字段与解析器净化预览。
    let progress = ToolRequestProgressSnapshot {
        api_call_id: tool_request_metadata_u64(metadata, "api_call_id")?,
        request_stream_id: request_stream_id.clone(),
        tool_call_id: tool_request_metadata_string(metadata, "tool_call_id")?,
        tool_name: tool_request_metadata_string(metadata, "tool_name")?,
        state: progress_state,
        received_bytes: tool_request_metadata_u64(metadata, "received_bytes")?,
        received_characters: tool_request_metadata_u64(metadata, "received_characters")?,
        request_revision: tool_request_metadata_u64(metadata, "request_revision")?,
        preview: metadata
            .get("preview")
            .filter(|value| !value.is_null())
            .cloned(),
    };
    Ok(Some((request_stream_id, Some(progress))))
}

/// Reads one required string field from the closed tool-request metadata contract.
/// 从封闭工具请求元数据契约读取单个必需字符串字段。
///
/// # Parameters
/// 参数
/// `metadata` is the exact coordinator metadata object and `field` is the required contract key.
/// `metadata` 是精确协调器元数据对象，`field` 是必需契约键。
///
/// # Returns
/// 返回值
/// The non-empty string, or an explicit contract error.
/// 返回非空字符串，否则返回明确契约错误。
fn tool_request_metadata_string(
    metadata: &serde_json::Value,
    field: &str,
) -> Result<String, String> {
    metadata
        .get(field)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("tool request metadata field `{field}` must be a non-empty string"))
}

/// Reads one required unsigned integer field from the closed tool-request metadata contract.
/// 从封闭工具请求元数据契约读取单个必需无符号整数字段。
///
/// # Parameters
/// 参数
/// `metadata` is the exact coordinator metadata object and `field` is the required contract key.
/// `metadata` 是精确协调器元数据对象，`field` 是必需契约键。
///
/// # Returns
/// 返回值
/// The unsigned integer, or an explicit contract error.
/// 返回无符号整数，否则返回明确契约错误。
fn tool_request_metadata_u64(metadata: &serde_json::Value, field: &str) -> Result<u64, String> {
    metadata
        .get(field)
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| format!("tool request metadata field `{field}` must be an unsigned integer"))
}
/// Commits one workspace snapshot and emits a phase-only conversation delta.
/// 提交单个工作区快照，并发出纯会话阶段增量。
///
/// # Parameters
/// 参数
/// `app` emits the delta event to connected WebUI clients.
/// `app` 将增量事件发送给已连接的 WebUI 客户端。
/// `state` owns the retained workbench and workspace snapshots.
/// `state` 持有保留的工作台与工作区快照。
/// `workspace_snapshot` is the authoritative workspace state after the phase change.
/// `workspace_snapshot` 是阶段变化后的权威工作区状态。
/// `session_id` selects the conversation that owns the phase.
/// `session_id` 选择拥有该阶段的会话。
/// `run_phase` is the backend lifecycle phase to expose.
/// `run_phase` 是需要暴露的后端生命周期阶段。
///
/// # Returns
/// 返回值
/// `Ok(())` when state was committed and the delta was emitted.
/// 当状态已提交且增量已发送时返回 `Ok(())`。
pub(crate) fn commit_workspace_snapshot_and_emit_session_phase_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    workspace_snapshot: WorkspaceSnapshot,
    session_id: SessionId,
    run_phase: SessionRunPhase,
) -> Result<(), String> {
    commit_workspace_snapshot_and_emit_session_phase_delta_internal(
        app,
        state,
        workspace_snapshot,
        session_id,
        run_phase,
        None,
    )
    .map(|_| ())
}

/// Mutates the latest retained workspace and commits its session phase in one shared-state transaction.
/// 在同一个共享状态事务中修改最新保留工作区并提交其会话阶段。
///
/// # Parameters
/// 参数
/// `app` emits the committed delta and `state` owns the retained workspace and workbench.
/// `app` 发送已提交增量，`state` 持有保留工作区与工作台。
/// `session_id` selects the conversation, `run_phase` fixes its phase, and `mutation` applies the exact in-lock state change.
/// `session_id` 选择会话，`run_phase` 固定其阶段，`mutation` 应用精确的锁内状态变更。
///
/// # Returns
/// 返回值
/// Success after the mutation, workspace phase, workbench revision, and delta are committed, or an explicit mutation/state error.
/// 在变更、工作区阶段、工作台修订与增量提交后返回成功，否则返回明确的变更或状态错误。
pub(crate) fn commit_retained_workspace_session_phase_mutation_and_emit_delta<Mutation>(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    run_phase: SessionRunPhase,
    mutation: Mutation,
) -> Result<(), String>
where
    Mutation: FnOnce(&mut WorkspaceSnapshot) -> Result<(), String>,
{
    let conversation_id = session_id.to_string();
    let session_state = session_state_for_run_phase(run_phase);
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace_snapshot = guard.workspace_snapshot.as_mut().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        mutation(workspace_snapshot)?;
        workspace_snapshot
            .session_run_phases
            .insert(session_id, run_phase);
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        for conversation in &mut guard.workbench.conversations {
            if conversation.id == conversation_id {
                conversation.run_phase = run_phase.to_string();
                conversation.status = session_state.label().to_string();
                conversation.last_updated_label = String::from("刚刚");
                conversation.last_updated_at = current_unix_millis_text();
                break;
            }
        }
        WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::UpdateConversationRunPhase(UpdateConversationRunPhaseDelta {
                conversation_id,
                run_phase: run_phase.to_string(),
                status: Some(session_state.label().to_string()),
                last_updated_label: Some(String::from("now")),
            }),
        }
    };
    emit_service_event_with_trace(
        app,
        "workbench_retained_phase_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(())
}

/// Commits a prepared workspace phase only while its exact kernel generation still owns the turn.
/// 仅当精确 kernel 代际仍拥有当前回合时才提交准备工作区阶段。
///
/// # Parameters
/// 参数
/// The workspace projection parameters retain the ordinary phase-commit contract.
/// 工作区投影参数保留普通阶段提交契约。
/// `expected_generation` and `turn_id` bind the detached candidate to its immutable preparation owner.
/// `expected_generation` 与 `turn_id` 将分离候选绑定到其不可变准备所有者。
///
/// # Returns
/// 返回值
/// `true` after commit, `false` when Stop/replacement won, or an explicit state error.
/// 提交后返回 `true`；Stop/替代已胜出时返回 `false`；状态异常时返回显式错误。
pub(crate) fn commit_workspace_snapshot_and_emit_session_phase_delta_if_prompt_owner_current(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    workspace_snapshot: WorkspaceSnapshot,
    session_id: SessionId,
    run_phase: SessionRunPhase,
    expected_generation: u64,
    turn_id: &str,
) -> Result<bool, String> {
    commit_workspace_snapshot_and_emit_session_phase_delta_internal(
        app,
        state,
        workspace_snapshot,
        session_id,
        run_phase,
        Some((expected_generation, turn_id)),
    )
}

/// Moves one exact prompt-session slice into the retained workspace without replacing unrelated state.
/// 把单个精确提示词会话切片移入保留工作区，且不替换无关状态。
///
/// # Parameters
/// 参数
/// `retained` is the authoritative full workspace borrowed under the service-state lock.
/// `retained` 是在服务状态锁下借用的权威完整工作区。
/// `candidate` owns the one-session detached preparation slice.
/// `candidate` 持有单会话分离准备切片。
/// `session_id` identifies the only retained keys allowed to change.
/// `session_id` 标识唯一允许变化的保留键。
/// `run_phase` is the authoritative phase committed with the slice.
/// `run_phase` 是随切片提交的权威阶段。
///
/// # Returns
/// 返回值
/// Success after ownership transfer, or an explicit missing-session contract error.
/// 所有权转移完成后返回成功，否则返回明确的会话缺失契约错误。
fn merge_prompt_session_slice_into_retained_workspace(
    retained: &mut WorkspaceSnapshot,
    mut candidate: WorkspaceSnapshot,
    session_id: SessionId,
    run_phase: SessionRunPhase,
) -> Result<(), String> {
    let candidate_position = candidate
        .sessions
        .iter()
        .position(|session| session.id == session_id)
        .ok_or_else(|| format!("prompt preparation candidate is missing session `{session_id}`"))?;
    let session = candidate.sessions.remove(candidate_position);
    let retained_session = workspace_session_summary_for_session_mut(retained, session_id)
        .ok_or_else(|| {
            format!("prompt preparation owner session `{session_id}` left retained state")
        })?;
    *retained_session = session;
    let runtime_state = candidate
        .session_runtime_state
        .remove(&session_id)
        .ok_or_else(|| {
            format!("prompt preparation candidate is missing runtime state for `{session_id}`")
        })?;
    retained
        .session_runtime_state
        .insert(session_id, runtime_state);
    retained.session_run_phases.insert(session_id, run_phase);
    if candidate.hydrated_session_ids.contains(&session_id) {
        retained.hydrated_session_ids.insert(session_id);
    }
    Ok(())
}

/// Commits and emits one phase delta with an optional exact prompt-preparation owner fence.
/// 使用可选的精确提示词准备所有者栅栏提交并发送一条阶段增量。
///
/// # Parameters
/// 参数
/// The phase parameters match the public helpers; `expected_owner` optionally carries `(kernel generation, turn id)`.
/// 阶段参数与公开辅助函数一致；`expected_owner` 可选携带“kernel 代际、回合标识”。
///
/// # Returns
/// 返回值
/// Whether the workspace and phase committed, or an explicit state error.
/// 返回工作区与阶段是否已提交，或显式状态错误。
fn commit_workspace_snapshot_and_emit_session_phase_delta_internal(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    mut workspace_snapshot: WorkspaceSnapshot,
    session_id: SessionId,
    run_phase: SessionRunPhase,
    expected_owner: Option<(u64, &str)>,
) -> Result<bool, String> {
    let conversation_id = session_id.to_string();
    let session_state = session_state_for_run_phase(run_phase);
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if let Some((expected_generation, turn_id)) = expected_owner
            && !webui_prompt_preparation_owner_is_current(
                &guard,
                session_id,
                expected_generation,
                turn_id,
            )
        {
            return Ok(false);
        }
        // Only the completed Preparing candidate has a source validated by profile reconciliation.
        // 只有完成后的 Preparing 候选具有经过画像对账校验的来源。
        let profile_publication =
            if expected_owner.is_some() && run_phase == SessionRunPhase::Preparing {
                workspace_snapshot
                    .session_runtime_state
                    .get(&session_id)
                    .and_then(|runtime| runtime.vmm_profile_sync.read_source.as_ref())
                    .map(|source| {
                        let retained = guard.workspace_snapshot.as_ref().ok_or_else(|| {
                            "VMM profile publication requires a retained workspace.".to_owned()
                        })?;
                        let runtime = guard.agent_runtime.as_ref().ok_or_else(|| {
                            "VMM profile publication requires the current host runtime.".to_owned()
                        })?;
                        Ok::<_, String>((
                            runtime.vmm_hook_runtime.clone(),
                            crate::workspace_path_for_session(retained, session_id),
                            source.clone(),
                        ))
                    })
                    .transpose()?
            } else {
                None
            };
        // Keep the existing service lock and, for profile candidates, the hook read lock through this exact merge.
        // 在本次准确合并期间保留既有服务锁；存在画像候选时同时保留 Hook 读锁。
        let publish = || -> Result<WorkbenchDeltaEnvelope, String> {
            if expected_owner.is_some() {
                // latestUiState belongs to concurrent queue, composer, read, and collapse interactions rather than detached prompt preparation.
                // latestUiState 归并发队列、输入框、已读与折叠交互所有，而不归分离提示词准备所有。
                let latest_ui_state = guard
                    .workspace_snapshot
                    .as_ref()
                    .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
                    .map(|runtime_state| runtime_state.ui_state.clone());
                if let Some(latest_ui_state) = latest_ui_state {
                    // candidateRuntimeState must exist because an owner-checked preparation commit cannot synthesize a partial session.
                    // candidateRuntimeState 必须存在，因为经过所有者校验的准备提交不得合成不完整会话。
                    let candidate_runtime_state = workspace_snapshot
                    .session_runtime_state
                    .get_mut(&session_id)
                    .ok_or_else(|| {
                        format!(
                            "prompt preparation candidate is missing runtime state for session `{session_id}`"
                        )
                    })?;
                    candidate_runtime_state.ui_state = latest_ui_state;
                }
            }
            guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
            let generation = guard.workbench_generation;
            guard.workbench.revision = generation;
            if expected_owner.is_some() {
                let retained_workspace = guard
                    .workspace_snapshot
                    .as_mut()
                    .ok_or_else(|| String::from("retained workspace is no longer initialized"))?;
                merge_prompt_session_slice_into_retained_workspace(
                    retained_workspace,
                    workspace_snapshot,
                    session_id,
                    run_phase,
                )?;
            } else {
                guard.workspace_snapshot = Some(workspace_snapshot);
            }
            for conversation in &mut guard.workbench.conversations {
                if conversation.id == conversation_id {
                    conversation.run_phase = run_phase.to_string();
                    conversation.status = session_state.label().to_string();
                    conversation.last_updated_label = String::from("刚刚");
                    conversation.last_updated_at = current_unix_millis_text();
                    break;
                }
            }
            Ok(WorkbenchDeltaEnvelope {
                generation,
                delta: WorkbenchDelta::UpdateConversationRunPhase(
                    UpdateConversationRunPhaseDelta {
                        conversation_id,
                        run_phase: run_phase.to_string(),
                        status: Some(session_state.label().to_string()),
                        last_updated_label: Some(String::from("now")),
                    },
                ),
            })
        };
        if let Some((runtime, workspace_path, source)) = profile_publication {
            runtime.with_current_profile_publication(
                session_id,
                &workspace_path,
                &source,
                publish,
            )??
        } else {
            publish()?
        }
    };
    emit_service_event_with_trace(
        app,
        "workbench_phase_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(true)
}

/// Emits the selected-session context injection side-panel projection as its own delta.
/// 将当前选中会话的上下文注入侧面板投影作为独立增量发出。
///
/// # Parameters
/// 参数
/// `app` emits the delta event to connected WebUI clients.
/// `app` 将增量事件发送给已连接的 WebUI 客户端。
/// `state` owns the retained workspace and workbench projections.
/// `state` 持有保留的工作区与工作台投影。
/// `session_id` selects the session whose memory-panel projection changed.
/// `session_id` 选择记忆面板投影发生变化的会话。
///
/// # Returns
/// 返回值
/// `Ok(())` when the selected-session projection was emitted or skipped for a background session.
/// 当前选中会话投影已发出，或后台会话被跳过时返回 `Ok(())`。
pub(crate) fn emit_context_injection_changed_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<(), String> {
    emit_context_injection_changed_delta_internal(app, state, session_id, true)
}

/// Emits one exact session context-injection projection even when another session is selected locally.
/// 即使本地选中了其他会话，也发送一个准确会话的上下文注入投影。
///
/// # Parameters
/// 参数
/// `app` broadcasts the delta event to every connected WebUI client.
/// `app` 将增量事件广播给每个已连接 WebUI 客户端。
/// `state` owns the retained workspace and workbench projections.
/// `state` 持有保留的工作区与工作台投影。
/// `session_id` is the exact persisted session changed by the initiating request.
/// `session_id` 是发起请求所修改的准确持久化会话。
///
/// # Returns
/// 返回值
/// `Ok(())` after the exact session projection is committed and emitted.
/// 准确会话投影提交并发送后返回 `Ok(())`。
pub(crate) fn emit_context_injection_changed_delta_for_session(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<(), String> {
    emit_context_injection_changed_delta_internal(app, state, session_id, false)
}

/// Commits and emits one context-injection delta with an explicit selection policy.
/// 按明确的选中策略提交并发送一条上下文注入增量。
///
/// # Parameters
/// 参数
/// `app` broadcasts the committed delta.
/// `app` 广播已提交增量。
/// `state` owns the retained workspace and workbench projections.
/// `state` 持有保留的工作区与工作台投影。
/// `session_id` selects the exact session projection to rebuild.
/// `session_id` 选择需要重建的准确会话投影。
/// `require_selected_session` skips background sessions for selected-surface-only callers.
/// `require_selected_session` 为仅处理选中界面的调用方跳过后台会话。
///
/// # Returns
/// 返回值
/// `Ok(())` after emission, or after an allowed selected-session skip.
/// 发送完成后返回 `Ok(())`；允许跳过非选中会话时也返回该值。
fn emit_context_injection_changed_delta_internal(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    require_selected_session: bool,
) -> Result<(), String> {
    // VMM projection is absent unless both service configuration and runtime are ready.
    // 除非 VMM 服务配置与运行态同时就绪，否则投影必须为空。
    let vmm_session_memory = if webui_vmm_runtime_readiness(state)?.service_ready {
        Some(
            open_registered_session_vmm_controller(state, session_id, &current_unix_millis_text())?
                .snapshot()?,
        )
    } else {
        None
    };
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let Some(workspace_snapshot) = guard.workspace_snapshot.as_ref() else {
            return Ok(());
        };
        // delta is rebuilt from the exact session runtime entry instead of one client's mutable selection.
        // delta 从准确的会话运行态条目重建，而不是依赖某个客户端的可变选中项。
        let delta = if require_selected_session {
            let Some(delta) = context_injection_changed_delta_from_workspace_snapshot(
                workspace_snapshot,
                session_id,
                vmm_session_memory.as_ref(),
            ) else {
                return Ok(());
            };
            delta
        } else {
            context_injection_changed_delta_from_runtime_state(
                session_id,
                workspace_snapshot.session_runtime_state.get(&session_id),
                vmm_session_memory.as_ref(),
            )
        };
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        if guard.workbench.selected_conversation_id.as_deref()
            == Some(delta.conversation_id.as_str())
        {
            guard.workbench.context_injection = delta.context_injection.clone();
        }
        replace_conversation_scoped_projection_entry(
            &mut guard.workbench.context_injection_by_conversation,
            delta.conversation_id.as_str(),
            delta.context_injection.is_some().then(|| delta.clone()),
            |entry| entry.conversation_id.as_str(),
        );
        WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::ContextInjectionChanged(delta),
        }
    };
    emit_service_event_with_trace(
        app,
        "workbench_context_injection_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(())
}

/// Builds a memory-panel delta from the selected session runtime projection.
/// 从当前选中会话运行态投影构建记忆面板增量。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` provides the retained session runtime state.
/// `workspace_snapshot` 提供保留的会话运行态。
/// `session_id` selects the session whose projection may have changed.
/// `session_id` 选择可能发生投影变化的会话。
///
/// # Returns
/// 返回值
/// A context-injection delta when `session_id` is the selected session.
/// 当 `session_id` 是当前选中会话时返回上下文注入增量。
fn context_injection_changed_delta_from_workspace_snapshot(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    vmm_session_memory: Option<&vulcan_vmm::VmmSessionMemoryState>,
) -> Option<ContextInjectionChangedDelta> {
    if workspace_snapshot.selected_session != session_id {
        return None;
    }
    Some(context_injection_changed_delta_from_runtime_state(
        session_id,
        workspace_snapshot.session_runtime_state.get(&session_id),
        vmm_session_memory,
    ))
}

/// Returns whether the retained workbench currently displays one conversation surface.
/// 返回保留工作台当前是否正在展示指定会话界面。
pub(crate) fn workbench_surface_matches_conversation(
    workbench: &WorkbenchSnapshot,
    conversation_id: &str,
) -> bool {
    workbench.selected_conversation_id.as_deref() == Some(conversation_id)
        || workbench
            .subagent_navigation
            .as_ref()
            .is_some_and(|navigation| navigation.active_conversation_id == conversation_id)
}

/// Runs one runtime-event projection attempt and releases its barrier before returning.
/// 执行一次运行时事件投影尝试，并在返回前释放对应屏障。
///
/// # Parameters
/// # 参数
/// `projection` performs the fallible derived-state projection.
/// `projection` 执行可能失败的派生状态投影。
/// `release_barrier` attempts to release the Agent Runtime wait boundary exactly once.
/// `release_barrier` 严格尝试一次 Agent Runtime 等待边界释放。
///
/// # Returns
/// # 返回值
/// Success when both operations succeed, otherwise a diagnostic preserving every failure.
/// 两项操作都成功时返回成功，否则返回保留全部失败的诊断。
fn finish_runtime_event_projection_attempt(
    projection: impl FnOnce() -> Result<(), String>,
    release_barrier: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    // projectionResult preserves the derived-state outcome while barrier release is attempted independently.
    // projectionResult 保留派生状态结果，同时独立尝试释放屏障。
    let projection_result = projection();
    // barrierReleaseResult preserves the runtime wait-boundary outcome for combined diagnostics.
    // barrierReleaseResult 保留运行时等待边界结果以生成合并诊断。
    let barrier_release_result = release_barrier();
    match (projection_result, barrier_release_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(projection_error), Err(barrier_error)) => Err(format!(
            "{projection_error} Runtime-event projection barrier release also failed: {barrier_error}"
        )),
    }
}

/// Verifies that a rejected projection still releases its barrier after the projection attempt.
/// 验证被拒绝的投影仍会在投影尝试结束后释放屏障。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if ordering, release, or error propagation changes.
/// 此测试无返回值；顺序、释放动作或错误传播发生变化时测试失败。
#[cfg(test)]
#[test]
fn rejected_runtime_event_projection_still_releases_barrier() {
    // Track whether the fallible projection closure has executed.
    // 跟踪可失败投影闭包是否已经执行。
    let projection_attempted = std::cell::Cell::new(false);
    // Track whether the independent barrier release closure has executed.
    // 跟踪独立屏障释放闭包是否已经执行。
    let barrier_released = std::cell::Cell::new(false);
    // Capture the original error returned after the mandatory release boundary.
    // 捕获强制释放边界完成后返回的原始错误。
    let result = finish_runtime_event_projection_attempt(
        || {
            projection_attempted.set(true);
            Err(String::from("projection rejected"))
        },
        || {
            assert!(projection_attempted.get());
            barrier_released.set(true);
            Ok(())
        },
    );

    assert_eq!(result, Err(String::from("projection rejected")));
    assert!(barrier_released.get());
}

/// Verifies that a successful projection releases its barrier after the projection attempt.
/// 验证成功投影会在投影尝试结束后释放屏障。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if success propagation or release ordering changes.
/// 此测试无返回值；成功传播或释放顺序发生变化时测试失败。
#[cfg(test)]
#[test]
fn successful_runtime_event_projection_releases_barrier_after_projection() {
    // Track whether the successful projection closure has executed.
    // 跟踪成功投影闭包是否已经执行。
    let projection_attempted = std::cell::Cell::new(false);
    // Track whether the independent barrier release closure has executed.
    // 跟踪独立屏障释放闭包是否已经执行。
    let barrier_released = std::cell::Cell::new(false);
    // Capture the successful result returned after the mandatory release boundary.
    // 捕获强制释放边界完成后返回的成功结果。
    let result = finish_runtime_event_projection_attempt(
        || {
            projection_attempted.set(true);
            Ok(())
        },
        || {
            assert!(projection_attempted.get());
            barrier_released.set(true);
            Ok(())
        },
    );

    assert_eq!(result, Ok(()));
    assert!(barrier_released.get());
}

/// Verifies simultaneous projection and barrier-release failures remain visible together.
/// 验证投影与屏障释放同时失败时，两项错误都会保持可见。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if either independent error is discarded.
/// 此测试无返回值；任一独立错误被丢弃时测试失败。
#[cfg(test)]
#[test]
fn runtime_event_projection_preserves_projection_and_barrier_failures() {
    // error is the combined diagnostic returned after both independent operations fail.
    // error 是两项独立操作均失败后返回的合并诊断。
    let error = finish_runtime_event_projection_attempt(
        || Err(String::from("projection failed")),
        || Err(String::from("barrier release failed")),
    )
    .expect_err("dual projection failures must remain observable");

    assert!(error.contains("projection failed"));
    assert!(error.contains("barrier release failed"));
}

/// Identifies the concrete runtime authority allowed to commit one event projection.
/// 标识获准提交单次事件投影的具体运行时权限来源。
#[derive(Clone, Copy)]
pub(crate) enum RuntimeEventProjectionAuthority {
    /// A detached or compaction runtime owns the event independently of the root stream registry.
    /// 分离运行时或压缩运行时独立于根流注册表持有该事件。
    IndependentRuntime,
    /// The exact registered root stream task owns the event.
    /// 已精确注册的根流任务持有该事件。
    ActiveStreamTask(Uuid),
}

/// Executes one runtime-event projection mutation under its exact authority and state lock.
/// 在精确权限与状态锁保护下执行单次运行时事件投影变更。
///
/// # Parameters
/// # 参数
/// `state` owns the active-stream registry and all retained projection state.
/// `state` 持有活跃流注册表与全部保留投影状态。
/// `session_id` identifies the session whose projection is being committed.
/// `session_id` 标识正在提交投影的会话。
/// `authority` identifies whether the event belongs to an independent runtime or one exact root task.
/// `authority` 标识事件属于独立运行时还是某个精确根任务。
/// `projection` performs all validated in-memory mutations while the ownership decision remains stable.
/// `projection` 在所有权判定保持稳定期间执行全部已验证的内存变更。
///
/// # Returns
/// # 返回值
/// Returns the projection result, or an explicit error before mutation when root-task ownership was superseded.
/// 返回投影结果；根任务所有权已被替换时会在变更前返回明确错误。
pub(crate) fn commit_runtime_event_projection_state<T>(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    authority: RuntimeEventProjectionAuthority,
    projection: impl FnOnce(&mut ServiceBridgeState) -> Result<T, String>,
) -> Result<T, String> {
    // The guard makes exact-owner validation and every following mutation one atomic transaction.
    // 该锁守卫将精确所有者校验与后续全部变更合并为一个原子事务。
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let RuntimeEventProjectionAuthority::ActiveStreamTask(task_id) = authority
        && !active_stream_task_matches_id(&guard, session_id, task_id)
    {
        return Err(format!(
            "runtime event projection ownership was superseded: session={session_id} task={task_id}"
        ));
    }
    projection(&mut guard)
}

/// Projects one runtime event and always releases its Agent Runtime projection barrier.
/// 投影单条运行时事件，并始终释放其 Agent Runtime 投影屏障。
///
/// # Parameters
/// # 参数
/// `app` emits the resulting WebUI deltas.
/// `app` 发出生成的 WebUI 增量。
/// `state` owns the shared retained projection state.
/// `state` 持有共享的保留投影状态。
/// `session_id` identifies the session that owns the projection.
/// `session_id` 标识拥有该投影的会话。
/// `event` is the authoritative runtime event to project and release.
/// `event` 是需要投影并释放屏障的权威运行时事件。
///
/// # Returns
/// # 返回值
/// Returns the projection result; the barrier is released before either success or failure is returned.
/// 返回投影结果；无论成功或失败，屏障都会在返回前释放。
pub(crate) fn append_runtime_event_projection_and_release_barrier(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    event: &RuntimeEvent,
) -> Result<(), String> {
    finish_runtime_event_projection_attempt(
        || append_runtime_event_projection(app, state, session_id, event),
        || release_runtime_event_projection_barrier_after_attempt(event).map(|_| ()),
    )
}

/// Projects one active root stream event through its exact registered task authority.
/// 通过精确注册的任务权限投影单条活跃根流事件。
///
/// # Parameters
/// # 参数
/// `app` emits WebUI deltas produced after the atomic projection commit.
/// `app` 发出原子投影提交后生成的 WebUI 增量。
/// `state` owns the root stream registry and retained projection state.
/// `state` 持有根流注册表与保留投影状态。
/// `session_id` identifies the root session that owns the event.
/// `session_id` 标识持有该事件的根会话。
/// `active_stream_task_id` identifies the exact registered task allowed to mutate the session.
/// `active_stream_task_id` 标识获准修改该会话的精确注册任务。
/// `event` is the authoritative runtime event emitted by that task.
/// `event` 是该任务发出的权威运行时事件。
///
/// # Returns
/// # 返回值
/// Returns `Ok(())` after projection, or an explicit error when task ownership or projection validation fails.
/// 投影完成后返回 `Ok(())`；任务所有权或投影验证失败时返回明确错误。
pub(crate) fn append_active_stream_runtime_event_projection(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    active_stream_task_id: Uuid,
    event: &RuntimeEvent,
) -> Result<(), String> {
    append_runtime_event_projection_with_authority(
        app,
        state,
        session_id,
        RuntimeEventProjectionAuthority::ActiveStreamTask(active_stream_task_id),
        event,
    )
}

/// Projects lifecycle state for a durable V1 tool event or an independent detached/compaction event.
/// 投影已可靠提交的 V1 工具事件或独立的分离、压缩事件所携带的生命周期状态。
///
/// # Parameters
/// # 参数
/// `app` emits WebUI deltas produced after projection commit.
/// `app` 发出投影提交后生成的 WebUI 增量。
/// `state` owns the retained projection state.
/// `state` 持有保留投影状态。
/// `session_id` identifies the event owner; the V1 tool path takes it from the durable sink handle.
/// `session_id` 标识事件所有者；V1 工具路径从可靠接收器句柄取得该值。
/// `event` is the exact executor event; V1 callers must commit its typed tool boundary first.
/// `event` 是执行器的精确事件；V1 调用方必须先提交其强类型工具边界。
///
/// # Returns
/// # 返回值
/// Returns `Ok(())` after projection, or a validation or state error before partial commit.
/// 投影完成后返回 `Ok(())`；验证或状态错误会在局部提交前返回。
pub(crate) fn append_runtime_event_projection(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    event: &RuntimeEvent,
) -> Result<(), String> {
    append_runtime_event_projection_with_authority(
        app,
        state,
        session_id,
        RuntimeEventProjectionAuthority::IndependentRuntime,
        event,
    )
}

/// Projects one runtime event under the caller's verified authority contract.
/// 根据调用方已验证的权限契约投影单条运行时事件。
///
/// # Parameters
/// # 参数
/// `app` emits WebUI deltas produced after projection commit.
/// `app` 发出投影提交后生成的 WebUI 增量。
/// `state` owns all retained state and runtime ownership registries.
/// `state` 持有全部保留状态与运行时所有权注册表。
/// `session_id` identifies the session receiving the event projection.
/// `session_id` 标识接收事件投影的会话。
/// `authority` carries the concrete ownership rule for this event source.
/// `authority` 携带该事件来源的具体所有权规则。
/// `event` is the authoritative runtime event to validate and project.
/// `event` 是需要验证并投影的权威运行时事件。
///
/// # Returns
/// # 返回值
/// Returns `Ok(())` after a complete projection, or an error before any fallible partial commit.
/// 完整投影后返回 `Ok(())`；任何可失败的局部提交前会返回错误。
fn append_runtime_event_projection_with_authority(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    authority: RuntimeEventProjectionAuthority,
    event: &RuntimeEvent,
) -> Result<(), String> {
    if matches!(
        event.kind,
        RuntimeEventKind::ToolRequestProgress | RuntimeEventKind::ToolRequestPreviewUpdated
    ) {
        return emit_tool_request_progress_changed(app, state, session_id, authority, event);
    }

    // Command-session and subagent-navigation surfaces are lifecycle projections, not transcript authorities.
    // 命令会话与子代理导航界面属于生命周期投影，不是转录权威。
    let refreshes_command_sessions = runtime_event_changes_command_sessions(event);
    let refreshes_subagent_navigation = runtime_event_changes_subagent_navigation(event);
    let closed_subagent_task_ids = close_agent_closed_subagent_task_ids_from_event(event)?;
    // The close owner retains active child handles until their original terminal is durable.
    // 关闭所有者保留活动子句柄，直到其原始终态可靠落盘。
    let subagent_navigation_changed = if closed_subagent_task_ids.is_empty() {
        false
    } else {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        unload_webui_closed_subagent_runtime_state(&mut guard, &closed_subagent_task_ids)
    };

    if refreshes_command_sessions
        && let Err(error) = emit_live_command_sessions_changed_delta(app, state)
    {
        tracing::warn!(
            "[webui-service] runtime terminal event command-session delta emit failed: {error}"
        );
    }
    if (refreshes_subagent_navigation || subagent_navigation_changed)
        && let Err(error) =
            emit_webui_subagent_navigation_changed_delta_for_parent(app, state, session_id)
    {
        tracing::warn!("[webui-service] runtime event navigation delta emit failed: {error}");
    }

    // Terminal request events clear only the request-local progress cache after the V1 mutation is durable.
    // 请求终态事件仅在 V1 变更持久化后清理请求局部进度缓存。
    emit_tool_request_progress_changed(app, state, session_id, authority, event)
}
/// Reports whether one runtime event completed a tool that owns command-session lifecycle state.
/// 判断单个运行时事件是否完成了持有命令会话生命周期状态的工具。
///
/// # Parameters
/// 参数
/// `event` is the runtime event currently being projected into WebUI state.
/// `event` 是当前正投影到 WebUI 状态的运行时事件。
///
/// # Returns
/// 返回值
/// `true` only after a terminal-session tool has finished mutating the shared runtime.
/// 仅当终端会话工具已经完成共享运行时变更后返回 `true`。
fn runtime_event_changes_command_sessions(event: &RuntimeEvent) -> bool {
    event.kind == RuntimeEventKind::ToolFinished
        && matches!(
            runtime_event_tool_name(event),
            Some("terminal_session" | "shell_session")
        )
}

/// Reports whether one runtime event changed the open-subagent navigation lifecycle.
/// 判断单个运行时事件是否改变了未关闭子代理的导航生命周期。
///
/// # Parameters
/// 参数
/// `event` is the runtime event currently being projected into WebUI state.
/// `event` 是当前正投影到 WebUI 状态的运行时事件。
///
/// # Returns
/// 返回值
/// `true` after `spawn_agent` registers a child or `close_agent` removes one from the open tree.
/// `spawn_agent` 注册子任务或 `close_agent` 从开放树移除子任务后返回 `true`。
fn runtime_event_changes_subagent_navigation(event: &RuntimeEvent) -> bool {
    event.kind == RuntimeEventKind::ToolFinished
        && matches!(
            runtime_event_tool_name(event),
            Some("spawn_agent" | "close_agent" | "send_message")
        )
}

/// Extracts closed subagent task ids from a structured `close_agent` host result.
/// 从结构化 `close_agent` 宿主结果中提取已关闭的子代理任务标识。
fn close_agent_closed_subagent_task_ids_from_event(
    event: &RuntimeEvent,
) -> Result<Vec<SessionId>, String> {
    if event.kind != RuntimeEventKind::ToolFinished {
        return Ok(Vec::new());
    }
    let Some(metadata) = event.metadata.as_ref() else {
        return Ok(Vec::new());
    };
    Ok(
        subagent_close_agent_projection_from_tool_metadata(metadata)?
            .map(|projection| projection.affected_task_ids)
            .unwrap_or_default(),
    )
}

/// Returns the UI run phase that must advance together with one runtime-event message patch.
/// 返回必须与单条运行时事件消息补丁一起推进的界面运行阶段。
/// Caches the latest provider usage into the live context-budget snapshot.
/// 将最近一次 Provider 用量缓存进实时上下文预算快照。
/// Applies one observed provider usage sample to an existing budget snapshot.
/// 将一份 Provider 实测用量应用到既有预算快照。
pub(crate) fn apply_observed_usage_to_context_budget(
    context_budget: &mut ContextBudgetSnapshot,
    usage: &ProviderUsageSnapshot,
) {
    apply_observed_usage_to_context_budget_with_local_state(context_budget, usage, None);
}

/// Applies observed usage and post-response local messages to one live budget snapshot.
/// 将 Provider 实测用量与响应后的本地消息增量应用到实时预算快照。
fn apply_observed_usage_to_context_budget_with_local_state(
    context_budget: &mut ContextBudgetSnapshot,
    usage: &ProviderUsageSnapshot,
    conversation_state: Option<&vulcan_context::SessionConversationState>,
) {
    let Some(input_tokens) =
        vulcan_context::observed_input_tokens_from_usage(Some(usage)).or(usage.total_tokens)
    else {
        context_budget.observed_usage = Some(usage.clone());
        return;
    };
    context_budget.estimated_input_tokens = input_tokens;
    let provider_context_tokens = provider_context_tokens_from_usage(usage, input_tokens);
    // Codex adds messages written after the last model-generated item to the latest Provider total.
    // Codex 会把最近一次模型输出之后写入的消息增量加到最近一次 Provider 总量上。
    let local_context_tokens = conversation_state
        .map(local_context_tokens_after_latest_assistant)
        .map(|result| {
            result.unwrap_or_else(|error| {
                format!("Context projection failed: {error}")
                    .chars()
                    .count()
                    .saturating_add(3)
                    / 4
            })
        })
        .unwrap_or_default();
    let active_context_tokens = provider_context_tokens.saturating_add(local_context_tokens);
    let auto_compact_context_tokens = match context_budget.auto_compact_scope {
        vulcan_context::ContextWindowUsageScope::Total => active_context_tokens,
        vulcan_context::ContextWindowUsageScope::BodyAfterPrefix => active_context_tokens
            .saturating_sub(
                context_budget
                    .auto_compact_prefill_input_tokens
                    .unwrap_or(active_context_tokens),
            ),
    };
    // The latest provider total is the active-context baseline; cumulative usage never drives the window.
    // 最近一次 Provider 总量是活动上下文基线；累计用量永远不能驱动窗口判断。
    context_budget.active_context_tokens = active_context_tokens;
    context_budget.auto_compact_context_tokens = auto_compact_context_tokens;
    context_budget.estimated_total_tokens = active_context_tokens;
    context_budget.usage_ratio = context_budget
        .effective_context_window
        .or(context_budget.effective_input_limit)
        .map(|limit| active_context_tokens as f32 / limit.max(1) as f32)
        .unwrap_or_else(|| if input_tokens > 0 { 0.01 } else { 0.0 });
    context_budget.severity =
        vulcan_context::budget::context_budget_severity_for_limits_with_compaction_context(
            active_context_tokens,
            context_budget.effective_context_window,
            context_budget.auto_compact_limit,
            auto_compact_context_tokens,
        );
    context_budget.observed_usage = Some(usage.clone());
}

/// Estimates canonical messages written after the latest assistant response.
/// 估算最近一次助手响应之后写入的规范消息 Token 数。
fn local_context_tokens_after_latest_assistant(
    conversation_state: &vulcan_context::SessionConversationState,
) -> Result<usize, String> {
    let source_messages =
        vulcan_context::conversation_state_to_compaction_source_messages(conversation_state)?;
    let start = source_messages
        .iter()
        .rposition(|message| {
            message.record_kind == vulcan_context::SessionMessageRecordKind::AssistantOutput
        })
        .map(|index| index.saturating_add(1))
        .unwrap_or_default();
    Ok(vulcan_context_compaction::estimate_compaction_source_tokens(&source_messages[start..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use vulcan_context_injection::InjectionInsert;
    use vulcan_subagent_core::STOPPED_SUBAGENT_TASK_IDS_KEY;

    /// Verifies close-agent UI cleanup reads only the structured host result task ids.
    /// 验证关闭代理的界面清理只读取结构化宿主结果中的任务标识。
    #[test]
    fn close_agent_closed_task_ids_are_extracted_from_matched_agents() {
        let task_id = SessionId::new_v4();
        let event = RuntimeEvent::new(
            RuntimeEventKind::ToolFinished,
            "工具返回 · close_agent",
            "closed",
        )
        .with_metadata(Some(json!({
            "host_result": {
                "kind": "subagent_control",
                "payload": {
                    "tool": "close_agent",
                    "matched_agents": [{
                        (SUBAGENT_TASK_ID_KEY): task_id.to_string(),
                        "agent_id": "code-review"
                    }],
                    (STOPPED_SUBAGENT_TASK_IDS_KEY): [],
                }
            }
        })));

        assert_eq!(
            close_agent_closed_subagent_task_ids_from_event(&event).expect("close ids"),
            vec![task_id]
        );
    }
    /// Verifies request-progress decoding preserves exact safe metadata and terminal lifecycle semantics.
    /// 验证请求进度解码保留精确安全元数据与终态生命周期语义。
    ///
    /// # Returns
    /// 返回值
    /// This test returns no value and reports contract drift through assertions.
    /// 此测试不返回值，并通过断言报告契约漂移。
    #[test]
    fn tool_request_progress_change_uses_the_closed_runtime_contract() {
        // progress mirrors one exact coordinator event and excludes raw argument content.
        // progress 镜像一条精确协调器事件，并排除原始参数内容。
        let progress = RuntimeEvent::new(
            RuntimeEventKind::ToolRequestProgress,
            "tool request",
            String::new(),
        )
        .with_metadata(Some(serde_json::json!({
            "api_call_id": 1,
            "request_stream_id": "request-1",
            "tool_call_id": "call-1",
            "tool_name": "terminal_session",
            "tool_display_state": "receiving",
            "received_bytes": 512,
            "received_characters": 500,
            "request_revision": 3,
            "preview": { "kind": "command" },
        })));
        let (request_stream_id, snapshot) = tool_request_progress_change_from_event(&progress)
            .expect("progress metadata should satisfy the closed contract")
            .expect("progress event should produce one replacement");
        assert_eq!(request_stream_id, "request-1");
        let snapshot = snapshot.expect("progress event should carry one snapshot");
        assert_eq!(snapshot.request_stream_id, "request-1");
        assert_eq!(snapshot.api_call_id, 1);
        assert_eq!(snapshot.tool_call_id, "call-1");
        assert_eq!(snapshot.tool_name, "terminal_session");
        assert_eq!(snapshot.state, ToolRequestProgressState::Receiving);
        assert_eq!(snapshot.received_bytes, 512);
        assert_eq!(snapshot.received_characters, 500);
        assert_eq!(snapshot.request_revision, 3);
        assert_eq!(
            snapshot.preview,
            Some(serde_json::json!({ "kind": "command" }))
        );
        // Name discovery alone must create a visible draft before arguments or previews arrive.
        // 仅名称发现就必须创建可见草稿，无需等待参数或预览到达。
        let mut started = progress.clone();
        started.kind = RuntimeEventKind::ToolRequestStarted;
        started.metadata.as_mut().unwrap()["received_bytes"] = serde_json::json!(0);
        started.metadata.as_mut().unwrap()["received_characters"] = serde_json::json!(0);
        started.metadata.as_mut().unwrap()["request_revision"] = serde_json::json!(1);
        started.metadata.as_mut().unwrap()["preview"] = serde_json::Value::Null;
        let (_, started_snapshot) = tool_request_progress_change_from_event(&started)
            .expect("started metadata must satisfy the contract")
            .expect("name discovery must emit a draft");
        let started_snapshot = started_snapshot.expect("started must carry a snapshot");
        assert_eq!(started_snapshot.received_bytes, 0);
        assert_eq!(started_snapshot.preview, None);

        // assembled closes only the receiving presentation while the Provider response may remain open.
        // assembled 仅关闭接收展示状态，Provider 响应仍可保持打开。
        let assembled = RuntimeEvent::new(
            RuntimeEventKind::ToolRequestProgress,
            "tool request assembled",
            String::new(),
        )
        .with_metadata(Some(serde_json::json!({
            "api_call_id": 1,
            "request_stream_id": "request-1",
            "tool_call_id": "call-1",
            "tool_name": "terminal_session",
            "tool_display_state": "arguments_assembled",
            "received_bytes": 512,
            "received_characters": 500,
            "request_revision": 4,
        })));
        let (_, assembled_snapshot) = tool_request_progress_change_from_event(&assembled)
            .expect("assembled metadata should satisfy the closed contract")
            .expect("assembled event should produce one replacement");
        assert_eq!(
            assembled_snapshot
                .expect("assembled event should carry one snapshot")
                .state,
            ToolRequestProgressState::ArgumentsAssembled
        );

        // ready retains the final exact counts until the committed ToolStarted card releases the leaf Store.
        // ready 保留最终精确计数，直至已提交的 ToolStarted 卡片释放叶子 Store。
        let ready = RuntimeEvent::new(
            RuntimeEventKind::ToolRequestReady,
            "tool request ready",
            String::new(),
        )
        .with_metadata(Some(serde_json::json!({
            "api_call_id": 1,
            "request_stream_id": "request-1",
            "tool_call_id": "call-1",
            "tool_name": "terminal_session",
            "tool_display_state": "ready",
            "received_bytes": 512,
            "received_characters": 500,
            "request_revision": 5,
        })));
        let (_, ready_snapshot) = tool_request_progress_change_from_event(&ready)
            .expect("ready metadata should satisfy the snapshot contract")
            .expect("ready event should produce one replacement");
        assert_eq!(
            ready_snapshot
                .expect("ready event should retain one final snapshot")
                .state,
            ToolRequestProgressState::Ready,
        );
    }
    /// Verifies selected-session memory injections are projected through a dedicated workbench delta.
    /// 验证当前选中会话的记忆注入会通过专用工作台增量投影。
    #[test]
    fn context_injection_delta_projects_selected_session_memory_panel() {
        let session_id = SessionId::new_v4();
        let background_session_id = SessionId::new_v4();
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.context_injection_state.apply_insert(
            InjectionInsert::single(
                InjectionKind::Transient,
                "vmm_inject",
                "Dynamic memory",
                "动态记忆内容",
                Some(1),
            ),
            "turn-1",
        );
        runtime_state.context_injection_state.apply_insert(
            InjectionInsert::single(
                InjectionKind::Persistent,
                "profile",
                "Static profile",
                "持久画像内容",
                None,
            ),
            "turn-1",
        );
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };

        let delta = context_injection_changed_delta_from_workspace_snapshot(
            &workspace_snapshot,
            session_id,
            None,
        )
        .expect("selected session should produce context injection delta");

        assert_eq!(delta.conversation_id, session_id.to_string());
        let snapshot = delta
            .context_injection
            .expect("runtime state should produce a memory-panel snapshot");
        assert_eq!(snapshot.dynamic_cards.len(), 1);
        assert_eq!(snapshot.static_cards.len(), 1);
        assert!(
            snapshot.dynamic_cards[0].items[0]
                .content
                .contains("动态记忆内容")
        );
        assert!(
            snapshot.static_cards[0].items[0]
                .content
                .contains("持久画像内容")
        );
        assert!(
            context_injection_changed_delta_from_workspace_snapshot(
                &workspace_snapshot,
                background_session_id,
                None,
            )
            .is_none()
        );
    }

    /// Verifies selected-session deltas can clear the memory panel while retained lists omit empties.
    /// 验证选中会话增量可清空记忆面板，而保留列表会省略空条目。
    #[test]
    fn context_injection_delta_clears_selected_session_but_omits_empty_retained_entries() {
        // Selected session whose runtime state is absent after memory cleanup.
        // 记忆清理后运行态缺失的选中会话。
        let selected_session_id = SessionId::new_v4();
        // Hydrated background session with no runtime state.
        // 没有运行态的已水合后台会话。
        let hydrated_session_id = SessionId::new_v4();
        // Workspace snapshot used by both stream delta and retained projection paths.
        // 流式增量与保留投影路径共同使用的工作区快照。
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: selected_session_id,
            hydrated_session_ids: BTreeSet::from([hydrated_session_id]),
        };

        // Selected session still produces a delta with a None payload so the UI can clear it.
        // 选中会话仍会产生 None 载荷增量，以便 UI 清空面板。
        let selected_delta = context_injection_changed_delta_from_workspace_snapshot(
            &workspace_snapshot,
            selected_session_id,
            None,
        )
        .expect("selected session should emit a clearing delta");
        // Retained by-conversation projection omits empty entries.
        // 按会话保留的投影会省略空条目。
        let retained_deltas =
            context_injection_by_conversation_from_workspace(&workspace_snapshot, &BTreeMap::new());

        assert_eq!(
            selected_delta.conversation_id,
            selected_session_id.to_string()
        );
        assert!(selected_delta.context_injection.is_none());
        assert!(retained_deltas.is_empty());
    }

    /// Verifies terminal lifecycle synchronization runs only after supported terminal tools finish.
    /// 验证终端生命周期同步仅在受支持的终端工具完成后运行。
    #[test]
    fn runtime_event_command_session_refresh_matches_finished_terminal_tools() {
        // terminal_finished represents the current terminal tool contract after it has mutated runtime state.
        // terminal_finished 表示当前终端工具已完成运行时状态变更后的事件。
        let terminal_finished =
            RuntimeEvent::new(RuntimeEventKind::ToolFinished, "terminal finished", "")
                .with_metadata(Some(json!({ "tool_name": "terminal_session" })));
        // shell_finished preserves synchronization for the retained legacy shell-session tool.
        // shell_finished 为保留的旧版 shell-session 工具维持同步能力。
        let shell_finished =
            RuntimeEvent::new(RuntimeEventKind::ToolFinished, "shell finished", "")
                .with_metadata(Some(json!({ "tool_name": "shell_session" })));
        // terminal_started must not refresh before the start action has inserted its live runtime record.
        // terminal_started 不得在启动动作写入实时运行时记录前提前刷新。
        let terminal_started =
            RuntimeEvent::new(RuntimeEventKind::ToolStarted, "terminal started", "")
                .with_metadata(Some(json!({ "tool_name": "terminal_session" })));
        // unrelated_finished proves ordinary tools do not cause command-session workbench churn.
        // unrelated_finished 证明普通工具不会引发命令会话工作台抖动。
        let unrelated_finished =
            RuntimeEvent::new(RuntimeEventKind::ToolFinished, "file read finished", "")
                .with_metadata(Some(json!({ "tool_name": "vulcan_file_read" })));

        assert!(runtime_event_changes_command_sessions(&terminal_finished));
        assert!(runtime_event_changes_command_sessions(&shell_finished));
        assert!(!runtime_event_changes_command_sessions(&terminal_started));
        assert!(!runtime_event_changes_command_sessions(&unrelated_finished));
    }

    /// Verifies subagent navigation synchronization runs only after a spawn tool finishes.
    /// 验证子代理导航同步仅在创建工具完成后运行。
    #[test]
    fn runtime_event_subagent_navigation_refresh_matches_finished_lifecycle_tools() {
        // spawn_finished represents a child task already registered by the orchestration runtime.
        // spawn_finished 表示编排运行时已经注册完成的子任务。
        let spawn_finished =
            RuntimeEvent::new(RuntimeEventKind::ToolFinished, "spawn finished", "")
                .with_metadata(Some(json!({ "tool_name": "spawn_agent" })));
        // spawn_started must not project navigation before the child task registration commits.
        // spawn_started 不得在子任务注册提交前投影导航。
        let spawn_started = RuntimeEvent::new(RuntimeEventKind::ToolStarted, "spawn started", "")
            .with_metadata(Some(json!({ "tool_name": "spawn_agent" })));
        // close_finished must publish the rebuilt parent tree even when cache cleanup already happened.
        // close_finished 即使发生在缓存清理之后，也必须发布重建后的父级导航树。
        let close_finished =
            RuntimeEvent::new(RuntimeEventKind::ToolFinished, "close finished", "")
                .with_metadata(Some(json!({ "tool_name": "close_agent" })));
        // send_finished refreshes the pending queue even while the child keeps running.
        // send_finished 在子代理继续运行时也刷新待处理消息队列。
        let send_finished = RuntimeEvent::new(RuntimeEventKind::ToolFinished, "send finished", "")
            .with_metadata(Some(json!({ "tool_name": "send_message" })));

        assert!(runtime_event_changes_subagent_navigation(&spawn_finished));
        assert!(runtime_event_changes_subagent_navigation(&close_finished));
        assert!(!runtime_event_changes_subagent_navigation(&spawn_started));
        assert!(runtime_event_changes_subagent_navigation(&send_finished));
    }

    /// Verifies post-response tool results advance the active context beyond the latest Provider total.
    /// 验证响应后的工具结果会在最近一次 Provider 总量之上推进活动上下文。
    #[test]
    fn local_context_tokens_after_latest_assistant_counts_tool_results_only() {
        let state = vulcan_context::SessionConversationState {
            messages: vec![
                vulcan_context::SessionConversationMessage {
                    id: String::from("user-1"),
                    payload: vulcan_context::SessionConversationPayload::UserText {
                        text: String::from("run the tool"),
                        attachments: Vec::new(),
                    },
                },
                vulcan_context::SessionConversationMessage {
                    id: String::from("assistant-1"),
                    payload: vulcan_context::SessionConversationPayload::Assistant {
                        text: String::from("I will call it"),
                        reasoning: String::new(),
                        provider_message_id: None,
                        provider_message_state: None,
                        usage: None,
                        tool_calls: Vec::new(),
                    },
                },
                vulcan_context::SessionConversationMessage {
                    id: String::from("tool-1"),
                    payload: vulcan_context::SessionConversationPayload::ToolResult {
                        tool_call_id: Some(String::from("call-1")),
                        is_error: false,
                        text: String::from("tool result body"),
                    },
                },
            ],
            ..vulcan_context::SessionConversationState::default()
        };

        assert!(
            local_context_tokens_after_latest_assistant(&state)
                .expect("valid compaction source messages")
                > 0
        );
    }
}
