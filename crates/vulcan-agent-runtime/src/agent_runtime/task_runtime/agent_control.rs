use super::*;
use crate::agent_runtime::provider_request_bridge_conversation::core_snapshot_to_session_conversation_state;
use crate::agent_runtime::runtime_stream_intervention::{
    RuntimePromptIntervention, RuntimePromptInterventionError,
};
use vulcan_core::controller::{
    AgentMessageAppendRequest, DisplayRecordAppendRequest, VulcanCoreController,
};
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationRecord as CoreConversationRecord, TranscriptRecordKind as CoreTranscriptRecordKind,
    TranscriptRole as CoreTranscriptRole,
};

/// Delivery mode selected by send_message.
/// `send_message` 选择的投递模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InterAgentDeliveryMode {
    /// Queue the message behind the current task.
    /// 将消息排在当前任务之后。
    Queue,
    /// Interrupt the current task before scheduling the message.
    /// 在调度消息前中断当前任务。
    Interrupt,
}

impl InterAgentDeliveryMode {
    /// Returns whether this delivery mode is an interrupt request.
    /// 返回该投递模式是否为插队中断请求。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the caller asked to interrupt current work.
    /// 调用方请求中断当前工作时返回 `true`。
    const fn interrupts_current_task(self) -> bool {
        matches!(self, Self::Interrupt)
    }

    /// Returns the stable wire value for this delivery mode.
    /// 返回该投递模式的稳定线协议值。
    ///
    /// # Returns
    /// 返回值
    /// The model-visible delivery_mode value.
    /// 模型可见的 delivery_mode 值。
    const fn as_str(self) -> &'static str {
        match self {
            Self::Queue => "queue",
            Self::Interrupt => "interrupt",
        }
    }
}

/// Result of immediately resuming a child agent for one delivered message.
/// 单次消息投递后立即续跑子代理得到的结果。
#[derive(Debug, Clone)]
struct InterAgentImmediateResumeResult {
    /// Concrete child execution id that processed the delivered message.
    /// 处理本次投递消息的具体子执行标识。
    execution_id: String,
    /// Status produced by that resumed child execution.
    /// 该次子执行产出的状态。
    execution_status: TaskExecutionStatus,
    /// Final or status text returned by the resumed child execution.
    /// 该次子执行返回的最终文本或状态文本。
    result_text: String,
}

impl InterAgentImmediateResumeResult {
    /// Returns whether the resumed execution already reached a terminal status.
    /// 返回本次续跑执行是否已经到达终态。
    const fn is_terminal(&self) -> bool {
        matches!(
            self.execution_status,
            TaskExecutionStatus::Completed | TaskExecutionStatus::Stopped
        )
    }
}

/// Structured failure produced after restoring messages drained from one child inbox.
/// 恢复从子代理 inbox 取出的消息后产生的结构化失败。
#[derive(Debug, Eq, PartialEq)]
pub(super) enum InterAgentMessageRestoreError {
    /// The original operation failed but inbox restoration succeeded.
    /// 原始操作失败，但 inbox 恢复成功。
    Operation {
        /// Original operation error.
        /// 原始操作错误。
        message: String,
    },
    /// Both the original operation and inbox restoration failed.
    /// 原始操作与 inbox 恢复均失败。
    Recovery {
        /// Original operation error.
        /// 原始操作错误。
        operation_error: String,
        /// Inbox restoration error.
        /// inbox 恢复错误。
        recovery_error: String,
    },
}

impl std::fmt::Display for InterAgentMessageRestoreError {
    /// Formats one restore outcome for existing offline resume diagnostics.
    /// 为既有离线续跑诊断格式化一次恢复结果。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Operation { message } => formatter.write_str(message),
            Self::Recovery {
                operation_error,
                recovery_error,
            } => write!(
                formatter,
                "{operation_error} Additionally, failed to restore queued inter-agent messages: {recovery_error}"
            ),
        }
    }
}

impl TaskRuntime {
    /// Sends one message to a known child agent.
    /// 向一个已知子代理发送一条消息。
    pub async fn invoke_send_message(
        self: &Arc<Self>,
        current_session_id: SessionId,
        args: Value,
    ) -> Result<VulcanToolInvocationResult, String> {
        self.invoke_inter_agent_message(current_session_id, args)
            .await
    }

    /// Handles the shared inter-agent message submission flow.
    /// 处理共享的代理间消息提交流程。
    ///
    /// # Parameters
    /// 参数
    /// `current_session_id` is the session invoking the tool.
    /// `current_session_id` 是发起工具调用的会话。
    /// `args` contains the strict `target` and `message` payload.
    /// `args` 包含严格的 `target` 与 `message` 载荷。
    ///
    /// # Returns
    /// 返回值
    /// A tool result whose host payload preserves the structured delivery record.
    /// 工具结果，其宿主载荷保留结构化投递记录。
    async fn invoke_inter_agent_message(
        self: &Arc<Self>,
        current_session_id: SessionId,
        args: Value,
    ) -> Result<VulcanToolInvocationResult, String> {
        let parsed = parse_inter_agent_message_args(&args, SEND_MESSAGE_TOOL)?;
        if is_root_agent_target(&parsed.target) {
            if parsed.delivery_mode.interrupts_current_task() {
                return Err(String::from(
                    "send_message delivery_mode `interrupt` cannot target the root agent.",
                ));
            }
            return self.deliver_root_inter_agent_message(current_session_id, &parsed.message);
        }
        let target = self.resolve_inter_agent_message_target(current_session_id, &parsed.target)?;
        let target_session_id = SessionId::from(target.session_id);
        let stopped_task_ids = if parsed.delivery_mode.interrupts_current_task() {
            self.stop_task_tree(target_session_id)?
        } else {
            Vec::new()
        };
        let transaction_lock = self.task_execution_state_lock(target_session_id)?;
        let transaction_guard = transaction_lock.lock().map_err(|_| {
            String::from(
                "The task execution transaction is locked and cannot admit the inter-agent message.",
            )
        })?;
        let admitted_target =
            self.resolve_inter_agent_message_target(current_session_id, &parsed.target)?;
        if admitted_target.session_id != target.session_id {
            return Err(format!(
                "Child agent `{}` changed from session `{}` to `{}` before message admission.",
                target.agent_id, target.session_id, admitted_target.session_id
            ));
        }
        let delivery_id = self.enqueue_inter_agent_message(
            current_session_id,
            target_session_id,
            admitted_target.agent_id.clone(),
            parsed.message.clone(),
        )?;
        let should_resume = !self.task_has_active_or_starting_run(target_session_id)?;
        drop(transaction_guard);

        let immediate_resume = if should_resume {
            self.resume_idle_agent_from_inbox(current_session_id, &admitted_target)
                .await?
        } else {
            None
        };

        Ok(build_inter_agent_message_tool_result(
            SEND_MESSAGE_TOOL,
            current_session_id,
            &admitted_target,
            &parsed.message,
            &delivery_id,
            parsed.delivery_mode,
            &stopped_task_ids,
            immediate_resume.as_ref(),
        ))
    }

