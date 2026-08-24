use super::*;
use crate::types::ProviderContinuation;

/// Request object used to append one provider-visible inter-agent message.
/// 用于追加单条 Provider 可见代理间消息的请求对象。
#[derive(Debug, Clone)]
pub struct AgentMessageAppendRequest {
    /// Stable model-visible inter-agent message type.
    /// 模型可见的稳定代理间消息类型。
    pub message_type: String,
    /// Logical task or agent path associated with the message.
    /// 该消息关联的逻辑任务或代理路径。
    pub task_name: String,
    /// Logical sender name or path.
    /// 逻辑发送方名称或路径。
    pub sender: String,
    /// Logical recipient name or path.
    /// 逻辑接收方名称或路径。
    pub recipient: String,
    /// Model-visible child-agent result text.
    /// 模型可见的子代理结果文本。
    pub payload: String,
    /// Whether the message may wake an idle parent turn.
    /// 该消息是否可以唤醒空闲父级回合。
    pub trigger_turn: bool,
    /// Host-only structured audit data.
    /// 仅供宿主侧使用的结构化审计数据。
    pub metadata: Option<Value>,
}

/// Request object used to append one transcript-only display record.
/// 用于追加一条仅转录显示记录的请求对象。
#[derive(Debug, Clone)]
pub struct DisplayRecordAppendRequest {
    /// Optional stable record id provided by the host.
    /// 宿主提供的可选稳定记录标识。
    pub record_id: Option<String>,
    /// Transcript role used by the display projection.
    /// 显示投影使用的转录角色。
    pub role: TranscriptRole,
    /// Display title for the transcript row.
    /// 转录行使用的显示标题。
    pub title: String,
    /// Display body for the transcript row.
    /// 转录行使用的显示正文。
    pub body: String,
    /// Host-only metadata that should not enter the provider message window.
    /// 不应进入 Provider 消息窗口的宿主侧元数据。
    pub metadata: Option<Value>,
}

impl VulcanCoreController {
    /// Appends one user-authored prompt into the canonical message window and durable record stream.
    /// 将一条用户输入追加到规范消息窗口与持久化记录流中。
    pub(super) fn append_user_turn(
        &mut self,
        prompt: &str,
        attachments: Vec<ConversationAttachment>,
    ) -> String {
        let metadata = user_turn_record_metadata(prompt, &attachments);
        self.append_user_turn_with_metadata(prompt, attachments, metadata)
    }

    /// Appends one user turn with caller-provided transcript metadata.
    /// 使用调用方提供的转录元数据追加单个用户回合。
    ///
    /// # Parameters
    /// 参数
    /// `prompt` is the canonical user text appended to provider-visible history.
    /// `prompt` 是追加到 Provider 可见历史中的规范用户文本。
    /// `attachments` are structured attachments attached to the same user turn.
    /// `attachments` 是附着到同一用户回合的结构化附件。
    /// `metadata` is the exact visible transcript metadata stored on the user record.
    /// `metadata` 是存储在用户可见转录记录上的精确元数据。
    ///
    /// # Returns
    /// 返回值
    /// The generated provider-visible message id.
    /// 返回生成的 Provider 可见消息标识。
    pub(super) fn append_user_turn_with_metadata(
        &mut self,
        prompt: &str,
        attachments: Vec<ConversationAttachment>,
        metadata: Option<Value>,
    ) -> String {
        self.append_user_turn_with_record_body_and_metadata(prompt, prompt, attachments, metadata)
    }

