use super::*;

impl VulcanCoreController {
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
            if response.tool_calls.is_empty() {
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
                    tool_call_id: resolved_tool_call_id(tool_call),
                    tool_name: tool_call.tool_name.clone(),
                    arguments: tool_call.arguments.clone(),
                })
                .collect::<Vec<_>>();
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

    /// Ensures the snapshot already contains a provider-visible message that can start a seeded turn.
    /// 确认快照中已经存在可启动播种回合的 Provider 可见消息。
    fn ensure_seeded_provider_turn(&mut self) -> Result<(), CoreError> {
        let has_seeded_provider_input = matches!(
            self.snapshot.messages.last().map(|message| &message.payload),
            Some(ConversationPayload::UserText { text, .. }) if !text.trim().is_empty()
        ) || matches!(
            self.snapshot.messages.last().map(|message| &message.payload),
            Some(ConversationPayload::AgentMessage { payload, .. }) if !payload.trim().is_empty()
        ) || matches!(
            self.snapshot.messages.last().map(|message| &message.payload),
            Some(ConversationPayload::ToolResult { text, .. }) if !text.trim().is_empty()
        );
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

            if response.tool_calls.is_empty() {
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
                    tool_call_id: resolved_tool_call_id(tool_call),
                    tool_name: tool_call.tool_name.clone(),
                    arguments: tool_call.arguments.clone(),
                })
                .collect::<Vec<_>>();
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
                on_safe_checkpoint(&self.snapshot, self.last_successful_provider_usage.as_ref());
            }
            // intervention_draft prevents a failed or falsely reported host callback from partially mutating Core state.
            // intervention_draft 防止失败或错误报告未变化的宿主回调局部修改 Core 状态。
            let mut intervention_draft = self.clone();
            let intervention_changed = match on_before_next_provider_turn(&mut intervention_draft) {
                Ok(changed) => changed,
                Err(error) => {
                    self.mark_failed(error.to_string());
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