    /// Delivers one child-authored message to its direct parent mailbox.
    /// 将一条子代理发出的消息投递到其直接父级 mailbox。
    ///
    /// # Parameters
    /// 参数
    /// `current_session_id` is the child-agent session that sends the message.
    /// `current_session_id` 是发送消息的子代理会话。
    /// `message` is the message body to expose to the parent.
    /// `message` 是需要暴露给父级的消息正文。
    ///
    /// # Returns
    /// 返回值
    /// A structured send_message result for the root target.
    /// 面向 root 目标的结构化 `send_message` 结果。
    fn deliver_root_inter_agent_message(
        &self,
        current_session_id: SessionId,
        message: &str,
    ) -> Result<VulcanToolInvocationResult, String> {
        let record = self
            .sessions
            .read()
            .map_err(|_| String::from("The task-session registry is locked."))?
            .get(&current_session_id)
            .cloned()
            .ok_or_else(|| String::from("The root target can only be used from a child agent."))?;
        let parent_session_id = record.parent_task_id.unwrap_or(record.origin_session_id);
        if parent_session_id == current_session_id {
            return Err(String::from(
                "The root target cannot resolve to the current agent.",
            ));
        }
        let event = SubagentMailboxEvent {
            parent_thread_id: SessionNodeId::from(parent_session_id),
            parent_turn_id: None,
            child_thread_id: SessionNodeId::from(current_session_id),
            kind: SubagentMailboxEventKind::Message,
            timestamp_ms: current_unix_millis_u64(),
            payload: json!({
                "child_thread_id": current_session_id.to_string(),
                "public_id": record.public_task_id,
                "parent_thread_id": parent_session_id.to_string(),
                "root_session_id": record.origin_session_id.to_string(),
                "parent_task_id": record.parent_task_id.map(|value| value.to_string()),
                "status": "message",
                "task_status": "message",
                "description": record.description,
                "display_title": record.display_title,
                "depth": record.depth,
                "prompt": record.last_prompt,
                "result_text": message,
                "error_text": Value::Null,
            }),
        };
        self.subagent_mailbox.push_event(event)?;
        self.notify_subagent_mailbox_activity();
        Ok(build_root_inter_agent_message_tool_result(
            current_session_id,
            parent_session_id,
            message,
        ))
    }