    /// Appends one provider-visible user turn while retaining an independently controlled visible record body.
    /// 追加一条 Provider 可见用户回合，同时保留独立控制的可见记录正文。
    ///
    /// # Parameters
    /// 参数
    /// `provider_prompt` is the exact text sent in Provider-visible history.
    /// `provider_prompt` 是发送到 Provider 可见历史的精确文本。
    /// `record_body` is the user-visible transcript body without host-only guidance.
    /// `record_body` 是不含仅宿主可见引导的用户可见转录正文。
    /// `attachments` and `metadata` retain the structured user-input projection.
    /// `attachments` 与 `metadata` 保留结构化用户输入投影。
    ///
    /// # Returns
    /// 返回值
    /// The generated provider-visible message id.
    /// 返回生成的 Provider 可见消息标识。
    fn append_user_turn_with_record_body_and_metadata(
        &mut self,
        provider_prompt: &str,
        record_body: &str,
        attachments: Vec<ConversationAttachment>,
        metadata: Option<Value>,
    ) -> String {
        let message_id = self.next_local_id("user-message");
        let record_id = self.next_local_id("user-record");
        let timestamp = current_unix_millis_text();
        self.snapshot.messages.push(ConversationMessage {
            id: message_id.clone(),
            payload: ConversationPayload::UserText {
                text: provider_prompt.to_string(),
                attachments: attachments.clone(),
            },
        });
        self.snapshot.records.push(ConversationRecord {
            id: record_id,
            role: TranscriptRole::User,
            record_kind: TranscriptRecordKind::Visible,
            visible: true,
            title: String::from("用户指令"),
            body: record_body.to_string(),
            metadata,
            created_at: timestamp,
        });
        message_id
    }

    /// Appends one durable recovery follow-up after all preceding tool calls are paired with results.
    /// 在前序工具调用全部与结果配对后追加一条可靠恢复补充消息。
    ///
    /// # Parameters
    /// 参数
    /// `followup` contains distinct visible and Provider prompt text plus its stable admission identity.
    /// `followup` 包含相互区分的可见文本、Provider 文本及其稳定准入身份。
    ///
    /// # Returns
    /// 返回值
    /// The appended message id, or an explicit invalid-state error for malformed input.
    /// 返回追加后的消息标识；输入不合法时返回显式状态错误。
    pub fn append_restored_user_followup(
        &mut self,
        followup: RestoredUserFollowup,
    ) -> Result<String, CoreError> {
        let visible_prompt = followup.visible_prompt.trim();
        let provider_prompt = followup.provider_prompt.trim();
        let admission_id = followup.admission_id.trim();
        if visible_prompt.is_empty() || provider_prompt.is_empty() || admission_id.is_empty() {
            return Err(CoreError::InvalidState {
                message: String::from(
                    "Restored user follow-up requires non-empty visible text, provider text, and admission identity.",
                ),
            });
        }
        let metadata = interjected_user_turn_record_metadata(
            visible_prompt,
            &followup.attachments,
            Some(admission_id),
        );
        Ok(self.append_user_turn_with_record_body_and_metadata(
            provider_prompt,
            visible_prompt,
            followup.attachments,
            metadata,
        ))
    }

    /// Appends a host-guidance user turn that should be visible to the next provider request.
    /// 追加一条应对下一次 Provider 请求可见的宿主引导用户回合。
    ///
    /// # Parameters
    /// 参数
    /// `prompt` is the guidance text that entered the shared pending prompt queue.
    /// `prompt` 是已经进入共享待发送队列的引导文本。
    ///
    /// # Returns
    /// 返回值
    /// The appended message id, or `None` when the guidance is blank.
    /// 返回追加后的消息标识；若引导为空白则返回 `None`。
    pub fn append_interjected_user_turn(&mut self, prompt: &str) -> Option<String> {
        self.append_interjected_user_turn_with_attachments(prompt, Vec::new())
    }

    /// Appends a host-guidance user turn with attachments before the next provider request.
    /// 在下一次 Provider 请求前追加一条带附件的宿主引导用户回合。
    ///
    /// # Parameters
    /// 参数
    /// `prompt` is the guidance text that entered the shared pending prompt queue.
    /// `prompt` 是已经进入共享待发送队列的引导文本。
    /// `attachments` are the structured attachments captured with the queued prompt.
    /// `attachments` 是随排队提示词一起捕获的结构化附件。
    ///
    /// # Returns
    /// 返回值
    /// The appended message id, or `None` when both guidance and attachments are empty.
    /// 返回追加后的消息标识；若引导与附件均为空则返回 `None`。
    pub fn append_interjected_user_turn_with_attachments(
        &mut self,
        prompt: &str,
        attachments: Vec<ConversationAttachment>,
    ) -> Option<String> {
        self.append_admitted_interjected_user_turn_with_attachments(prompt, attachments, None)
    }

