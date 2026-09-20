use super::tool_execution::TOOL_ARGUMENT_PARSE_FAILURE_PREFIX;
use super::*;

/// Returns whether the active user turn already consumed its one malformed-tool correction attempt.
/// 返回当前用户回合是否已经消耗唯一一次工具参数纠错机会。
fn malformed_tool_correction_already_used(snapshot: &ConversationSnapshot) -> bool {
    for message in snapshot.messages.iter().rev() {
        match &message.payload {
            ConversationPayload::UserText { .. } => return false,
            ConversationPayload::ToolResult {
                is_error: true,
                text,
                ..
            } if text.starts_with(TOOL_ARGUMENT_PARSE_FAILURE_PREFIX) => return true,
            _ => {}
        }
    }
    false
}

/// Builds the explicit typed failure for one repeated malformed tool-call argument batch.
/// 为一次重复的非法工具调用参数批次构建显式的类型化失败。
///
/// The failure carries the exact repeated tool identity and the decoder reason so the terminal is
/// audit-grade across Controller, Agent, durable session state, and WebUI projection.
/// 失败携带精确的重复工具身份与解码原因，使终态在 Controller、Agent、可靠会话状态及
/// WebUI 投影之间具备可审计性。
///
/// # Parameters
/// 参数
/// `identity` optionally retains the first repeated tool name and its decoder reason; without it the
/// failure stays a stable protocol error that never names an unknown tool.
/// `identity` 可选保留首个重复工具名称及其解码原因；缺失时失败仍为稳定的协议错误，
/// 不会命名未知工具。
///
/// # Returns
/// 返回值
/// The typed tool-protocol failure that must close the user turn without a successful final.
/// 返回必须结束用户回合且不得产生成功 Final 的类型化工具协议失败。
fn build_repeated_malformed_tool_protocol_failure(
    identity: Option<&(String, String)>,
) -> CoreError {
    // toolName and decoderReason keep the observable identity of the repeated malformed call.
    // toolName 与 decoderReason 保留被重复提交的非法调用的可观测身份。
    let (tool_name, reason) = match identity {
        Some((tool_name, reason)) => (tool_name.as_str(), reason.as_str()),
        None => ("unknown", "the Provider did not report a decoder reason"),
    };
    CoreError::ToolProtocolFailure {
        tool_name: tool_name.to_string(),
        message: format!(
            "tool_argument_parse_failure_exhausted: the Provider emitted invalid arguments for tool `{tool_name}` twice in one user turn and only one automatic correction round is allowed. Tool `{tool_name}` was not executed. Decoder reason: {reason}",
        ),
    }
}