    /// Resolves one model-provided target against the native subagent registry.
    /// 将模型提供的目标解析到原生子代理注册表。
    ///
    /// # Parameters
    /// 参数
    /// `current_session_id` is the direct parent session that owns the tool call.
    /// `current_session_id` 是拥有工具调用的直接父会话。
    /// `target` is the public child-agent id.
    /// `target` 是公开子代理标识。
    ///
    /// # Returns
    /// 返回值
    /// The exact known child-agent registry entry.
    /// 精确匹配的已知子代理注册表条目。
    fn resolve_inter_agent_message_target(
        &self,
        current_session_id: SessionId,
        target: &str,
    ) -> Result<SubagentRegistryEntry, String> {
        let normalized_target = normalize_public_task_identifier(target)?;
        let registry = self.subagent_registry_snapshot()?;
        let parent_session_id = SessionNodeId::from(current_session_id);
        let entry = registry
            .get_for_parent(parent_session_id, &normalized_target)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Unknown child agent `{normalized_target}` for parent session `{current_session_id}`."
                )
            })?;
        if matches!(entry.status, SubagentStatus::Archived) {
            return Err(format!(
                "Child agent `{normalized_target}` is archived and cannot receive messages."
            ));
        }
        Ok(entry)
    }

    /// Queues one inter-agent message for a child-agent inbox.
    /// 将一条代理间消息排入子代理 inbox。
    ///
    /// # Parameters
    /// 参数
    /// `author_session_id` is the sender session id.
    /// `author_session_id` 是发送方会话标识。
    /// `target_task_id` is the target child task id.
    /// `target_task_id` 是目标子任务标识。
    /// `target_agent_id` is the caller-facing target child-agent id.
    /// `target_agent_id` 是调用方面向的目标智能体标识。
    /// `content` is the message body.
    /// `content` 是消息正文。
    fn enqueue_inter_agent_message(
        &self,
        author_session_id: SessionId,
        target_task_id: SessionId,
        target_agent_id: String,
        content: String,
    ) -> Result<String, String> {
        let created_at_ms = current_unix_millis_u64();
        let delivery_id = format!("subagent-message-{target_task_id}-{}", Uuid::new_v4());
        let message = QueuedInterAgentMessage {
            delivery_id: delivery_id.clone(),
            author_session_id,
            target_task_id,
            target_agent_id,
            content,
            created_at_ms,
        };
        self.agent_inboxes
            .write()
            .map_err(|_| String::from("The subagent agent-control inbox is locked."))?
            .entry(target_task_id)
            .or_default()
            .push_back(message);
        Ok(delivery_id)
    }

    /// Takes every queued message for one child-agent inbox.
    /// 取出单个子代理 inbox 中的所有排队消息。
    ///
    /// # Parameters
    /// 参数
    /// `target_task_id` is the target child task id.
    /// `target_task_id` 是目标子任务标识。
    ///
    /// # Returns
    /// 返回值
    /// Messages in FIFO order.
    /// 按 FIFO 顺序返回的消息。
    fn take_inter_agent_messages(
        &self,
        target_task_id: SessionId,
    ) -> Result<Vec<QueuedInterAgentMessage>, String> {
        let mut inboxes = self
            .agent_inboxes
            .write()
            .map_err(|_| String::from("The subagent agent-control inbox is locked."))?;
        Ok(inboxes
            .remove(&target_task_id)
            .unwrap_or_default()
            .into_iter()
            .collect())
    }

    /// Requeues messages at the front when a resume attempt fails.
    /// 当续跑尝试失败时，将消息重新排回队首。
    ///
    /// # Parameters
    /// 参数
    /// `target_task_id` is the target child task id.
    /// `target_task_id` 是目标子任务标识。
    /// `messages` are messages that must be restored.
    /// `messages` 是必须恢复的消息集合。
    fn requeue_inter_agent_messages_front(
        &self,
        target_task_id: SessionId,
        messages: Vec<QueuedInterAgentMessage>,
    ) -> Result<(), String> {
        if messages.is_empty() {
            return Ok(());
        }
        let mut inboxes = self
            .agent_inboxes
            .write()
            .map_err(|_| String::from("The subagent agent-control inbox is locked."))?;
        let inbox = inboxes.entry(target_task_id).or_default();
        for message in messages.into_iter().rev() {
            inbox.push_front(message);
        }
        Ok(())
    }

    /// Restores drained inter-agent messages after a failed resume stage and returns a combined error.
    /// 在续跑阶段失败后恢复已取出的代理间消息，并返回合并后的错误。
    ///
    /// # Parameters
    /// 参数
    /// `target_task_id` identifies the child-agent inbox that must receive the messages again.
    /// `target_task_id` 标识必须重新接收消息的子代理 inbox。
    /// `messages` are the drained messages that must not be lost.
    /// `messages` 是已经取出且不能丢失的消息集合。
    /// `stage` names the failed resume stage for diagnostics.
    /// `stage` 是用于诊断的失败续跑阶段名称。
    /// `error` is the original failure that triggered restoration.
    /// `error` 是触发恢复动作的原始失败信息。
    ///
    /// # Returns
    /// 返回值
    /// A structured original-operation or recovery failure after diagnostics are emitted.
    /// 发出诊断后返回结构化的原操作失败或恢复失败。
    pub(super) fn restore_inter_agent_messages_front_after_failure(
        &self,
        target_task_id: SessionId,
        messages: Vec<QueuedInterAgentMessage>,
        stage: &str,
        error: String,
    ) -> InterAgentMessageRestoreError {
        let message_count = messages.len();
        let transaction_lock = match self.task_execution_state_lock(target_task_id) {
            Ok(transaction_lock) => transaction_lock,
            Err(lock_error) => {
                return InterAgentMessageRestoreError::Recovery {
                    operation_error: error,
                    recovery_error: format!(
                        "cannot coordinate recovery of {message_count} queued inter-agent message(s) for child agent `{target_task_id}` after `{stage}`: {lock_error}"
                    ),
                };
            }
        };
        let _transaction_guard = match transaction_lock.lock() {
            Ok(transaction_guard) => transaction_guard,
            Err(_) => {
                return InterAgentMessageRestoreError::Recovery {
                    operation_error: error,
                    recovery_error: format!(
                        "cannot acquire the task transaction lock to recover {message_count} queued inter-agent message(s) for child agent `{target_task_id}` after `{stage}`"
                    ),
                };
            }
        };
        let target_status = match self.subagent_registry.read() {
            Ok(registry) => registry
                .list_all()
                .into_iter()
                .find(|entry| entry.session_id == SessionNodeId::from(target_task_id))
                .map(|entry| entry.status),
            Err(_) => {
                return InterAgentMessageRestoreError::Recovery {
                    operation_error: error,
                    recovery_error: format!(
                        "cannot determine whether child agent `{target_task_id}` still owns {message_count} queued inter-agent message(s) after `{stage}` because the subagent registry is locked"
                    ),
                };
            }
        };
        if target_status.is_none_or(|status| status == SubagentStatus::Archived) {
            let discarded_error = format!(
                "{error} The {message_count} in-flight inter-agent message(s) were discarded because child agent `{target_task_id}` is no longer open."
            );
            task_runtime_trace(
                "subagent_message_discard_after_archived_failure",
                format!("target={target_task_id} stage={stage} message_count={message_count}"),
            );
            return InterAgentMessageRestoreError::Operation {
                message: discarded_error,
            };
        }
        match self.requeue_inter_agent_messages_front(target_task_id, messages) {
            Ok(()) => {
                task_runtime_trace(
                    "subagent_message_requeue_after_failure",
                    format!("target={target_task_id} stage={stage} message_count={message_count}"),
                );
                InterAgentMessageRestoreError::Operation { message: error }
            }
            Err(restore_error) => {
                let combined_error = format!(
                    "{error} Additionally, failed to restore {message_count} queued inter-agent message(s) for child agent `{target_task_id}` after `{stage}`: {restore_error}"
                );
                eprintln!("[subagent-agent-control] {combined_error}");
                task_runtime_trace(
                    "subagent_message_requeue_failed",
                    format!(
                        "target={target_task_id} stage={stage} message_count={message_count} error={restore_error}"
                    ),
                );
                InterAgentMessageRestoreError::Recovery {
                    operation_error: error,
                    recovery_error: format!(
                        "failed to restore {message_count} queued inter-agent message(s) for child agent `{target_task_id}` after `{stage}`: {restore_error}"
                    ),
                }
            }
        }
    }

    /// Counts queued inter-agent messages for one target task.
    /// 统计一个目标任务当前排队的代理间消息数量。
    ///
    /// # Parameters
    /// 参数
    /// `target_task_id` identifies the target child-agent inbox.
    /// `target_task_id` 标识目标子代理 inbox。
    ///
    /// # Returns
    /// 返回值
    /// Number of queued messages currently waiting for delivery.
    /// 当前等待投递的消息数量。
    pub(super) fn inter_agent_message_count_for_task(
        &self,
        target_task_id: SessionId,
    ) -> Result<usize, String> {
        let inboxes = self
            .agent_inboxes
            .read()
            .map_err(|_| String::from("The subagent agent-control inbox is locked."))?;
        Ok(inboxes.get(&target_task_id).map(VecDeque::len).unwrap_or(0))
    }

    /// Counts queued inter-agent messages for direct children of one parent session.
    /// 统计单个父会话直接子代理的待处理代理间消息数量。
    pub(super) fn queued_inter_agent_message_count_for_parent(
        &self,
        parent_session_id: SessionId,
    ) -> Result<usize, String> {
        let child_task_ids = self
            .sessions
            .read()
            .map_err(|_| String::from("The task-session registry is locked."))?
            .values()
            .filter(|record| {
                record
                    .parent_task_id
                    .map(|value| value == parent_session_id)
                    .unwrap_or(record.origin_session_id == parent_session_id)
            })
            .map(|record| record.task_id)
            .collect::<BTreeSet<_>>();
        let inboxes = self
            .agent_inboxes
            .read()
            .map_err(|_| String::from("The subagent agent-control inbox is locked."))?;
        Ok(child_task_ids
            .iter()
            .filter_map(|task_id| inboxes.get(task_id))
            .map(VecDeque::len)
            .sum())
    }

    /// Resumes one idle child agent by converting queued inbox messages into the next user turn.
    /// 将排队 inbox 消息转换成下一条用户回合以续跑一个空闲子代理。
    ///
    /// # Parameters
    /// 参数
    /// `current_session_id` is the direct parent that invoked send_message.
    /// `current_session_id` 是调用 `send_message` 的直接父级。
    /// `target` is the known target child-agent entry.
    /// `target` 是已知目标子代理条目。
    async fn resume_idle_agent_from_inbox(
        self: &Arc<Self>,
        current_session_id: SessionId,
        target: &SubagentRegistryEntry,
    ) -> Result<Option<InterAgentImmediateResumeResult>, String> {
        let backend = self
            .backend_for_session(current_session_id)
            .ok_or_else(|| {
                format!(
                    "The current session `{}` does not have a task backend installed, so the child agent cannot resume.",
                    current_session_id
                )
            })?;
        let target_task_id = SessionId::from(target.session_id);
        let messages = self.take_inter_agent_messages(target_task_id)?;
        if messages.is_empty() {
            return Ok(None);
        }
        let prompt = format_inter_agent_messages_for_model(&messages);
        let parsed_args = match parse_task_tool_arguments(json!({
            "task_id": target.agent_id,
            "prompt": prompt,
        })) {
            Ok(parsed_args) => parsed_args,
            Err(error) => {
                return Err(self
                    .restore_inter_agent_messages_front_after_failure(
                        target_task_id,
                        messages,
                        "parse_task_tool_arguments",
                        error,
                    )
                    .to_string());
            }
        };
        let resolved_identifier = ResolvedTaskSessionIdentifier {
            task_id: target_task_id,
            public_task_id: target.agent_id.clone(),
            is_resumed: true,
            persisted_metadata: None,
        };
        let mut invocation = match prepare_task_execution_request(
            self,
            current_session_id,
            resolved_identifier,
            parsed_args,
        ) {
            Ok(invocation) => invocation,
            Err(error) => {
                return Err(self
                    .restore_inter_agent_messages_front_after_failure(
                        target_task_id,
                        messages,
                        "prepare_task_execution_request",
                        error,
                    )
                    .to_string());
            }
        };
        if let Err(error) =
            append_inter_agent_messages_to_snapshot(&mut invocation.snapshot, &messages)
        {
            return Err(self
                .restore_inter_agent_messages_front_after_failure(
                    target_task_id,
                    messages,
                    "append_inter_agent_messages_to_snapshot",
                    error,
                )
                .to_string());
        }
        let outcome = match backend(invocation.clone()).await {
            Ok(outcome) => outcome,
            Err(error) => {
                let restored_error = self
                    .restore_inter_agent_messages_front_after_failure(
                        target_task_id,
                        messages,
                        "backend_invocation",
                        error,
                    )
                    .to_string();
                if let Err(mark_error) =
                    self.mark_task_failure(current_session_id, &invocation, &restored_error)
                {
                    return Err(format!(
                        "{restored_error} Additionally, failed to mark the child agent failure: {mark_error}"
                    ));
                }
                return Err(restored_error);
            }
        };
        if outcome.execution_status == TaskExecutionStatus::Running {
            self.store_running_task_outcome_in_memory(current_session_id, &invocation, &outcome)?;
        } else {
            self.store_task_outcome(current_session_id, &invocation, &outcome)?;
        }
        Ok(Some(InterAgentImmediateResumeResult {
            execution_id: invocation.execution_id,
            execution_status: outcome.execution_status,
            result_text: outcome.final_output_text,
        }))
    }

    /// Resumes a just-settled child agent when queue-mode messages arrived too late for in-turn injection.
    /// 当 queue 模式消息来得太晚、无法在当前 turn 内注入时，续跑刚结束的子代理。
    ///
    /// # Parameters
    /// 参数
    /// `current_session_id` is the direct parent that owns the target child agent.
    /// `current_session_id` 是持有目标子代理的直接父级。
    /// `target_task_id` identifies the child agent whose active turn has just settled.
    /// `target_task_id` 标识活动 turn 刚刚结束的子代理。
    ///
    /// # Returns
    /// 返回值
    /// `true` when queued messages were converted into a new child-agent run.
    /// 当排队消息已经转换成新的子代理运行时返回 `true`。
    pub(super) async fn resume_agent_from_inbox_after_task_settled(
        self: &Arc<Self>,
        current_session_id: SessionId,
        target_task_id: SessionId,
    ) -> Result<bool, String> {
        if self.task_has_active_or_starting_run(target_task_id)? {
            return Ok(false);
        }
        if self.inter_agent_message_count_for_task(target_task_id)? == 0 {
            return Ok(false);
        }
        let target = self
            .subagent_registry
            .read()
            .map_err(|_| String::from("The subagent registry is locked."))?
            .list_for_parent(SessionNodeId::from(current_session_id))
            .into_iter()
            .find(|entry| entry.session_id == SessionNodeId::from(target_task_id))
            .cloned()
            .ok_or_else(|| {
                format!(
                    "The child agent `{}` is no longer registered and cannot resume queued messages.",
                    target_task_id
                )
            })?;
        if matches!(target.status, SubagentStatus::Archived) {
            return Ok(false);
        }
        self.resume_idle_agent_from_inbox(current_session_id, &target)
            .await?;
        Ok(true)
    }
}