    /// Appends one interjected user turn and binds it to an optional admission projection.
    /// 追加一条插队用户输入，并将其绑定到可选的接纳阶段投影。
    ///
    /// # Parameters
    /// 参数
    /// `prompt` is the canonical model-visible user text.
    /// `prompt` 是模型可见的规范用户文本。
    /// `attachments` are the structured user attachments delivered with the text.
    /// `attachments` 是随文本一同投递的结构化用户附件。
    /// `admission_id` identifies the already-visible admission projection when present.
    /// `admission_id` 在存在时标识已经可见的接纳阶段投影。
    ///
    /// # Returns
    /// 返回值
    /// The appended provider-visible message id, or no value for empty input.
    /// 返回追加后的 Provider 可见消息标识；输入为空时返回空值。
    pub fn append_admitted_interjected_user_turn_with_attachments(
        &mut self,
        prompt: &str,
        attachments: Vec<ConversationAttachment>,
        admission_id: Option<&str>,
    ) -> Option<String> {
        let trimmed_prompt = prompt.trim();
        if trimmed_prompt.is_empty() && attachments.is_empty() {
            return None;
        }
        let metadata =
            interjected_user_turn_record_metadata(trimmed_prompt, &attachments, admission_id);
        Some(self.append_user_turn_with_metadata(trimmed_prompt, attachments, metadata))
    }

    /// Appends one typed inter-agent message into canonical history and transcript records.
    /// 将一条类型化代理间消息追加到规范历史与转录记录中。
    ///
    /// # Parameters
    /// 参数
    /// `request` carries the complete inter-agent message protocol payload.
    /// `request` 携带完整的代理间消息协议载荷。
    ///
    /// # Returns
    /// 返回值
    /// The generated provider-visible message id.
    /// 返回生成的 Provider 可见消息标识。
    pub fn append_agent_message(&mut self, request: AgentMessageAppendRequest) -> String {
        let message_id = self.next_local_id("agent-message");
        let record_id = self.next_local_id("agent-record");
        let timestamp = current_unix_millis_text();
        let AgentMessageAppendRequest {
            message_type,
            task_name,
            sender,
            recipient,
            payload,
            trigger_turn,
            metadata,
        } = request;
        let model_text = ConversationPayload::render_agent_message_text(
            &message_type,
            &task_name,
            &sender,
            &payload,
        );
        self.snapshot.messages.push(ConversationMessage {
            id: message_id.clone(),
            payload: ConversationPayload::AgentMessage {
                message_type,
                task_name,
                sender,
                recipient,
                payload,
                trigger_turn,
                metadata: metadata.clone(),
            },
        });
        self.snapshot.records.push(ConversationRecord {
            id: record_id,
            role: TranscriptRole::Assistant,
            record_kind: TranscriptRecordKind::Visible,
            visible: false,
            title: String::from("智能体消息"),
            body: model_text,
            metadata,
            created_at: timestamp,
        });
        message_id
    }

    /// Appends one display-only transcript record without changing provider-visible history.
    /// 追加一条仅显示转录记录，不改变 Provider 可见历史。
    ///
    /// # Parameters
    /// 参数
    /// `request` carries the stable record id, display text, role, and host metadata.
    /// `request` 携带稳定记录标识、显示文本、角色与宿主元数据。
    ///
    /// # Returns
    /// 返回值
    /// The generated or supplied record id, `None` for blank display text, or an identity error without mutation.
    /// 返回生成或传入的记录标识；显示文本为空时返回 `None`；身份无效时返回错误且不修改状态。
    pub fn append_display_record(
        &mut self,
        request: DisplayRecordAppendRequest,
    ) -> Result<Option<String>, CoreError> {
        if request.title.trim().is_empty() && request.body.trim().is_empty() {
            return Ok(None);
        }
        // record_id preserves a supplied durable identity only after exact namespace validation.
        // record_id 仅在完成精确命名空间校验后保留调用方提供的持久化身份。
        let record_id = match request.record_id {
            Some(record_id) => {
                self.validate_new_record_id(record_id.as_str())?;
                record_id
            }
            None => self.next_local_id("display-record"),
        };
        let timestamp = current_unix_millis_text();
        self.snapshot.records.push(ConversationRecord {
            id: record_id.clone(),
            role: request.role,
            record_kind: TranscriptRecordKind::ProjectionOnly,
            visible: true,
            title: request.title,
            body: request.body,
            metadata: request.metadata,
            created_at: timestamp,
        });
        Ok(Some(record_id))
    }

