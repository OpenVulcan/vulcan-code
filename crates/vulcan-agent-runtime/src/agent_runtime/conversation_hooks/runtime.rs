use super::*;

/// Independently dispatchable profile and precheck requests for one prompt turn.
/// 单个提示词回合中可独立派发的画像与 precheck 请求。
#[derive(Clone)]
pub struct ConversationPromptHookRequests {
    /// Inert user observation; the host releases it only after durable Hook publication.
    /// 尚不执行的用户观察；宿主仅在可靠发布 Hook 后释放。
    pub deferred_user_event: Option<ConversationUserEventRequest>,
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
    /// Drops only process-local preparation after the host proves that no Hook was ever published for this business.
    /// 宿主证明本业务从未发布 Hook 后，仅丢弃进程内准备。
    /// `source` must be freshly validated under submission exclusion; no observation or completion event is sent.
    /// `source` 必须在提交互斥内刚完成重验；不发送观察或完成事件。
    pub fn discard_unpublished_business_preparation(
        &mut self,
        source: &vulcan_session_core::v1::business_hook::InitialBusinessPreparation,
    ) -> Result<(), String> {
        // The domain source fixes the physical session; a display ID cannot choose another owner.
        // 领域来源固定物理会话；显示 ID 不能选择其他所有者。
        let session_id = source
            .session_id
            .parse::<SessionId>()
            .map_err(|error| format!("Invalid initial business session identity: {error}"))?;
        self.active_turns.remove(&session_id);
        Ok(())
    }

    /// Restores canonical input observations with their original Hook identity, without dispatching notifications.
    /// 将规范输入观察与原 Hook 身份一起恢复，不派发通知。
    /// `state` must come from validated durable recovery; conflicts leave all live state unchanged.
    /// `state` 必须来自已验证可靠恢复；冲突保持全部活动状态不变。
    /// Returns whether ownership or a missing canonical timeline item was installed; other timeline kinds are preserved.
    /// 返回是否安装归属或缺失规范时间线条目；其他时间线种类保持不变。
    pub fn restore_business_hook_state(
        &mut self,
        state: &vulcan_session_core::v1::business_hook::BusinessHookMetadata,
    ) -> Result<bool, String> {
        // Stage every validation and timeline change before publishing the process-local owner.
        // 发布进程内所有者之前暂存全部校验和时间线变更。
        let binding = state
            .binding
            .as_ref()
            .ok_or_else(|| String::from("Business Hook recovery state has no original binding."))?;
        state
            .validate(binding.segment_generation)
            .map_err(|error| error.to_string())?;
        let session_id =
            SessionId::parse_str(&binding.session_id).map_err(|error| error.to_string())?;
        let mut staged = self
            .active_turns
            .get(&session_id)
            .cloned()
            .unwrap_or_else(|| {
                ActiveConversationTurn::new(
                    binding.hook_turn_id.clone(),
                    binding.user_content.clone(),
                    binding.vmm_runtime_ready,
                )
                .with_user_observation(binding.observes_user)
            });
        if staged.turn_id != binding.hook_turn_id
            || staged.user_content != binding.user_content
            || staged.observes_user != binding.observes_user
        {
            return Err(String::from(
                "Business Hook recovery state conflicts with the retained owner.",
            ));
        }
        // Canonical order cannot be inferred from asynchronous callback arrival order.
        // 规范顺序不能从异步回调到达顺序推断。
        let mut last_position = None;
        let mut changed = !self.active_turns.contains_key(&session_id);
        for entry in &state.observed_inputs {
            if entry.turn_id != binding.turn_id {
                return Err(String::from(
                    "Observed Hook input belongs to a different business turn.",
                ));
            }
            // The admission kind owns raw-queue versus sanitized-interrupt projection, never role or text heuristics.
            // 准入种类决定原始队列或清理插队投影，绝不通过角色或正文猜测。
            let (kind, content) = match entry.kind {
                vulcan_session_core::v1::business_hook::BusinessHookInputKind::Queued => {
                    ("user_followup", entry.content.clone())
                }
                vulcan_session_core::v1::business_hook::BusinessHookInputKind::Interrupt => {
                    let (_, sanitized) = crate::agent_runtime::context_injection_runtime::collect_builtin_manual_injection_ops_and_sanitized_prompt(&entry.content);
                    ("user_interrupt", sanitized)
                }
            };
            let identity = (entry.input_id, entry.kind, entry.observed_revision);
            if staged
                .observed_input_identities
                .get(entry.client_action_id.as_str())
                .is_some_and(|prior| prior != &identity)
                || staged
                    .canonical_input_receipts
                    .get(entry.client_action_id.as_str())
                    .is_some_and(|prior| prior.0 != entry.input_id)
            {
                return Err(String::from(
                    "Observed Hook input changed its admitted identity.",
                ));
            }
            let expected = ConversationTimelineItem::new(
                "user",
                kind,
                content.trim().to_owned(),
                Some(entry.client_action_id.as_str().to_owned()),
            );
            let matches = staged
                .timeline
                .iter()
                .enumerate()
                .filter(|(_, item)| {
                    item.source_id.as_deref() == Some(entry.client_action_id.as_str())
                })
                .collect::<Vec<_>>();
            let position = if matches.is_empty() {
                // Append a missing observation only after all earlier canonical observations have been checked.
                // 仅在全部更早规范观察通过检查后追加缺失观察。
                staged.timeline.push(expected);
                changed = true;
                staged.timeline.len() - 1
            } else if matches.len() == 1 && matches[0].1 == &expected {
                matches[0].0
            } else {
                return Err(String::from(
                    "Observed Hook input conflicts with its retained timeline content.",
                ));
            };
            if last_position.is_some_and(|previous| position <= previous) {
                return Err(String::from(
                    "Observed Hook timeline order conflicts with canonical admission order.",
                ));
            }
            last_position = Some(position);
            changed |= staged
                .observed_input_identities
                .insert(entry.client_action_id.as_str().to_owned(), identity)
                .is_none();
            if entry.kind == vulcan_session_core::v1::business_hook::BusinessHookInputKind::Queued {
                changed |= staged
                    .recorded_prompt_entry_ids
                    .insert(entry.client_action_id.as_str().to_owned());
            }
        }
        if changed {
            self.active_turns.insert(session_id, staged);
        }
        Ok(changed)
    }

