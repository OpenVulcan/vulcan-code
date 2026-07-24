use super::*;

/// Independently dispatchable profile and precheck requests for one prompt turn.
/// 单个提示词回合中可独立派发的画像与 precheck 请求。
#[derive(Clone)]
pub struct ConversationPromptHookRequests {
    /// Profile preparation request whose payload owns system-profile placement metadata.
    /// 持有系统画像放置元数据的画像准备请求。
    pub profile: ConversationProfileRequest,
    /// User-recall precheck request whose payload contains no system-profile metadata.
    /// 不包含系统画像元数据的用户召回 precheck 请求。
    pub precheck: ConversationPrecheckRequest,
    /// Shared immutable prompt-turn correlation id.
    /// 两条线路共享的不可变提示词回合关联标识。
    pub turn_id: String,
}

/// Session-level conversation hook dispatcher and turn assembler.
/// 会话级会话 hook 调度器与回合组装器。
#[derive(Clone, Default)]
pub struct ConversationHookRuntime {
    /// Shared hook dispatcher used by every conversation hook point.
    /// 所有会话 hook 点共用的 hook 调度器。
    dispatcher: HookDispatcher,
    /// Active logical turns keyed by session id.
    /// 按会话标识索引的活动逻辑回合。
    pub(super) active_turns: BTreeMap<SessionId, ActiveConversationTurn>,
}

impl ConversationHookRuntime {
    /// Builds an empty conversation hook runtime.
    /// 构建一个空的会话 hook 运行时。
    pub fn new() -> Self {
        let mut runtime = Self::default();
        runtime.register_builtin_points();
        runtime
    }

    /// Returns the mutable shared hook dispatcher for registration.
    /// 返回用于注册的可变共享 hook 调度器。
    pub fn dispatcher_mut(&mut self) -> &mut HookDispatcher {
        &mut self.dispatcher
    }

    /// Returns deterministic snapshots for handlers registered on the precheck hook point.
    /// 返回注册在 precheck hook 点位上的确定性处理器快照。
    pub fn precheck_hook_snapshots(&self) -> Vec<HookRegistrationSnapshot> {
        self.dispatcher
            .registry()
            .registration_snapshots(Some(HOOK_CONVERSATION_TURN_PRECHECK))
    }

    /// Returns whether a logical user turn is currently open for the session.
    /// 返回该会话当前是否存在打开中的逻辑用户回合。
    pub fn has_active_turn(&self, session_id: SessionId) -> bool {
        self.active_turns.contains_key(&session_id)
    }

    /// Returns the exact active logical turn identifier for one session without mutating ownership.
    /// 在不改变所有权的前提下返回单个会话的精确活动逻辑回合标识。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the exact session whose retained turn identity is requested.
    /// `session_id` 是需要读取保留回合身份的精确会话。
    ///
    /// # Returns
    /// 返回值
    /// The borrowed active turn id when present, otherwise `None`.
    /// 存在时返回借用的活动回合标识，否则返回 `None`。
    pub fn active_turn_id(&self, session_id: SessionId) -> Option<&str> {
        self.active_turns
            .get(&session_id)
            .map(|turn| turn.turn_id.as_str())
    }

    /// Registers every built-in conversation hook point descriptor.
    /// 注册所有内置会话 hook 点位描述。
    fn register_builtin_points(&mut self) {
        let descriptors = host_hook_point_descriptors();
        self.dispatcher
            .register_points(descriptors)
            .expect("built-in conversation hook points should be valid");
    }