impl VulcanCoreController {
    /// Reconciles a retained tool batch, appends exactly one result per original call, and resumes the Provider loop.
    /// 协调已保留工具批次，为每条原始调用精确追加一个结果，然后恢复 Provider 循环。
    ///
    /// # Parameters
    /// 参数
    /// `adapter`, `executor`, and `policy` own the resumed Provider and tool execution loop.
    /// `adapter`、`executor` 与 `policy` 持有恢复后的 Provider 与工具执行循环。
    /// `recovery` binds the exact retained tool batch to the optional user follow-up admitted after reconciliation.
    /// `recovery` 将精确保留的工具批次与仅在对账后准入的可选用户补充绑定。
    /// `on_safe_checkpoint` publishes each canonical recovery boundary and `on_before_next_provider_turn` applies route or mailbox changes.
    /// `on_safe_checkpoint` 发布每个规范恢复边界，`on_before_next_provider_turn` 应用路由或 mailbox 变更。
    ///
    /// # Returns
    /// 返回值
    /// The completed controller result, or the first exact recovery, Provider, or tool error.
    /// 返回控制器完成结果，或首个精确的恢复、Provider 或工具错误。
    pub async fn run_restored_tool_batch_streaming_with_checkpoints_and_interventions<
        A,
        E,
        P,
        F,
        I,
    >(
        &mut self,
        adapter: &A,
        executor: &E,
        policy: &P,
        recovery: RestoredToolBatchRecovery,
        mut on_safe_checkpoint: F,
        mut on_before_next_provider_turn: I,
    ) -> Result<ControllerRunResult, CoreError>
    where
        A: VulcanStreamingProviderAdapter<Response = ProviderTurnResponse>,
        E: VulcanToolExecutor,
        P: VulcanToolPolicy,
        F: FnMut(&ConversationSnapshot, Option<&ProviderUsageSnapshot>) + Send,
        I: FnMut(&mut Self) -> Result<bool, CoreError> + Send,
    {
        // results contains exactly one reconciled output for every retained tool call in original Provider order.
        // results 按原始 Provider 顺序为每条保留工具调用保存且仅保存一条对账输出。
        let results = match self
            .execute_restored_tool_batch(executor, policy, recovery.tools)
            .await
        {
            Ok(results) => results,
            Err(error) => {
                self.mark_failed(error.to_string());
                return Err(error);
            }
        };
        for result in results {
            self.append_tool_result_message(&result);
        }
        // restoredUserFollowup is appended only after every old tool call has received a paired result.
        // restoredUserFollowup 仅在所有旧工具调用都获得配对结果后追加。
        if let Some(followup) = recovery.user_followup
            && let Err(error) = self.append_restored_user_followup(followup)
        {
            self.mark_failed(error.to_string());
            return Err(error);
        }
        on_safe_checkpoint(&self.snapshot, self.last_successful_provider_usage.as_ref());

        // interventionDraft makes route or mailbox intervention transactional against Core state.
        // interventionDraft 使路由或 mailbox 干预相对于 Core 状态具备事务性。
        let mut intervention_draft = self.clone();
        // interventionChanged records whether the host committed a Provider-visible mutation before dispatch.
        // interventionChanged 记录宿主是否在分派前提交了 Provider 可见变更。
        let intervention_changed = match on_before_next_provider_turn(&mut intervention_draft) {
            Ok(changed) => changed,
            Err(error) => {
                if !matches!(
                    &error,
                    CoreError::ProviderRouteSwitchRequired { .. }
                        | CoreError::ContextCompactionPolicyRefreshRequired { .. }
                ) {
                    self.mark_failed(error.to_string());
                }
                return Err(error);
            }
        };
        if intervention_changed {
            *self = intervention_draft;
            on_safe_checkpoint(&self.snapshot, self.last_successful_provider_usage.as_ref());
        }
        self.run_provider_tool_streaming_loop(
            adapter,
            executor,
            policy,
            on_safe_checkpoint,
            on_before_next_provider_turn,
        )
        .await
    }