/// Builds a runtime intervention that injects queued messages into one running child agent.
/// 构建一个运行时干预钩子，将排队消息注入到单个运行中子代理。
///
/// # Parameters
/// 参数
/// `task_runtime` owns the shared agent-control inbox.
/// `task_runtime` 持有共享的 AgentControl inbox。
/// `task_id` identifies the child agent whose stream reached a safe boundary.
/// `task_id` 标识到达安全边界的子代理。
///
/// # Returns
/// 返回值
/// A prompt intervention closure for the child runtime stream.
/// 用于子运行时流的提示词干预闭包。
pub(super) fn build_child_agent_prompt_intervention(
    task_runtime: Arc<TaskRuntime>,
    task_id: SessionId,
) -> RuntimePromptIntervention {
    Arc::new(move |_session_id, controller| {
        let messages = task_runtime
            .take_inter_agent_messages(task_id)
            .map_err(|message| RuntimePromptInterventionError::StateAccess { message })?;
        if messages.is_empty() {
            return Ok(false);
        }
        // controller_draft keeps provider-visible and display projections atomic on identity failure.
        // controller_draft 在身份失败时保持 Provider 可见投影与显示投影的原子性。
        let mut controller_draft = controller.clone();
        append_inter_agent_agent_messages_to_controller(&mut controller_draft, &messages);
        if let Err(error) =
            append_inter_agent_display_records_to_controller(&mut controller_draft, &messages)
        {
            let operation_error = format!("Failed to append child display record: {error}");
            let restore_error = task_runtime.restore_inter_agent_messages_front_after_failure(
                task_id,
                messages,
                "append_child_display_record",
                operation_error,
            );
            return Err(match restore_error {
                InterAgentMessageRestoreError::Operation { message } => {
                    RuntimePromptInterventionError::ProjectionConflict { message }
                }
                InterAgentMessageRestoreError::Recovery {
                    operation_error,
                    recovery_error,
                } => RuntimePromptInterventionError::RecoveryFailure {
                    operation_error,
                    recovery_error,
                },
            });
        }
        *controller = controller_draft;
        Ok(true)
    })
}