    /// Opens or updates one logical user turn and prepares independent profile and precheck requests.
    /// 为一条用户提示打开或更新逻辑回合，并准备相互独立的画像与 precheck 请求。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` identifies the owning conversation session.
    /// `session_id` 标识所属会话。
    /// `prompt_entry_id` identifies the exact queued or submitted prompt entry.
    /// `prompt_entry_id` 标识精确的排队或已提交提示项。
    /// `project_id` optionally identifies the owning project.
    /// `project_id` 可选标识所属项目。
    /// `workspace_path` is the exact workspace used for layered Hook configuration.
    /// `workspace_path` 是 Hook 分层配置使用的精确工作区。
    /// `source` identifies how this prompt entered the conversation.
    /// `source` 标识该提示如何进入会话。
    /// `system_prompt_hook_phase` belongs only to the profile-preparation payload.
    /// `system_prompt_hook_phase` 仅属于画像准备载荷。
    /// `prompt` is the sanitized user-visible or hidden prompt content.
    /// `prompt` 是经过清理的用户可见或隐藏提示内容。
    /// `conversation_state` supplies immutable history counters without sharing mutable payloads.
    /// `conversation_state` 提供不可变历史计数，但不会让两条线路共享可变载荷。
    ///
    /// # Returns
    /// 返回值
    /// Two sibling requests with one shared turn id and distinct payload ownership.
    /// 返回共享一个回合标识且载荷归属不同的两条同级请求。
    pub fn prepare_user_prompt_hook_requests(
        &mut self,
        session_id: SessionId,
        prompt_entry_id: &str,
        project_id: Option<String>,
        workspace_path: String,
        source: PendingSessionPromptSource,
        system_prompt_hook_phase: SystemPromptHookPhase,
        prompt: &str,
        conversation_state: &SessionConversationState,
    ) -> ConversationPromptHookRequests {
        let event_kind = pending_prompt_source_event_kind(source);
        let was_recorded_while_queued = self
            .active_turns
            .get(&session_id)
            .is_some_and(|turn| turn.recorded_prompt_entry_ids.contains(prompt_entry_id));
        let is_new_turn = !self.active_turns.contains_key(&session_id);
        let turn_id = if is_new_turn {
            let turn_id = new_conversation_turn_id(session_id);
            self.active_turns.insert(
                session_id,
                ActiveConversationTurn::new(turn_id.clone(), prompt.to_string()),
            );
            turn_id
        } else if was_recorded_while_queued {
            self.active_turns
                .get(&session_id)
                .map(|turn| turn.turn_id.clone())
                .unwrap_or_default()
        } else {
            let turn = self
                .active_turns
                .get_mut(&session_id)
                .expect("active turn should exist after contains_key");
            turn.timeline.push(ConversationTimelineItem::new(
                "user",
                event_kind,
                prompt.trim().to_string(),
                None,
            ));
            turn.turn_id.clone()
        };

        // precheck_payload owns user-prompt recall context and contains no system-profile fields.
        // precheck_payload 持有用户提示召回上下文，且不包含系统画像字段。
        let precheck_payload = json!({
            "session_id": session_id.to_string(),
            "project_id": project_id.clone(),
            "workspace_path": workspace_path.clone(),
            "turn_id": turn_id,
            "event_kind": event_kind,
            "is_new_turn": is_new_turn,
            "was_recorded_while_queued": was_recorded_while_queued,
            "user_content": prompt,
            "visible_history_count": visible_history_count(conversation_state),
            "message_count": conversation_state.messages.len(),
            "record_count": conversation_state.records.len(),
        });
        if !was_recorded_while_queued {
            self.dispatch_json(
                HOOK_CONVERSATION_USER_EVENT,
                session_id,
                precheck_payload.clone(),
                HookScope::Session,
            );
        }
        // profile_payload owns only session profile context and cannot inspect the user prompt.
        // profile_payload 仅持有会话画像上下文，无法读取用户提示词。
        let profile_payload = json!({
            "session_id": session_id.to_string(),
            "project_id": project_id,
            "workspace_path": workspace_path,
            "turn_id": turn_id,
            "system_prompt_hook_phase": system_prompt_hook_phase.as_str(),
            "message_count": conversation_state.messages.len(),
            "record_count": conversation_state.records.len(),
        });
        ConversationPromptHookRequests {
            profile: ConversationProfileRequest::new(
                self.dispatcher.clone(),
                session_id,
                profile_payload,
                turn_id.clone(),
            ),
            precheck: ConversationPrecheckRequest::new(
                self.dispatcher.clone(),
                session_id,
                precheck_payload,
                turn_id.clone(),
            ),
            turn_id,
        }
    }

