//! Streaming transcript and runtime event projection helpers.
//! 流式转录与运行时事件投影辅助函数。

use super::*;
use vulcan_service_api::ConversationMessagesPatchedDelta;

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

    /// Returns the stable append failure context for this content kind.
    /// 返回当前内容类型的稳定追加失败上下文。
    ///
    /// # Parameters
    /// # 参数
    /// This method has no parameters beyond the content kind.
    /// 此方法除内容类型外没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// The existing diagnostic prefix associated with the content kind.
    /// 返回与当前内容类型关联的既有诊断前缀。
    pub(crate) fn append_failure_context(self) -> &'static str {
        match self {
            Self::Thinking => "append thinking delta failed",
            Self::Output => "append output delta failed",
        }
    }

    /// Selects the streaming record identifier slot owned by this content kind.
    /// 选择当前内容类型拥有的流式记录标识槽位。
    ///
    /// # Parameters
    /// # 参数
    /// `record_ids` owns independent thinking and output record identifiers.
    /// `record_ids` 持有相互独立的思考与输出记录标识。
    ///
    /// # Returns
    /// # 返回值
    /// A mutable reference to the exact identifier slot for this content kind.
    /// 返回当前内容类型精确标识槽位的可变引用。
    fn record_id_slot(self, record_ids: &mut SessionStreamingRecordIds) -> &mut Option<String> {
        match self {
            Self::Thinking => &mut record_ids.thinking_record_id,
            Self::Output => &mut record_ids.output_record_id,
        }
    }

    /// Returns the transcript record kind, title, and default collapse state for this content kind.
    /// 返回当前内容类型的转录记录类型、标题与默认折叠状态。
    ///
    /// # Parameters
    /// # 参数
    /// This method has no parameters beyond the content kind.
    /// 此方法除内容类型外没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// The exact projection tuple consumed by the low-level record appender.
    /// 返回底层记录追加器消费的精确投影元组。
    fn projection_spec(self) -> (SessionMessageRecordKind, &'static str, bool) {
        match self {
            Self::Thinking => (SessionMessageRecordKind::ThinkingOutput, "思考", true),
            Self::Output => (SessionMessageRecordKind::AssistantOutput, "输出", false),
        }
    }
}

/// Appends one typed provider content delta to its exact streaming transcript record.
/// 将单个强类型 Provider 内容增量追加到其精确流式转录记录。
///
/// # Parameters
/// # 参数
/// `app` emits the lightweight workbench message delta.
/// `app` 发送轻量工作台消息增量。
/// `state` owns retained workspace and runtime transcript state.
/// `state` 持有保留工作区与运行时转录状态。
/// `session_id` identifies the session receiving the content delta.
/// `session_id` 标识接收内容增量的会话。
/// `record_ids` owns the current streaming record identifiers.
/// `record_ids` 持有当前流式记录标识。
/// `kind` selects the exact record semantics for the delta.
/// `kind` 选择当前增量的精确记录语义。
/// `delta` is the provider content fragment to append.
/// `delta` 是需要追加的 Provider 内容片段。
///
/// # Returns
/// # 返回值
/// Success after retained state and the emitted delta are aligned, or an explicit projection error.
/// 保留状态与发送增量对齐后返回成功，否则返回显式投影错误。
pub(crate) fn append_stream_content_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    record_ids: &mut SessionStreamingRecordIds,
    kind: StreamContentKind,
    delta: &str,
) -> Result<(), String> {
    // The immutable projection specification is captured before borrowing its mutable record slot.
    // 不可变投影规格在借用可变记录槽位前完成捕获。
    let (record_kind, title, collapsed_by_default) = kind.projection_spec();
    append_stream_delta_record(
        app,
        state,
        session_id,
        kind.record_id_slot(record_ids),
        record_kind,
        title,
        delta,
        collapsed_by_default,
    )
}

/// Permanently closes every active content record at one provider boundary.
/// 在单个 Provider 边界永久封口所有活跃内容记录。
///
/// # Parameters
/// # 参数
/// `app` emits one authoritative terminal body event for every closed record.
/// `app` 为每条封口记录发送一条权威终态正文事件。
/// `state` owns retained runtime and Workbench projections.
/// `state` 持有保留运行时与 Workbench 投影。
/// `session_id` identifies the session that owns the active records.
/// `session_id` 标识拥有活跃记录的会话。
/// `record_ids` owns the request-local thinking and output identities.
/// `record_ids` 持有请求局部的思考与输出身份。
///
/// # Returns
/// # 返回值
/// Success after both identities are empty, or the first explicit sealing error.
/// 两个身份均清空后返回成功，否则返回首个显式封口错误。
pub(crate) fn seal_stream_content_records(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    record_ids: &mut SessionStreamingRecordIds,
) -> Result<(), String> {
    seal_stream_content_kind(
        app,
        state,
        session_id,
        record_ids,
        StreamContentKind::Thinking,
    )?;
    seal_stream_content_kind(
        app,
        state,
        session_id,
        record_ids,
        StreamContentKind::Output,
    )
}

/// Permanently closes the active record for one exact provider content kind.
/// 永久封口一个精确 Provider 内容类型对应的活跃记录。
///
/// # Parameters
/// # 参数
/// `app` emits the terminal body event.
/// `app` 发送终态正文事件。
/// `state` owns retained runtime and Workbench projections.
/// `state` 持有保留运行时与 Workbench 投影。
/// `session_id` identifies the owning session.
/// `session_id` 标识所属会话。
/// `record_ids` owns the request-local record identities.
/// `record_ids` 持有请求局部记录身份。
/// `kind` selects the exact record identity slot.
/// `kind` 选择精确记录身份槽位。
///
/// # Returns
/// # 返回值
/// Success after the selected slot is empty and its record is sealed, or an explicit error.
/// 所选槽位清空且记录完成封口后返回成功，否则返回显式错误。
pub(crate) fn seal_stream_content_kind(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    record_ids: &mut SessionStreamingRecordIds,
    kind: StreamContentKind,
) -> Result<(), String> {
    // record_id is taken before sealing so a successful close can never receive later deltas.
    // record_id 在封口前从槽位取出，确保成功关闭后绝不再接收后续增量。
    let record_id_slot = kind.record_id_slot(record_ids);
    let Some(record_id) = record_id_slot.take() else {
        return Ok(());
    };
    if let Err(error) = seal_stream_delta_record(app, state, session_id, &record_id) {
        // A failed state mutation restores the exact identity so a later terminal path can retry sealing.
        // 状态变更失败时恢复精确身份，使后续终止路径仍可重试封口。
        *record_id_slot = Some(record_id);
        return Err(error);
    }
    Ok(())
}

/// Appends or extends one streaming transcript record.
/// 追加或扩展单条流式转录记录。
fn append_stream_delta_record(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    record_id_slot: &mut Option<String>,
    record_kind: SessionMessageRecordKind,
    title: &str,
    delta: &str,
    collapsed_by_default: bool,
) -> Result<(), String> {
    if delta.is_empty() {
        return Ok(());
    }
    let conversation_id = session_id.to_string();
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        if workspace.session_run_phases.get(&session_id) == Some(&SessionRunPhase::Stopped) {
            return Err(format!(
                "cannot append streaming content to stopped session {session_id}"
            ));
        }
        let now = current_unix_millis_text();
        let run_phase = match record_kind {
            SessionMessageRecordKind::ThinkingOutput => SessionRunPhase::Thinking,
            _ => SessionRunPhase::Requesting,
        };
        let (projected_entry, is_new_message) = {
            let mut runtime_state = workspace
                .session_runtime_state
                .get(&session_id)
                .cloned()
                .unwrap_or_else(|| SessionRuntimeStateEntry {
                    updated_at: now.clone(),
                    ..SessionRuntimeStateEntry::default()
                });
            // previous_record_id distinguishes an append from recovery that allocates a replacement row.
            // previous_record_id 区分普通追加与分配替代行的恢复路径。
            let previous_record_id = record_id_slot.clone();
            let mut record_id_draft = previous_record_id.clone();
            append_stream_delta_record_to_state(
                &mut runtime_state,
                session_id,
                &now,
                &mut record_id_draft,
                record_kind,
                title,
                delta,
                collapsed_by_default,
            )?;
            let Some(message_id) = record_id_draft.as_deref() else {
                return Err(String::from("streaming record id was not materialized"));
            };
            let projected_entry = runtime_state
                .conversation_state
                .records
                .iter()
                .find(|record| record.id == message_id)
                .map(session_message_entry_from_conversation_record);
            runtime_state.updated_at = now.clone();
            workspace
                .session_runtime_state
                .insert(session_id, runtime_state);
            let is_new_message = previous_record_id.as_deref() != record_id_draft.as_deref();
            *record_id_slot = record_id_draft;
            (projected_entry, is_new_message)
        };
        workspace.session_run_phases.insert(session_id, run_phase);
        sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
        let Some(projected_entry) = projected_entry else {
            return Ok(());
        };
        let projected_message =
            conversation_message_from_session_entry(&conversation_id, &projected_entry);
        let initial_message = is_new_message.then(|| projected_message.clone());
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        if is_new_message {
            if let Some(existing) = guard
                .workbench
                .messages
                .iter_mut()
                .find(|message| message.id == projected_message.id)
            {
                *existing = projected_message.clone();
            } else if workbench_surface_matches_conversation(&guard.workbench, &conversation_id) {
                guard.workbench.messages.push(projected_message.clone());
            }
        }
        for conversation in &mut guard.workbench.conversations {
            if conversation.id == conversation_id {
                conversation.run_phase = run_phase.to_string();
                conversation.status = SessionState::Running.label().to_string();
                conversation.last_updated_label = String::from("刚刚");
                conversation.last_updated_at = now.clone();
                break;
            }
        }
        WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::AppendMessageBody(AppendMessageBodyDelta {
                conversation_id,
                message_id: projected_entry.id,
                body_delta: delta.to_string(),
                current_body: None,
                initial_message,
                run_phase: Some(run_phase.to_string()),
                stream_state: Some(ConversationMessageStreamState::Active),
            }),
        }
    };
    emit_service_event_with_trace(
        app,
        "workbench_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(())
}

