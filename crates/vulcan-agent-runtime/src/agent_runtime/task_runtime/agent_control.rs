use super::*;
use crate::agent_runtime::runtime_stream_intervention::RuntimePromptIntervention;

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
        self.restore_persisted_subagent_registry_for_parent_tree(current_session_id)?;
        let target = self.resolve_inter_agent_message_target(current_session_id, &parsed.target)?;
        let target_session_id = SessionId::from(target.session_id);
        let stopped_task_ids = if parsed.delivery_mode.interrupts_current_task() {
            self.stop_task_tree(target_session_id)?
        } else {
            Vec::new()
        };
        self.enqueue_inter_agent_message(
            current_session_id,
            target_session_id,
            parsed.message.clone(),
        )?;

        if !self.task_has_active_or_starting_run(target_session_id)? {
            self.resume_idle_agent_from_inbox(current_session_id, &target)
                .await?;
        }

        Ok(build_inter_agent_message_tool_result(
            SEND_MESSAGE_TOOL,
            current_session_id,
            &target,
            &parsed.message,
            parsed.delivery_mode,
            &stopped_task_ids,
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
            .get(&normalized_target)
            .filter(|entry| entry.parent_session_id == parent_session_id)
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
    /// `content` is the message body.
    /// `content` 是消息正文。
    fn enqueue_inter_agent_message(
        &self,
        author_session_id: SessionId,
        target_task_id: SessionId,
        content: String,
    ) -> Result<(), String> {
        let message = QueuedInterAgentMessage {
            author_session_id,
            target_task_id,
            content,
            created_at_ms: current_unix_millis_u64(),
        };
        self.agent_inboxes
            .write()
            .map_err(|_| String::from("The subagent agent-control inbox is locked."))?
            .entry(target_task_id)
            .or_default()
            .push_back(message);
        Ok(())
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
    /// The original error, or a combined error when restoration also fails.
    /// 原始错误；如果恢复也失败，则返回合并错误。
    fn restore_inter_agent_messages_front_after_failure(
        &self,
        target_task_id: SessionId,
        messages: Vec<QueuedInterAgentMessage>,
        stage: &str,
        error: String,
    ) -> String {
        let message_count = messages.len();
        match self.requeue_inter_agent_messages_front(target_task_id, messages) {
            Ok(()) => {
                task_runtime_trace(
                    "subagent_message_requeue_after_failure",
                    format!("target={target_task_id} stage={stage} message_count={message_count}"),
                );
                error
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
                combined_error
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

    /// Clears queued inter-agent messages for closed child agents.
    /// 清理已关闭子代理的排队代理间消息。
    pub(super) fn clear_inter_agent_messages_for_tasks(
        &self,
        // Target child task ids whose inboxes should be removed.
        // 需要移除 inbox 的目标子任务标识。
        target_task_ids: &[SessionId],
    ) -> Result<usize, String> {
        if target_task_ids.is_empty() {
            return Ok(0);
        }
        let mut inboxes = self
            .agent_inboxes
            .write()
            .map_err(|_| String::from("The subagent agent-control inbox is locked."))?;
        let mut cleared_count = 0;
        for task_id in target_task_ids {
            cleared_count += inboxes
                .remove(task_id)
                .map(|queue| queue.len())
                .unwrap_or(0);
        }
        Ok(cleared_count)
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
    pub(super) async fn resume_idle_agent_from_inbox(
        self: &Arc<Self>,
        current_session_id: SessionId,
        target: &SubagentRegistryEntry,
    ) -> Result<(), String> {
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
        let prompt = format_inter_agent_messages(&messages);
        let parsed_args = match parse_task_tool_arguments(json!({
            "task_id": target.agent_id,
            "prompt": prompt,
        })) {
            Ok(parsed_args) => parsed_args,
            Err(error) => {
                return Err(self.restore_inter_agent_messages_front_after_failure(
                    target_task_id,
                    messages,
                    "parse_task_tool_arguments",
                    error,
                ));
            }
        };
        let resolved_identifier =
            match resolve_task_session_id(self, current_session_id, &parsed_args) {
                Ok(resolved_identifier) => resolved_identifier,
                Err(error) => {
                    return Err(self.restore_inter_agent_messages_front_after_failure(
                        target_task_id,
                        messages,
                        "resolve_task_session_id",
                        error,
                    ));
                }
            };
        let invocation = match prepare_task_execution_request(
            self,
            current_session_id,
            resolved_identifier,
            parsed_args,
        ) {
            Ok(invocation) => invocation,
            Err(error) => {
                return Err(self.restore_inter_agent_messages_front_after_failure(
                    target_task_id,
                    messages,
                    "prepare_task_execution_request",
                    error,
                ));
            }
        };
        let outcome = match backend(invocation.clone()).await {
            Ok(outcome) => outcome,
            Err(error) => {
                let restored_error = self.restore_inter_agent_messages_front_after_failure(
                    target_task_id,
                    messages,
                    "backend_invocation",
                    error,
                );
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
        Ok(())
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
        let messages = match task_runtime.take_inter_agent_messages(task_id) {
            Ok(messages) => messages,
            Err(error) => {
                eprintln!("[subagent-agent-control] failed to drain child inbox: {error}");
                Vec::new()
            }
        };
        if messages.is_empty() {
            return false;
        }
        let prompt = format_inter_agent_messages(&messages);
        if controller.append_interjected_user_turn(&prompt).is_some() {
            return true;
        }
        task_runtime.restore_inter_agent_messages_front_after_failure(
            task_id,
            messages,
            "prompt_intervention",
            String::from("The child-agent stream could not accept an injected user turn."),
        );
        false
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

/// Formats queued inter-agent messages as one replay-safe injected user turn.
/// 将排队的代理间消息格式化为一个可回放安全的插入用户回合。
///
/// # Parameters
/// 参数
/// `messages` are queued child-agent inbox messages.
/// `messages` 是排队的子代理 inbox 消息。
///
/// # Returns
/// 返回值
/// A single prompt containing every queued message in delivery order.
/// 按投递顺序包含所有排队消息的单条提示词。
fn format_inter_agent_messages(messages: &[QueuedInterAgentMessage]) -> String {
    let mut prompt = String::from("[Inter-agent messages]\n");
    for (index, message) in messages.iter().enumerate() {
        prompt.push_str(&format!(
            "\nMessage {} from {} to {} via send_message at {}:\n{}\n",
            index + 1,
            message.author_session_id,
            message.target_task_id,
            message.created_at_ms,
            message.content
        ));
    }
    prompt
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
/// `delivery_mode` records whether this delivery queued or interrupted.
/// `delivery_mode` 记录本次投递是排队还是插队。
/// `stopped_task_ids` lists active tasks stopped before interrupt delivery.
/// `stopped_task_ids` 列出插队投递前停止的活动任务。
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
    delivery_mode: InterAgentDeliveryMode,
    stopped_task_ids: &[SessionId],
) -> VulcanToolInvocationResult {
    let model_payload = json!({
        "ok": true,
        "target": target.agent_id,
        (SUBAGENT_TASK_ID_KEY): target.session_id.to_string(),
        "queued": true,
        "delivery_mode": delivery_mode.as_str(),
        "interrupted": delivery_mode.interrupts_current_task(),
        "status": target.status,
    });
    let host_payload = json!({
        "tool": tool_name,
        "parent_session_id": current_session_id.to_string(),
        "target": target.agent_id,
        (SUBAGENT_TASK_ID_KEY): target.session_id.to_string(),
        "message": message,
        "queued": true,
        "delivery_mode": delivery_mode.as_str(),
        "interrupted": delivery_mode.interrupts_current_task(),
        (STOPPED_SUBAGENT_TASK_IDS_KEY): stopped_task_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "status": target.status,
    });
    VulcanToolInvocationResult::from_model_output(model_payload).with_host_result(
        VulcanToolHostResult {
            kind: String::from("subagent_message"),
            payload: host_payload,
        },
    )
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