    /// Records a queued user prompt as timeline-only input without running precheck.
    /// 将已排队用户提示记录为仅 timeline 输入，不运行 precheck。
    pub fn record_queued_user_prompt(
        &mut self,
        session_id: SessionId,
        prompt_entry_id: &str,
        project_id: Option<String>,
        workspace_path: String,
        source: PendingSessionPromptSource,
        prompt: &str,
    ) {
        let turn_id = self.ensure_turn_for_runtime_event(session_id);
        let event_kind = pending_prompt_source_event_kind(source);
        if let Some(turn) = self.active_turns.get_mut(&session_id)
            && turn
                .recorded_prompt_entry_ids
                .insert(prompt_entry_id.to_string())
        {
            turn.timeline.push(ConversationTimelineItem::new(
                "user",
                event_kind,
                prompt.trim().to_string(),
                Some(prompt_entry_id.to_string()),
            ));
        }
        self.dispatch_json(
            HOOK_CONVERSATION_USER_EVENT,
            session_id,
            json!({
                "session_id": session_id.to_string(),
                "project_id": project_id,
                "workspace_path": workspace_path,
                "turn_id": turn_id,
                "event_kind": event_kind,
                "is_new_turn": false,
                "queued": true,
                "prompt_entry_id": prompt_entry_id,
                "user_content": prompt,
            }),
            HookScope::Session,
        );
    }

    /// Records an ask/question prompt opened by the runtime.
    /// 记录运行时打开的 ask/question 提示。
    pub fn record_question_asked(
        &mut self,
        session_id: SessionId,
        request_id: &str,
        questions: &[QuestionPrompt],
    ) {
        let content = questions
            .iter()
            .map(question_prompt_summary)
            .collect::<Vec<_>>()
            .join("\n");
        let turn_id = self.ensure_turn_for_runtime_event(session_id);
        if let Some(turn) = self.active_turns.get_mut(&session_id) {
            turn.timeline.push(ConversationTimelineItem::new(
                "assistant",
                "ask",
                content.clone(),
                Some(request_id.to_string()),
            ));
        }
        self.dispatch_json(
            HOOK_CONVERSATION_QUESTION_ASKED,
            session_id,
            json!({
                "session_id": session_id.to_string(),
                "turn_id": turn_id,
                "request_id": request_id,
                "questions": question_prompt_payloads(questions),
                "content": content,
            }),
            HookScope::Session,
        );
    }

    /// Records user answers submitted to the runtime ask/question prompt.
    /// 记录用户提交给运行时 ask/question 提示的回答。
    pub fn record_question_answered(
        &mut self,
        session_id: SessionId,
        request_id: &str,
        questions: &[QuestionPrompt],
        answers: &[Vec<String>],
    ) {
        let content = answers
            .iter()
            .enumerate()
            .map(|(index, answer)| {
                let title = questions
                    .get(index)
                    .map(|question| question.header.as_str())
                    .unwrap_or("question");
                format!("{title}: {}", answer.join(", "))
            })
            .collect::<Vec<_>>()
            .join("\n");
        let turn_id = self.ensure_turn_for_runtime_event(session_id);
        if let Some(turn) = self.active_turns.get_mut(&session_id) {
            turn.timeline.push(ConversationTimelineItem::new(
                "user",
                "ask_answer",
                content.clone(),
                Some(request_id.to_string()),
            ));
        }
        self.dispatch_json(
            HOOK_CONVERSATION_QUESTION_ANSWERED,
            session_id,
            json!({
                "session_id": session_id.to_string(),
                "turn_id": turn_id,
                "request_id": request_id,
                "answers": answers,
                "content": content,
            }),
            HookScope::Session,
        );
    }

    /// Records user rejection of the runtime ask/question prompt.
    /// 记录用户拒绝运行时 ask/question 提示。
    pub fn record_question_rejected(&mut self, session_id: SessionId, request_id: &str) {
        let turn_id = self.ensure_turn_for_runtime_event(session_id);
        if let Some(turn) = self.active_turns.get_mut(&session_id) {
            turn.timeline.push(ConversationTimelineItem::new(
                "user",
                "ask_rejected",
                "The user rejected the runtime question.",
                Some(request_id.to_string()),
            ));
        }
        self.dispatch_json(
            HOOK_CONVERSATION_QUESTION_REJECTED,
            session_id,
            json!({
                "session_id": session_id.to_string(),
                "turn_id": turn_id,
                "request_id": request_id,
            }),
            HookScope::Session,
        );
    }