    /// Returns whether the controller still needs to append one new user turn instead of reusing the host-preseeded latest user prompt.
    /// 返回控制器当前是否仍需追加一条新的用户回合，而不是复用宿主已预播种的最新用户提示词。
    pub(super) fn should_append_user_turn(&self, prompt: &str) -> bool {
        let prompt = prompt.trim();
        let latest_user_matches = matches!(
            self.snapshot.messages.last(),
            Some(ConversationMessage {
                payload: ConversationPayload::UserText { text, .. },
                ..
            }) if text.trim() == prompt
        );
        if latest_user_matches {
            return false;
        }
        trailing_agent_messages_prompt(&self.snapshot.messages).as_deref() != Some(prompt)
    }

    /// Appends one provider assistant turn into the canonical message window and the durable record stream.
    /// 将一条 Provider 助手回合追加到规范消息窗口与持久化记录流中。
    pub(super) fn append_assistant_turn(&mut self, response: &ProviderTurnResponse) -> String {
        let message_id = self.next_local_id("assistant-message");
        let timestamp = current_unix_millis_text();
        self.snapshot.messages.push(ConversationMessage {
            id: message_id.clone(),
            payload: ConversationPayload::Assistant {
                text: response.assistant_text.clone(),
                reasoning: response.assistant_reasoning.clone(),
                provider_message_id: response.provider_message_id.clone(),
                provider_message_state: response.provider_message_state.clone(),
                // Physical-call Usage is persisted once by the session Provider ledger.
                // 物理调用用量仅由会话 Provider 账本持久化一次。
                usage: None,
                tool_calls: response.tool_calls.clone(),
            },
        });

        if !response.assistant_text.trim().is_empty() {
            let record_id = self.next_local_id("assistant-record");
            self.snapshot.records.push(ConversationRecord {
                id: record_id,
                role: TranscriptRole::Assistant,
                record_kind: TranscriptRecordKind::Visible,
                visible: true,
                title: String::from("模型输出"),
                body: response.assistant_text.clone(),
                metadata: None,
                created_at: timestamp.clone(),
            });
        }

        if !response.assistant_reasoning.trim().is_empty() {
            let record_id = self.next_local_id("thinking-record");
            self.snapshot.records.push(ConversationRecord {
                id: record_id,
                role: TranscriptRole::Thinking,
                record_kind: TranscriptRecordKind::Hidden,
                visible: false,
                title: String::from("模型思考"),
                body: response.assistant_reasoning.clone(),
                metadata: None,
                created_at: timestamp.clone(),
            });
        }

        for tool_call in &response.tool_calls {
            let record_id = self.next_local_id("tool-call-record");
            self.snapshot.records.push(ConversationRecord {
                id: record_id,
                role: TranscriptRole::Tool,
                record_kind: TranscriptRecordKind::Visible,
                visible: true,
                title: format!("调用 {} 工具", tool_call.tool_name),
                body: render_json_pretty(&tool_call.arguments),
                metadata: Some(json!({
                    "tool_call_id": resolved_tool_call_id(tool_call),
                    "tool_name": tool_call.tool_name,
                    "arguments": tool_call.arguments,
                    "argument_parse_error": tool_call.argument_parse_error,
                })),
                created_at: timestamp.clone(),
            });
        }

        message_id
    }