    /// Resumes a retained Provider boundary after applying a user follow-up and runtime interventions in protocol-safe order.
    /// 在按协议安全顺序应用用户补充消息与运行时干预后恢复保留的 Provider 边界。
    ///
    /// # Parameters
    /// 参数
    /// `adapter`, `executor`, and `policy` own the next Provider/tool loop.
    /// `adapter`、`executor` 与 `policy` 持有下一段 Provider/工具循环。
    /// `restored_user_followup` is appended before any new Provider request but after already-reconciled tool results.
    /// `restored_user_followup` 会在任何新 Provider 请求前、且在已对账工具结果后追加。
    /// `on_safe_checkpoint` publishes canonical recovery state and `on_before_next_provider_turn` applies route or mailbox changes.
    /// `on_safe_checkpoint` 发布规范恢复状态，`on_before_next_provider_turn` 应用路由或 mailbox 变更。
    ///
    /// # Returns
    /// 返回值
    /// The completed controller result, or the first exact recovery/provider/tool error.
    /// 返回控制器完成结果，或首个精确恢复、Provider 或工具错误。
    pub async fn run_restored_provider_loop_streaming_with_checkpoints_and_interventions<
        A,
        E,
        P,
        F,
        I,
    >(
        &mut self,
        adapter: &A,
        executor: &E,
        policy: &P,
        restored_user_followup: Option<RestoredUserFollowup>,
        mut on_safe_checkpoint: F,
        mut on_before_next_provider_turn: I,
    ) -> Result<ControllerRunResult, CoreError>
    where
        A: VulcanStreamingProviderAdapter<Response = ProviderTurnResponse>,
        E: VulcanToolExecutor,
        P: VulcanToolPolicy,
        F: FnMut(&ConversationSnapshot, Option<&ProviderUsageSnapshot>) + Send,
        I: FnMut(&mut Self) -> Result<bool, CoreError> + Send,
    {
        // restoredUserFollowup is the only new user-authored boundary admitted into this retained Provider loop.
        // restoredUserFollowup 是获准进入该保留 Provider 循环的唯一新用户输入边界。
        if let Some(followup) = restored_user_followup {
            if let Err(error) = self.append_restored_user_followup(followup) {
                self.mark_failed(error.to_string());
                return Err(error);
            }
            on_safe_checkpoint(&self.snapshot, self.last_successful_provider_usage.as_ref());
        }

        // interventionDraft makes the pending model route or mailbox change atomic before the next real Provider request.
        // interventionDraft 使待生效模型路由或 mailbox 变更在下一次真实 Provider 请求前原子应用。
        let mut intervention_draft = self.clone();
        // interventionChanged controls whether the validated draft becomes the new canonical checkpoint.
        // interventionChanged 控制已校验草稿是否成为新的规范检查点。
        let intervention_changed = match on_before_next_provider_turn(&mut intervention_draft) {
            Ok(changed) => changed,
            Err(error) => {
                if !matches!(
                    &error,
                    CoreError::ProviderRouteSwitchRequired { .. }
                        | CoreError::ContextCompactionPolicyRefreshRequired { .. }
                ) {
                    self.mark_failed(error.to_string());
                }
                return Err(error);
            }
        };
        if intervention_changed {
            *self = intervention_draft;
            on_safe_checkpoint(&self.snapshot, self.last_successful_provider_usage.as_ref());
        }
        self.ensure_restored_provider_turn()?;
        self.run_provider_tool_streaming_loop(
            adapter,
            executor,
            policy,
            on_safe_checkpoint,
            on_before_next_provider_turn,
        )
        .await
    }

    /// Commits a Provider-proven final assistant tail without dispatching another Provider request.
    /// 在不再次分派 Provider 请求的情况下提交已由 Provider 证明的最终助手尾部。
    pub fn finalize_restored_assistant(&mut self) -> Result<ControllerRunResult, CoreError> {
        let assistant_message_id = match self.snapshot.messages.last() {
            Some(ConversationMessage {
                id,
                payload: ConversationPayload::Assistant { tool_calls, .. },
            }) if tool_calls.is_empty() => id.clone(),
            _ => {
                let error = CoreError::InvalidState {
                    message: String::from(
                        "Restored assistant finalization requires a tool-free assistant message at the canonical tail.",
                    ),
                };
                self.mark_failed(error.to_string());
                return Err(error);
            }
        };
        self.phase = ControllerPhase::Completed;
        self.push_final_event(0, 0, Some(assistant_message_id.as_str()));
        Ok(ControllerRunResult {
            provider_turns_executed: 0,
            tool_calls_processed: 0,
            final_assistant_message_id: Some(assistant_message_id),
        })
    }