    /// Dispatches a tool hook for one immutable request or execution correlation without creating a turn.
    /// 为一个不可变请求或执行关联派发工具 hook，且不创建回合。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` identifies the main, child, or standalone execution session that emitted the event.
    /// `session_id` 标识发出事件的主会话、子任务或独立执行会话。
    /// `execution_id` identifies the exact request turn or execution generation used for Hook correlation.
    /// `execution_id` 标识用于 Hook 关联的精确请求回合或执行代次。
    /// `event` is the runtime event; non-tool boundaries are ignored.
    /// `event` 是运行时事件；非工具边界事件会被忽略。
    ///
    /// # Returns
    /// 返回值
    /// This method returns no value and never mutates active conversation turns.
    /// 该方法无返回值，且绝不修改活跃会话回合。
    pub fn record_tool_runtime_event_for_execution(
        &self,
        session_id: SessionId,
        execution_id: &str,
        event: &RuntimeEvent,
    ) {
        let (hook_point, timeline_kind) = match event.kind {
            RuntimeEventKind::ToolStarted => (HOOK_CONVERSATION_TOOL_STARTED, "tool_started"),
            RuntimeEventKind::ToolFinished => (HOOK_CONVERSATION_TOOL_FINISHED, "tool_finished"),
            _ => return,
        };
        let source_id = event
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("tool_call_id"))
            .and_then(Value::as_str)
            .map(str::to_string);
        // Tool events still dispatch hook notifications, but they do not belong in the user-turn timeline.
        // 工具事件仍会派发 hook 通知，但不进入用户回合 timeline。
        self.dispatch_json(
            hook_point,
            session_id,
            json!({
                "session_id": session_id.to_string(),
                "turn_id": execution_id,
                "kind": timeline_kind,
                "title": event.title,
                "detail": event.detail,
                "source_id": source_id,
                "metadata": event.metadata,
            }),
            HookScope::Session,
        );
    }

    /// Completes one logical turn locally and returns a dispatch request that can run off the critical path.
    /// 在本地闭合一个逻辑回合，并返回可脱离关键路径执行的调度请求。
    pub fn prepare_turn_completion_dispatch(
        &mut self,
        session_id: SessionId,
        project_id: Option<String>,
        assistant_content: &str,
        provider_request_count: usize,
        conversation_state: &SessionConversationState,
    ) -> (String, ConversationCompletionRequest) {
        self.prepare_turn_completion_dispatch_with_status(
            session_id,
            project_id,
            assistant_content,
            provider_request_count,
            conversation_state,
            ConversationTurnCompletionStatus::Completed,
            0,
        )
    }

    /// Prepares one completed-turn hook payload while preserving unfinished logical turns when required.
    /// 准备一条完成回合 hook 载荷，并在需要时保留尚未结束的逻辑回合。
    pub fn prepare_turn_completion_dispatch_with_status(
        &mut self,
        session_id: SessionId,
        project_id: Option<String>,
        assistant_content: &str,
        provider_request_count: usize,
        conversation_state: &SessionConversationState,
        completion_status: ConversationTurnCompletionStatus,
        pending_subagent_count: usize,
    ) -> (String, ConversationCompletionRequest) {
        let turn = if completion_status.is_final() {
            self.active_turns
                .remove(&session_id)
                .unwrap_or_else(|| self.reconstruct_turn_from_state(session_id, conversation_state))
        } else if let Some(turn) = self.active_turns.get(&session_id) {
            turn.clone()
        } else {
            let turn = self.reconstruct_turn_from_state(session_id, conversation_state);
            self.active_turns.insert(session_id, turn.clone());
            turn
        };
        let completed_turn_id = turn.turn_id.clone();
        let turn_complete = completion_status.is_final();
        let payload = json!({
            "session_id": session_id.to_string(),
            "project_id": project_id,
            "turn_id": turn.turn_id,
            "user_content": turn.user_content,
            "timeline": turn.timeline,
            "assistant_content": assistant_content,
            "provider_request_count": provider_request_count,
            "completion_status": completion_status.as_str(),
            "turn_complete": turn_complete,
            "pending_subagent_count": pending_subagent_count,
            "message_count": conversation_state.messages.len(),
            "record_count": conversation_state.records.len(),
            "turn_messages": &conversation_state.messages,
            "turn_records": &conversation_state.records,
            "conversation_state": conversation_state,
        });
        (
            completed_turn_id.clone(),
            ConversationCompletionRequest::new(
                self.dispatcher.clone(),
                session_id,
                payload,
                completed_turn_id,
            ),
        )
    }

    /// Terminates one logical turn and fires the terminated hook payload.
    /// 终止一个逻辑回合并触发 terminated hook payload。
    pub fn terminate_turn(
        &mut self,
        session_id: SessionId,
        project_id: Option<String>,
        reason: &str,
        provider_request_count: usize,
    ) {
        let turn = self.active_turns.remove(&session_id);
        self.dispatch_json(
            HOOK_CONVERSATION_TURN_TERMINATED,
            session_id,
            json!({
                "session_id": session_id.to_string(),
                "project_id": project_id,
                "turn_id": turn.as_ref().map(|turn| turn.turn_id.as_str()),
                "user_content": turn.as_ref().map(|turn| turn.user_content.as_str()),
                "timeline": turn.as_ref().map(|turn| &turn.timeline),
                "reason": reason,
                "provider_request_count": provider_request_count,
            }),
            HookScope::Session,
        );
    }

    /// Records a completed compaction boundary.
    /// 记录一次已完成的压缩边界。
    pub fn record_compacted(
        &mut self,
        session_id: SessionId,
        project_id: Option<String>,
        trigger: &str,
        fallback_used: bool,
        provider_request_count: usize,
        summary: &str,
    ) {
        self.dispatch_json(
            HOOK_CONVERSATION_COMPACTED,
            session_id,
            json!({
                "session_id": session_id.to_string(),
                "project_id": project_id,
                "trigger": trigger,
                "fallback_used": fallback_used,
                "provider_request_count": provider_request_count,
                "summary": summary,
            }),
            HookScope::Session,
        );
    }

    /// Reconstructs a minimal turn from canonical state when active state is missing.
    /// 当活动状态缺失时从规范状态重建一个最小回合。
    pub(super) fn reconstruct_turn_from_state(
        &mut self,
        session_id: SessionId,
        conversation_state: &SessionConversationState,
    ) -> ActiveConversationTurn {
        let user_content = conversation_state
            .messages
            .iter()
            .rev()
            .find_map(|message| match &message.payload {
                SessionConversationPayload::UserText { text, .. } => Some(text.clone()),
                _ => None,
            })
            .unwrap_or_default();
        ActiveConversationTurn::new(new_conversation_turn_id(session_id), user_content)
    }

    /// Ensures runtime-only events still have a turn container.
    /// 确保仅运行时事件仍然拥有回合容器。
    fn ensure_turn_for_runtime_event(&mut self, session_id: SessionId) -> String {
        if let Some(turn) = self.active_turns.get(&session_id) {
            return turn.turn_id.clone();
        }
        let turn_id = new_conversation_turn_id(session_id);
        self.active_turns.insert(
            session_id,
            ActiveConversationTurn::new(turn_id.clone(), String::new()),
        );
        turn_id
    }

    /// Dispatches one JSON hook payload without blocking the UI thread.
    /// 在不阻塞 UI 线程的情况下调度一条 JSON hook payload。
    fn dispatch_json(
        &self,
        point: &'static str,
        session_id: SessionId,
        payload: Value,
        scope: HookScope,
    ) {
        let dispatcher = self.dispatcher.clone();
        let context = HookContext {
            run_id: format!("{point}:{session_id}"),
            point: point.to_string(),
            scope,
            actor: HookActor::new("vulcan-code-runtime"),
            workspace_id: None,
            project_id: None,
            session_id: Some(session_id.to_string()),
            conversation_id: Some(session_id.to_string()),
            user_id: None,
            capabilities: Default::default(),
            metadata: BTreeMap::new(),
        };
        let request = HookDispatchRequest::new(context, HookPayload { value: payload });
        spawn_hook_dispatch(dispatcher, request);
    }
}

/// Generates one collision-resistant runtime-only logical turn identifier.
/// 生成一个抗碰撞的仅运行期逻辑回合标识。
///
/// # Parameters
/// 参数
/// `session_id` is the exact session owner embedded for diagnostics.
/// `session_id` 是嵌入标识、用于诊断的精确会话归属。
///
/// # Returns
/// 返回值
/// A session-scoped UUID turn id that cannot silently repeat after numeric exhaustion.
/// 返回会话作用域的 UUID 回合标识，不会因数值耗尽而静默重复。
fn new_conversation_turn_id(session_id: SessionId) -> String {
    format!("session-{session_id}-turn-{}", uuid::Uuid::new_v4())
}