    /// Captures the actual in-memory Hook seed against host-proven V1 coordinates.
    /// 对照宿主已证明的 V1 坐标捕获实际内存 Hook 种子。
    /// The host supplies canonical session/generation/turn/start; missing ownership returns an error.
    /// 宿主提供规范会话、代次、回合及起点；缺失归属时返回错误。
    pub fn capture_business_hook_binding(
        &self,
        session_id: SessionId,
        segment_generation: u64,
        turn_id: vulcan_session_core::v1::TurnId,
        start_seq: vulcan_session_core::v1::Seq,
    ) -> Result<vulcan_session_core::v1::business_hook::BusinessHookBinding, String> {
        // The retained owner supplies original content; callers cannot replace it with a continuation prompt.
        // 保留所有者提供原正文；调用方不能将其替换为续跑提示词。
        let turn = self.active_turns.get(&session_id).ok_or_else(|| {
            String::from("Business Hook capture requires its original active owner.")
        })?;
        Ok(
            vulcan_session_core::v1::business_hook::BusinessHookBinding {
                session_id: session_id.to_string(),
                segment_generation,
                turn_id,
                start_seq,
                hook_turn_id: turn.turn_id.clone(),
                user_content: turn.user_content.clone(),
                vmm_runtime_ready: turn.vmm_runtime_ready,
                observes_user: turn.observes_user,
            },
        )
    }

    /// Restores only a host-validated durable Hook seed, without replaying profile or user-event hooks.
    /// 仅恢复宿主已校验的可靠 Hook 种子，不重放画像或用户事件 Hook。
    /// `binding` comes from full canonical recovery; returns true only when a missing owner is installed.
    /// `binding` 来自完整规范恢复；仅在安装缺失所有者时返回 true。
    /// Existing matching ownership preserves its live timeline; conflicts never replace it.
    /// 已有匹配归属保留活动时间线；冲突绝不替换它。
    pub fn restore_business_hook_binding(
        &mut self,
        binding: &vulcan_session_core::v1::business_hook::BusinessHookBinding,
    ) -> Result<bool, String> {
        // Parse and validate before allocating process-local ownership.
        // 分配进程内归属之前解析并校验。
        let session_id =
            SessionId::parse_str(&binding.session_id).map_err(|error| error.to_string())?;
        vulcan_session_core::v1::business_hook::BusinessHookMetadata {
            version: 1,
            binding: Some(binding.clone()),
            observed_inputs: Vec::new(),
        }
        .validate(binding.segment_generation)
        .map_err(|error| error.to_string())?;
        if let Some(existing) = self.active_turns.get(&session_id) {
            if existing.turn_id != binding.hook_turn_id
                || existing.user_content != binding.user_content
                || existing.observes_user != binding.observes_user
            {
                return Err(String::from(
                    "Business Hook restoration conflicts with the retained owner.",
                ));
            }
            return Ok(false);
        }
        self.active_turns.insert(
            session_id,
            ActiveConversationTurn::new(
                binding.hook_turn_id.clone(),
                binding.user_content.clone(),
                binding.vmm_runtime_ready,
            )
            .with_user_observation(binding.observes_user),
        );
        Ok(true)
    }