/// Seals one exact streamed record and emits its authoritative final body once.
/// 封口一条精确流式记录，并仅发送一次其权威最终正文。
///
/// # Parameters
/// # 参数
/// `app` emits the sealed message delta.
/// `app` 发送已封口消息增量。
/// `state` owns runtime records and retained Workbench state.
/// `state` 持有运行时记录与保留 Workbench 状态。
/// `session_id` identifies the owning conversation.
/// `session_id` 标识所属会话。
/// `record_id` identifies the exact runtime record being sealed.
/// `record_id` 标识正在封口的精确运行时记录。
///
/// # Returns
/// # 返回值
/// Success after the final event is emitted, or an explicit state/projection error.
/// 最终事件发送后返回成功，否则返回显式状态或投影错误。
fn seal_stream_delta_record(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    record_id: &str,
) -> Result<(), String> {
    // conversation_id is the stable WebUI owner identity for the sealed message.
    // conversation_id 是已封口消息稳定的 WebUI 所属身份。
    let conversation_id = session_id.to_string();
    // event is created under the same state lock as the authoritative lifecycle mutation.
    // event 与权威生命周期变更在同一个状态锁内创建。
    let event = {
        // guard serializes runtime sealing, retained projection, and generation allocation.
        // guard 串行化运行时封口、保留投影与 generation 分配。
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        // now timestamps the exact sealed runtime record revision.
        // now 为精确的已封口运行时记录版本提供时间戳。
        let now = current_unix_millis_text();
        // sealed_record is cloned from the authoritative runtime record only after metadata is sealed.
        // sealed_record 仅在权威运行时记录元数据完成封口后克隆。
        let sealed_record = {
            // workspace owns the exact session runtime record collection.
            // workspace 持有精确的会话运行时记录集合。
            let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
                String::from("service is still starting; workspace snapshot is not ready")
            })?;
            // runtime_state is the exact session entry referenced by the request-local record id.
            // runtime_state 是请求局部记录标识指向的精确会话条目。
            let Some(runtime_state) = workspace.session_runtime_state.get_mut(&session_id) else {
                return Ok(());
            };
            seal_streaming_record_in_state(runtime_state, &now, record_id)?
        };
        let Some(sealed_record) = sealed_record else {
            return Ok(());
        };
        // projected_entry reuses the canonical runtime-to-session projection contract.
        // projected_entry 复用规范的运行时到会话投影契约。
        let projected_entry = session_message_entry_from_conversation_record(&sealed_record);
        // projected_message is the authoritative final WebUI row carried by retained state.
        // projected_message 是保留状态携带的权威最终 WebUI 行。
        let projected_message =
            conversation_message_from_session_entry(&conversation_id, &projected_entry);
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        // generation orders sealing after every body delta already emitted for this record.
        // generation 将封口事件排序在该记录已发送的所有正文增量之后。
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        if let Some(existing) = guard
            .workbench
            .messages
            .iter_mut()
            .find(|message| message.id == projected_message.id)
        {
            *existing = projected_message.clone();
        } else if workbench_surface_matches_conversation(&guard.workbench, &conversation_id) {
            guard.workbench.messages.push(projected_message.clone());
        }
        WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::AppendMessageBody(AppendMessageBodyDelta {
                conversation_id,
                message_id: projected_message.id,
                body_delta: String::new(),
                current_body: Some(projected_message.body),
                initial_message: None,
                run_phase: None,
                stream_state: Some(ConversationMessageStreamState::Sealed),
            }),
        }
    };
    emit_service_event_with_trace(
        app,
        "workbench_stream_message_sealed_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(())
}

#[cfg(test)]
mod stream_content_tests {
    use super::*;

    /// Verifies that typed content kinds select independent record slots and exact projection specs.
    /// 验证强类型内容类型会选择独立记录槽位与精确投影规格。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails through assertions.
    /// 此测试无返回值，并通过断言报告失败。
    #[test]
    fn content_kinds_select_exact_record_semantics() {
        // Independent identifiers prove that selecting one kind cannot overwrite the other slot.
        // 独立标识证明选择一种类型不会覆盖另一种类型的槽位。
        let mut record_ids = SessionStreamingRecordIds::default();
        *StreamContentKind::Thinking.record_id_slot(&mut record_ids) = Some(String::from("think"));
        *StreamContentKind::Output.record_id_slot(&mut record_ids) = Some(String::from("output"));

        assert_eq!(record_ids.thinking_record_id.as_deref(), Some("think"));
        assert_eq!(record_ids.output_record_id.as_deref(), Some("output"));
        assert!(matches!(
            StreamContentKind::Thinking.projection_spec(),
            (SessionMessageRecordKind::ThinkingOutput, "思考", true)
        ));
        assert!(matches!(
            StreamContentKind::Output.projection_spec(),
            (SessionMessageRecordKind::AssistantOutput, "输出", false)
        ));
    }

    /// Verifies that taking one selected content identity cannot clear the other slot.
    /// 验证取出一个选定内容身份不会清除另一个槽位。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails through assertions.
    /// 此测试无返回值，并通过断言报告失败。
    #[test]
    fn content_kind_take_clears_only_selected_record_id() {
        // Populated identifiers model independent thinking and output streams.
        // 已填充标识模拟相互独立的思考流与输出流。
        let mut record_ids = SessionStreamingRecordIds {
            thinking_record_id: Some(String::from("think")),
            output_record_id: Some(String::from("output")),
        };

        // selected_id is removed from the thinking slot before its authoritative record is sealed.
        // selected_id 在对应权威记录封口前从思考槽位中移除。
        let selected_id = StreamContentKind::Thinking
            .record_id_slot(&mut record_ids)
            .take();

        assert_eq!(selected_id.as_deref(), Some("think"));
        assert!(record_ids.thinking_record_id.is_none());
        assert_eq!(record_ids.output_record_id.as_deref(), Some("output"));
    }
}