    /// Appends one tool result back into the canonical message window and durable record stream.
    /// 将一条工具结果回写到规范消息窗口与持久化记录流中。
    pub(super) fn append_tool_result_message(&mut self, result: &ToolInvocationResult) -> String {
        let message_id = self.next_local_id("tool-result-message");
        let record_id = self.next_local_id("tool-result-record");
        let timestamp = current_unix_millis_text();
        self.snapshot.messages.push(ConversationMessage {
            id: message_id.clone(),
            payload: ConversationPayload::ToolResult {
                tool_call_id: Some(result.tool_call_id.clone()),
                is_error: result.is_error,
                text: result.model_output.clone(),
            },
        });
        self.snapshot.records.push(ConversationRecord {
            id: record_id,
            role: TranscriptRole::Tool,
            record_kind: TranscriptRecordKind::Visible,
            visible: true,
            title: format!("调用 {} 工具", result.tool_name),
            body: result.model_output.clone(),
            metadata: Some(build_tool_result_metadata(result)),
            created_at: timestamp,
        });
        message_id
    }

    /// Refreshes the provider continuation snapshot from the latest assistant response and its exact local boundary.
    /// 根据最近一次助手响应及其精确本地边界刷新 Provider continuation 快照。
    ///
    /// # Parameters
    /// 参数
    /// `response` is the authoritative provider response carrying the upstream cursor.
    /// `response` 是携带上游游标的权威 Provider 响应。
    /// `assistant_message_id` is the exact local canonical assistant message represented by that response.
    /// `assistant_message_id` 是该响应所表示的精确本地规范助手消息标识。
    pub(super) fn refresh_continuation(
        &mut self,
        response: &ProviderTurnResponse,
        assistant_message_id: &str,
    ) {
        let mut next = response.continuation.clone().unwrap_or_default();
        next.provider_id
            .get_or_insert_with(|| self.config.provider_id.clone());
        next.model_id
            .get_or_insert_with(|| self.config.model_id.clone());
        next.last_provider_message_id
            .get_or_insert_with(|| response.provider_message_id.clone().unwrap_or_default());
        next.last_local_message_id = Some(assistant_message_id.to_string());
        next.context_cache_epoch_id = self
            .config
            .context_injection
            .as_ref()
            .and_then(|injection| injection.cache_epoch_id.clone());
        next.context_anchor_revision = self
            .config
            .context_injection
            .as_ref()
            .and_then(|injection| injection.anchor_revision);
        next.context_rewrite_fingerprint = Some(self.config.provider_rewrite_fingerprint());
        if next
            .last_provider_message_id
            .as_ref()
            .is_some_and(|message_id| !message_id.is_empty())
        {
            next.updated_at.get_or_insert_with(current_unix_millis_text);
            self.snapshot.continuation = next;
        } else {
            // A response without a usable upstream cursor invalidates any cursor retained from an older response.
            // 未携带可用上游游标的响应会使此前响应保留的任何游标失效。
            self.snapshot.continuation = ProviderContinuation::default();
        }
    }
}

/// Renders the contiguous trailing typed agent messages as one provider prompt.
/// 将末尾连续的类型化智能体消息渲染为一段 Provider 提示词。
///
/// # Parameters
/// 参数
/// `messages` is the canonical provider-visible conversation window.
/// `messages` 是规范的 Provider 可见会话窗口。
///
/// # Returns
/// 返回值
/// The joined model text when the tail contains at least one agent message.
/// 当尾部至少包含一条智能体消息时返回拼接后的模型文本。
fn trailing_agent_messages_prompt(messages: &[ConversationMessage]) -> Option<String> {
    let mut rendered_messages = Vec::new();
    for message in messages.iter().rev() {
        let Some(model_text) = message.payload.agent_message_model_text() else {
            break;
        };
        rendered_messages.push(model_text);
    }
    if rendered_messages.is_empty() {
        return None;
    }
    rendered_messages.reverse();
    Some(rendered_messages.join("\n\n"))
}