    /// Restores prepared Hook requests for an already-recorded logical user turn.
    /// 为已经记录的逻辑用户回合恢复已准备 Hook 请求。
    ///
    /// # Parameters
    /// 参数
    /// - `session_id`: Session that owns the restored logical turn.
    /// - `session_id`：拥有恢复后逻辑回合的会话。
    /// - `turn_id`: Exact logical turn identity captured at acceptance.
    /// - `turn_id`：接纳时捕获的精确逻辑回合身份。
    /// - `prompt`: Original immutable prompt used only to restore Hook timeline ownership.
    /// - `prompt`：仅用于恢复 Hook 时间线所有权的原始不可变提示词。
    /// - `precheck_payload`: Exact precheck payload captured before interruption.
    /// - `precheck_payload`：中断前捕获的精确 precheck 载荷。
    /// - `profile_payload`: Exact profile payload captured before interruption.
    /// - `profile_payload`：中断前捕获的精确画像载荷。
    ///
    /// # Returns
    /// 返回值
    /// Prepared sibling Hook requests without redispatching the original user event.
    /// 不重复派发原始用户事件的已准备同级 Hook 请求。
    pub fn restore_user_prompt_hook_requests(
        &mut self,
        session_id: SessionId,
        turn_id: String,
        prompt: &str,
        precheck_payload: Value,
        profile_payload: Value,
    ) -> Result<ConversationPromptHookRequests, String> {
        // Missing readiness is deliberately treated as stopped for legacy retry payloads.
        // 旧重试载荷缺失就绪事实时按未启动处理，禁止恢复时误触发 VMM。
        let vmm_runtime_ready = precheck_payload
            .get("vmm_runtime_ready")
            .and_then(Value::as_bool)
            == Some(true);
        match self.active_turns.get(&session_id) {
            Some(active_turn)
                if active_turn.turn_id != turn_id || active_turn.user_content != prompt =>
            {
                return Err(String::from(
                    "ERR_CONVERSATION_CONTINUE_RETRY_LOGICAL_TURN_CHANGED",
                ));
            }
            Some(_) => {}
            None => {
                self.active_turns.insert(
                    session_id,
                    ActiveConversationTurn::new(
                        turn_id.clone(),
                        prompt.to_string(),
                        vmm_runtime_ready,
                    ),
                );
            }
        }
        Ok(ConversationPromptHookRequests {
            deferred_user_event: None,
            profile: ConversationProfileRequest::new(
                self.dispatcher.clone(),
                session_id,
                profile_payload,
                turn_id.clone(),
                self.permits_user_observation(session_id),
            ),
            precheck: ConversationPrecheckRequest::new(
                self.dispatcher.clone(),
                session_id,
                precheck_payload,
                turn_id.clone(),
            ),
            turn_id,
        })
    }

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
        vmm_runtime_ready: bool,
    ) -> ConversationPromptHookRequests {
        self.prepare_prompt_hook_requests_with_timeline_policy(
            session_id,
            prompt_entry_id,
            project_id,
            workspace_path,
            source,
            system_prompt_hook_phase,
            prompt,
            conversation_state,
            vmm_runtime_ready,
            false,
        )
    }

    /// Assembles the existing preparation payloads with an explicit timeline ownership policy.
    /// 使用明确时间线归属策略组装既有准备载荷。
    /// `timeline_prepared` means a validated canonical batch already owns user events and timeline mutation.
    /// `timeline_prepared` 表示已验证规范批次已经持有用户事件及时间线修改。
    /// All other parameters retain the public prompt preparation contract; returns sibling Hook requests.
    /// 其余参数沿用公开提示准备契约；返回同级 Hook 请求。
    pub(super) fn prepare_prompt_hook_requests_with_timeline_policy(
        &mut self,
        session_id: SessionId,
        prompt_entry_id: &str,
        project_id: Option<String>,
        workspace_path: String,
        source: PendingSessionPromptSource,
        system_prompt_hook_phase: SystemPromptHookPhase,
        prompt: &str,
        conversation_state: &SessionConversationState,
        vmm_runtime_ready: bool,
        timeline_prepared: bool,
    ) -> ConversationPromptHookRequests {
        let event_kind = pending_prompt_source_event_kind(source);
        // Auxiliary preparation has its own durable execution identity and never owns a business Hook turn.
        // 辅助准备使用独立的持久执行身份，绝不持有业务 Hook 回合。
        let is_question_execution = source == PendingSessionPromptSource::QuestionExecution;
        // Continuations inherit the restored origin; a new agent task cannot become a user observation.
        // 续跑继承恢复后的来源；新代理任务不能成为用户观察。
        let observes_user = source != PendingSessionPromptSource::AgentTask
            && self.permits_user_observation(session_id);
        // Host, Goal and child-return continuation prompts are dispatch instructions, never user observations.
        // 宿主、Goal 和子回流续跑提示属于分发指令，绝不是用户观察。
        let is_host_continuation = matches!(
            source,
            PendingSessionPromptSource::AgentContinuation
                | PendingSessionPromptSource::GoalContinuation
                | PendingSessionPromptSource::SubagentReturnContinuation
        );
        let was_recorded_while_queued = !is_question_execution
            && self
                .active_turns
                .get(&session_id)
                .is_some_and(|turn| turn.recorded_prompt_entry_ids.contains(prompt_entry_id));
        let is_new_turn = !is_question_execution && !self.active_turns.contains_key(&session_id);
        let turn_id = if is_question_execution {
            format!("session-{session_id}-question-execution-{prompt_entry_id}")
        } else if is_new_turn {
            let turn_id = new_conversation_turn_id(session_id);
            self.active_turns.insert(
                session_id,
                ActiveConversationTurn::new(turn_id.clone(), prompt.to_string(), vmm_runtime_ready)
                    .with_user_observation(observes_user),
            );
            turn_id
        } else if was_recorded_while_queued || is_host_continuation || timeline_prepared {
            self.active_turns
                .get(&session_id)
                .map(|turn| turn.turn_id.clone())
                .unwrap_or_default()
        } else {
            let turn = self
                .active_turns
                .get_mut(&session_id)
                .expect("active turn should exist after contains_key");
            turn.vmm_runtime_ready = vmm_runtime_ready;
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
            "project_id": project_id,
            "workspace_path": workspace_path,
            "turn_id": turn_id,
            "event_kind": event_kind,
            "is_new_turn": is_new_turn,
            "was_recorded_while_queued": was_recorded_while_queued,
            "canonical_batch_timeline_prepared": timeline_prepared,
            "user_content": prompt,
            "visible_history_count": visible_history_count(conversation_state),
            "message_count": conversation_state.messages.len(),
            "record_count": conversation_state.records.len(),
            "vmm_runtime_ready": vmm_runtime_ready,
        });
        // Preparing identity must not notify external observers before the host makes that identity durable.
        // 宿主使身份可靠落盘之前，准备身份不能通知外部观察器。
        let deferred_user_event = (!is_question_execution
            && observes_user
            && !is_host_continuation
            && !was_recorded_while_queued
            && !timeline_prepared)
            .then(|| {
                ConversationUserEventRequest::new(
                    self.dispatcher.clone(),
                    session_id,
                    precheck_payload.clone(),
                    turn_id.clone(),
                )
            });
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
            "vmm_runtime_ready": vmm_runtime_ready,
        });
        ConversationPromptHookRequests {
            deferred_user_event,
            profile: ConversationProfileRequest::new(
                self.dispatcher.clone(),
                session_id,
                profile_payload,
                turn_id.clone(),
                observes_user,
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
            true,
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
        vmm_runtime_ready: bool,
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
            "vmm_runtime_ready": turn.vmm_runtime_ready && vmm_runtime_ready,
            "conversation_state": conversation_state,
        });
        (
            completed_turn_id.clone(),
            ConversationCompletionRequest::new(
                self.dispatcher.clone(),
                session_id,
                payload,
                completed_turn_id,
                turn.observes_user,
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
        if turn.as_ref().is_some_and(|turn| !turn.observes_user) {
            return;
        }
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
        vmm_runtime_ready: bool,
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
                "vmm_runtime_ready": vmm_runtime_ready,
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
        ActiveConversationTurn::new(new_conversation_turn_id(session_id), user_content, false)
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
            ActiveConversationTurn::new(turn_id.clone(), String::new(), false),
        );
        turn_id
    }

    /// Dispatches one JSON hook payload without blocking the UI thread.
    /// 在不阻塞 UI 线程的情况下调度一条 JSON hook payload。
    pub(super) fn dispatch_json(
        &self,
        point: &'static str,
        session_id: SessionId,
        payload: Value,
        scope: HookScope,
    ) {
        // Share immutable host context construction with deferred first-prompt observations.
        // 与延迟首次提示观察共用不可变宿主上下文构建。
        let request = super::user_event::prepared_hook_dispatch(point, session_id, payload, scope);
        spawn_hook_dispatch(self.dispatcher.clone(), request);
    }

    /// Returns the retained turn's observer policy; an unowned session keeps normal non-turn hook behavior.
    /// 返回保留回合的观察策略；无回合所有者的会话保留正常的非回合钩子行为。
    pub fn permits_user_observation(&self, session_id: SessionId) -> bool {
        self.active_turns
            .get(&session_id)
            .is_none_or(|turn| turn.observes_user)
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