    /// Executes one provider turn and updates the controller phase according to the outcome.
    /// 执行一次 Provider 回合，并根据结果更新控制器阶段。
    pub async fn execute_once<A>(&mut self, adapter: &A) -> Result<A::Response, CoreError>
    where
        A: VulcanProviderAdapter,
    {
        let request = self.build_request(adapter).await?;
        if let Err(error) = adapter.validate_built_request(&request) {
            self.phase = ControllerPhase::Failed;
            self.push_event(CoreEvent {
                kind: CoreEventKind::Error,
                title: String::from("控制器失败"),
                body: error.to_string(),
                metadata: Some(Value::String(adapter.provider_id().to_string())),
            });
            return Err(error);
        }
        self.phase = ControllerPhase::WaitingForProvider;
        match adapter.execute_request(request).await {
            Ok(response) => {
                self.phase = ControllerPhase::Completed;
                self.push_event(CoreEvent {
                    kind: CoreEventKind::Final,
                    title: String::from("控制器完成"),
                    body: String::from("一次 Provider 回合已成功结束。"),
                    metadata: None,
                });
                Ok(response)
            }
            Err(error) => {
                self.phase = ControllerPhase::Failed;
                self.push_event(CoreEvent {
                    kind: CoreEventKind::Error,
                    title: String::from("控制器失败"),
                    body: error.to_string(),
                    metadata: Some(Value::String(adapter.provider_id().to_string())),
                });
                Err(error)
            }
        }
    }

    /// Runs one complete user-authored turn through the host-owned provider and tool loop until the assistant settles.
    /// 运行一次完整的用户输入回合，并由宿主自有 Provider 与工具循环持续推进直到助手稳定结束。
    pub async fn run_user_turn<A, E, P>(
        &mut self,
        adapter: &A,
        executor: &E,
        policy: &P,
        user_prompt: impl Into<String>,
    ) -> Result<ControllerRunResult, CoreError>
    where
        A: VulcanProviderAdapter<Response = ProviderTurnResponse>,
        E: VulcanToolExecutor,
        P: VulcanToolPolicy,
    {
        let user_prompt = user_prompt.into();
        let trimmed_prompt = user_prompt.trim();
        if trimmed_prompt.is_empty() {
            let error = CoreError::InvalidState {
                message: String::from("控制器无法执行空白用户输入。"),
            };
            self.mark_failed(error.to_string());
            return Err(error);
        }

        if self.should_append_user_turn(trimmed_prompt) {
            self.append_user_turn(trimmed_prompt, Vec::new());
        }
        let mut provider_turns_executed = 0usize;
        let mut tool_calls_processed = 0usize;
        // malformedToolCorrectionUsed limits malformed Provider arguments to one model correction round.
        // malformedToolCorrectionUsed 将格式错误的 Provider 参数限制为一次模型纠错轮。
        let mut malformed_tool_correction_used =
            malformed_tool_correction_already_used(&self.snapshot);

        loop {
            if provider_turns_executed >= self.config.max_turns {
                let error = CoreError::InvalidState {
                    message: format!(
                        "控制器已达到最大 Provider 回合限制 `{}`，仍存在未收敛的工具循环。",
                        self.config.max_turns
                    ),
                };
                self.mark_failed(error.to_string());
                return Err(error);
            }

            let response = self.execute_provider_turn(adapter).await?;
            provider_turns_executed += 1;

            if !response.assistant_reasoning.trim().is_empty() {
                self.record_thinking(response.assistant_reasoning.clone());
            }
            if !response.assistant_text.trim().is_empty() {
                self.record_output(response.assistant_text.clone());
            }

            if let Some(usage) = response.usage.clone() {
                self.last_successful_provider_usage = Some(usage);
            }

            let assistant_message_id = self.append_assistant_turn(&response);
            self.refresh_continuation(&response, assistant_message_id.as_str());
            let explicit_end_turn = match response.explicit_end_turn() {
                Ok(value) => value,
                Err(error) => {
                    self.mark_failed(error.to_string());
                    return Err(error);
                }
            };
            if response.tool_calls.is_empty() && explicit_end_turn != Some(false) {
                self.phase = ControllerPhase::Completed;
                self.push_final_event(
                    provider_turns_executed,
                    tool_calls_processed,
                    Some(assistant_message_id.as_str()),
                );
                return Ok(ControllerRunResult {
                    provider_turns_executed,
                    tool_calls_processed,
                    final_assistant_message_id: Some(assistant_message_id),
                });
            }

            let requests = response
                .tool_calls
                .iter()
                .map(|tool_call| ToolInvocationRequest {
                    request_stream_id: tool_call.id.clone(),
                    tool_call_id: resolved_tool_call_id(tool_call),
                    tool_name: tool_call.tool_name.clone(),
                    arguments: tool_call.arguments.clone(),
                    argument_parse_error: tool_call.argument_parse_error.clone(),
                })
                .collect::<Vec<_>>();
            // malformedArgumentsObserved is decided from the authoritative completed Provider response.
            // malformedArgumentsObserved 由权威的完整 Provider 响应判定。
            let malformed_arguments_observed = requests
                .iter()
                .any(|request| request.argument_parse_error.is_some());
            // malformedToolIdentity retains the exact repeated tool identity and decoder reason for the typed protocol failure.
            // malformedToolIdentity 保留重复提交的精确工具身份与解码原因，用于类型化协议失败。
            let malformed_tool_identity = requests.iter().find_map(|request| {
                request
                    .argument_parse_error
                    .as_ref()
                    .map(|reason| (request.tool_name.clone(), reason.clone()))
            });
            let results = match self.execute_tool_batch(executor, policy, requests).await {
                Ok(results) => results,
                Err(error) => {
                    self.mark_failed(error.to_string());
                    return Err(error);
                }
            };
            for result in results {
                tool_calls_processed += 1;
                self.append_tool_result_message(&result);
            }
            if malformed_arguments_observed {
                if malformed_tool_correction_used {
                    // toolProtocolFailure is the explicit typed terminal for a second malformed tool-argument batch.
                    // toolProtocolFailure 是第二个非法工具参数批次的显式类型化终态。
                    let error = build_repeated_malformed_tool_protocol_failure(
                        malformed_tool_identity.as_ref(),
                    );
                    self.mark_failed(error.to_string());
                    return Err(error);
                }
                malformed_tool_correction_used = true;
            }
        }
    }