/// Emits one already-materialized visible session message through the lightweight workbench delta channel.
/// 通过轻量工作台增量通道发送一条已经物化的可见会话消息。
///
/// # Parameters
/// 参数
/// `app` emits the delta event to connected WebUI clients.
/// `app` 将增量事件发送给已连接的 WebUI 客户端。
/// `state` owns the retained workbench projection that must stay aligned with emitted deltas.
/// `state` 持有需要与增量事件保持一致的保留工作台投影。
/// `session_id` selects the conversation that owns the message.
/// `session_id` 选择拥有该消息的会话。
/// `message` is the already-projected session transcript entry to expose immediately.
/// `message` 是需要立即展示的已投影会话转录项。
/// `run_phase` is the current session run phase after the message was accepted.
/// `run_phase` 是消息被接受后的当前会话运行阶段。
///
/// # Returns
/// 返回值
/// `Ok(())` when the delta was emitted or skipped because the target surface is not visible.
/// 当增量已发送，或因目标界面不可见而跳过时返回 `Ok(())`。
pub(crate) fn emit_materialized_session_message_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    message: &SessionMessageEntry,
    run_phase: SessionRunPhase,
) -> Result<(), String> {
    if !message.is_visible_to_user() {
        return Ok(());
    }
    let conversation_id = session_id.to_string();
    let projected_message = conversation_message_from_session_entry(&conversation_id, message);
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if !workbench_surface_matches_conversation(&guard.workbench, &conversation_id) {
            return Ok(());
        }
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        if let Some(existing) = guard
            .workbench
            .messages
            .iter_mut()
            .find(|candidate| candidate.id == projected_message.id)
        {
            *existing = projected_message.clone();
        } else {
            guard.workbench.messages.push(projected_message.clone());
        }
        for conversation in &mut guard.workbench.conversations {
            if conversation.id == conversation_id {
                conversation.run_phase = run_phase.to_string();
                conversation.status = session_state_for_run_phase(run_phase).label().to_string();
                conversation.last_updated_label = String::from("刚刚");
                conversation.last_updated_at = message.created_at.clone();
                break;
            }
        }
        WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::AppendMessageBody(AppendMessageBodyDelta {
                conversation_id,
                message_id: projected_message.id.clone(),
                body_delta: String::new(),
                current_body: Some(projected_message.body.clone()),
                initial_message: Some(projected_message),
                run_phase: Some(run_phase.to_string()),
                stream_state: None,
            }),
        }
    };
    webui_submit_trace(
        "submit_prompt_user_message_delta_emitted",
        format!("session={session_id} message_id={}", message.id),
    );
    emit_service_event_with_trace(
        app,
        "workbench_user_message_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(())
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
    let conversation_id = session_id.to_string();
    let session_state = session_state_for_run_phase(run_phase);
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        guard.workspace_snapshot = Some(workspace_snapshot);
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
        "workbench_phase_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(())
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
    // vmm_runtime_context is cloned without retaining the shared service lock.
    // vmm_runtime_context 在不保留共享服务锁的情况下完成克隆。
    let vmm_runtime_context = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard
            .controller_runtime
            .clone()
            .map(|controller_runtime| (controller_runtime, guard.vmm_controller_registry.clone()))
    };
    // vmm_session_memory is a read-only controller projection for this exact delta.
    // vmm_session_memory 是用于本次精确增量的只读控制器投影。
    let vmm_session_memory = if let Some((controller_runtime, registry)) = vmm_runtime_context {
        Some(
            registry
                .get_or_open(
                    &controller_runtime.data_root,
                    session_id,
                    &current_unix_millis_text(),
                )?
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

/// Converts one canonical conversation record into the transcript entry projection.
/// 将单条规范会话记录转换为转录项投影。
pub(crate) fn session_message_entry_from_conversation_record(
    record: &SessionConversationRecord,
) -> SessionMessageEntry {
    SessionMessageEntry {
        id: record.id.clone(),
        kind: record.kind,
        record_kind: record.record_kind,
        visible: record.visible,
        title: record.title.clone(),
        body: record.body.clone(),
        metadata: record.metadata.clone(),
        created_at: record.created_at.clone(),
    }
}

/// Converts one session transcript entry into a WebUI conversation message.
/// 将单条会话转录项转换为 WebUI 会话消息。
pub(crate) fn conversation_message_from_session_entry(
    conversation_id: &str,
    message: &SessionMessageEntry,
) -> ConversationMessage {
    ConversationMessage {
        id: if message.id.is_empty() {
            format!("{}-{}", conversation_id, message.created_at)
        } else {
            message.id.clone()
        },
        conversation_id: conversation_id.to_string(),
        created_at: message.created_at.clone(),
        role: match message.kind {
            SessionMessageKind::User => MessageRole::User,
            SessionMessageKind::Assistant => MessageRole::Assistant,
            SessionMessageKind::Error => MessageRole::System,
        },
        title: non_empty_string(&message.title),
        record_kind: Some(message.record_kind.as_persisted_key().to_string()),
        body: message.body.clone(),
        metadata: message.metadata.clone(),
        attachments: composer_message_attachments_from_metadata(message.metadata.as_ref()),
        editable_user_prompt: false,
    }
}

/// Builds a node-level message patch by comparing visible messages before and after projection.
/// 通过比较投影前后的可见消息构建节点级消息补丁。
pub(crate) fn build_conversation_messages_patched_delta(
    conversation_id: &str,
    before_messages: &[ConversationMessage],
    after_messages: &[ConversationMessage],
) -> Option<ConversationMessagesPatchedDelta> {
    let mut remove_message_ids = Vec::new();
    for before_message in before_messages {
        if !after_messages
            .iter()
            .any(|after_message| after_message.id == before_message.id)
        {
            remove_message_ids.push(before_message.id.clone());
        }
    }

    let mut upsert_messages = Vec::new();
    for after_message in after_messages {
        match before_messages
            .iter()
            .find(|before_message| before_message.id == after_message.id)
        {
            Some(before_message) if before_message == after_message => {}
            _ => upsert_messages.push(after_message.clone()),
        }
    }

    let before_order: Vec<&str> = before_messages
        .iter()
        .map(|message| message.id.as_str())
        .collect();
    let after_order: Vec<&str> = after_messages
        .iter()
        .map(|message| message.id.as_str())
        .collect();
    let message_order = if before_order == after_order {
        None
    } else {
        Some(
            after_messages
                .iter()
                .map(|message| message.id.clone())
                .collect(),
        )
    };

    if upsert_messages.is_empty() && remove_message_ids.is_empty() && message_order.is_none() {
        return None;
    }

    Some(ConversationMessagesPatchedDelta {
        conversation_id: conversation_id.to_string(),
        upsert_messages,
        remove_message_ids,
        message_order,
        run_phase: None,
    })
}

/// Builds a runtime-event message patch with the matching UI phase applied atomically.
/// 构建运行时事件消息补丁，并将匹配的界面阶段原子写入同一补丁。
fn build_runtime_event_conversation_messages_patched_delta(
    conversation_id: &str,
    before_messages: &[ConversationMessage],
    after_messages: &[ConversationMessage],
    event: &RuntimeEvent,
) -> Option<ConversationMessagesPatchedDelta> {
    let mut message_patch = build_conversation_messages_patched_delta(
        conversation_id,
        before_messages,
        after_messages,
    )?;
    message_patch.run_phase =
        runtime_event_projection_run_phase(event).map(|phase| phase.to_string());
    Some(message_patch)
}

/// Keeps visible streaming rows ahead of a newly projected runtime tool event.
/// 在新投影的运行时工具事件之前保留可见的流式记录。
///
/// `before_messages` is the exact visible order before the runtime event, while `projected_messages`
/// contains stable retained rows plus the newly projected tool row. The result replaces matching stable
/// rows in place, retains only transient thinking/assistant rows omitted from retained storage, and appends
/// genuinely new runtime rows after everything that was already visible.
///
/// `before_messages` 是运行时事件前的精确可见顺序，`projected_messages` 包含稳定保留记录与新投影工具记录。
/// 返回结果会原位替换匹配的稳定记录，仅保留未进入保留存储的临时思考/Assistant 记录，并将真正新增的运行时记录
/// 追加到此前已经可见的全部内容之后。
fn merge_visible_streaming_rows_with_runtime_projection(
    before_messages: &[ConversationMessage],
    projected_messages: &[ConversationMessage],
) -> Vec<ConversationMessage> {
    // projected_by_id provides authoritative stable replacements without changing prior visible positions.
    // projected_by_id 提供权威稳定替换项，同时不改变此前的可见位置。
    let projected_by_id = projected_messages
        .iter()
        .map(|message| (message.id.as_str(), message))
        .collect::<BTreeMap<_, _>>();
    // emitted_ids prevents stable rows already replaced in place from being appended again.
    // emitted_ids 防止已经原位替换的稳定记录被再次追加。
    let mut emitted_ids = BTreeSet::new();
    // merged_messages stores the exact visible order for the runtime-event patch.
    // merged_messages 保存运行时事件补丁使用的精确可见顺序。
    let mut merged_messages = Vec::with_capacity(
        before_messages
            .len()
            .saturating_add(projected_messages.len()),
    );
    for before_message in before_messages {
        if let Some(projected_message) = projected_by_id.get(before_message.id.as_str()) {
            merged_messages.push((*projected_message).clone());
            emitted_ids.insert(before_message.id.as_str());
            continue;
        }
        if runtime_projection_retains_visible_streaming_row(before_message) {
            merged_messages.push(before_message.clone());
            emitted_ids.insert(before_message.id.as_str());
        }
    }
    for projected_message in projected_messages {
        if emitted_ids.insert(projected_message.id.as_str()) {
            merged_messages.push(projected_message.clone());
        }
    }
    merged_messages
}

/// Returns whether one visible row is a transient stream that stable tool projection intentionally omits.
/// 返回单条可见记录是否为稳定工具投影有意省略的临时流记录。
fn runtime_projection_retains_visible_streaming_row(message: &ConversationMessage) -> bool {
    matches!(
        message.record_kind.as_deref(),
        Some(kind)
            if kind == SessionMessageRecordKind::ThinkingOutput.as_persisted_key()
                || kind == SessionMessageRecordKind::AssistantOutput.as_persisted_key()
    )
}

/// Replaces one conversation's visible message block while preserving unrelated message positions.
/// 替换单个会话的可见消息块，同时保留无关消息的位置。
pub(crate) fn replace_conversation_messages_in_workbench_window(
    messages: &mut Vec<ConversationMessage>,
    conversation_id: &str,
    next_conversation_messages: Vec<ConversationMessage>,
) {
    let previous_messages = std::mem::take(messages);
    let mut pending_conversation_messages = Some(next_conversation_messages);
    let mut inserted_conversation_messages = false;
    for message in previous_messages {
        if message.conversation_id != conversation_id {
            messages.push(message);
            continue;
        }
        if !inserted_conversation_messages {
            if let Some(next_messages) = pending_conversation_messages.take() {
                messages.extend(next_messages);
            }
            inserted_conversation_messages = true;
        }
    }
    if !inserted_conversation_messages
        && let Some(next_messages) = pending_conversation_messages.take()
    {
        messages.extend(next_messages);
    }
}

/// Completes one visible thinking phase by collapsing its active row and restoring the requesting phase.
/// 通过折叠当前活动思考行并恢复请求阶段，完成一个可见思考阶段。
///
/// # Parameters
/// # 参数
/// `app` emits the requesting-phase workbench delta after retained thinking state is sealed.
/// `app` 在保留思考状态封口后发送请求阶段工作台增量。
/// `state` owns the retained runtime record UI state and session phase.
/// `state` 持有保留运行时记录界面状态与会话阶段。
/// `session_id` identifies the session whose thinking phase completed.
/// `session_id` 标识思考阶段已经完成的会话。
/// `record_ids` owns the active thinking record identifier selected by prior thinking deltas.
/// `record_ids` 持有先前思考增量选定的活动思考记录标识。
///
/// # Returns
/// # 返回值
/// Success after both retained collapse state and requesting phase are applied, or an explicit step-specific error.
/// 在保留折叠状态与请求阶段均应用后返回成功，否则返回带明确步骤的错误。
pub(crate) fn complete_streaming_thinking_phase(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    record_ids: &mut SessionStreamingRecordIds,
) -> Result<(), String> {
    // This scope releases the service-state mutex before apply_session_phase reacquires it, preventing same-thread self-deadlock.
    // 此作用域会在 apply_session_phase 再次获取服务状态互斥锁前释放锁，防止同线程自锁。
    {
        // guard serializes the retained collapse mutation before the phase delta becomes visible.
        // guard 在阶段增量可见前串行化保留折叠状态变更。
        let mut guard = state.lock().map_err(|error| {
            format!("failed to collapse completed thinking: failed to lock service state: {error}")
        })?;
        // workspace is the authoritative owner of streamed runtime record UI state.
        // workspace 是流式运行时记录界面状态的权威所有者。
        let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
            String::from(
                "failed to collapse completed thinking: service is still starting; workspace snapshot is not ready",
            )
        })?;
        // now timestamps the exact retained-state mutation performed by this completion event.
        // now 为当前完成事件执行的精确保留状态变更提供时间戳。
        let now = current_unix_millis_text();
        // runtime_state is the exact session entry that owns the active thinking row UI state.
        // runtime_state 是持有活动思考行界面状态的精确会话条目。
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: now.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        collapse_streaming_thinking_in_state(runtime_state, &now, record_ids);
    }

    // Thinking is sealed before the requesting phase becomes visible, preserving lifecycle order for every client.
    // 思考记录在请求阶段可见前完成封口，从而为所有客户端保持生命周期顺序。
    seal_stream_content_kind(
        app,
        state,
        session_id,
        record_ids,
        StreamContentKind::Thinking,
    )?;

    // Requesting becomes visible only after the retained thinking row is sealed, preserving event order for every consumer.
    // 仅在保留思考行完成封口后才显示请求阶段，从而为所有消费者保持事件顺序。
    apply_session_phase(app, state, session_id, SessionRunPhase::Requesting)
        .map_err(|error| format!("failed to restore request phase after thinking: {error}"))
}

