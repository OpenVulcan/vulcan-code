use super::*;

#[cfg(test)]
#[path = "tests/durable_return.rs"]
mod durable_return_tests;
use crate::agent_runtime::provider_request_bridge_conversation::core_snapshot_to_session_conversation_state;
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
    /// Steer the current task at its next physical API boundary without cancellation.
    /// 在下一次物理 API 边界引导当前任务，不取消执行。
    Interrupt,
}

impl InterAgentDeliveryMode {
    /// Returns whether delivery should precede the next API request.
    /// 返回该投递是否应排在下一次 API 请求之前。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the caller requested guidance before the next API.
    /// 调用方请求在下次 API 前追加引导时返回 `true`。
    const fn steers_current_task(self) -> bool {
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
    /// Removes an unpublished return gate after this exact startup was rejected before Provider launch.
    /// 此精确启动在 Provider 启动前被拒绝后，移除尚未发布的回流门。
    /// `task_id` and `execution_id` identify the rejected admission; a replacement is left intact.
    /// `task_id` 与 `execution_id` 标识被拒准入；替代执行保持原状。
    /// Returns whether the gate was removed, or an explicit registry/publication conflict.
    /// 返回是否移除了门控，或明确的注册表及发布冲突。
    pub fn cancel_rejected_task_return(
        &self,
        task_id: SessionId,
        execution_id: &str,
    ) -> Result<bool, String> {
        // Compare and remove under one lock so late rejection cannot clear a newer execution.
        // 在同一锁内比较并删除，防止迟到拒绝清除较新执行。
        let mut returns = self
            .durable_return_mailboxes
            .write()
            .map_err(|_| "The durable return registry is locked.")?;
        match returns.get(&task_id) {
            Some((expected, None)) if expected == execution_id => {
                returns.remove(&task_id);
                drop(returns);
                self.notify_subagent_mailbox_activity();
                Ok(true)
            }
            Some((expected, Some(_))) if expected == execution_id => Err(String::from(
                "A published child return cannot be cancelled as a rejected startup.",
            )),
            _ => Ok(false),
        }
    }

    /// Releases an unpublished gate after its exact host listener has exited.
    /// 精确宿主监听器退出后释放尚未发布的门控。
    /// `task_id` and `execution_id` identify the exited listener; published or replacement results survive.
    /// `task_id` 与 `execution_id` 标识已退出监听器；已发布及替代执行的结果保持不变。
    /// Returns whether an orphaned gate was removed, or a registry lock error.
    /// 返回是否移除了孤立门控，或注册表锁错误。
    pub fn release_unpublished_task_return(
        &self,
        task_id: SessionId,
        execution_id: &str,
    ) -> Result<bool, String> {
        // Only the concrete listener can attest that no future publisher remains for this gate.
        // 只有具体监听器能够证明此门控已不存在后续发布者。
        let mut returns = self
            .durable_return_mailboxes
            .write()
            .map_err(|_| "The durable return registry is locked.")?;
        if !matches!(returns.get(&task_id), Some((expected, None)) if expected == execution_id) {
            return Ok(false);
        }
        returns.remove(&task_id);
        drop(returns);
        self.notify_subagent_mailbox_activity();
        Ok(true)
    }

    /// Defers successful feedback until the V1 owner publishes its exact return statistics.
    /// 将成功反馈延迟到 V1 所有者发布精确回流统计之后。
    /// `task_id` and `execution_id` identify the admitted generation; returns registration errors.
    /// `task_id` 与 `execution_id` 标识准入代际；返回注册错误。
    pub fn expect_durable_task_return(
        &self,
        task_id: SessionId,
        execution_id: &str,
    ) -> Result<(), String> {
        // Retain one publication gate per stable child and exact execution generation.
        // 为稳定子代理及其精确执行代际保留一个发布门控。
        let mut returns = self
            .durable_return_mailboxes
            .write()
            .map_err(|_| "The durable return registry is locked.")?;
        if returns
            .get(&task_id)
            .is_none_or(|(expected, _)| expected != execution_id)
        {
            returns.insert(task_id, (execution_id.to_string(), None));
            drop(returns);
            self.notify_subagent_mailbox_activity();
        }
        Ok(())
    }

    /// Waits for the host-published result of the exact registered child execution without consuming parent feedback.
    /// 等待宿主发布精确已登记子执行的结果，不消费父级反馈。
    /// `task_id` and `execution_id` identify the original run; missing or replaced gates return an explicit error.
    /// `task_id` 与 `execution_id` 标识原运行；缺失或被替换的门控返回明确错误。
    /// The caller must still validate current durable ownership before dispatching any auxiliary work.
    /// 调用者在派发任何辅助工作前仍须校验当前可靠归属。
    pub async fn wait_for_durable_task_return(
        &self,
        task_id: SessionId,
        execution_id: &str,
    ) -> Result<SubagentMailboxEvent, String> {
        // Subscribe before inspecting the cache so publication between inspection and waiting cannot be lost.
        // 在检查缓存之前订阅，避免丢失检查与等待之间的发布。
        let mut activity = self.subscribe_subagent_mailbox_activity();
        loop {
            activity.borrow_and_update();
            {
                // Drop the synchronous read lock before awaiting; this cache is the sole publication authority.
                // 在等待之前释放同步读锁；此缓存是唯一发布权威。
                let returns = self
                    .durable_return_mailboxes
                    .read()
                    .map_err(|_| String::from("The durable return registry is locked."))?;
                let (expected, receipt) = returns.get(&task_id).ok_or_else(|| {
                    String::from("The durable return was not registered or was cancelled.")
                })?;
                if expected != execution_id {
                    return Err(String::from("The durable return execution was replaced."));
                }
                if let Some(receipt) = receipt {
                    return Ok(receipt.clone());
                }
            }
            activity
                .changed()
                .await
                .map_err(|_| String::from("The durable return publication channel closed."))?;
        }
    }

    /// Publishes exact V1 feedback once per execution, then retains it for repeated waits.
    /// 按执行只发布一次精确 V1 反馈，并为重复等待保留反馈。
    /// `event` contains the canonical statistics; returns identity or mailbox errors.
    /// `event` 包含规范统计；返回身份或 mailbox 错误。
    pub fn publish_durable_task_return(
        &self,
        execution_id: &str,
        event: SubagentMailboxEvent,
    ) -> Result<(), String> {
        // Resolve the child solely from the typed mailbox identity.
        // 仅从强类型 mailbox 身份解析子代理。
        let task_id = SessionId::from(event.child_thread_id);
        // Serialize publication and cache replacement for this execution.
        // 将本次执行的发布与缓存替换串行化。
        let mut returns = self
            .durable_return_mailboxes
            .write()
            .map_err(|_| "The durable return registry is locked.")?;
        // Preserve the registered generation and its already published receipt.
        // 保留注册的代际及其已发布回执。
        let (expected, cached) = returns
            .get_mut(&task_id)
            .ok_or("The durable return was not registered.")?;
        if expected != execution_id {
            return Err("The durable return execution is stale.".into());
        }
        if let Some(previous) = cached {
            if previous != &event {
                return Err("The durable return differs from its published feedback.".into());
            }
            return Ok(());
        }
        self.subagent_mailbox.push_event(event.clone())?;
        *cached = Some(event);
        drop(returns);
        self.notify_subagent_mailbox_activity();
        Ok(())
    }

    /// Commits steering messages at a physical request boundary, removing each only after durable acceptance.
    /// 在物理请求边界提交插队消息，仅在可靠接纳后移除对应消息。
    /// `task_id` selects the child; `accept` receives the stable delivery id and model envelope.
    /// `task_id` 选择子代理；`accept` 接收稳定投递 ID 与模型消息信封。
    /// Returns the consumed count; an acceptance failure leaves the unaccepted message queued.
    /// 返回消费数量；接纳失败时尚未接纳的消息保留在队列中。
    pub fn consume_task_steering_messages(
        &self,
        task_id: SessionId,
        mut accept: impl FnMut(&str, &str) -> Result<(), String>,
    ) -> Result<usize, String> {
        // Keep admission ordered while each accepted message is committed durably.
        // 每条接纳消息可靠提交期间保持准入有序。
        let mut inboxes = self
            .agent_inboxes
            .write()
            .map_err(|_| String::from("The subagent agent-control inbox is locked."))?;
        // Select the exact child inbox; an absent inbox has no pending delivery.
        // 选择精确子代理 inbox；不存在 inbox 即没有待处理投递。
        let Some(messages) = inboxes.get_mut(&task_id) else {
            return Ok(0);
        };
        // Count only messages whose durable acceptance succeeded.
        // 只计算可靠接纳成功的消息。
        let mut consumed = 0;
        while let Some(index) = messages.iter().position(|message| message.steering) {
            // Preserve delivery identity until its durable acceptance succeeds.
            // 在可靠接纳成功之前保留投递身份。
            let message = &messages[index];
            let model_text = format_inter_agent_messages_for_model(std::slice::from_ref(message))?;
            accept(&message.delivery_id, &model_text)?;
            messages.remove(index);
            consumed += 1;
        }
        Ok(consumed)
    }

    /// Returns the authoritative pending message list for a child panel, in delivery order.
    /// 按投递顺序返回子代理面板的权威待处理消息列表。
    /// `task_id` selects one child; returns public content without consuming messages.
    /// `task_id` 选择一个子代理；返回公开内容且不消费消息。
    pub fn pending_task_messages(
        &self,
        task_id: SessionId,
    ) -> Result<Vec<TaskPendingMessage>, String> {
        // Read the same inbox used by API-boundary steering and idle follow-ups.
        // 读取 API 边界插队与空闲续接共同使用的 inbox。
        let inboxes = self
            .agent_inboxes
            .read()
            .map_err(|_| String::from("The subagent agent-control inbox is locked."))?;
        Ok(inboxes
            .get(&task_id)
            .into_iter()
            .flatten()
            .map(|message| TaskPendingMessage {
                delivery_id: message.delivery_id.clone(),
                message: message.content.clone(),
                steering: message.steering,
            })
            .collect())
    }

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
            if parsed.delivery_mode.steers_current_task() {
                return Err(String::from(
                    "send_message delivery_mode `interrupt` cannot target the root agent.",
                ));
            }
            return self.deliver_root_inter_agent_message(current_session_id, &parsed.message);
        }
        let target = self.resolve_inter_agent_message_target(current_session_id, &parsed.target)?;
        let target_session_id = SessionId::from(target.session_id);
        // transaction_task_ids serialize message admission with this child finalizer.
        // transaction_task_ids 将消息准入与当前子代理收尾串行化。
        let transaction_task_ids = vec![target_session_id];
        // transaction_locks preserve inbox admission before the finalizer selects a successor.
        // transaction_locks 确保 inbox 准入先于收尾选择后继执行。
        let transaction_locks =
            self.ordered_task_execution_state_locks(transaction_task_ids.as_slice())?;
        // transaction_guards retain the task boundary through inbox admission.
        // transaction_guards 在 inbox 准入期间持有任务边界。
        let mut transaction_guards = Vec::with_capacity(transaction_locks.len());
        for transaction_lock in &transaction_locks {
            transaction_guards.push(transaction_lock.lock().map_err(|_| {
                String::from(
                    "The task execution transaction is locked and cannot admit the inter-agent message.",
                )
            })?);
        }
        let admitted_target =
            self.resolve_inter_agent_message_target(current_session_id, &parsed.target)?;
        if admitted_target.session_id != target.session_id {
            return Err(format!(
                "Child agent `{}` changed from session `{}` to `{}` before message admission.",
                target.agent_id, target.session_id, admitted_target.session_id
            ));
        }
        // continues_task_turn is sampled inside the message admission transaction.
        // continues_task_turn 在消息准入事务内取样。
        let continues_task_turn = self.task_has_active_or_starting_run(target_session_id)?;
        // stopped_task_ids remain empty because guidance never cancels active work.
        // stopped_task_ids 保持为空，因为引导不会取消当前工作。
        let stopped_task_ids = Vec::new();
        let delivery_id = self.enqueue_inter_agent_message(
            current_session_id,
            target_session_id,
            admitted_target.agent_id.clone(),
            parsed.message.clone(),
            continues_task_turn,
            parsed.delivery_mode.steers_current_task(),
        )?;
        let should_resume = !self.task_has_active_or_starting_run(target_session_id)?;
        drop(transaction_guards);