/// Strict parsed arguments for send_message.
/// `send_message` 的严格参数。
struct InterAgentMessageArgs {
    /// Target public child-agent id.
    /// 目标公开子代理标识。
    target: String,
    /// Message body.
    /// 消息正文。
    message: String,
    /// Delivery mode requested by the caller.
    /// 调用方请求的投递模式。
    delivery_mode: InterAgentDeliveryMode,
}

/// Parses strict inter-agent message arguments.
/// 解析严格的代理间消息参数。
///
/// # Parameters
/// 参数
/// `args` is the raw JSON tool payload.
/// `args` 是原始 JSON 工具载荷。
/// `tool_name` is used in user-facing error text.
/// `tool_name` 用于面向用户的错误文本。
///
/// # Returns
/// 返回值
/// Parsed target and message values.
/// 解析出的目标与消息值。
fn parse_inter_agent_message_args(
    args: &Value,
    tool_name: &str,
) -> Result<InterAgentMessageArgs, String> {
    let Value::Object(map) = args else {
        return Err(format!("{tool_name} arguments must be a JSON object."));
    };
    let unsupported_keys = map
        .keys()
        .filter(|key| !matches!(key.as_str(), "target" | "message" | "delivery_mode"))
        .cloned()
        .collect::<Vec<_>>();
    if !unsupported_keys.is_empty() {
        return Err(format!(
            "{tool_name} received unsupported keys: {}",
            unsupported_keys.join(", ")
        ));
    }
    let target = string_field(map, &["target"])
        .ok_or_else(|| format!("{tool_name} requires target with the child-agent task name."))?;
    let message = string_field(map, &["message"])
        .ok_or_else(|| format!("{tool_name} requires a non-empty message."))?;
    let delivery_mode = parse_inter_agent_delivery_mode(string_field(map, &["delivery_mode"]))?;
    Ok(InterAgentMessageArgs {
        target,
        message,
        delivery_mode,
    })
}

/// Parses one send_message delivery mode.
/// 解析一个 send_message 投递模式。
fn parse_inter_agent_delivery_mode(
    value: Option<String>,
) -> Result<InterAgentDeliveryMode, String> {
    let Some(value) = value else {
        return Ok(InterAgentDeliveryMode::Queue);
    };
    match value.as_str() {
        "queue" => Ok(InterAgentDeliveryMode::Queue),
        "interrupt" => Ok(InterAgentDeliveryMode::Interrupt),
        other => Err(format!(
            "send_message delivery_mode must be `queue` or `interrupt`; received `{other}`."
        )),
    }
}

/// Returns whether a model target names the root agent.
/// 返回模型目标是否指向 root 代理。
///
/// # Parameters
/// 参数
/// `target` is the raw model-provided target value.
/// `target` 是模型提供的原始目标值。
///
/// # Returns
/// 返回值
/// `true` when the target is a supported root alias.
/// 当目标是支持的 root 别名时返回 `true`。
pub(super) fn is_root_agent_target(target: &str) -> bool {
    matches!(target.trim(), "root" | "/root" | "/")
}