/// Detached workspace fields changed by one runtime tool event.
/// 单个运行时工具事件修改的分离工作区字段。
struct RuntimeEventWorkspaceDraft {
    /// Parent runtime state after record, todo-list, and plan projection.
    /// 完成记录、待办列表与计划投影后的父会话运行态。
    runtime_state: SessionRuntimeStateEntry,
    /// Replacement parent transcript when stable runtime records changed.
    /// 稳定运行时记录变化时使用的父会话替换转录。
    parent_messages: Option<Vec<SessionMessageEntry>>,
    /// Provider-visible parent projection derived from the replacement transcript.
    /// 从替换转录派生的 Provider 可见父会话投影。
    projected_parent_messages: Vec<ConversationMessage>,
    /// Optional child transcript append and its projected surface rows.
    /// 可选的子会话转录追加及其表面投影行。
    child_projection: Option<RuntimeEventChildTranscriptDraft>,
    /// Todo-list state changed by this event.
    /// 本事件修改后的待办列表状态。
    changed_todolist: Option<SessionTodolistState>,
    /// Plan-mode state changed by this event.
    /// 本事件修改后的计划模式状态。
    changed_plan_mode: Option<SessionPlanModeState>,
}

/// Detached child transcript projection owned by one send-message event.
/// 单个 send-message 事件持有的分离子会话转录投影。
struct RuntimeEventChildTranscriptDraft {
    /// Child task session identity.
    /// 子任务会话身份。
    task_id: SessionId,
    /// Complete child display transcript after append.
    /// 追加后的完整子会话显示转录。
    messages: Vec<SessionMessageEntry>,
    /// Projected child workbench messages.
    /// 投影后的子会话工作台消息。
    projected_messages: Vec<ConversationMessage>,
    /// Whether the child hydrated marker must remain set.
    /// 子会话水合标记是否必须保持置位。
    hydrated: bool,
}

impl RuntimeEventWorkspaceDraft {
    /// Commits only the parent/child session fields owned by this runtime event.
    /// 仅提交本运行时事件拥有的父子会话字段。
    ///
    /// # Parameters
    /// 参数
    /// `workspace` is the authoritative workspace receiving the validated patch.
    /// `workspace` 是接收已验证补丁的权威工作区。
    /// `session_id` identifies the parent session owning the runtime projection.
    /// `session_id` 标识持有运行时投影的父会话。
    fn commit(&self, workspace: &mut WorkspaceSnapshot, session_id: SessionId) {
        workspace
            .session_runtime_state
            .insert(session_id, self.runtime_state.clone());
        if let Some(messages) = self.parent_messages.as_ref() {
            workspace
                .session_messages
                .insert(session_id, messages.clone());
            set_workspace_session_hydrated_marker(
                workspace,
                session_id,
                runtime_state_has_concrete_conversation_context(&self.runtime_state),
            );
        }
        if let Some(child) = self.child_projection.as_ref() {
            workspace
                .session_messages
                .insert(child.task_id, child.messages.clone());
            set_workspace_session_hydrated_marker(workspace, child.task_id, child.hydrated);
        }
    }
}

/// Sets one workspace hydrated marker from an already-proven dialogue readiness value.
/// 根据已经证明的对话就绪值设置单个工作区水合标记。
fn set_workspace_session_hydrated_marker(
    workspace: &mut WorkspaceSnapshot,
    session_id: SessionId,
    hydrated: bool,
) {
    if hydrated {
        workspace.hydrated_session_ids.insert(session_id);
    } else {
        workspace.hydrated_session_ids.remove(&session_id);
    }
}