        if should_resume {
            self.spawn_idle_agent_resume_from_inbox(
                current_session_id,
                admitted_target.clone(),
                delivery_id.clone(),
            );
        }

        Ok(build_inter_agent_message_tool_result(
            SEND_MESSAGE_TOOL,
            current_session_id,
            &admitted_target,
            &parsed.message,
            &delivery_id,
            parsed.delivery_mode,
            &stopped_task_ids,
            continues_task_turn,
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
        continues_task_turn: bool,
        steering: bool,
    ) -> Result<String, String> {
        let created_at_ms = current_unix_millis_u64();
        let delivery_id = format!("subagent-message-{target_task_id}-{}", Uuid::new_v4());
        // Queue deliveries receive a distinct task identity so repeated follow-ups cannot reuse the public agent path.
        // 排队投递使用独立任务身份，确保重复后续消息不会复用公开代理路径。
        let task_id = if steering {
            target_task_id.to_string()
        } else {
            format!("queued-task-{delivery_id}")
        };
        let message = QueuedInterAgentMessage {
            kind: if steering {
                vulcan_ai_contract::HostMessageKind::TaskSteer
            } else {
                vulcan_ai_contract::HostMessageKind::TaskStart
            },
            task_id,
            steering,
            delivery_id: delivery_id.clone(),
            author_session_id,
            target_task_id,
            target_agent_id,
            content,
            created_at_ms,
            continues_task_turn,
        };
        self.agent_inboxes
            .write()
            .map_err(|_| String::from("The subagent agent-control inbox is locked."))?
            .entry(target_task_id)
            .or_default()
            .push_back(message);
        Ok(delivery_id)
    }

    /// Starts an accepted idle-child delivery without delaying the parent tool result.
    /// 在不延迟父级工具结果的情况下启动已接纳的空闲子代理投递。
    ///
    /// # Parameters
    /// 参数
    /// `current_session_id` identifies the parent session that owns the child backend.
    /// `current_session_id` 标识拥有子代理后端的父会话。
    /// `target` is the exact admitted child registry entry to resume.
    /// `target` 是要续跑的精确已接纳子代理注册项。
    /// `delivery_id` correlates any asynchronous resume failure with the accepted message.
    /// `delivery_id` 将异步续跑失败与已接纳消息关联起来。
    fn spawn_idle_agent_resume_from_inbox(
        self: &Arc<Self>,
        current_session_id: SessionId,
        target: SubagentRegistryEntry,
        delivery_id: String,
    ) {
        let task_runtime = Arc::clone(self);
        tokio::spawn(async move {
            if let Err(error) = task_runtime
                .resume_idle_agent_from_inbox(current_session_id, &target)
                .await
                && let Err(publish_error) = task_runtime.publish_inter_agent_resume_failure(
                    current_session_id,
                    &target,
                    delivery_id.as_str(),
                    error.as_str(),
                )
            {
                eprintln!(
                    "[task-runtime] accepted inter-agent message resume failed for task {}: {error}; parent feedback also failed: {publish_error}",
                    target.session_id
                );
            }
        });
    }

    /// Publishes an asynchronous child-resume failure after the parent already received admission.
    /// 在父级已经收到接纳结果后发布异步子代理续跑失败。
    ///
    /// # Parameters
    /// 参数
    /// `current_session_id` is the parent mailbox that must receive the failure.
    /// `current_session_id` 是必须接收失败反馈的父级 mailbox。
    /// `target` identifies the exact child whose accepted message could not resume.
    /// `target` 标识已接纳消息却无法续跑的精确子代理。
    /// `delivery_id` correlates the failure with the accepted message row.
    /// `delivery_id` 将失败与已接纳消息行关联起来。
    /// `error` is the exact resume error retained for diagnostics and model feedback.
    /// `error` 是为诊断和模型反馈保留的精确续跑错误。
    ///
    /// # Returns
    /// 返回值
    /// Success after mailbox publication and waiter notification, or the exact mailbox error.
    /// mailbox 发布并通知等待方后返回成功，否则返回精确 mailbox 错误。
    fn publish_inter_agent_resume_failure(
        &self,
        current_session_id: SessionId,
        target: &SubagentRegistryEntry,
        delivery_id: &str,
        error: &str,
    ) -> Result<(), String> {
        let feedback = format!(
            "Accepted message `{delivery_id}` could not start child-agent execution: {error}"
        );
        let event = SubagentMailboxEvent {
            parent_thread_id: SessionNodeId::from(current_session_id),
            parent_turn_id: None,
            child_thread_id: target.session_id,
            kind: SubagentMailboxEventKind::Message,
            timestamp_ms: current_unix_millis_u64(),
            payload: json!({
                "child_thread_id": target.session_id.to_string(),
                "public_id": target.agent_id,
                "parent_thread_id": current_session_id.to_string(),
                "status": "message",
                "task_status": "message",
                "delivery_id": delivery_id,
                "delivery_status": "resume_failed",
                "result_text": feedback,
                "error_text": error,
            }),
        };
        self.subagent_mailbox.push_event(event)?;
        self.notify_subagent_mailbox_activity();
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
    ) -> Result<bool, String> {
        // backend is the exact parent-session handler required to resume queued child work.
        // backend 是续跑排队子任务所需的精确父会话处理器。
        let backend = self
            .backend_for_session(current_session_id)?
            .ok_or_else(|| {
                format!(
                    "The current session `{}` does not have a task backend installed, so the child agent cannot resume.",
                    current_session_id
                )
            })?;
        let target_task_id = SessionId::from(target.session_id);
        let messages = self.take_inter_agent_messages(target_task_id)?;
        if messages.is_empty() {
            return Ok(false);
        }
        let prompt = format_inter_agent_messages_for_model(&messages)?;
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
        };
        let mut invocation = match prepare_task_execution_request_with_execution_id(
            self,
            current_session_id,
            resolved_identifier,
            parsed_args,
            inter_agent_execution_id(&messages),
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
                if let Err(mark_error) = self
                    .mark_task_failure_without_parent_mailbox(
                        current_session_id,
                        &invocation,
                        &restored_error,
                    )
                    .map(|_| ())
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
        Ok(true)
    }

    /// Resumes a settled child by admitting queued messages as the next durable V1 root input.
    /// 在子代理结束后，将排队消息准入为下一条可靠 V1 根输入并续跑。
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
            .await
    }
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
/// A single prompt containing every typed agent message in delivery order, or the exact envelope error.
/// 返回按投递顺序包含所有类型化智能体消息的单条提示词，或精确的信封渲染错误。
fn format_inter_agent_messages_for_model(
    messages: &[QueuedInterAgentMessage],
) -> Result<String, String> {
    messages
        .iter()
        .map(inter_agent_message_model_text)
        .collect::<Result<Vec<_>, _>>()
        .map(|messages| messages.join("\n\n"))
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
            title: String::from("Agent message"),
            body: inter_agent_message_model_text(message)?,
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
        kind: message.kind,
        message_type: String::from("MESSAGE"),
        task_id: message.task_id.clone(),
        task_name: message.target_agent_id.clone(),
        sender: inter_agent_message_sender(message),
        recipient: inter_agent_message_recipient(message),
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
/// The model text produced by the shared `AgentMessage` envelope, or its validation error.
/// 返回共享 `AgentMessage` 信封产生的模型文本，或其校验错误。
fn inter_agent_message_model_text(message: &QueuedInterAgentMessage) -> Result<String, String> {
    CoreConversationPayload::render_agent_message_text(
        &inter_agent_message_id(message),
        message.kind,
        &inter_agent_message_sender(message),
        &inter_agent_message_recipient(message),
        &message.task_id,
        &message.content,
    )
}

/// Returns the exact source session identity for one parent-to-child message.
/// 返回单条父到子消息的精确源会话身份。
///
/// # Parameters
/// 参数
/// `message` is the delivered inter-agent message.
/// `message` 是已投递的智能体间消息。
///
/// # Returns
/// 返回值
/// The UUID-shaped parent session identity used by the host envelope.
/// 宿主消息信封使用的 UUID 形父级会话身份。
fn inter_agent_message_sender(message: &QueuedInterAgentMessage) -> String {
    message.author_session_id.to_string()
}

/// Returns the exact target session identity for one parent-to-child message.
/// 返回单条父到子消息的精确目标会话身份。
fn inter_agent_message_recipient(message: &QueuedInterAgentMessage) -> String {
    message.target_task_id.to_string()
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

/// Allocates the child execution identity for one inbox resume.
/// 为一次 inbox 续跑分配子执行身份。
///
/// # Parameters
/// 参数
/// `messages` are the non-empty delivered messages rendered into the execution prompt.
/// `messages` 是渲染进执行提示词的非空已投递消息。
///
/// # Returns
/// 返回值
/// `agent-message:{first envelope event_id}:{attempt uuid}`; the root input action id thereby proves
/// to display clients that its text is host-rendered envelopes, while every attempt stays unique.
/// `agent-message:{首个信封 event_id}:{尝试 uuid}`；根输入动作身份借此向显示端证明其正文是宿主
/// 渲染的信封，同时每次尝试保持唯一。
fn inter_agent_execution_id(messages: &[QueuedInterAgentMessage]) -> String {
    let event_id = messages
        .first()
        .map(inter_agent_message_id)
        .unwrap_or_default();
    format!("agent-message:{event_id}:{}", Uuid::new_v4())
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
        (INTER_AGENT_CONTINUES_TASK_TURN_KEY): message.continues_task_turn,
    })
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
        title: String::from("Agent message"),
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
        (INTER_AGENT_CONTINUES_TASK_TURN_KEY): message.continues_task_turn,
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
/// `continues_task_turn` records whether this delivery belongs to the active logical child turn.
/// `continues_task_turn` 记录本次投递是否属于活动的子代理逻辑轮次。
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
    continues_task_turn: bool,
) -> VulcanToolInvocationResult {
    let model_payload = json!({
        "ok": true,
        "target": target.agent_id,
        (SUBAGENT_TASK_ID_KEY): target.session_id.to_string(),
        "queued": true,
        "delivery_mode": delivery_mode.as_str(),
        "interrupted": false,
        "delivery_status": "queued",
        "message": "The message was accepted for the child agent. Do not report a final child result from assumptions; wait for child-agent feedback.",
    });
    let host_payload = json!({
        "tool": tool_name,
        "parent_session_id": current_session_id.to_string(),
        "target": target.agent_id,
        (SUBAGENT_TASK_ID_KEY): target.session_id.to_string(),
        "delivery_id": delivery_id,
        "message": message,
        "queued": true,
        "delivery_mode": delivery_mode.as_str(),
        "interrupted": false,
        (INTER_AGENT_CONTINUES_TASK_TURN_KEY): continues_task_turn,
        (STOPPED_SUBAGENT_TASK_IDS_KEY): stopped_task_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "target_status_before_delivery": target.status,
        "delivery_status": "queued",
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