/// Formats queued inter-agent messages as the model text of their typed runtime messages.
/// 将排队的代理间消息格式化为其类型化运行时消息的模型文本。
///
/// # Parameters
/// 参数
/// `messages` are queued child-agent inbox messages.
/// `messages` 是排队的子代理 inbox 消息。
///
/// # Returns
/// 返回值
/// A single prompt containing every typed agent message in delivery order.
/// 按投递顺序包含所有类型化智能体消息的单条提示词。
fn format_inter_agent_messages_for_model(messages: &[QueuedInterAgentMessage]) -> String {
    messages
        .iter()
        .map(inter_agent_message_model_text)
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Appends provider-visible typed messages for delivered parent-to-child messages to a live controller.
/// 将已投递的父到子消息作为 Provider 可见类型化消息追加到实时控制器。
///
/// # Parameters
/// 参数
/// `controller` owns the child-agent core snapshot being updated.
/// `controller` 持有正在更新的子智能体核心快照。
/// `messages` are the delivered inter-agent messages.
/// `messages` 是已投递的智能体间消息。
fn append_inter_agent_agent_messages_to_controller(
    controller: &mut VulcanCoreController,
    messages: &[QueuedInterAgentMessage],
) {
    for message in messages {
        controller.append_agent_message(inter_agent_message_append_request(message));
    }
}

/// Atomically appends provider-visible and display projections for delivered messages to a stored snapshot.
/// 将已投递消息的 Provider 可见投影与显示投影原子追加到已存储快照。
///
/// # Parameters
/// 参数
/// `snapshot` is the child-agent core snapshot used for the next run.
/// `snapshot` 是下一次运行使用的子智能体核心快照。
/// `messages` are the delivered inter-agent messages whose deterministic identities must agree with retained owners.
/// `messages` 是确定性身份必须与既有所有者一致的已投递智能体间消息。
///
/// # Returns
/// 返回值
/// Returns success after one whole-snapshot commit, or the exact identity conflict without mutation.
/// 完整快照一次提交后返回成功；身份冲突时返回准确错误且不修改状态。
pub(super) fn append_inter_agent_messages_to_snapshot(
    snapshot: &mut CoreConversationSnapshot,
    messages: &[QueuedInterAgentMessage],
) -> Result<(), String> {
    // candidate isolates all three projections until their deterministic ownership is proven.
    // candidate 隔离全部三种投影，直到其确定性所有权得到证明。
    let mut candidate = snapshot.clone();
    for message in messages {
        // expected_message is the exact provider-visible projection owned by this delivery.
        // expected_message 是该投递拥有的精确 Provider 可见投影。
        let expected_message = CoreConversationMessage {
            id: inter_agent_message_id(message),
            payload: inter_agent_message_payload(message),
        };
        retain_or_append_inter_agent_projection(
            &mut candidate.messages,
            expected_message,
            message.delivery_id.as_str(),
            "provider-visible message",
            |entry| entry.id.as_str(),
        )?;
        // expected_agent_record is the hidden transcript counterpart of the provider-visible message.
        // expected_agent_record 是 Provider 可见消息对应的隐藏转录记录。
        let expected_agent_record = CoreConversationRecord {
            id: inter_agent_agent_record_id(message),
            role: CoreTranscriptRole::Assistant,
            record_kind: CoreTranscriptRecordKind::Visible,
            visible: false,
            title: String::from("智能体消息"),
            body: inter_agent_message_model_text(message),
            metadata: Some(inter_agent_agent_message_metadata(message)),
            created_at: message.created_at_ms.to_string(),
        };
        retain_or_append_inter_agent_projection(
            &mut candidate.records,
            expected_agent_record,
            message.delivery_id.as_str(),
            "agent record",
            |entry| entry.id.as_str(),
        )?;
        // expected_display_record is the visible delivery card with the externally stable delivery id.
        // expected_display_record 是使用外部稳定投递标识的可见投递卡片。
        let expected_display_record = inter_agent_display_conversation_record(message);
        retain_or_append_inter_agent_projection(
            &mut candidate.records,
            expected_display_record,
            message.delivery_id.as_str(),
            "display record",
            |entry| entry.id.as_str(),
        )?;
    }
    core_snapshot_to_session_conversation_state(&candidate)?;
    *snapshot = candidate;
    Ok(())
}

/// Retains an identical deterministic projection or appends it when the identity is unowned.
/// 保留内容相同的确定性投影，或在身份尚未被占用时追加该投影。
///
/// # Parameters
/// 参数
/// `entries` is the canonical projection collection being updated.
/// `entries` 是正在更新的规范投影集合。
/// `expected` is the exact projection owned by the current delivery.
/// `expected` 是当前投递拥有的精确投影。
/// `delivery_id` identifies the owning inter-agent delivery in diagnostics.
/// `delivery_id` 在诊断中标识拥有该投影的智能体间投递。
/// `projection_name` labels the projection kind in conflict errors.
/// `projection_name` 在冲突错误中标记投影类型。
/// `entry_id` returns the stable identity of one projection entry.
/// `entry_id` 返回单个投影条目的稳定身份。
///
/// # Returns
/// 返回值
/// Success for an identical replay or append, and an identity conflict for different retained content.
/// 内容相同的重放或追加返回成功；既有内容不同时返回身份冲突。
fn retain_or_append_inter_agent_projection<T, EntryId>(
    entries: &mut Vec<T>,
    expected: T,
    delivery_id: &str,
    projection_name: &str,
    entry_id: EntryId,
) -> Result<(), String>
where
    T: PartialEq,
    EntryId: Fn(&T) -> &str,
{
    let expected_id = entry_id(&expected).to_string();
    if let Some(existing) = entries
        .iter()
        .find(|existing| entry_id(existing) == expected_id)
    {
        if existing != &expected {
            return Err(format!(
                "Inter-agent delivery `{delivery_id}` conflicts with retained {projection_name} id {expected_id:?}."
            ));
        }
    } else {
        entries.push(expected);
    }
    Ok(())
}

/// Builds a controller append request for one typed parent-to-child message.
/// 为单条父到子类型化消息构建控制器追加请求。
///
/// # Parameters
/// 参数
/// `message` is the delivered inter-agent message.
/// `message` 是已投递的智能体间消息。
///
/// # Returns
/// 返回值
/// A provider-visible agent-message append request.
/// 一条 Provider 可见智能体消息追加请求。
fn inter_agent_message_append_request(
    message: &QueuedInterAgentMessage,
) -> AgentMessageAppendRequest {
    AgentMessageAppendRequest {
        message_type: String::from("MESSAGE"),
        task_name: message.target_agent_id.clone(),
        sender: inter_agent_message_sender(message),
        recipient: message.target_agent_id.clone(),
        payload: message.content.clone(),
        trigger_turn: true,
        metadata: Some(inter_agent_agent_message_metadata(message)),
    }
}

/// Builds the canonical payload for one typed parent-to-child message.
/// 为单条父到子类型化消息构建规范载荷。
///
/// # Parameters
/// 参数
/// `message` is the delivered inter-agent message.
/// `message` 是已投递的智能体间消息。
///
/// # Returns
/// 返回值
/// A canonical provider-visible conversation payload.
/// 一个规范的 Provider 可见会话载荷。
fn inter_agent_message_payload(message: &QueuedInterAgentMessage) -> CoreConversationPayload {
    CoreConversationPayload::AgentMessage {
        message_type: String::from("MESSAGE"),
        task_name: message.target_agent_id.clone(),
        sender: inter_agent_message_sender(message),
        recipient: message.target_agent_id.clone(),
        payload: message.content.clone(),
        trigger_turn: true,
        metadata: Some(inter_agent_agent_message_metadata(message)),
    }
}

/// Renders one typed parent-to-child message as provider-compatible text.
/// 将单条父到子类型化消息渲染为 Provider 兼容文本。
///
/// # Parameters
/// 参数
/// `message` is the delivered inter-agent message.
/// `message` 是已投递的智能体间消息。
///
/// # Returns
/// 返回值
/// The model text produced by the shared `AgentMessage` envelope.
/// 共享 `AgentMessage` 信封产生的模型文本。
fn inter_agent_message_model_text(message: &QueuedInterAgentMessage) -> String {
    CoreConversationPayload::render_agent_message_text(
        "MESSAGE",
        &message.target_agent_id,
        &inter_agent_message_sender(message),
        &message.content,
    )
}

/// Returns the logical sender for one parent-to-child message.
/// 返回单条父到子消息的逻辑发送方。
///
/// # Parameters
/// 参数
/// `message` is the delivered inter-agent message.
/// `message` 是已投递的智能体间消息。
///
/// # Returns
/// 返回值
/// A stable parent-scoped sender label.
/// 稳定的父级作用域发送方标签。
fn inter_agent_message_sender(message: &QueuedInterAgentMessage) -> String {
    format!("parent:{}", message.author_session_id)
}

/// Returns the stable canonical message id for one delivered message.
/// 返回单条已投递消息的稳定规范消息标识。
///
/// # Parameters
/// 参数
/// `message` is the delivered inter-agent message.
/// `message` 是已投递的智能体间消息。
///
/// # Returns
/// 返回值
/// A message id derived from the delivery id.
/// 基于投递标识派生的消息标识。
fn inter_agent_message_id(message: &QueuedInterAgentMessage) -> String {
    format!("agent-message-{}", message.delivery_id)
}

/// Returns the stable hidden transcript record id for one delivered message.
/// 返回单条已投递消息的稳定隐藏转录记录标识。
///
/// # Parameters
/// 参数
/// `message` is the delivered inter-agent message.
/// `message` 是已投递的智能体间消息。
///
/// # Returns
/// 返回值
/// A transcript record id derived from the delivery id.
/// 基于投递标识派生的转录记录标识。
fn inter_agent_agent_record_id(message: &QueuedInterAgentMessage) -> String {
    format!("agent-record-{}", message.delivery_id)
}

/// Builds host-only metadata for a typed parent-to-child message.
/// 为父到子类型化消息构建宿主侧元数据。
///
/// # Parameters
/// 参数
/// `message` is the delivered inter-agent message.
/// `message` 是已投递的智能体间消息。
///
/// # Returns
/// 返回值
/// Metadata linking the provider-visible message to the display delivery record.
/// 将 Provider 可见消息关联到显示投递记录的元数据。
fn inter_agent_agent_message_metadata(message: &QueuedInterAgentMessage) -> Value {
    json!({
        "kind": "subagent_message",
        "tool": SEND_MESSAGE_TOOL,
        "delivery_id": message.delivery_id.clone(),
        "parent_session_id": message.author_session_id.to_string(),
        (SUBAGENT_TASK_ID_KEY): message.target_task_id.to_string(),
        "target": message.target_agent_id.clone(),
        "created_at_ms": message.created_at_ms,
    })
}

/// Appends display-only records for delivered parent-to-child messages to a live controller.
/// 将已投递的父到子消息作为仅显示记录追加到实时控制器。
///
/// # Parameters
/// 参数
/// `controller` owns the child-agent core snapshot being updated.
/// `controller` 持有正在更新的子智能体核心快照。
/// `messages` are the delivered inter-agent messages.
/// `messages` 是已投递的智能体间消息。
///
/// # Returns
/// 返回值
/// Returns success after all display records append, or the exact Core identity failure.
/// 全部显示记录追加后返回成功，否则返回准确的 Core 身份错误。
fn append_inter_agent_display_records_to_controller(
    controller: &mut VulcanCoreController,
    messages: &[QueuedInterAgentMessage],
) -> Result<(), String> {
    for message in messages {
        controller
            .append_display_record(inter_agent_display_record_request(message))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Builds one controller display-record request for a parent-to-child message.
/// 为一条父到子消息构建控制器显示记录请求。
///
/// # Parameters
/// 参数
/// `message` is the delivered inter-agent message.
/// `message` 是已投递的智能体间消息。
///
/// # Returns
/// 返回值
/// A display-only append request that does not alter provider-visible history.
/// 不改变 Provider 可见历史的仅显示追加请求。
fn inter_agent_display_record_request(
    message: &QueuedInterAgentMessage,
) -> DisplayRecordAppendRequest {
    DisplayRecordAppendRequest {
        record_id: Some(message.delivery_id.clone()),
        role: CoreTranscriptRole::User,
        title: String::from("智能体消息"),
        body: message.content.clone(),
        metadata: Some(inter_agent_display_record_metadata(message)),
    }
}

/// Builds one core conversation record for a parent-to-child message.
/// 为一条父到子消息构建核心会话记录。
///
/// # Parameters
/// 参数
/// `message` is the delivered inter-agent message.
/// `message` 是已投递的智能体间消息。
///
/// # Returns
/// 返回值
/// A projection-only record retained outside provider-visible history.
/// 保留在 Provider 可见历史之外的仅投影记录。
fn inter_agent_display_conversation_record(
    message: &QueuedInterAgentMessage,
) -> CoreConversationRecord {
    CoreConversationRecord {
        id: message.delivery_id.clone(),
        role: CoreTranscriptRole::User,
        record_kind: CoreTranscriptRecordKind::ProjectionOnly,
        visible: true,
        title: String::from("智能体消息"),
        body: message.content.clone(),
        metadata: Some(inter_agent_display_record_metadata(message)),
        created_at: message.created_at_ms.to_string(),
    }
}

/// Builds shared display metadata for a parent-to-child message.
/// 为一条父到子消息构建共享显示元数据。
///
/// # Parameters
/// 参数
/// `message` is the delivered inter-agent message.
/// `message` 是已投递的智能体间消息。
///
/// # Returns
/// 返回值
/// Host-only metadata that links display records with the delivery event.
/// 将显示记录与投递事件关联起来的宿主侧元数据。
fn inter_agent_display_record_metadata(message: &QueuedInterAgentMessage) -> Value {
    json!({
        "display_note": true,
        "kind": "subagent_message",
        "tool": SEND_MESSAGE_TOOL,
        "delivery_id": message.delivery_id.clone(),
        "parent_session_id": message.author_session_id.to_string(),
        (SUBAGENT_TASK_ID_KEY): message.target_task_id.to_string(),
        "created_at_ms": message.created_at_ms,
    })
}

/// Builds the structured tool result for inter-agent message delivery.
/// 构建代理间消息投递的结构化工具结果。
///
/// # Parameters
/// 参数
/// `tool_name` is the invoked subagent tool.
/// `tool_name` 是被调用的子代理工具。
/// `current_session_id` is the sender session id.
/// `current_session_id` 是发送方会话标识。
/// `target` is the resolved target child-agent entry.
/// `target` 是解析出的目标子代理条目。
/// `message` is the delivered message body.
/// `message` 是已投递消息正文。
/// `delivery_id` is the stable id shared by runtime delivery and display records.
/// `delivery_id` 是运行时投递与显示记录共享的稳定标识。
/// `delivery_mode` records whether this delivery queued or interrupted.
/// `delivery_mode` 记录本次投递是排队还是插队。
/// `stopped_task_ids` lists active tasks stopped before interrupt delivery.
/// `stopped_task_ids` 列出插队投递前停止的活动任务。
/// `immediate_resume` reports a child turn that was synchronously resumed by this delivery.
/// `immediate_resume` 描述本次投递同步续跑得到的子回合结果。
///
/// # Returns
/// 返回值
/// A Vulcan structured model result plus host-only delivery metadata.
/// 一个 Vulcan 结构化模型结果与宿主专用投递元数据。
fn build_inter_agent_message_tool_result(
    tool_name: &str,
    current_session_id: SessionId,
    target: &SubagentRegistryEntry,
    message: &str,
    delivery_id: &str,
    delivery_mode: InterAgentDeliveryMode,
    stopped_task_ids: &[SessionId],
    immediate_resume: Option<&InterAgentImmediateResumeResult>,
) -> VulcanToolInvocationResult {
    let delivery_status = immediate_resume
        .map(|result| result.execution_status.as_str())
        .unwrap_or("queued");
    let model_payload = json!({
        "ok": true,
        "target": target.agent_id,
        (SUBAGENT_TASK_ID_KEY): target.session_id.to_string(),
        "queued": true,
        "delivery_mode": delivery_mode.as_str(),
        "interrupted": delivery_mode.interrupts_current_task(),
        "delivery_status": delivery_status,
        "message": inter_agent_message_delivery_model_hint(immediate_resume),
    });
    let model_payload = if let Some(immediate_resume) = immediate_resume {
        inter_agent_message_tool_result_with_resume_model_payload(model_payload, immediate_resume)
    } else {
        model_payload
    };
    let host_payload = json!({
        "tool": tool_name,
        "parent_session_id": current_session_id.to_string(),
        "target": target.agent_id,
        (SUBAGENT_TASK_ID_KEY): target.session_id.to_string(),
        "delivery_id": delivery_id,
        "message": message,
        "queued": true,
        "delivery_mode": delivery_mode.as_str(),
        "interrupted": delivery_mode.interrupts_current_task(),
        (STOPPED_SUBAGENT_TASK_IDS_KEY): stopped_task_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "target_status_before_delivery": target.status,
        "delivery_status": delivery_status,
        "resumed_execution_id": immediate_resume.map(|result| result.execution_id.as_str()),
        "resumed_execution_status": immediate_resume.map(|result| result.execution_status.as_str()),
    });
    VulcanToolInvocationResult::from_model_output(model_payload).with_host_result(
        VulcanToolHostResult {
            kind: String::from("subagent_message"),
            payload: host_payload,
        },
    )
}

/// Builds the model-facing delivery hint for a `send_message` result.
/// 构建 `send_message` 结果中面向模型的投递提示。
fn inter_agent_message_delivery_model_hint(
    immediate_resume: Option<&InterAgentImmediateResumeResult>,
) -> &'static str {
    match immediate_resume {
        Some(result) if result.is_terminal() => {
            "The child agent processed this message immediately. Use `result`; do not wait for another mailbox turn for this delivery."
        }
        Some(_) => {
            "The child agent accepted this message and is still running. Do not report a final child result from assumptions; wait for child-agent feedback."
        }
        None => {
            "The message was queued for the child agent. Do not report a final child result from assumptions; wait for child-agent feedback."
        }
    }
}

/// Adds immediate child execution result fields to the model-facing payload.
/// 将立即完成的子执行结果字段加入面向模型的载荷。
fn inter_agent_message_tool_result_with_resume_model_payload(
    mut model_payload: Value,
    immediate_resume: &InterAgentImmediateResumeResult,
) -> Value {
    if let Some(object) = model_payload.as_object_mut() {
        object.insert(
            String::from("resumed_execution_id"),
            Value::String(immediate_resume.execution_id.clone()),
        );
        object.insert(
            String::from("execution_status"),
            Value::String(immediate_resume.execution_status.as_str().to_string()),
        );
        if immediate_resume.is_terminal() {
            object.insert(
                String::from("result"),
                Value::String(immediate_resume.result_text.clone()),
            );
        }
    }
    model_payload
}

/// Builds the structured tool result for a child-to-root message.
/// 构建子代理到 root 消息的结构化工具结果。
///
/// # Parameters
/// 参数
/// `current_session_id` is the sender child-agent session id.
/// `current_session_id` 是发送方子代理会话标识。
/// `parent_session_id` is the resolved parent mailbox receiver.
/// `parent_session_id` 是解析出的父级 mailbox 接收者。
/// `message` is the delivered message body.
/// `message` 是已投递消息正文。
///
/// # Returns
/// 返回值
/// A result with compact model-visible delivery confirmation and host-only message text.
/// 一个带有紧凑模型可见投递确认与宿主专用消息正文的结果。
fn build_root_inter_agent_message_tool_result(
    current_session_id: SessionId,
    parent_session_id: SessionId,
    message: &str,
) -> VulcanToolInvocationResult {
    let model_payload = json!({
        "ok": true,
        "target": "root",
        "sender_session_id": current_session_id.to_string(),
        "parent_session_id": parent_session_id.to_string(),
        "queued": true,
        "delivery_mode": InterAgentDeliveryMode::Queue.as_str(),
        "interrupted": false,
    });
    let host_payload = json!({
        "tool": SEND_MESSAGE_TOOL,
        "parent_session_id": parent_session_id.to_string(),
        "sender_session_id": current_session_id.to_string(),
        "target": "root",
        "message": message,
        "queued": true,
        "delivery_mode": InterAgentDeliveryMode::Queue.as_str(),
        "interrupted": false,
    });
    VulcanToolInvocationResult::from_model_output(model_payload).with_host_result(
        VulcanToolHostResult {
            kind: String::from("subagent_message"),
            payload: host_payload,
        },
    )
}