    /// Runs one complete user-authored turn through one streaming-capable provider adapter while preserving streamed thinking and output deltas.
    /// 通过一个支持流式增量的 Provider 适配器运行一次完整用户回合，并保留流式思考与输出增量。
    pub async fn run_user_turn_streaming<A, E, P>(
        &mut self,
        adapter: &A,
        executor: &E,
        policy: &P,
        user_prompt: impl Into<String>,
    ) -> Result<ControllerRunResult, CoreError>
    where
        A: VulcanStreamingProviderAdapter<Response = ProviderTurnResponse>,
        E: VulcanToolExecutor,
        P: VulcanToolPolicy,
    {
        self.run_user_turn_streaming_with_checkpoints(
            adapter,
            executor,
            policy,
            user_prompt,
            |_, _| {},
        )
        .await
    }

    /// Runs one streaming user-authored turn and calls the provided hook whenever the canonical snapshot reaches a replay-safe boundary.
    /// 运行一次流式用户回合，并在规范快照到达可回放安全边界时调用给定钩子。
    pub async fn run_user_turn_streaming_with_checkpoints<A, E, P, F>(
        &mut self,
        adapter: &A,
        executor: &E,
        policy: &P,
        user_prompt: impl Into<String>,
        on_safe_checkpoint: F,
    ) -> Result<ControllerRunResult, CoreError>
    where
        A: VulcanStreamingProviderAdapter<Response = ProviderTurnResponse>,
        E: VulcanToolExecutor,
        P: VulcanToolPolicy,
        F: FnMut(&ConversationSnapshot, Option<&ProviderUsageSnapshot>) + Send,
    {
        self.run_user_turn_streaming_with_checkpoints_and_interventions(
            adapter,
            executor,
            policy,
            user_prompt,
            on_safe_checkpoint,
            |_| Ok(false),
        )
        .await
    }