/// Builds a session-scoped workspace patch for one preflighted runtime event.
/// 为一个已预检运行时事件构建会话级工作区补丁。
///
/// # Parameters
/// 参数
/// `workspace` is the immutable authoritative workspace used as the draft baseline.
/// `workspace` 是作为草稿基线的不可变权威工作区。
/// `session_id` identifies the parent runtime session.
/// `session_id` 标识父运行时会话。
/// `conversation_id` is the parent workbench conversation identity.
/// `conversation_id` 是父工作台会话身份。
/// `event` is the preflighted runtime tool event.
/// `event` 是已预检的运行时工具事件。
/// `child_message` is the already-parsed optional child display row.
/// `child_message` 是已经解析的可选子会话显示行。
///
/// # Returns
/// 返回值
/// Returns the complete session-scoped patch, or an identity error without modifying workspace state.
/// 返回完整会话级补丁；身份错误时返回错误且不修改工作区状态。
fn build_runtime_event_workspace_draft(
    workspace: &WorkspaceSnapshot,
    session_id: SessionId,
    conversation_id: &str,
    event: &RuntimeEvent,
    child_message: Option<(SessionId, SessionMessageEntry)>,
) -> Result<RuntimeEventWorkspaceDraft, String> {
    let now = current_unix_millis_text();
    // runtime_state is detached from the parent session until every projection succeeds.
    // runtime_state 与父会话分离，直到全部投影成功。
    let mut runtime_state = workspace
        .session_runtime_state
        .get(&session_id)
        .cloned()
        .unwrap_or_else(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    let before_projection_records = runtime_state.conversation_state.records.clone();
    let projection_changed =
        append_runtime_event_projection_to_state(&mut runtime_state, session_id, &now, event)?;
    let projection_entries = if projection_changed {
        runtime_event_projection_entries_from_changed_records(
            before_projection_records.as_slice(),
            runtime_state.conversation_state.records.as_slice(),
        )
    } else {
        Vec::new()
    };
    let parent_messages = (!projection_entries.is_empty()).then(|| {
        merge_session_messages_with_runtime_event_entries(
            workspace
                .session_messages
                .get(&session_id)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            projection_entries.as_slice(),
        )
    });
    let projected_parent_messages = parent_messages
        .as_ref()
        .map(|messages| conversation_messages_from_session(conversation_id, messages, false))
        .unwrap_or_default();
    let changed_todolist = todolist_state_from_tool_finished_event(event, &now)
        .filter(|todolist| runtime_state.todolist != *todolist);
    if let Some(todolist) = changed_todolist.as_ref() {
        runtime_state.todolist = todolist.clone();
        runtime_state.updated_at = now.clone();
    }
    let changed_plan_mode =
        plan_artifact_state_from_tool_finished_event(event).and_then(|artifact| {
            let mut plan_mode = runtime_state.plan_mode.clone();
            plan_mode.artifact = Some(artifact);
            (runtime_state.plan_mode != plan_mode).then_some(plan_mode)
        });
    if let Some(plan_mode) = changed_plan_mode.as_ref() {
        runtime_state.plan_mode = plan_mode.clone();
        runtime_state.updated_at = now;
    }
    let child_projection = child_message.and_then(|(task_id, message)| {
        let mut messages = workspace
            .session_messages
            .get(&task_id)
            .cloned()
            .unwrap_or_default();
        if messages.iter().any(|existing| existing.id == message.id) {
            return None;
        }
        messages.push(message);
        let projected_messages =
            conversation_messages_from_session(&task_id.to_string(), messages.as_slice(), false);
        let hydrated = workspace
            .session_runtime_state
            .get(&task_id)
            .is_some_and(runtime_state_has_concrete_conversation_context);
        Some(RuntimeEventChildTranscriptDraft {
            task_id,
            messages,
            projected_messages,
            hydrated,
        })
    });
    Ok(RuntimeEventWorkspaceDraft {
        runtime_state,
        parent_messages,
        projected_parent_messages,
        child_projection,
        changed_todolist,
        changed_plan_mode,
    })
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

/// Projects an event owned by a detached or compaction runtime into retained session state.
/// 将分离运行时或压缩运行时持有的事件投影到保留会话状态中。
///
/// # Parameters
/// # 参数
/// `app` emits WebUI deltas produced after projection commit.
/// `app` 发出投影提交后生成的 WebUI 增量。
/// `state` owns the retained projection state.
/// `state` 持有保留投影状态。
/// `session_id` identifies the session that owns the independent runtime event.
/// `session_id` 标识持有独立运行时事件的会话。
/// `event` is the authoritative detached or compaction runtime event.
/// `event` 是权威的分离运行时或压缩运行时事件。
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
    if !runtime_event_projects_conversation_transcript(event.kind) {
        return Ok(());
    }
    // Preflight every fallible event decoder and persistence dependency before any service state mutation.
    // 在修改任何服务状态前，预检全部可失败事件解码器与持久化依赖。
    let subagent_send_message_resume = terminal_subagent_send_message_resume_from_event(event)?;
    let running_subagent_send_message_resume =
        running_subagent_send_message_resume_from_event(event)?;
    let subagent_wait_consumed_executions =
        terminal_subagent_wait_consumed_executions_from_event(event)?;
    let closed_subagent_task_ids = close_agent_closed_subagent_task_ids_from_event(event)?;
    // refreshesCommandSessions identifies terminal tool completions that may create, stop, or otherwise mutate a live session.
    // refreshesCommandSessions 标识可能创建、停止或以其他方式变更实时会话的终端工具完成事件。
    let refreshes_command_sessions = runtime_event_changes_command_sessions(event);
    // refreshesSubagentNavigation identifies a completed spawn whose new task must immediately reach the floating panel.
    // refreshesSubagentNavigation 标识已完成的子代理创建，其新任务必须立即进入浮动面板。
    let refreshes_subagent_navigation = runtime_event_changes_subagent_navigation(event);
    let child_send_message = webui_subagent_send_message_from_event(event)?;
    let goal_update = goal_summary_from_tool_finished_event(event)?;
    if let Some(Some(summary)) = goal_update.as_ref() {
        let goal_session_id = summary.session_id.parse::<SessionId>().map_err(|error| {
            format!("session_goal host result contains an invalid session id: {error}")
        })?;
        if goal_session_id != session_id {
            return Err(format!(
                "session_goal host result belongs to session {goal_session_id}, not runtime event session {session_id}"
            ));
        }
    }
    let controller_runtime = controller_runtime_from_state(state)?;
    let mut changed_addon_session_id: Option<SessionId> = None;
    let mut subagent_navigation_changed = false;
    let (
        changed_todolist,
        changed_plan_mode,
        transcript_changed,
        transcript_messages_for_persistence,
        addon_task_projects,
        addon_session,
        message_window_deltas,
    ) = commit_runtime_event_projection_state(state, session_id, authority, |guard| {
        let conversation_id = session_id.to_string();
        let before_visible_messages: Vec<ConversationMessage> = guard
            .workbench
            .messages
            .iter()
            .filter(|message| message.conversation_id == conversation_id)
            .cloned()
            .collect();
        let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let workspace_draft = build_runtime_event_workspace_draft(
            workspace,
            session_id,
            &conversation_id,
            event,
            child_send_message,
        )?;
        let mut projected_messages = workspace_draft.projected_parent_messages.clone();
        let transcript_changed = workspace_draft.parent_messages.is_some();
        let changed_todolist = workspace_draft.changed_todolist.clone();
        let changed_plan_mode = workspace_draft.changed_plan_mode.clone();
        if changed_todolist.is_some() || changed_plan_mode.is_some() {
            changed_addon_session_id = Some(session_id);
        }
        let transcript_messages_for_persistence = workspace_draft.parent_messages.clone();
        let addon_session = workspace
            .sessions
            .iter()
            .find(|session| session.id == session_id)
            .cloned();
        let addon_task_projects = if changed_todolist.is_some() || changed_plan_mode.is_some() {
            workspace_task_projects_for_persistence(workspace)
        } else {
            Vec::new()
        };
        let next_goal_dispatch_version = goal_update
            .as_ref()
            .map(|_| {
                guard
                    .session_goal_dispatch_versions
                    .get(&session_id)
                    .copied()
                    .unwrap_or_default()
                    .checked_add(1)
                    .ok_or_else(|| {
                        format!(
                            "Goal dispatch authority version overflow for session `{session_id}`"
                        )
                    })
            })
            .transpose()?;
        let child_message_projection = workspace_draft.child_projection.as_ref().map(|child| {
            (
                child.task_id,
                child.messages.clone(),
                child.projected_messages.clone(),
            )
        });
        // No fallible operation may follow this commit boundary while the service lock is held.
        // 持有服务锁时，该提交边界之后不允许再出现可失败操作。
        workspace_draft.commit(
            guard
                .workspace_snapshot
                .as_mut()
                .expect("workspace preflight guarantees commit target"),
            session_id,
        );
        if let Some(goal_update) = goal_update {
            guard.session_goal_dispatch_versions.insert(
                session_id,
                next_goal_dispatch_version
                    .expect("Goal update preflight guarantees a dispatch version"),
            );
            match goal_update {
                Some(summary) => {
                    // An AI-created Goal becomes the active stream's stop/accounting owner in the same atomic projection.
                    // AI 创建的目标会在同一原子投影中成为活动流的停止与记账所有者。
                    if let RuntimeEventProjectionAuthority::ActiveStreamTask(task_id) = authority {
                        let _ = bind_active_stream_task_goal_if_id(
                            guard,
                            session_id,
                            task_id,
                            summary.goal_id.clone(),
                        );
                    }
                    guard.session_goal_summaries.insert(session_id, summary);
                    changed_addon_session_id = Some(session_id);
                }
                None => {
                    guard.session_goal_summaries.remove(&session_id);
                    changed_addon_session_id = Some(session_id);
                }
            }
        }
        if let Some((child_task_id, execution_id)) = subagent_send_message_resume {
            consume_webui_subagent_mailbox_input_for_execution(
                guard,
                session_id,
                child_task_id,
                execution_id.as_str(),
            );
        }
        for (child_task_id, execution_id) in subagent_wait_consumed_executions {
            consume_webui_subagent_mailbox_input_for_execution(
                guard,
                session_id,
                child_task_id,
                execution_id.as_str(),
            );
        }
        if let Some((child_task_id, execution_id)) = running_subagent_send_message_resume {
            mark_webui_subagent_completion_gate_ignored(
                guard,
                child_task_id,
                execution_id.as_str(),
            );
        }
        if remove_webui_closed_subagent_tasks_in_state(guard, &closed_subagent_task_ids) {
            subagent_navigation_changed = true;
        }
        let surface_matches =
            workbench_should_emit_runtime_event_message_patch(guard, &conversation_id, session_id);
        let mut message_window_deltas = Vec::new();
        if surface_matches && transcript_changed {
            projected_messages = merge_visible_streaming_rows_with_runtime_projection(
                before_visible_messages.as_slice(),
                projected_messages.as_slice(),
            );
            let message_patch = build_runtime_event_conversation_messages_patched_delta(
                &conversation_id,
                before_visible_messages.as_slice(),
                projected_messages.as_slice(),
                event,
            );
            if let Some(message_patch) = message_patch {
                guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
                let generation = guard.workbench_generation;
                guard.workbench.revision = generation;
                replace_conversation_messages_in_workbench_window(
                    &mut guard.workbench.messages,
                    &conversation_id,
                    projected_messages.clone(),
                );
                message_window_deltas.push(WorkbenchDeltaEnvelope {
                    generation,
                    delta: WorkbenchDelta::ConversationMessagesPatched(message_patch),
                });
            }
        }
        if let Some((child_task_id, child_display_messages, child_projected_messages)) =
            child_message_projection
        {
            if let Some(cache_entry) = guard.subagent_surface_detail_cache.get_mut(&child_task_id) {
                cache_entry.messages = child_display_messages;
                cache_entry.last_access_ms = current_unix_millis_value();
            }
            let child_conversation_id = child_task_id.to_string();
            let before_child_visible_messages = guard
                .workbench
                .messages
                .iter()
                .filter(|message| message.conversation_id == child_conversation_id)
                .cloned()
                .collect::<Vec<_>>();
            if let Some(message_patch) = build_conversation_messages_patched_delta(
                &child_conversation_id,
                before_child_visible_messages.as_slice(),
                child_projected_messages.as_slice(),
            ) {
                guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
                let generation = guard.workbench_generation;
                guard.workbench.revision = generation;
                replace_conversation_messages_in_workbench_window(
                    &mut guard.workbench.messages,
                    &child_conversation_id,
                    child_projected_messages,
                );
                message_window_deltas.push(WorkbenchDeltaEnvelope {
                    generation,
                    delta: WorkbenchDelta::ConversationMessagesPatched(message_patch),
                });
            }
        }
        Ok((
            changed_todolist,
            changed_plan_mode,
            transcript_changed,
            transcript_messages_for_persistence,
            addon_task_projects,
            addon_session,
            message_window_deltas,
        ))
    })?;
    for event in message_window_deltas {
        emit_service_event_with_trace(
            app,
            "workbench_runtime_event_messages_delta_emit",
            ServiceEvent::WorkbenchDelta(Box::new(event)),
        );
    }
    if refreshes_command_sessions
        && let Err(error) = emit_live_command_sessions_changed_delta(app, state)
    {
        eprintln!(
            "[webui-service] runtime terminal event command-session delta emit failed after commit: {error}"
        );
    }
    if refreshes_subagent_navigation
        && let Err(error) =
            emit_webui_subagent_navigation_changed_delta_for_parent(app, state, session_id)
    {
        eprintln!(
            "[webui-service] runtime spawn-agent navigation delta emit failed after commit: {error}"
        );
    }
    if subagent_navigation_changed
        && let Err(error) =
            emit_webui_subagent_navigation_changed_delta_for_parent(app, state, session_id)
    {
        eprintln!(
            "[webui-service] runtime event navigation delta emit failed after commit: {error}"
        );
    }
    if let Some(addon_session_id) = changed_addon_session_id
        && let Err(error) = emit_session_addons_changed_delta(app, state, addon_session_id)
    {
        eprintln!("[webui-service] runtime event addon delta emit failed after commit: {error}");
    }
    if transcript_changed
        && addon_session.is_some()
        && let Some(messages) = transcript_messages_for_persistence
    {
        spawn_session_transcript_messages_persist_task(
            state.clone(),
            controller_runtime.clone(),
            session_id,
            messages,
            Some(String::from("runtime_event_projection")),
        );
    }
    if let Some(todolist) = changed_todolist
        && let Some(session) = addon_session.as_ref()
    {
        let updated_at = todolist
            .updated_at
            .clone()
            .unwrap_or_else(current_unix_millis_text);
        let mut addon_state = SessionAddonState::new(session_id);
        let _ = addon_state.set_todolist(&todolist, updated_at);
        spawn_session_addon_state_persist_task(
            app.clone(),
            state.clone(),
            controller_runtime.clone(),
            SessionAddonPersistenceScope {
                task_projects: addon_task_projects.clone(),
                session: session.clone(),
                archived: false,
            },
            addon_state,
            vec![SessionAddonNamespace::Todolist],
            Some(String::from("todolist_tool")),
        );
    }
    if let Some(plan_mode) = changed_plan_mode
        && let Some(session) = addon_session
    {
        let updated_at = plan_mode
            .artifact
            .as_ref()
            .map(|artifact| artifact.updated_at.clone())
            .unwrap_or_else(current_unix_millis_text);
        let mut addon_state = SessionAddonState::new(session_id);
        let _ = addon_state.set_work_mode(
            &SessionWorkMode {
                execution_mode: String::from("plan"),
                plan_mode,
            },
            updated_at,
        );
        spawn_session_addon_state_persist_task(
            app.clone(),
            state.clone(),
            controller_runtime,
            SessionAddonPersistenceScope {
                task_projects: addon_task_projects,
                session,
                archived: false,
            },
            addon_state,
            vec![SessionAddonNamespace::WorkMode],
            Some(String::from("submit_plan_tool")),
        );
    }
    Ok(())
}

/// Reports whether one runtime event can create or update a retained conversation record.
/// 判断一条运行时事件是否可以创建或更新保留的会话记录。
///
/// # Parameters
/// 参数
/// `event_kind` is the runtime event discriminator emitted by the Agent runtime.
/// `event_kind` 是 Agent 运行时发送的运行事件判别值。
///
/// # Returns
/// 返回值
/// Returns `true` for runtime events that own a visible transcript record or update.
/// 对拥有可见转录记录或更新的运行时事件返回 `true`。
fn runtime_event_projects_conversation_transcript(event_kind: RuntimeEventKind) -> bool {
    matches!(
        event_kind,
        RuntimeEventKind::ToolRequestStarted
            | RuntimeEventKind::ToolRequestProgress
            | RuntimeEventKind::ToolRequestPreviewUpdated
            | RuntimeEventKind::ToolRequestReady
            | RuntimeEventKind::ToolRequestInvalid
            | RuntimeEventKind::ToolRequestAborted
            | RuntimeEventKind::ToolStarted
            | RuntimeEventKind::ToolFinished
            | RuntimeEventKind::Retrying
    )
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

/// Reports whether one runtime event created a subagent that must enter parent-session navigation.
/// 判断单个运行时事件是否创建了必须进入父会话导航的子代理。
///
/// # Parameters
/// 参数
/// `event` is the runtime event currently being projected into WebUI state.
/// `event` 是当前正投影到 WebUI 状态的运行时事件。
///
/// # Returns
/// 返回值
/// `true` only after `spawn_agent` has registered its new child task successfully.
/// 仅当 `spawn_agent` 已成功注册新子任务后返回 `true`。
fn runtime_event_changes_subagent_navigation(event: &RuntimeEvent) -> bool {
    event.kind == RuntimeEventKind::ToolFinished
        && runtime_event_tool_name(event) == Some("spawn_agent")
}

/// Returns whether one runtime-event transcript patch should be emitted to the WebUI message window.
/// 返回单个运行时事件转录补丁是否应该发送到 WebUI 消息窗口。
pub(crate) fn workbench_should_emit_runtime_event_message_patch(
    guard: &ServiceBridgeState,
    conversation_id: &str,
    session_id: SessionId,
) -> bool {
    workbench_surface_matches_conversation(&guard.workbench, conversation_id)
        || guard
            .subagent_surface_detail_cache
            .contains_key(&session_id)
        || guard
            .detached_subagent_task_listeners
            .contains_key(&session_id)
}

/// Returns stable transcript entries created or updated by one runtime tool projection.
/// 返回单次运行时工具投影创建或更新的稳定转录项。
fn runtime_event_projection_entries_from_changed_records(
    before_records: &[SessionConversationRecord],
    after_records: &[SessionConversationRecord],
) -> Vec<SessionMessageEntry> {
    let before_by_id = before_records
        .iter()
        .map(|record| (record.id.as_str(), record))
        .collect::<BTreeMap<_, _>>();
    after_records
        .iter()
        .filter(|record| runtime_event_projection_record_is_stable(record))
        .filter(|record| {
            before_by_id
                .get(record.id.as_str())
                .is_none_or(|before| !session_conversation_records_equal(before, record))
        })
        .map(session_message_entry_from_conversation_record)
        .collect()
}

/// Returns whether one runtime record is safe to retain before the turn has completed.
/// 返回单条运行时记录是否可以在当前轮次完成前稳定保留。
fn runtime_event_projection_record_is_stable(record: &SessionConversationRecord) -> bool {
    matches!(
        record.record_kind,
        SessionMessageRecordKind::ToolCallOutput
            | SessionMessageRecordKind::ToolResultOutput
            | SessionMessageRecordKind::ToolDetailOutput
    )
}

/// Compares two runtime conversation records without requiring protocol-level PartialEq.
/// 在不要求协议类型实现 PartialEq 的情况下比较两条运行时会话记录。
fn session_conversation_records_equal(
    left: &SessionConversationRecord,
    right: &SessionConversationRecord,
) -> bool {
    left.id == right.id
        && left.kind == right.kind
        && left.record_kind == right.record_kind
        && left.visible == right.visible
        && left.title == right.title
        && left.body == right.body
        && left.metadata == right.metadata
        && left.created_at == right.created_at
}

/// Merges changed runtime tool entries into retained display transcript rows.
/// 将变化的运行时工具项合并到已保留的显示转录行中。
pub(crate) fn merge_session_messages_with_runtime_event_entries(
    existing_messages: &[SessionMessageEntry],
    projection_entries: &[SessionMessageEntry],
) -> Vec<SessionMessageEntry> {
    let mut merged_messages = existing_messages.to_vec();
    for entry in projection_entries {
        if let Some(existing_entry) = merged_messages
            .iter_mut()
            .find(|message| message.id == entry.id)
        {
            *existing_entry = entry.clone();
        } else {
            merged_messages.push(entry.clone());
        }
    }
    merged_messages
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
fn runtime_event_projection_run_phase(event: &RuntimeEvent) -> Option<SessionRunPhase> {
    match event.kind {
        RuntimeEventKind::ToolRequestStarted
        | RuntimeEventKind::ToolRequestProgress
        | RuntimeEventKind::ToolRequestPreviewUpdated
        | RuntimeEventKind::ToolRequestReady => Some(SessionRunPhase::ToolCalling),
        RuntimeEventKind::ToolRequestInvalid | RuntimeEventKind::ToolRequestAborted => {
            Some(SessionRunPhase::Requesting)
        }
        RuntimeEventKind::ToolStarted => Some(SessionRunPhase::ToolRunning),
        RuntimeEventKind::ToolFinished => Some(SessionRunPhase::Requesting),
        _ => None,
    }
}

/// Caches the latest provider usage into the live context-budget snapshot.
/// 将最近一次 Provider 用量缓存进实时上下文预算快照。
pub(crate) fn cache_live_context_budget_observed_usage(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    usage: &ProviderUsageSnapshot,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(context_budget) = guard.live_context_budgets.get_mut(&session_id) else {
        return Ok(());
    };
    apply_observed_usage_to_context_budget(context_budget, usage);
    Ok(())
}

/// Applies one observed provider usage sample to an existing budget snapshot.
/// 将一份 Provider 实测用量应用到既有预算快照。
pub(crate) fn apply_observed_usage_to_context_budget(
    context_budget: &mut ContextBudgetSnapshot,
    usage: &ProviderUsageSnapshot,
) {
    let Some(input_tokens) =
        vulcan_context::observed_input_tokens_from_usage(Some(usage)).or(usage.total_tokens)
    else {
        context_budget.observed_usage = Some(usage.clone());
        return;
    };
    context_budget.estimated_input_tokens = input_tokens;
    context_budget.estimated_total_tokens = usage.total_tokens.unwrap_or_else(|| {
        input_tokens
            .saturating_add(usage.output_tokens.unwrap_or_default())
            .saturating_add(usage.reasoning_tokens.unwrap_or_default())
    });
    context_budget.usage_ratio = context_budget
        .effective_input_limit
        .map(|limit| input_tokens as f32 / limit.max(1) as f32)
        .unwrap_or_else(|| if input_tokens > 0 { 0.01 } else { 0.0 });
    context_budget.severity = historical_context_budget_severity(context_budget.usage_ratio);
    context_budget.observed_usage = Some(usage.clone());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use vulcan_subagent_core::STOPPED_SUBAGENT_TASK_IDS_KEY;

    /// Builds one minimal visible message for runtime-projection ordering tests.
    /// 为运行时投影顺序测试构建一条最小可见消息。
    fn test_runtime_projection_message(
        id: &str,
        record_kind: SessionMessageRecordKind,
    ) -> ConversationMessage {
        ConversationMessage {
            id: id.to_string(),
            conversation_id: String::from("conversation"),
            created_at: id.to_string(),
            role: MessageRole::Assistant,
            title: None,
            record_kind: Some(record_kind.as_persisted_key().to_string()),
            body: id.to_string(),
            metadata: None,
            attachments: Vec::new(),
            editable_user_prompt: false,
        }
    }

    /// Verifies a new tool projection remains after already visible thinking and assistant stream rows.
    /// 验证新工具投影保持在已经可见的思考与 Assistant 流记录之后。
    #[test]
    fn runtime_tool_projection_preserves_visible_stream_order() {
        // before_messages mirrors thinking and Assistant rows already shown before the tool event.
        // before_messages 模拟工具事件前已经显示的思考与 Assistant 记录。
        let before_messages = vec![
            test_runtime_projection_message("1000", SessionMessageRecordKind::ThinkingOutput),
            test_runtime_projection_message("1001", SessionMessageRecordKind::AssistantOutput),
        ];
        // projected_messages contains the newly stable tool row and intentionally omits active stream drafts.
        // projected_messages 包含新稳定工具记录，并有意省略活跃流式草稿。
        let projected_messages = vec![test_runtime_projection_message(
            "1002",
            SessionMessageRecordKind::ToolCallOutput,
        )];

        // merged_messages must retain the visible stream rows before appending the new tool.
        // merged_messages 必须先保留可见流记录，再追加新工具。
        let merged_messages = merge_visible_streaming_rows_with_runtime_projection(
            before_messages.as_slice(),
            projected_messages.as_slice(),
        );

        assert_eq!(
            merged_messages
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["1000", "1001", "1002"]
        );
    }

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

    /// Verifies runtime tool patches use the same visible transcript projection as final hydration.
    /// 验证运行时工具补丁使用与最终水合一致的可见转录投影。
    #[test]
    fn runtime_tool_projection_filters_hidden_tool_detail_records() {
        let session_id = SessionId::new_v4();
        let conversation_id = session_id.to_string();
        let mut runtime_state = SessionRuntimeStateEntry::default();
        let started = RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            "工具调用 · call-shell · shell_command",
            "## Tool Call",
        )
        .with_metadata(Some(json!({
            "tool_call_id": "call-shell",
            "tool_name": "shell_command",
            "arguments": {
                "command": "git diff --stat"
            }
        })));
        assert!(
            append_runtime_event_projection_to_state(
                &mut runtime_state,
                session_id,
                "1000",
                &started,
            )
            .expect("valid runtime event record identities")
        );
        let finished = RuntimeEvent::new(
            RuntimeEventKind::ToolFinished,
            "工具返回 · call-shell",
            "## Result ID\n\n`tool-result-call-shell`\n\n## Tool Result\n\nCommand: git diff --stat\n\n[stdout]\nfiles changed",
        )
        .with_metadata(Some(json!({
            "tool_call_id": "call-shell",
            "tool_name": "shell_command",
            "display_state": "completed",
            "host_result": {
                "kind": "command_run",
                "payload": {
                    "command": "git diff --stat",
                    "status": "completed"
                }
            }
        })));
        assert!(
            append_runtime_event_projection_to_state(
                &mut runtime_state,
                session_id,
                "1001",
                &finished,
            )
            .expect("valid runtime event record identities")
        );

        let merged_messages = merge_session_messages_with_conversation_state_records(
            &[],
            &runtime_state.conversation_state,
        );
        assert!(
            merged_messages
                .iter()
                .any(|message| message.record_kind == SessionMessageRecordKind::ToolDetailOutput)
        );

        let projected_messages =
            conversation_messages_from_session(&conversation_id, &merged_messages, false);
        assert_eq!(projected_messages.len(), 1);
        let tool_message = projected_messages
            .first()
            .expect("tool call output should remain visible");
        assert_eq!(
            tool_message.record_kind.as_deref(),
            Some("tool-call-output")
        );
        assert!(tool_message.body.is_empty());
        assert!(
            !projected_messages
                .iter()
                .any(|message| message.record_kind.as_deref() == Some("tool-detail-output"))
        );
        assert_eq!(
            tool_message
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("result"))
                .and_then(|result| result.get("detail_ref"))
                .and_then(|detail_ref| detail_ref.get("message_index"))
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
    }

    /// Verifies runtime tool projection does not promote active streaming drafts into retained transcript rows.
    /// 验证运行时工具投影不会把活跃流式草稿提升为保留转录行。
    #[test]
    fn runtime_tool_projection_keeps_streaming_drafts_out_of_retained_rows() {
        let session_id = SessionId::new_v4();
        let mut runtime_state = SessionRuntimeStateEntry::default();
        let mut thinking_record_id = None;
        append_stream_delta_record_to_state(
            &mut runtime_state,
            session_id,
            "1000",
            &mut thinking_record_id,
            SessionMessageRecordKind::ThinkingOutput,
            "思考过程",
            "正在整理工具调用。",
            true,
        )
        .expect("valid streaming record identity");
        let before_projection_records = runtime_state.conversation_state.records.clone();
        let started = RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            "工具调用 · call-read · vulcan_file_read",
            "## Tool Call",
        )
        .with_metadata(Some(json!({
            "tool_call_id": "call-read",
            "tool_name": "vulcan_file_read",
            "arguments": {
                "path": "README.md"
            }
        })));

        assert!(
            append_runtime_event_projection_to_state(
                &mut runtime_state,
                session_id,
                "1001",
                &started,
            )
            .expect("valid runtime event record identities")
        );
        let projection_entries = runtime_event_projection_entries_from_changed_records(
            before_projection_records.as_slice(),
            runtime_state.conversation_state.records.as_slice(),
        );
        let merged_messages =
            merge_session_messages_with_runtime_event_entries(&[], projection_entries.as_slice());

        assert_eq!(merged_messages.len(), 1);
        assert_eq!(
            merged_messages[0].record_kind,
            SessionMessageRecordKind::ToolCallOutput
        );
        assert!(
            !merged_messages
                .iter()
                .any(|message| message.record_kind == SessionMessageRecordKind::ThinkingOutput)
        );
    }

    /// Verifies every visible tool-request revision reaches the retained transcript projection.
    /// 验证每个可见工具请求修订都能进入保留转录投影。
    ///
    /// # Returns
    /// 返回值
    /// This test returns no value and reports failures through assertions.
    /// 此测试无返回值，并通过断言报告失败。
    #[test]
    fn visible_runtime_events_project_conversation_transcript() {
        // visibleRuntimeEvents enumerates tool-card lifecycle events and independent retry notes.
        // visibleRuntimeEvents 枚举工具卡片生命周期事件与独立重试备注。
        let visible_runtime_events = [
            RuntimeEventKind::ToolRequestStarted,
            RuntimeEventKind::ToolRequestProgress,
            RuntimeEventKind::ToolRequestPreviewUpdated,
            RuntimeEventKind::ToolRequestReady,
            RuntimeEventKind::ToolRequestInvalid,
            RuntimeEventKind::ToolRequestAborted,
            RuntimeEventKind::ToolStarted,
            RuntimeEventKind::ToolFinished,
            RuntimeEventKind::Retrying,
        ];

        // eventKind must pass the projection gate so its exact visible record can be persisted.
        // eventKind 必须通过投影门禁，确保其精确可见记录能够持久化。
        for event_kind in visible_runtime_events {
            assert!(
                runtime_event_projects_conversation_transcript(event_kind),
                "visible runtime event {event_kind:?} must reach transcript projection"
            );
        }
        assert!(!runtime_event_projects_conversation_transcript(
            RuntimeEventKind::TokenDelta
        ));
    }

    /// Verifies a session-scoped runtime-event patch leaves unrelated workspace collections untouched.
    /// 验证会话级运行时事件补丁不会修改无关工作区集合。
    #[test]
    fn runtime_event_workspace_draft_commits_only_owned_session_fields() {
        let session_id = SessionId::new_v4();
        let unrelated_session_id = SessionId::new_v4();
        let unrelated_message =
            SessionMessageEntry::new(SessionMessageKind::Assistant, "Unrelated", "unrelated body")
                .with_runtime_metadata("unrelated-record", "1");
        let mut workspace = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::from([(
                unrelated_session_id,
                vec![unrelated_message.clone()],
            )]),
            archived_session_messages: BTreeMap::from([(
                unrelated_session_id,
                vec![unrelated_message],
            )]),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::from([(
                unrelated_session_id,
                SessionRuntimeStateEntry::default(),
            )]),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::from([String::from("unrelated-project")]),
            selected_session: unrelated_session_id,
            hydrated_session_ids: BTreeSet::from([unrelated_session_id]),
        };
        // unrelated_state captures every collection that the parent runtime patch does not own.
        // unrelated_state 捕获父运行时补丁不拥有的全部集合。
        let unrelated_state = serde_json::json!({
            "active_messages": workspace.session_messages.get(&unrelated_session_id),
            "archived_messages": workspace.archived_session_messages.get(&unrelated_session_id),
            "archived_runtime": workspace.archived_session_runtime_state.get(&unrelated_session_id),
            "expanded_projects": workspace.expanded_task_projects,
            "selected_session": workspace.selected_session,
        });
        let event = RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            "工具调用 · call-read · read_file",
            "starting",
        )
        .with_metadata(Some(json!({
            "tool_call_id": "call-read",
            "tool_name": "read_file",
            "arguments": { "path": "README.md" }
        })));

        let draft = build_runtime_event_workspace_draft(
            &workspace,
            session_id,
            &session_id.to_string(),
            &event,
            None,
        )
        .expect("runtime event workspace draft should build");
        draft.commit(&mut workspace, session_id);

        assert!(workspace.session_runtime_state.contains_key(&session_id));
        assert_eq!(
            serde_json::json!({
                "active_messages": workspace.session_messages.get(&unrelated_session_id),
                "archived_messages": workspace.archived_session_messages.get(&unrelated_session_id),
                "archived_runtime": workspace.archived_session_runtime_state.get(&unrelated_session_id),
                "expanded_projects": workspace.expanded_task_projects,
                "selected_session": workspace.selected_session,
            }),
            unrelated_state
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
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
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
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
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

    /// Verifies a tool-start message patch carries the phase that closes the thinking overlay.
    /// 验证工具开始消息补丁携带用于关闭思考浮层的运行阶段。
    #[test]
    fn runtime_tool_started_message_patch_carries_tool_running_phase() {
        let conversation_id = SessionId::new_v4().to_string();
        let started = RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            "工具调用 · call-read · vulcan_file_read",
            "## Tool Call",
        );
        let tool_message = ConversationMessage {
            id: String::from("tool-call-call-read"),
            conversation_id: conversation_id.clone(),
            created_at: String::from("1000"),
            role: MessageRole::Assistant,
            title: Some(String::from("工具调用 · vulcan_file_read")),
            record_kind: Some(String::from("tool-call-output")),
            body: String::new(),
            metadata: Some(json!({
                "tool_call_id": "call-read",
                "tool_name": "vulcan_file_read",
            })),
            attachments: Vec::new(),
            editable_user_prompt: false,
        };

        let patch = build_runtime_event_conversation_messages_patched_delta(
            &conversation_id,
            &[],
            &[tool_message],
            &started,
        )
        .expect("tool start should insert a visible tool-call message");

        assert_eq!(
            patch.run_phase,
            Some(SessionRunPhase::ToolRunning.to_string())
        );
        assert_eq!(patch.upsert_messages.len(), 1);
        assert_eq!(
            patch.upsert_messages[0].record_kind.as_deref(),
            Some("tool-call-output")
        );
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
    fn runtime_event_subagent_navigation_refresh_matches_finished_spawn() {
        // spawn_finished represents a child task already registered by the orchestration runtime.
        // spawn_finished 表示编排运行时已经注册完成的子任务。
        let spawn_finished =
            RuntimeEvent::new(RuntimeEventKind::ToolFinished, "spawn finished", "")
                .with_metadata(Some(json!({ "tool_name": "spawn_agent" })));
        // spawn_started must not project navigation before the child task registration commits.
        // spawn_started 不得在子任务注册提交前投影导航。
        let spawn_started = RuntimeEvent::new(RuntimeEventKind::ToolStarted, "spawn started", "")
            .with_metadata(Some(json!({ "tool_name": "spawn_agent" })));
        // send_finished updates an existing child and must rely on its dedicated event-ingest path.
        // send_finished 更新既有子任务，必须继续使用其专用事件摄取路径。
        let send_finished = RuntimeEvent::new(RuntimeEventKind::ToolFinished, "send finished", "")
            .with_metadata(Some(json!({ "tool_name": "send_message" })));

        assert!(runtime_event_changes_subagent_navigation(&spawn_finished));
        assert!(!runtime_event_changes_subagent_navigation(&spawn_started));
        assert!(!runtime_event_changes_subagent_navigation(&send_finished));
    }
}