    /// Runs one streaming user turn and lets the host append guidance at replay-safe tool boundaries.
    /// 运行一次流式用户回合，并允许宿主在可回放安全的工具边界追加引导。
    pub async fn run_user_turn_streaming_with_checkpoints_and_interventions<A, E, P, F, I>(
        &mut self,
        adapter: &A,
        executor: &E,
        policy: &P,
        user_prompt: impl Into<String>,
        on_safe_checkpoint: F,
        on_before_next_provider_turn: I,
    ) -> Result<ControllerRunResult, CoreError>
    where
        A: VulcanStreamingProviderAdapter<Response = ProviderTurnResponse>,
        E: VulcanToolExecutor,
        P: VulcanToolPolicy,
        F: FnMut(&ConversationSnapshot, Option<&ProviderUsageSnapshot>) + Send,
        I: FnMut(&mut Self) -> Result<bool, CoreError> + Send,
    {
        let user_prompt = user_prompt.into();
        let trimmed_prompt = user_prompt.trim();
        if trimmed_prompt.is_empty() {
            let error = CoreError::InvalidState {
                message: String::from("控制器无法执行空白用户输入。"),
            };
            self.mark_failed(error.to_string());
            return Err(error);
        }

        if self.should_append_user_turn(trimmed_prompt) {
            self.append_user_turn(trimmed_prompt, Vec::new());
        }
        self.run_provider_tool_streaming_loop(
            adapter,
            executor,
            policy,
            on_safe_checkpoint,
            on_before_next_provider_turn,
        )
        .await
    }

    /// Runs one streaming turn from provider-visible input that has already been appended to the canonical snapshot.
    /// 从已经追加到规范快照的 Provider 可见输入运行一次流式回合。
    pub async fn run_seeded_turn_streaming_with_checkpoints_and_interventions<A, E, P, F, I>(
        &mut self,
        adapter: &A,
        executor: &E,
        policy: &P,
        on_safe_checkpoint: F,
        on_before_next_provider_turn: I,
    ) -> Result<ControllerRunResult, CoreError>
    where
        A: VulcanStreamingProviderAdapter<Response = ProviderTurnResponse>,
        E: VulcanToolExecutor,
        P: VulcanToolPolicy,
        F: FnMut(&ConversationSnapshot, Option<&ProviderUsageSnapshot>) + Send,
        I: FnMut(&mut Self) -> Result<bool, CoreError> + Send,
    {
        self.ensure_seeded_provider_turn()?;
        self.run_provider_tool_streaming_loop(
            adapter,
            executor,
            policy,
            on_safe_checkpoint,
            on_before_next_provider_turn,
        )
        .await
    }

    /// Validate a retained loop boundary without changing the stricter seeded-input entrypoint.
    /// 校验保留循环边界，不改变更严格的播种输入入口。
    /// No arguments; accepts seeded input or a tool-free assistant tail with preceding nonempty input.
    /// 无参数；允许播种输入，或具有前置非空输入且不含工具调用的助手尾部。
    /// Returns an explicit error before dispatch for empty history or unresolved assistant tool calls.
    /// 对空历史或尚未解决的助手工具调用，在派发前返回明确错误。
    fn ensure_restored_provider_turn(&mut self) -> Result<(), CoreError> {
        if conversation_snapshot_has_seeded_provider_input(&self.snapshot) {
            return Ok(());
        }
        // Continue preserves interrupted text as history; it does not fabricate another user prompt.
        // 继续动作将中断文本保留为历史，不伪造另一条用户提示词。
        let retained_assistant = matches!(
            self.snapshot.messages.last().map(|message| &message.payload),
            Some(ConversationPayload::Assistant { text, reasoning, tool_calls, .. })
                if tool_calls.is_empty() && (!text.trim().is_empty() || !reasoning.trim().is_empty())
        );
        // An assistant-only transcript is not a valid source of new work even through the restore API.
        // 即使通过恢复接口，只有助手消息的转录也不是合法的新工作来源。
        let has_original_input = self.snapshot.messages.iter().rev().skip(1).any(|message| {
            matches!(&message.payload,
                ConversationPayload::UserText { text, attachments }
                    if !text.trim().is_empty() || !attachments.is_empty())
                || matches!(&message.payload,
                    ConversationPayload::AgentMessage { payload, .. } if !payload.trim().is_empty())
        });
        if retained_assistant && has_original_input {
            return Ok(());
        }
        // Keep malformed recovery explicit and never fall back to a new user turn.
        // 明确暴露非法恢复，不降级为新用户回合。
        let error = CoreError::InvalidState {
            message: String::from(
                "Restored Provider execution requires seeded input or a tool-free assistant tail with prior input.",
            ),
        };
        self.mark_failed(error.to_string());
        Err(error)
    }

    /// Ensures the snapshot already contains a provider-visible message that can start a seeded turn.
    /// 确认快照中已经存在可启动播种回合的 Provider 可见消息。
    fn ensure_seeded_provider_turn(&mut self) -> Result<(), CoreError> {
        let has_seeded_provider_input =
            conversation_snapshot_has_seeded_provider_input(&self.snapshot);
        if has_seeded_provider_input {
            return Ok(());
        }

        let error = CoreError::InvalidState {
            message: String::from("控制器无法执行缺少 Provider 可见输入的播种回合。"),
        };
        self.mark_failed(error.to_string());
        Err(error)
    }

    /// Runs the shared streaming provider/tool loop after the turn's opening input has been prepared.
    /// 在回合起始输入准备完成后运行共享的流式 Provider/工具循环。
    async fn run_provider_tool_streaming_loop<A, E, P, F, I>(
        &mut self,
        adapter: &A,
        executor: &E,
        policy: &P,
        mut on_safe_checkpoint: F,
        mut on_before_next_provider_turn: I,
    ) -> Result<ControllerRunResult, CoreError>
    where
        A: VulcanStreamingProviderAdapter<Response = ProviderTurnResponse>,
        E: VulcanToolExecutor,
        P: VulcanToolPolicy,
        F: FnMut(&ConversationSnapshot, Option<&ProviderUsageSnapshot>) + Send,
        I: FnMut(&mut Self) -> Result<bool, CoreError> + Send,
    {
        let mut provider_turns_executed = 0usize;
        let mut tool_calls_processed = 0usize;
        // malformedToolCorrectionUsed survives recovered checkpoints through the exact generated error marker.
        // malformedToolCorrectionUsed 通过精确的生成错误标记跨恢复检查点保留。
        let mut malformed_tool_correction_used =
            malformed_tool_correction_already_used(&self.snapshot);

        loop {
            if provider_turns_executed >= self.config.max_turns {
                let error = CoreError::InvalidState {
                    message: format!(
                        "控制器已达到最大 Provider 回合限制 `{}`，仍存在未收敛的工具循环。",
                        self.config.max_turns
                    ),
                };
                self.mark_failed(error.to_string());
                return Err(error);
            }

            let (response, deltas) = self.execute_provider_turn_streaming(adapter).await?;
            provider_turns_executed += 1;
            self.apply_provider_turn_deltas(&deltas);

            if let Some(usage) = response.usage.clone() {
                self.last_successful_provider_usage = Some(usage);
            }

            let assistant_message_id = self.append_assistant_turn(&response);
            self.refresh_continuation(&response, assistant_message_id.as_str());
            // checkpoint_before_tools publishes the completed Provider call usage before any tool can block the next turn.
            // checkpoint_before_tools 在任何工具可能阻塞下一回合前发布已完成 Provider 调用的用量。
            on_safe_checkpoint(&self.snapshot, self.last_successful_provider_usage.as_ref());

            let explicit_end_turn = match response.explicit_end_turn() {
                Ok(value) => value,
                Err(error) => {
                    self.mark_failed(error.to_string());
                    return Err(error);
                }
            };
            if response.tool_calls.is_empty() && explicit_end_turn != Some(false) {
                self.phase = ControllerPhase::Completed;
                self.push_final_event(
                    provider_turns_executed,
                    tool_calls_processed,
                    Some(assistant_message_id.as_str()),
                );
                return Ok(ControllerRunResult {
                    provider_turns_executed,
                    tool_calls_processed,
                    final_assistant_message_id: Some(assistant_message_id),
                });
            }

            if response.tool_calls.is_empty() {
                // endTurnFalseIntervention gives queued input and route-policy owners the same
                // safe sampling boundary used after a completed tool batch.
                // endTurnFalseIntervention 让排队输入与路由策略所有者复用工具批次完成后的安全采样边界。
                let mut intervention_draft = self.clone();
                let intervention_changed =
                    match on_before_next_provider_turn(&mut intervention_draft) {
                        Ok(changed) => changed,
                        Err(error) => {
                            if !matches!(
                                &error,
                                CoreError::ProviderRouteSwitchRequired { .. }
                                    | CoreError::ContextCompactionPolicyRefreshRequired { .. }
                            ) {
                                self.mark_failed(error.to_string());
                            }
                            return Err(error);
                        }
                    };
                if intervention_changed {
                    *self = intervention_draft;
                    on_safe_checkpoint(
                        &self.snapshot,
                        self.last_successful_provider_usage.as_ref(),
                    );
                }
                continue;
            }

            let requests = response
                .tool_calls
                .iter()
                .map(|tool_call| ToolInvocationRequest {
                    request_stream_id: tool_call.id.clone(),
                    tool_call_id: resolved_tool_call_id(tool_call),
                    tool_name: tool_call.tool_name.clone(),
                    arguments: tool_call.arguments.clone(),
                    argument_parse_error: tool_call.argument_parse_error.clone(),
                })
                .collect::<Vec<_>>();
            // malformedArgumentsObserved is true when any call in this authoritative batch is non-executable JSON.
            // malformedArgumentsObserved 在该权威批次中任一调用为不可执行 JSON 时为 true。
            let malformed_arguments_observed = requests
                .iter()
                .any(|request| request.argument_parse_error.is_some());
            // malformedToolIdentity retains the exact repeated tool identity and decoder reason for the typed protocol failure.
            // malformedToolIdentity 保留重复提交的精确工具身份与解码原因，用于类型化协议失败。
            let malformed_tool_identity = requests.iter().find_map(|request| {
                request
                    .argument_parse_error
                    .as_ref()
                    .map(|reason| (request.tool_name.clone(), reason.clone()))
            });
            let results = match self.execute_tool_batch(executor, policy, requests).await {
                Ok(results) => results,
                Err(error) => {
                    self.mark_failed(error.to_string());
                    return Err(error);
                }
            };
            for result in results {
                tool_calls_processed += 1;
                self.append_tool_result_message(&result);
            }
            // checkpoint_after_tool_batch publishes only the stable batch where every tool call has its result.
            // checkpoint_after_tool_batch 仅发布每个工具调用均已有结果的稳定批次。
            on_safe_checkpoint(&self.snapshot, self.last_successful_provider_usage.as_ref());
            if malformed_arguments_observed {
                if malformed_tool_correction_used {
                    // toolProtocolFailure is the explicit typed terminal for a second malformed tool-argument batch.
                    // toolProtocolFailure 是第二个非法工具参数批次的显式类型化终态。
                    let error = build_repeated_malformed_tool_protocol_failure(
                        malformed_tool_identity.as_ref(),
                    );
                    self.mark_failed(error.to_string());
                    return Err(error);
                }
                malformed_tool_correction_used = true;
            }
            // intervention_draft prevents a failed or falsely reported host callback from partially mutating Core state.
            // intervention_draft 防止失败或错误报告未变化的宿主回调局部修改 Core 状态。
            let mut intervention_draft = self.clone();
            let intervention_changed = match on_before_next_provider_turn(&mut intervention_draft) {
                Ok(changed) => changed,
                Err(error) => {
                    if !matches!(
                        &error,
                        CoreError::ProviderRouteSwitchRequired { .. }
                            | CoreError::ContextCompactionPolicyRefreshRequired { .. }
                    ) {
                        self.mark_failed(error.to_string());
                    }
                    return Err(error);
                }
            };
            if intervention_changed {
                *self = intervention_draft;
                on_safe_checkpoint(&self.snapshot, self.last_successful_provider_usage.as_ref());
            }
        }
    }
}
